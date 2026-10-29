"""The rig (``assets/rig/default_rig.json``) and forward kinematics, as the app has them.

Rotations act on the rig's world axes (Z up), with every bone at rest along those axes at its head.
So for a body with rest heads h:

    R_i = R_parent(i) q_i
    P_i = P_parent(i) + R_parent(i) (h_i - h_parent(i))
    P_root = h_root + root_offset * hip_height

which is ``ac_core``'s forward kinematics. Pure numpy.
"""

from __future__ import annotations

import json
import pathlib
from dataclasses import dataclass

import numpy as np

from . import quat


def quat_to_mat(q: np.ndarray) -> np.ndarray:
    q = quat.normalize(q)
    x, y, z, w = np.moveaxis(q, -1, 0)
    return np.stack(
        [
            np.stack([1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)], -1),
            np.stack([2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)], -1),
            np.stack([2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)], -1),
        ],
        -2,
    )


def mat_to_quat(M: np.ndarray) -> np.ndarray:
    """Rotation matrix -> unit quaternion (x, y, z, w) with w >= 0 (Shepperd's method)."""
    M = np.asarray(M, dtype=np.float64)
    t = np.trace(M)
    if t > 0:
        s = np.sqrt(t + 1.0) * 2
        q = [(M[2, 1] - M[1, 2]) / s, (M[0, 2] - M[2, 0]) / s, (M[1, 0] - M[0, 1]) / s, 0.25 * s]
    elif M[0, 0] > M[1, 1] and M[0, 0] > M[2, 2]:
        s = np.sqrt(1.0 + M[0, 0] - M[1, 1] - M[2, 2]) * 2
        q = [0.25 * s, (M[0, 1] + M[1, 0]) / s, (M[0, 2] + M[2, 0]) / s, (M[2, 1] - M[1, 2]) / s]
    elif M[1, 1] > M[2, 2]:
        s = np.sqrt(1.0 + M[1, 1] - M[0, 0] - M[2, 2]) * 2
        q = [(M[0, 1] + M[1, 0]) / s, 0.25 * s, (M[1, 2] + M[2, 1]) / s, (M[0, 2] - M[2, 0]) / s]
    else:
        s = np.sqrt(1.0 + M[2, 2] - M[0, 0] - M[1, 1]) * 2
        q = [(M[0, 2] + M[2, 0]) / s, (M[1, 2] + M[2, 1]) / s, 0.25 * s, (M[1, 0] - M[0, 1]) / s]
    q = quat.normalize(np.array(q))
    return q if q[3] >= 0 else -q


@dataclass
class Skeleton:
    names: list[str]
    parents: np.ndarray  # (B,) int, -1 for the root
    heads: np.ndarray  # (B, 3) rest heads in the rig's world
    hip_height: float  # root height above the lowest rest vertex (scales root offsets)

    def __post_init__(self):
        self.parents = np.asarray(self.parents, dtype=int)
        self.heads = np.asarray(self.heads, dtype=np.float64)
        self.index = {n: i for i, n in enumerate(self.names)}
        for i, p in enumerate(self.parents):
            if p >= i:
                raise ValueError("bones must be ordered parents first")

    def chain(self, bone: str, up_to: str | None = None) -> list[int]:
        """Indices from ``bone`` up to (and including) ``up_to``, or to the root."""
        out, i = [], self.index[bone]
        while i >= 0:
            out.append(i)
            if up_to is not None and self.names[i] == up_to:
                return out
            i = int(self.parents[i])
        if up_to is not None:
            raise ValueError(f"{up_to} is not an ancestor of {bone}")
        return out

    def fk(self, rotations: np.ndarray, root: np.ndarray):
        """World rotations (B, 3, 3) and joint positions (B, 3) of one pose."""
        local = quat_to_mat(rotations)
        R = np.empty_like(local)
        P = np.empty((len(self.names), 3))
        for i, p in enumerate(self.parents):
            if p < 0:
                R[i] = local[i]
                P[i] = self.heads[i] + np.asarray(root) * self.hip_height
            else:
                R[i] = R[p] @ local[i]
                P[i] = P[p] + R[p] @ (self.heads[i] - self.heads[p])
        return R, P

    @classmethod
    def load(cls, path=None) -> "Skeleton":
        """The app's rig (``assets/rig/default_rig.json`` unless ``path`` names another)."""
        from . import ASSETS

        rig = json.loads(pathlib.Path(path or ASSETS / "rig" / "default_rig.json").read_text(encoding="utf-8"))
        return cls(names=rig["names"], parents=rig["parents"], heads=np.array(rig["heads"], dtype=np.float64),
                   hip_height=float(rig["hip_height"]))
