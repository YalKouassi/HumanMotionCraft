"""The whole training path end to end, small, to check that it still fits together.

    python -m ac_train.pipeline --quick [--keep] [--no-cargo] [--steps N]

1. retarget: the built-in CC0 loops, plus the first takes of each downloaded set (``--limit``);
2. scan them (``ac_train.quality``): training skips what the scan cuts;
3. train a few steps (CPU or GPU) on them;
4. export the weights and goldens;
5. ``cargo test -p ac-ai --test golden`` against that export (``AC_AI_MODEL``, ``AC_AI_GOLDEN``):
   the Rust port must run the new weights exactly as PyTorch did.

Everything is written to a temporary folder (``AC_TRAIN_OUT``), removed afterwards unless
``--keep``: ``training/data``, ``assets/models`` and the committed goldens are not touched. The
model it makes is useless; the point is that every step runs and the port agrees with it.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time

from . import DATA, REPO


def _run(what: str, cmd: list[str], env: dict, cwd=None) -> None:
    print(f"--- {what}: {' '.join(cmd)}", flush=True)
    t = time.time()
    done = subprocess.run(cmd, env=env, cwd=cwd)
    if done.returncode != 0:
        raise SystemExit(f"{what} failed (exit {done.returncode})")
    print(f"--- {what}: ok in {time.time() - t:.0f} s", flush=True)


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description="Retarget, train, export and check a tiny model end to end")
    ap.add_argument("--quick", action="store_true", help="the small run (the only kind there is for now)")
    ap.add_argument("--steps", type=int, default=20, help="training steps")
    ap.add_argument("--limit", type=int, default=2, help="takes per downloaded set")
    ap.add_argument("--keep", action="store_true", help="keep the temporary folder")
    ap.add_argument("--no-cargo", action="store_true", help="skip the Rust check")
    args = ap.parse_args(argv)
    if not args.quick:
        ap.error("only --quick is supported: a full run is datasets, train and export by hand (README.md)")

    from .datasets import RAW, REGISTRY

    tmp = tempfile.mkdtemp(prefix="ac-train-quick-")
    env = {**os.environ, "AC_TRAIN_OUT": tmp, "PYTHONUNBUFFERED": "1"}
    py = [sys.executable, "-m"]
    downloaded = [n for n, ds in REGISTRY.items() if ds.files and next(RAW.glob(ds.files), None) is not None]
    sets = ["builtin_loops", *downloaded]
    print(f"quick pipeline in {tmp}; sets: {', '.join(sets)}"
          + ("" if downloaded else f" (no downloads under {DATA / 'raw'}: built-in loops only)"), flush=True)
    try:
        _run("retarget", [*py, "ac_train.datasets", *sets, "--limit", str(args.limit)], env)
        _run("scan", [*py, "ac_train.quality", *sets], env)
        free = [s for s in sets if REGISTRY[s].commercial]  # what ships (train's default)
        _run("train", [*py, "ac_train.train", "--sets", ",".join(free), "--steps", str(args.steps), "--batch", "16",
                       "--minutes", "10", "--eval-windows", "32", "--tag", "quick"], env)
        export = os.path.join(tmp, "export")
        _run("export", [*py, "ac_train.export", "quick", "--out", os.path.join(export, "quick"),
                        "--golden", os.path.join(export, "golden")], env)
        if args.no_cargo:
            print("--- cargo test skipped (--no-cargo)")
        elif shutil.which("cargo") is None:
            print("--- cargo is not on PATH: the Rust check did not run (install Rust, or pass --no-cargo)")
            raise SystemExit(1)
        else:
            cargo_env = {**env, "AC_AI_MODEL": os.path.join(export, "quick"), "AC_AI_GOLDEN": os.path.join(export, "golden")}
            _run("rust port", ["cargo", "test", "-p", "ac-ai", "--test", "golden"], cargo_env, cwd=REPO)
        print("quick pipeline passed")
    finally:
        if args.keep:
            print(f"kept {tmp}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
