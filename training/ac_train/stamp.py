"""What a retargeted set was built from, written into its npz and checked wherever it is read.

A set is only right for the rig it was retargeted onto: the same npz read against another rig
loads without complaint and is wrong. :func:`build_stamp` gives what ``datasets.build`` writes:

- ``rig_crc32``: CRC-32 of ``assets/rig/default_rig.json`` (line endings made ``\\n``, so a
  Windows checkout and a Linux one agree). The review app (``ac-review``) checks it too.
- ``code``: a hash of the pipeline code that shapes the output (the readers, the retarget, the
  levelling); a different one only means "built by other code", which is fine unless the
  retarget changed.
- ``commit``: the commit it was built at (``+changes`` when ``ac_train`` had uncommitted edits).

:func:`check` stops on a set built for another rig and says what else differs.
"""

from __future__ import annotations

import hashlib
import inspect
import pathlib
import subprocess
import zlib

from . import ASSETS, REPO

RIG_JSON = ASSETS / "rig" / "default_rig.json"
HERE = pathlib.Path(__file__).resolve().parent
# the modules whose code decides what a set's frames are
SOURCES = ("bvh.py", "clip.py", "datasets.py", "glb.py", "gltf_motion.py", "quat.py", "retarget.py", "rig.py")


def _text(b: bytes) -> bytes:
    return b.replace(b"\r\n", b"\n")


def rig_crc32(path: pathlib.Path = RIG_JSON) -> int:
    return zlib.crc32(_text(path.read_bytes()))


def code_hash() -> str:
    from . import quality

    h = hashlib.sha256()
    for name in SOURCES:
        h.update(_text((HERE / name).read_bytes()))
    for f in (quality._stand_lift, quality.flat_heights, quality.feet, quality.lowest, quality.floor_of, quality.level):
        h.update(inspect.getsource(f).replace("\r\n", "\n").encode())
    return h.hexdigest()[:16]


def commit() -> str:
    """The checkout's commit, ``+changes`` when ``ac_train`` has uncommitted edits; ``unknown`` with no git."""
    try:
        head = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO, capture_output=True, text=True, check=True)
        dirty = subprocess.run(["git", "status", "--porcelain", "--", "training/ac_train"], cwd=REPO,
                               capture_output=True, text=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return "unknown"
    return head.stdout.strip() + ("+changes" if dirty.stdout.strip() else "")


def build_stamp() -> dict:
    return {"rig_crc32": rig_crc32(), "code": code_hash(), "commit": commit()}


def read(d) -> dict | None:
    """A set's stamp (an npz, or any mapping of its arrays); None when it was built before stamps."""
    if "rig_crc32" not in d:
        return None
    return {"rig_crc32": int(d["rig_crc32"]), "code": str(d["code"]), "commit": str(d["commit"])}


def check(name: str, d) -> list[str]:
    """Notes about how a set was built; SystemExit when it was built for another rig."""
    s = read(d)
    if s is None:
        return [f"{name} has no build stamp (built before stamps): rebuild it to be sure it fits the rig "
                f"(python -m ac_train.datasets {name})"]
    if s["rig_crc32"] != rig_crc32():
        raise SystemExit(f"{name} was built for another rig (assets/rig/default_rig.json has changed since, "
                         f"commit {s['commit']}): rebuild it (python -m ac_train.datasets {name}) and scan it again")
    if s["code"] != code_hash():
        return [f"{name} was built by other pipeline code (commit {s['commit']}): fine, unless the retarget "
                f"changed since; then rebuild it"]
    return []
