"""Anatomical joint angles, as ``ac_core::anatomy`` computes them (read its module doc for the maths).

For each joint, ``T = R_P^-1 R_D Z`` (the distal bone's world rotation against the proximal
bone's, times the turn from the anatomical zero to the rest direction) splits into a swing after a
twist about the zero direction ``u``: flexion and abduction are the swing's rotation vector along
``u x d`` (``d`` the direction the segment moves toward), rotation is the signed twist. Degrees.
Vectorised over frames; ``assets/rig/ranges.json`` carries sample poses that the Rust tests check
against these numbers.
"""

from __future__ import annotations

import numpy as np

from . import quat
from .rig import Skeleton

FWD = (0.0, -1.0, 0.0)
BACK = (0.0, 1.0, 0.0)
UP = (0.0, 0.0, 1.0)
DOWN = (0.0, 0.0, -1.0)
OUT = (1.0, 0.0, 0.0)
IN = (-1.0, 0.0, 0.0)

# joint: (sided, proximal, distal, zero (a body direction or two bones), rest bones or None,
#         flex_to, abd_to, (twist_from, twist_to))
DEFS = {
    "hip": (True, "root", "upperleg02", DOWN, ("upperleg01", "lowerleg01"), FWD, OUT, (FWD, IN)),
    "knee": (True, "upperleg02", "lowerleg02", ("upperleg01", "lowerleg01"), ("lowerleg01", "foot"), BACK, OUT, (FWD, IN)),
    "ankle": (True, "lowerleg02", "foot", ("foot", "toe3-1"), None, UP, OUT, (DOWN, IN)),
    "shoulder": (True, "spine01", "upperarm02", DOWN, ("upperarm01", "lowerarm01"), FWD, OUT, (FWD, IN)),
    "trunk": (False, "root", "spine01", UP, None, FWD, OUT, (FWD, OUT)),
    "neck": (False, "spine01", "head", UP, None, FWD, OUT, (FWD, OUT)),
}
JOINTS = tuple(DEFS)
COMPONENTS = ("flexion", "abduction", "rotation")


def _unit(v):
    v = np.asarray(v, dtype=np.float64)
    return v / np.linalg.norm(v)


def _across(d, u):
    return d - u * np.dot(d, u)


def _arc(a, b):
    c = np.cross(a, b)
    return quat.normalize(np.array([c[0], c[1], c[2], 1.0 + np.dot(a, b)]))


def _mirror(v, side):
    v = np.asarray(v, dtype=np.float64)
    return v * np.array([-1.0, 1.0, 1.0]) if side == "R" else v


def _bone(skel: Skeleton, stem: str, side: str) -> int:
    return skel.index[stem] if stem in skel.index else skel.index[f"{stem}.{side}"]


def _rest_dir(skel, a, b, side):
    return _unit(skel.heads[_bone(skel, b, side)] - skel.heads[_bone(skel, a, side)])


class Joint:
    def __init__(self, skel: Skeleton, joint: str, side: str = "L"):
        sided, prox, dist, zero, rest, flex_to, abd_to, twist = DEFS[joint]
        side = side if sided else "L"
        u = _mirror(zero, side) if isinstance(zero[0], float) else _rest_dir(skel, *zero, side)
        self.zero_turn = _arc(u, _rest_dir(skel, *rest, side)) if rest else quat.IDENTITY.copy()
        self.u = u
        self.flex_axis = _unit(np.cross(u, _across(_mirror(flex_to, side), u)))
        self.abd_axis = _unit(np.cross(u, _across(_mirror(abd_to, side), u)))
        tw = np.cross(_across(_mirror(twist[0], side), u), _across(_mirror(twist[1], side), u))
        self.twist_sign = -1.0 if np.dot(tw, u) < 0.0 else 1.0
        self.proximal = _bone(skel, prox, side)
        self.distal = _bone(skel, dist, side)

    def angles(self, skel: Skeleton, rotations: np.ndarray) -> np.ndarray:
        """(N, B, 4) local rotations -> (N, 3) [flexion, abduction, rotation] degrees."""
        rp = world(skel, rotations, self.proximal)
        rd = world(skel, rotations, self.distal)
        t = quat.mul(quat.mul(quat.conj(rp), rd), self.zero_turn)
        along = t[:, :3] @ self.u
        w = np.concatenate([self.u[None, :] * along[:, None], t[:, 3:4]], axis=1)
        n = np.linalg.norm(w, axis=1, keepdims=True)
        w = np.where(n > 0.0, w / np.maximum(n, 1e-300), quat.IDENTITY)
        swing = quat.mul(t, quat.conj(w))
        swing = np.where(swing[:, 3:4] < 0.0, -swing, swing)
        s = 2.0 * quat.log(swing)
        twist = 2.0 * np.arctan2(w[:, :3] @ self.u, w[:, 3])
        twist = np.where(twist > np.pi, twist - 2 * np.pi, twist)
        twist = np.where(twist <= -np.pi, twist + 2 * np.pi, twist)
        return np.degrees(np.stack([s @ self.flex_axis, s @ self.abd_axis, twist * self.twist_sign], axis=1))


def world(skel: Skeleton, rotations: np.ndarray, i: int) -> np.ndarray:
    """(N, 4) world rotation of bone ``i``: its parents' local rotations, root first."""
    chain = []
    while i >= 0:
        chain.append(i)
        i = int(skel.parents[i])
    r = np.broadcast_to(quat.IDENTITY, (len(rotations), 4)).astype(np.float64)
    for b in reversed(chain):
        r = quat.mul(r, quat.normalize(rotations[:, b].astype(np.float64)))
    return r
