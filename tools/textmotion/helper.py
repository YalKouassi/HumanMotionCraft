"""The app's text-to-motion helper: JSON lines on stdin and stdout, one request a line.

    {"id": 1, "op": "hello"}
    {"id": 2, "op": "load"}                                   load the model now (else on first use)
    {"id": 3, "op": "generate", "prompt": "...", "frames": 90, "fps": 30, "rig": "<default_rig.json>",
     "seed": 1, "start": {"rotations": [[x,y,z,w], ...], "root": [x,y,z]}, "end": {...}}

Replies carry the request's id: {"id", "progress": "..."} while it works, then {"id", "result": {...}}
or {"id", "error": "..."}. A generate result is {"rotations": frames x bones x 4, "roots": frames x 3,
"notes": [...]}, in the rig's pose space (root offsets in hip heights). Everything the model prints
goes to stderr, so stdout carries only replies. The process ends when stdin closes.
"""

from __future__ import annotations

import json
import os
import pathlib
import sys
import traceback

# run isolated (python -I): the install's own folder is the one place modules come from
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

PROTOCOL = 1

# stdout is the reply channel: keep a private copy of it and send everything else to stderr
_out = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", buffering=1)
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
sys.stdout = sys.stderr


def reply(msg: dict) -> None:
    _out.write(json.dumps(msg, separators=(",", ":")) + "\n")
    _out.flush()


def _pose(p, bones: int):
    import numpy as np

    if p is None:
        return None
    rot = np.asarray(p["rotations"], dtype=np.float64)
    root = np.asarray(p["root"], dtype=np.float64)
    if rot.shape != (bones, 4) or root.shape != (3,) or not (np.isfinite(rot).all() and np.isfinite(root).all()):
        raise ValueError(f"a pose needs {bones} finite rotations [x,y,z,w] and a finite root [x,y,z]")
    return rot, root


def main() -> None:
    gen = None
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        rid = None
        try:
            req = json.loads(line)
            rid = req.get("id")
            op = req.get("op")

            def say(msg: str, rid=rid) -> None:
                reply({"id": rid, "progress": msg})

            if op == "hello":
                import engine as kc

                reply({"id": rid, "result": {"protocol": PROTOCOL, "model": kc.MODEL, "fps": kc.FPS,
                                             "max_frames": kc.MAX_FRAMES, "loaded": gen is not None}})
                continue
            if gen is None and op in ("load", "generate"):
                import engine as kc

                gen = kc.Generator(say=say)
            if op == "load":
                reply({"id": rid, "result": {"loaded": True}})
            elif op == "generate":
                prompt = str(req.get("prompt", "")).strip()
                if not prompt:
                    raise ValueError("an empty prompt")
                frames, fps = int(req["frames"]), float(req.get("fps", 30.0))
                skel = gen.rig(str(req["rig"]))
                start, end = _pose(req.get("start"), len(skel.names)), _pose(req.get("end"), len(skel.names))
                if end is not None and start is None:
                    raise ValueError("an end pose needs a start pose")
                rots, roots, notes = gen.generate(prompt, frames, fps, str(req["rig"]), seed=req.get("seed"),
                                                  start=start, end=end, say=say)
                reply({"id": rid, "result": {"rotations": rots.round(7).tolist(), "roots": roots.round(7).tolist(),
                                             "notes": notes}})
            else:
                raise ValueError(f"unknown op {op!r} (hello, load, generate)")
        except Exception as e:  # every failure is a reply; the helper keeps serving
            traceback.print_exc(file=sys.stderr)
            reply({"id": rid, "error": f"{type(e).__name__}: {e}"})


if __name__ == "__main__":
    main()
