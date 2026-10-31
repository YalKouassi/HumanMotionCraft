"""A stand-in for engine.py (no GPU, no numpy): holds the last context pose's rotations and moves
the root in a straight line to the first target's, so the helper's protocol can be tested anywhere.
`helper.py` loads it through `AC_POSEFILL_ENGINE=<this file>`; the tests also call it directly.

Requests whose first target root is [9, 9, 9] get a non-finite result, and [8, 8, 8] a result of
the wrong shape: the helper must refuse both.

Live takes (`live_start`, `live_step`) hold the start pose's rotations and move the root by the
velocity, m/s taken as hip heights a second (1/30 a frame), 30 frames a stretch with the transition
on frame 19, so a test can work the numbers out by hand. A velocity of [7, 7] gives a stretch of
the wrong shape."""

from __future__ import annotations

import json
import pathlib

MODEL = "fake"
MIN_FRAMES, MAX_FRAMES = 18, 58


def describe() -> dict:
    return {"model": MODEL, "fps": 30.0, "min_frames": MIN_FRAMES, "max_frames": MAX_FRAMES, "context": 4,
            "max_targets": 4, "plan": "not built (the first fill builds it)", "gpu": None, "versions": {}}


class _Rig:
    def __init__(self, names):
        self.names = names


class Engine:
    def __init__(self, say=print):
        say("fake engine ready")
        self.rigs = {}

    def rig(self, path: str) -> _Rig:
        if path not in self.rigs:
            self.rigs[path] = _Rig(json.loads(pathlib.Path(path).read_text(encoding="utf-8"))["names"])
        return self.rigs[path]

    def info(self) -> dict:
        return {"gpu": "none (fake)", "load_seconds": 0.0, "plan_was_built": False}

    def live_start(self, rig: str, targets: dict, start, say=print) -> dict:
        names = [t["name"] for t in targets.get("targets", [])]
        if not names:
            raise ValueError("targets: no targets")
        self.live = {"rig": rig, "rot": start[0], "root": list(start[1]), "names": names, "segment": None}
        self.live_id = getattr(self, "live_id", 0) + 1
        say(f"fake live session {self.live_id}")
        return {"session": self.live_id, "targets": names, "max_speed": max(t["speed"] for t in targets["targets"]),
                "position": [self.live["root"][0], self.live["root"][1]]}

    def live_rig(self) -> str:
        return self.live["rig"]

    def live_step(self, session: int, shown: int, velocity, facing, style, tokens, say=print) -> dict:
        live = getattr(self, "live", None)
        if live is None or session != self.live_id:
            raise ValueError(f"no live session {session}")
        if live["segment"] is None:
            if shown > 0:
                raise ValueError("shown: no stretch has been made yet")
        else:
            if not 0 <= shown <= len(live["segment"]) - 1:
                raise ValueError(f"shown: 0 to {len(live['segment']) - 1}")
            if shown:
                live["root"] = live["segment"][shown]
        if style is not None and style not in live["names"]:
            raise ValueError(f"style {style!r} is not in the target set")
        n = 30
        step = [velocity[0] / 30.0, velocity[1] / 30.0, 0.0]
        roots = [[live["root"][c] + step[c] * f for c in range(3)] for f in range(n)]
        if list(velocity) == [7.0, 7.0]:
            return {"rotations": [live["rot"]] * (n - 1), "roots": roots, "frames": n, "transition": 19, "notes": []}
        live["segment"] = roots
        return {"rotations": [live["rot"]] * n, "roots": roots, "contacts": [[1, 1, 1, 1]] * n, "frames": n, "transition": 19,
                "target": style or "fake", "position": [live["root"][0], live["root"][1]], "seconds": 0.0, "stages_ms": {},
                "convert_ms": {"in": 0.0, "out": 0.0}, "wall_ms": 0.0, "notes": ["made by the fake engine, not the model"]}

    def fill(self, context, targets, frames: int, fps: float, rig: str, say=print):
        n = int(round((frames - 1) * 30.0 / fps)) + 1
        if not MIN_FRAMES <= n <= MAX_FRAMES:
            raise ValueError(f"{frames} frames at {fps:g} fps is {n} model frames; the model makes {MIN_FRAMES} to {MAX_FRAMES}")
        rot0, root0 = context[-1]
        _, root1 = targets[0]
        if root1 == [9.0, 9.0, 9.0]:
            return {"rotations": [rot0] * frames, "roots": [[float("nan"), 0.0, 0.0]] * frames, "notes": []}
        if root1 == [8.0, 8.0, 8.0]:
            return {"rotations": [rot0] * (frames - 1), "roots": [root0] * frames, "notes": []}
        say("fake fill")
        roots = [[a + (b - a) * f / (frames - 1) for a, b in zip(root0, root1)] for f in range(frames)]
        notes = ["made by the fake engine, not the model"]
        if len(context) < 4:
            notes.append(f"{len(context)} context pose(s) given; the first was repeated to make 4")
        return {"rotations": [rot0] * frames, "roots": roots, "notes": notes, "model_frames": n, "tokens": 9,
                "seconds": 0.0}
