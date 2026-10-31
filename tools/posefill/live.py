"""Live motion control, the helper's half (roadmap 158000): a session that keeps the poses shown so
far in the model's own layout, blends our target set by speed, places the blended target ahead of
the character along its desired velocity, asks the model for the next stretch, and brings it back
onto the rig. The loop that decides *when* to ask (transition points, input changes) lives on the
app's side; this module only answers `live_start` and `live_step`.

The same pattern the upstream docs describe for a character runtime (record every pose you show,
build each context from the latest four at 30 Hz, place targets ahead along the desired velocity,
move only the root), written here for our rig and our own target poses.

Target sets come from `python -m ac_train.posetargets` (`assets/posefill-targets/*.json`): per
gait, four rig poses a frame apart with the motion along -Y and the first pose's hips over the
origin, and the gait's speed. Placement is done in rig space (a yaw about the vertical and a move
of the root alone), then each pose crosses to the model through the engine's `from_rig`.
"""

from __future__ import annotations

import math
import time

import numpy as np
from ac_rig import quat as Q
from ac_rig.retarget import retarget

FPS = 30.0
CONTEXT = 4
POSES = 4  # poses in a target token
HISTORY = 64  # shown poses kept (the context needs 4)
#: the desired velocity is scaled by this before the target's speed and placement are set (the
#: generated motion tends to run a little fast otherwise)
VELOCITY_SCALE = 0.97
#: seconds of travel the targets sit ahead of the character
POSITION_SCALE = 1.0
#: seconds of travel a stopping target sits ahead of the character, at its actual speed
STOP_SECONDS = 0.5
#: frames before the end of a generation at which the next one starts (the target is a guide, so
#: the character lines up towards it and moves on before it arrives)
TRANSITION_EARLY = 10
#: token counts the model may choose from (8 to 14 tokens: 26 to 50 frames)
TOKENS = tuple(range(8, 15))
STILL = 0.05  # m/s: a target group that moves less than this keeps its own travel


def yaw_quat(angle: float) -> np.ndarray:
    return np.array([0.0, 0.0, math.sin(angle / 2), math.cos(angle / 2)])


class Targets:
    """A target set as read from its file: gaits by speed, each four rig poses."""

    def __init__(self, doc: dict, bones: int):
        if not isinstance(doc, dict) or not str(doc.get("format", "")).endswith(".posetargets"):
            raise ValueError("targets: not a target pose set (format humanmotioncraft.posetargets)")
        if doc.get("version") != 1:
            raise ValueError(f"targets: version {doc.get('version')!r}; this helper reads version 1")
        if doc.get("bones") != bones:
            raise ValueError(f"targets: made for a {doc.get('bones')}-bone rig; this rig has {bones}")
        self.gaits: list[tuple[str, float, np.ndarray, np.ndarray]] = []
        for t in doc.get("targets", []):
            name, speed, poses = t.get("name"), t.get("speed"), t.get("poses")
            if not isinstance(name, str) or not isinstance(speed, (int, float)) or not isinstance(poses, list) or len(poses) != POSES:
                raise ValueError("targets: each target is {name, speed, poses: [4 poses]}")
            rots = np.array([p["rotations"] for p in poses], dtype=np.float64)
            roots = np.array([p["root"] for p in poses], dtype=np.float64)
            if rots.shape != (POSES, bones, 4) or roots.shape != (POSES, 3) or not (np.isfinite(rots).all() and np.isfinite(roots).all()):
                raise ValueError(f"targets: {name}: poses are {bones} finite rotations and a finite root each")
            self.gaits.append((name, float(speed), rots, roots))
        if not self.gaits:
            raise ValueError("targets: no targets")
        self.gaits.sort(key=lambda g: g[1])
        self.names = [g[0] for g in self.gaits]
        self.max_speed = self.gaits[-1][1]

    def blend(self, speed: float, style: str | None) -> tuple[np.ndarray, np.ndarray, str]:
        """The four poses for ``speed``: a named gait as it is, else the two gaits around the speed
        blended (rotations slerped, roots lerped). Returns (rotations (4, B, 4), roots (4, 3), what)."""
        if style:
            for name, _, rots, roots in self.gaits:
                if name == style:
                    return rots.copy(), roots.copy(), name
            raise ValueError(f"style {style!r} is not in the target set ({', '.join(self.names)})")
        lo = self.gaits[0]
        for g in self.gaits:
            if g[1] <= speed:
                lo = g
        hi = next((g for g in self.gaits if g[1] > speed), None)
        if hi is None or hi[1] - lo[1] < 1e-9:
            return lo[2].copy(), lo[3].copy(), lo[0]
        t = (speed - lo[1]) / (hi[1] - lo[1])
        rots = np.stack([Q.slerp(a, b, np.full(len(a), t)) for a, b in zip(lo[2], hi[2])])
        roots = lo[3] * (1 - t) + hi[3] * t
        return rots, roots, f"{lo[0]}-{hi[0]} {t:.2f}"


def hips(skel, roots: np.ndarray) -> np.ndarray:
    """Hips positions (F, 3) in rig metres from root offsets (F, 3)."""
    return skel.heads[skel.index["root"]][None] + np.asarray(roots, dtype=np.float64) * skel.hip_height


def offsets(skel, P: np.ndarray) -> np.ndarray:
    return (P - skel.heads[skel.index["root"]][None]) / skel.hip_height


def place(skel, rots: np.ndarray, roots: np.ndarray, speed: float, at: np.ndarray, facing: np.ndarray):
    """The target group (motion along -Y from the origin) with its ground travel scaled to ``speed``
    (m/s), turned to face ``facing`` (x, y) and moved so its first pose's hips stand over ``at``
    (x, y, rig metres). Only the root moves."""
    P = hips(skel, roots)
    rel = P[:, :2] - P[0, :2]
    travel = float(np.linalg.norm(P[-1, :2] - P[0, :2])) * FPS / (POSES - 1)
    if travel > STILL:
        rel = rel * (speed / travel)
    angle = math.atan2(float(facing[1]), float(facing[0])) - math.atan2(-1.0, 0.0)
    c, s = math.cos(angle), math.sin(angle)
    out = P.copy()
    out[:, 0] = c * rel[:, 0] - s * rel[:, 1] + at[0]
    out[:, 1] = s * rel[:, 0] + c * rel[:, 1] + at[1]
    q = yaw_quat(angle)
    rots = np.array(rots, dtype=np.float64)
    rots[:, 0] = Q.normalize(Q.mul(np.broadcast_to(q, rots[:, 0].shape), rots[:, 0]))
    return rots, offsets(skel, out)


class Session:
    """One live take on the helper's side."""

    def __init__(self, engine, rig: str, targets: dict, start: tuple):
        self.eng = engine
        body = engine.body
        self.rig = rig
        self.skel, self.table = body.rig(rig)
        self.targets = Targets(targets, len(self.skel.names))
        self.scale = body.leg / engine.leg_of(self.skel)
        rot, root = start
        first = body.from_rig(self.skel, self.table, np.asarray(rot, dtype=np.float64), np.asarray(root, dtype=np.float64), self.scale)
        self.history = [first] * CONTEXT
        self.segment: np.ndarray | None = None
        self.calls = 0

    # -- where the character is -------------------------------------------------------------------

    def _rig_xy(self, pose: np.ndarray) -> np.ndarray:
        """A model pose's hips on the ground, in rig metres (x, y)."""
        p = pose[0, :3] / self.scale  # model metres -> rig metres; model (x, y, z) is the rig's (x, -z, y)
        return np.array([p[0], -p[2]])

    def position(self) -> np.ndarray:
        return self._rig_xy(self.history[-1])

    def velocity(self) -> np.ndarray:
        """The hips' ground velocity (m/s) over the last shown poses."""
        if len(self.history) < 2:
            return np.zeros(2)
        n = min(len(self.history), CONTEXT)
        return (self._rig_xy(self.history[-1]) - self._rig_xy(self.history[-n])) * FPS / (n - 1)

    def shown(self, n: int) -> None:
        """Frames 1..n of the last segment were shown: they join the history."""
        if self.segment is None:
            if n > 0:
                raise ValueError("no segment has been made yet")
            return
        if not 0 <= n <= len(self.segment) - 1:
            raise ValueError(f"shown: 0 to {len(self.segment) - 1} frames of the last segment")
        self.history.extend(self.segment[1 : 1 + n])
        del self.history[:-HISTORY]

    # -- the step ---------------------------------------------------------------------------------

    def next(self, shown: int, velocity: np.ndarray, facing: np.ndarray, style: str | None, tokens, say=print) -> dict:
        """``shown`` frames join the history and the next stretch is made; if that fails the history
        is as it was, so the same request can be sent again without showing a frame twice."""
        before = list(self.history)
        self.shown(shown)
        try:
            return self.step(velocity, facing, style, tokens, say=say)
        except Exception:
            self.history = before
            raise

    def step(self, velocity: np.ndarray, facing: np.ndarray, style: str | None, tokens, say=print) -> dict:
        t_all = time.perf_counter()
        speed = float(np.linalg.norm(velocity))
        rots, roots, what = self.targets.blend(speed, style)
        if speed > 1e-6:
            ahead = self.position() + velocity * VELOCITY_SCALE * POSITION_SCALE
        else:
            # stopping: the stand pose sits a short way on at its actual speed
            ahead = self.position() + self.velocity() * STOP_SECONDS
        rots, roots = place(self.skel, rots, roots, speed * VELOCITY_SCALE, ahead, facing)
        t0 = time.perf_counter()
        tgt = np.stack([self.eng.body.from_rig(self.skel, self.table, r, o, self.scale) for r, o in zip(rots, roots)])
        ctx = np.stack(self.history[-CONTEXT:])
        convert_in = time.perf_counter() - t0
        poses, contacts, seconds, stages = self.eng.generate(ctx, tgt, tuple(tokens), say=say)
        self.segment = poses
        self.calls += 1
        t0 = time.perf_counter()
        src = self.eng.body.landmarks(poses)
        src = {k: v / self.scale for k, v in src.items()}
        rig_rots, rig_roots = retarget(self.skel, src)
        rig_rots, rig_roots = Q.normalize(np.asarray(rig_rots, dtype=np.float64)), np.asarray(rig_roots, dtype=np.float64)
        convert_out = time.perf_counter() - t0
        if not (np.isfinite(rig_rots).all() and np.isfinite(rig_roots).all()):
            raise ValueError("the retarget returned non-finite values")
        n = len(poses)
        return {
            "rotations": rig_rots, "roots": rig_roots, "contacts": contacts.round(3), "frames": n,
            "transition": max(1, n - 1 - TRANSITION_EARLY), "target": what,
            "position": [round(float(x), 4) for x in self.position()],
            "seconds": round(seconds, 4), "stages_ms": stages,
            "convert_ms": {"in": round(convert_in * 1000, 2), "out": round(convert_out * 1000, 2)},
            "wall_ms": round((time.perf_counter() - t_all) * 1000, 1),
            "notes": [],
        }
