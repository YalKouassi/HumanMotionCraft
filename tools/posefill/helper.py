"""The app's pose-to-pose helper: JSON lines on stdin and stdout, one request a line.

    {"id": 1, "op": "info"}                                   versions, GPU, whether the plan is built
    {"id": 2, "op": "load"}                                   load the model now (else on first use)
    {"id": 3, "op": "fill", "rig": "<default_rig.json>", "frames": 31, "fps": 30,
     "context": [{"rotations": [[x,y,z,w], ...], "root": [x,y,z]}, ... up to 4, the last on key A],
     "targets": [{...}, ... up to 4, the first on key B]}

Replies carry the request's id: {"id", "progress": "..."} while it works (the first load's plan
build reports its phases this way), then {"id", "result": {...}} or {"id", "error": "..."}. A fill
result is {"rotations": frames x bones x 4, "roots": frames x 3, "notes": [...], "model_frames",
"tokens", "seconds", "stages_ms", "gpu", "contacts": frames x 4}, in the rig's pose space (root
offsets in hip heights), its first frame on key A and its last on key B. Everything the model
prints goes to stderr, so stdout carries only replies. The process ends when stdin closes.

`AC_POSEFILL_ENGINE=<file.py>` loads another engine module (the tests' fake) in place of engine.py.
"""

from __future__ import annotations

import importlib.util
import json
import math
import os
import pathlib
import sys
import traceback

# run isolated (python -I): the install's own folder is the one place modules come from
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

PROTOCOL = 1
OPS = ("info", "load", "fill")


def engine_module():
    path = os.environ.get("AC_POSEFILL_ENGINE")
    if not path:
        import engine

        return engine
    spec = importlib.util.spec_from_file_location("posefill_engine", path)
    if spec is None or spec.loader is None:
        raise ValueError(f"AC_POSEFILL_ENGINE={path!r} is not a Python file")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def pose(p, bones: int, what: str):
    """A pose from the request: ``bones`` finite rotations [x, y, z, w] and a finite root [x, y, z]."""
    if not isinstance(p, dict):
        raise ValueError(f"{what}: a pose is an object with rotations and root")
    rot, root = p.get("rotations"), p.get("root")
    ok = (isinstance(rot, list) and len(rot) == bones
          and all(isinstance(q, list) and len(q) == 4 and all(isinstance(x, (int, float)) and math.isfinite(x) for x in q) for q in rot)
          and isinstance(root, list) and len(root) == 3 and all(isinstance(x, (int, float)) and math.isfinite(x) for x in root))
    if not ok:
        raise ValueError(f"{what}: a pose needs {bones} finite rotations [x, y, z, w] and a finite root [x, y, z]")
    return [[float(x) for x in q] for q in rot], [float(x) for x in root]


def poses(items, bones: int, what: str, most: int) -> list:
    if not isinstance(items, list) or not 1 <= len(items) <= most:
        raise ValueError(f"{what}: 1 to {most} poses")
    return [pose(p, bones, f"{what}[{i}]") for i, p in enumerate(items)]


def check_result(res: dict, frames: int, bones: int) -> dict:
    """The engine's fill, refused unless it is ``frames`` x ``bones`` x 4 finite rotations and ``frames`` x 3 finite roots."""
    rots, roots = res.get("rotations"), res.get("roots")
    rows = _rows(rots)
    if rows is None or len(rows) != frames or any(len(r) != bones or any(len(q) != 4 for q in r) for r in rows):
        raise ValueError(f"the engine returned rotations of the wrong shape (wanted {frames} x {bones} x 4)")
    roots_rows = _rows(roots)
    if roots_rows is None or len(roots_rows) != frames or any(len(r) != 3 for r in roots_rows):
        raise ValueError(f"the engine returned roots of the wrong shape (wanted {frames} x 3)")
    if not all(math.isfinite(x) for r in rows for q in r for x in q) or not all(math.isfinite(x) for r in roots_rows for x in r):
        raise ValueError("the engine returned non-finite values")
    out = dict(res)
    out["rotations"] = [[[round(float(x), 7) for x in q] for q in r] for r in rows]
    out["roots"] = [[round(float(x), 7) for x in r] for r in roots_rows]
    out["notes"] = [str(n) for n in res.get("notes", [])]
    if "contacts" in out:
        out["contacts"] = _rows(out["contacts"]) or []
        out["contacts"] = [[round(float(x), 3) for x in r] for r in out["contacts"]]
    return out


def _rows(x):
    """A nested list from a list or an array (``tolist``); None for anything else."""
    if x is None:
        return None
    if hasattr(x, "tolist"):
        x = x.tolist()
    return x if isinstance(x, list) else None


def serve(lines, reply, make_engine, describe) -> None:
    """Answer each request line through ``reply``; ``make_engine(say)`` loads the engine on first
    use and ``describe()`` says what `info` reports before it is loaded."""
    eng = None
    for line in lines:
        line = line.strip()
        if not line:
            continue
        rid = None
        try:
            req = json.loads(line)
            if not isinstance(req, dict):
                raise ValueError("a request is a JSON object")
            rid = req.get("id")
            op = req.get("op")

            def say(msg: str, rid=rid) -> None:
                reply({"id": rid, "progress": str(msg)})

            if op == "info":
                info = dict(describe())
                info.update({"protocol": PROTOCOL, "loaded": eng is not None, "ops": list(OPS)})
                if eng is not None:
                    info.update(eng.info())
                reply({"id": rid, "result": info})
                continue
            if op not in OPS:
                raise ValueError(f"unknown op {op!r} ({', '.join(OPS)})")
            if eng is None:
                eng = make_engine(say)
            if op == "load":
                reply({"id": rid, "result": {"loaded": True, **eng.info()}})
                continue
            frames = req.get("frames")
            if not isinstance(frames, int) or isinstance(frames, bool) or frames < 2:
                raise ValueError("frames: a whole number of at least 2 (the first frame on key A, the last on key B)")
            fps = req.get("fps", 30.0)
            if not isinstance(fps, (int, float)) or not math.isfinite(fps) or not 1.0 <= fps <= 1000.0:
                raise ValueError("fps: a number from 1 to 1000")
            rig = req.get("rig")
            if not isinstance(rig, str) or not rig:
                raise ValueError("rig: the path of the rig file")
            skel = eng.rig(rig)
            bones = len(skel.names)
            context = poses(req.get("context"), bones, "context", 4)
            targets = poses(req.get("targets"), bones, "targets", 4)
            res = eng.fill(context, targets, frames, float(fps), rig, say=say)
            reply({"id": rid, "result": check_result(res, frames, bones)})
        except Exception as e:  # every failure is a reply; the helper keeps serving
            traceback.print_exc(file=sys.stderr)
            reply({"id": rid, "error": f"{type(e).__name__}: {e}"})


def main() -> None:
    # stdout is the reply channel: keep a private copy of it and send everything else to stderr
    out = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", buffering=1)
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    sys.stdout = sys.stderr

    def reply(msg: dict) -> None:
        out.write(json.dumps(msg, separators=(",", ":")) + "\n")
        out.flush()

    mod = engine_module()
    serve(sys.stdin, reply, lambda say: mod.Engine(say=say), mod.describe)


if __name__ == "__main__":
    main()
