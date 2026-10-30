"""Get the retargeted sets ready to share as a GitHub release; nothing is uploaded.

    python -m ac_train.publish     # the sets whose licence lets a derived copy be shared

Copies each set's ``data/rig/<set>.npz`` and its scan (``<set>.quality.json``) to
``data/publish/<tag>/`` with ``NOTES.md`` (licences and credits), and writes the manifest
``training/rig-data.json`` (committed): the release tag, the rig the sets fit, and every file's
SHA-256, which ``python -m ac_train.fetch`` checks. Then it prints the ``gh release create``
command; uploading is left to a person.

Only stamped sets that fit today's rig, with a current scan, are published. A set marked
``share="never"`` (one you add whose licence forbids derived copies) is never published.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil

import numpy as np

from . import REPO, stamp
from .datasets import REGISTRY, RIG

MANIFEST = REPO / "training" / "rig-data.json"
FORMAT = "ac-rig-data"
VERSION = 1


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def shareable(name: str) -> str:
    """"" when the set may be published; otherwise why not."""
    if REGISTRY[name].share == "yes":
        return ""
    return f"{REGISTRY[name].licence}: its licence forbids sharing a derived copy; never published"


def ready(name: str, rig: pathlib.Path) -> tuple[dict, list[str]]:
    """The set's stamp, and why it can't be published (empty when it can)."""
    npz = rig / f"{name}.npz"
    if not npz.exists():
        return {}, [f"not built ({npz})"]
    d = np.load(npz)
    s = stamp.read(d)
    if s is None:
        return {}, ["no build stamp: rebuild it (python -m ac_train.datasets " + name + ")"]
    why = []
    if s["rig_crc32"] != stamp.rig_crc32():
        why.append("built for another rig: rebuild it")
    if s["commit"].endswith("+changes") or s["commit"] == "unknown":
        why.append(f"built from uncommitted code ({s['commit']}): commit, then rebuild it")
    scan = rig / f"{name}.quality.json"
    if not scan.exists():
        why.append(f"not scanned: python -m ac_train.quality {name}")
    else:
        rec = json.loads(scan.read_text(encoding="utf-8"))
        starts, n = d["starts"], len(d["roots"])
        frames = [int(e - a) for a, e in zip(starts, [*starts[1:], n])]
        if [t["frames"] for t in rec["takes"]] != frames or [t["take"] for t in rec["takes"]] != [str(t) for t in d["takes"]]:
            why.append(f"its scan is older than the set: python -m ac_train.quality {name}")
    return s, why


def publish(names: list[str], out_root: pathlib.Path, rig: pathlib.Path = RIG,
            manifest: pathlib.Path = MANIFEST, tag: str | None = None) -> dict:
    chosen, stamps = [], {}
    for n in names:
        why = shareable(n)
        if why:
            print(f"  {n}: left out ({why})")
            continue
        s, problems = ready(n, rig)
        if problems:
            raise SystemExit(f"{n} can't be published: {'; '.join(problems)}")
        chosen.append(n)
        stamps[n] = s
    if not chosen:
        raise SystemExit("nothing to publish")
    codes = {s["code"] for s in stamps.values()}
    commits = sorted({s["commit"] for s in stamps.values()})
    tag = tag or f"rig-data-{commits[-1]}"
    out = out_root / tag
    out.mkdir(parents=True, exist_ok=True)
    files = []
    for n in chosen:
        ds = REGISTRY[n]
        for src in (rig / f"{n}.npz", rig / f"{n}.quality.json"):
            dst = out / src.name
            shutil.copyfile(src, dst)
            files.append({"file": src.name, "set": n, "bytes": dst.stat().st_size, "sha256": sha256(dst),
                          "licence": ds.licence, "commercial": ds.commercial, "source": ds.url})
            print(f"  {src.name}: {dst.stat().st_size / 1e6:.1f} MB")
    record = {"format": FORMAT, "version": VERSION, "tag": tag, "rig_crc32": stamp.rig_crc32(),
              "code": sorted(codes), "commits": commits, "files": files}
    manifest.write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8", newline="\n")
    notes = ["Retargeted motion sets (training/data/rig) for the review app and training.",
             "Fetch with `python -m ac_train.fetch`; it checks every file against training/rig-data.json.", "",
             "| set | licence | commercial use | source |", "|---|---|---|---|"]
    for n in chosen:
        ds = REGISTRY[n]
        notes.append(f"| {ds.title} | {ds.licence} | {'yes' if ds.commercial else 'no'} | {ds.url} |")
    left = [n for n in REGISTRY if n not in chosen]
    if left:
        notes += ["", f"Not here: {', '.join(left)} (licence). Download and build them yourself: training/README.md."]
    (out / "NOTES.md").write_text("\n".join(notes) + "\n", encoding="utf-8", newline="\n")
    paths = " ".join(f'"{out / f["file"]}"' for f in files)
    print(f"\nmanifest -> {manifest} (commit it)\nfiles -> {out}\n\nto upload (a person decides):\n"
          f'  gh release create {tag} {paths} --title "{tag}" --notes-file "{out / "NOTES.md"}" --prerelease')
    return record


def main(argv=None):
    ap = argparse.ArgumentParser(description="Get the retargeted sets ready to share (nothing is uploaded)")
    ap.add_argument("sets", nargs="*", help="sets (default: every built one its licence allows)")
    ap.add_argument("--tag", help="release tag (default rig-data-<commit>)")
    args = ap.parse_args(argv)
    names = args.sets or [n for n in REGISTRY if (RIG / f"{n}.npz").exists()]
    publish(names, RIG.parent / "publish", tag=args.tag)


if __name__ == "__main__":
    main()
