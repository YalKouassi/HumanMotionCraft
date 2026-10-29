"""Download the published retargeted sets into ``training/data/rig``, checked file by file.

    python -m ac_train.fetch                 # every set in training/rig-data.json
    python -m ac_train.fetch cmu 100style    # some of them
    python -m ac_train.fetch --from DIR      # from a folder (a copy of the release) instead of GitHub

Reads the manifest ``training/rig-data.json`` (written by ``python -m ac_train.publish``), downloads
each file from the GitHub release it names, on this checkout's ``origin`` repository (plain HTTPS; a
private repository's release goes through the GitHub CLI, ``gh auth login`` once), and keeps it
only when its SHA-256 matches. A file already here
and matching is left alone; a different one (built here) is never overwritten without ``--force``.
Then the review app (``ac-review``) and training can use the sets.

A set the release doesn't have (one you add, or ``kimodo`` when it is newer than the release) is
built here: ``python -m ac_train.datasets <set>`` then ``python -m ac_train.quality <set>``.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import urllib.request

from . import REPO, stamp
from .datasets import REGISTRY, RIG
from .publish import FORMAT, MANIFEST, VERSION, sha256


def load_manifest(path: pathlib.Path = MANIFEST) -> dict:
    if not path.exists():
        raise SystemExit(f"nothing has been published yet ({path} is missing): build the sets "
                         "(training/README.md) or ask for python -m ac_train.publish")
    m = json.loads(path.read_text(encoding="utf-8"))
    if m.get("format") != FORMAT or m.get("version") != VERSION:
        raise SystemExit(f"{path}: not a {FORMAT} v{VERSION} manifest")
    return m


def origin_repo() -> str:
    """``owner/name`` of this checkout's GitHub ``origin``."""
    try:
        url = subprocess.run(["git", "remote", "get-url", "origin"], cwd=REPO, capture_output=True, text=True,
                             check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as e:
        raise SystemExit(f"can't read this checkout's origin ({e}): use --from DIR") from e
    m = re.search(r"github\.com[:/]([^/]+/[^/]+?)(?:\.git)?/?$", url)
    if not m:
        raise SystemExit(f"origin ({url}) is not a GitHub repository: use --from DIR")
    return m.group(1)


def _download(tag: str, name: str, into: pathlib.Path, source: pathlib.Path | None) -> pathlib.Path:
    if source is not None:
        src = source / name
        if not src.exists():
            raise SystemExit(f"{src} is missing")
        shutil.copyfile(src, into / name)
    else:
        repo = origin_repo()
        url = f"https://github.com/{repo}/releases/download/{tag}/{name}"
        try:
            with urllib.request.urlopen(url, timeout=120) as r, open(into / name, "wb") as fh:
                shutil.copyfileobj(r, fh, 1 << 20)
        except OSError as e:
            # a private repository's release needs a login: the GitHub CLI has one
            if shutil.which("gh") is None:
                raise SystemExit(f"downloading {url} failed ({e}); a private repository needs the GitHub CLI "
                                 "(gh auth login), or use --from DIR") from e
            done = subprocess.run(["gh", "release", "download", tag, "--repo", repo, "--pattern", name,
                                   "--dir", str(into), "--clobber"], check=False)
            if done.returncode != 0:
                raise SystemExit(f"downloading {name} from release {tag} failed ({e}; gh too: gh auth login? "
                                 "access to the repo?)") from e
    return into / name


def fetch(sets: list[str] | None = None, source: pathlib.Path | None = None, force: bool = False,
          rig: pathlib.Path = RIG, manifest: pathlib.Path = MANIFEST) -> list[str]:
    """Bring the manifest's files into ``rig``; returns the files written."""
    m = load_manifest(manifest)
    if m["rig_crc32"] != stamp.rig_crc32():
        raise SystemExit(f"release {m['tag']} was built for another rig (assets/rig/default_rig.json changed since): "
                         "build the sets here instead (training/README.md)")
    files = [f for f in m["files"] if not sets or f["set"] in sets]
    unknown = sorted(set(sets or []) - {f["set"] for f in m["files"]})
    for n in unknown:
        why = REGISTRY[n].licence if n in REGISTRY else "no such set"
        print(f"  {n}: not in release {m['tag']} ({why}): build it here (python -m ac_train.datasets {n})")
    rig.mkdir(parents=True, exist_ok=True)
    written = []
    with tempfile.TemporaryDirectory(dir=rig) as tmp:
        for f in files:
            dest = rig / f["file"]
            if dest.exists():
                if sha256(dest) == f["sha256"]:
                    print(f"  {f['file']}: up to date")
                    continue
                if not force:
                    print(f"  {f['file']}: kept (yours differs from the release; --force replaces it)")
                    continue
            got = _download(m["tag"], f["file"], pathlib.Path(tmp), source)
            if sha256(got) != f["sha256"]:
                raise SystemExit(f"{f['file']}: the download doesn't match the manifest (corrupt, or the release "
                                 "was changed): nothing of it was kept")
            os.replace(got, dest)
            written.append(f["file"])
            print(f"  {f['file']}: {f['bytes'] / 1e6:.1f} MB ({f['licence']})")
    missing = [n for n in REGISTRY if n not in {f["set"] for f in m["files"]} and not (rig / f"{n}.npz").exists()]
    if missing and not sets:
        print(f"not published, build them here if you want them: {', '.join(missing)} "
              "(python -m ac_train.datasets <set>, then python -m ac_train.quality <set>)")
    return written


def main(argv=None):
    ap = argparse.ArgumentParser(description="Download the published retargeted sets (checked against the manifest)")
    ap.add_argument("sets", nargs="*", help="sets (default: all in the manifest)")
    ap.add_argument("--from", dest="source", type=pathlib.Path, help="a folder holding the release's files")
    ap.add_argument("--force", action="store_true", help="replace local sets that differ from the release")
    args = ap.parse_args(argv)
    fetch(args.sets or None, args.source, args.force)


if __name__ == "__main__":
    main()
