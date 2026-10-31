"""A stand-in for engine.py (no GPU, no numpy): holds the last context pose's rotations and moves
the root in a straight line to the first target's, so the helper's protocol can be tested anywhere.
`helper.py` loads it through `AC_POSEFILL_ENGINE=<this file>`; the tests also call it directly.

Requests whose first target root is [9, 9, 9] get a non-finite result, and [8, 8, 8] a result of
the wrong shape: the helper must refuse both."""

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
