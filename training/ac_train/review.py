"""What training may use of each take: the quality scan's verdict, overridden by the review.

The scan (``python -m ac_train.quality``) writes ``data/rig/<set>.quality.json``: per take the
frame ranges to cut and whether the whole take is rejected. The review app (``ac-review``) writes
``training/review/decisions.json``: per ``<set>/<take>`` a person's (or an agent's) verdict, which
wins over the scan's:

- ``ok``: the whole take is good (the scan was wrong);
- ``cut``: the take without the decision's ``cuts`` (frame ranges, first and last included);
- ``reject``: the whole take is left out.

A decision made on a different length of take (``frames``: the set was rebuilt with another trim)
would cut the wrong frames: it is set aside, the scan applies, and the plan says so. A scan that no
longer matches its set (rebuilt and not scanned again) is an error, not a guess.

:func:`plan` gives each take of a set its :class:`Take`; :func:`starts` the window starts that keep
clear of every cut. Training (``train.py``) and the library (``library.py``) both read it.

``python -m ac_train.review [set ...]`` prints the plan without training: per set the takes used and
left out, the hours that go, and anything set aside.
"""

from __future__ import annotations

import argparse
import json
import pathlib
from dataclasses import dataclass, field

import numpy as np

from . import REPO, stamp
from .quality import RIG

DECISIONS = REPO / "training" / "review" / "decisions.json"
VERSION = 1
STATUSES = ("ok", "cut", "reject")


@dataclass
class Take:
    name: str
    frames: int
    #: why the whole take is left out ("" when it is used)
    drop: str = ""
    #: [(first, last)] frames not to train on, first and last included
    cuts: list[tuple[int, int]] = field(default_factory=list)
    #: who said so: "decision", "scan" or "none" (no scan was asked for)
    source: str = "scan"
    #: the review's motion type ("" when the take name's stands)
    kind: str = ""
    tags: list[str] = field(default_factory=list)

    def bad(self) -> np.ndarray:
        """(frames,) True on the frames cut."""
        m = np.zeros(self.frames, bool)
        for a, b in self.cuts:
            m[max(a, 0) : min(b, self.frames - 1) + 1] = True
        return m


@dataclass
class Plan:
    set: str
    takes: list[Take]
    #: what the plan had to set aside or work around, to be printed
    notes: list[str] = field(default_factory=list)

    def summary(self) -> str:
        used = [t for t in self.takes if not t.drop]
        dropped = len(self.takes) - len(used)
        frames = sum(t.frames for t in self.takes)
        cut = sum(int(t.bad().sum()) for t in used) + sum(t.frames for t in self.takes if t.drop)
        decided = sum(t.source == "decision" for t in self.takes)
        return (f"{self.set}: {len(used)} of {len(self.takes)} takes used ({dropped} left out), "
                f"{cut / 30 / 3600:.2f} of {frames / 30 / 3600:.2f} h out; {decided} reviewed")


def load_decisions(path: pathlib.Path = DECISIONS) -> dict[str, dict]:
    """``<set>/<take>`` -> decision. No file is no decisions; a file that won't read is an error."""
    if not path.exists():
        return {}
    try:
        d = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise SystemExit(f"{path}: {e} (fix it, or check it out again)") from e
    if not isinstance(d, dict) or not str(d.get("format", "")).endswith(".review"):
        raise SystemExit(f"{path}: not a review decisions file")
    if d.get("version") != VERSION:
        raise SystemExit(f"{path}: version {d.get('version')} (this pipeline reads {VERSION})")
    takes = d.get("takes", {})
    for k, v in takes.items():
        if not isinstance(v, dict) or v.get("status") not in STATUSES:
            raise SystemExit(f"{path}: {k}: status must be one of {', '.join(STATUSES)}")
    return takes


def load_scan(set_name: str, rig: pathlib.Path | None = None) -> dict | None:
    """The set's scan record, or None when it has not been scanned."""
    p = (rig or RIG) / f"{set_name}.quality.json"
    if not p.exists():
        return None
    return json.loads(p.read_text(encoding="utf-8"))


def plan(set_name: str, takes, frames, decisions: dict[str, dict] | None = None,
         scan: dict | None = None, scanned: bool = True) -> Plan:
    """Each take's verdict. ``takes`` and ``frames`` are the set's take names and lengths, in order.
    ``scan`` is the set's scan record (``scanned=False``: there is none and that is meant)."""
    takes = [str(t) for t in takes]
    frames = [int(f) for f in frames]
    decisions = decisions or {}
    notes: list[str] = []
    by_take: dict[str, dict] = {}
    if scanned:
        if scan is None:
            raise SystemExit(f"{set_name} has not been scanned: run python -m ac_train.quality {set_name}"
                             " (or train with --unscanned to use every frame)")
        by_take = {t["take"]: t for t in scan.get("takes", [])}
        stale = [n for n, f in zip(takes, frames) if n not in by_take or by_take[n]["frames"] != f]
        if stale or len(by_take) != len(takes):
            raise SystemExit(f"{set_name}'s scan doesn't match the set ({len(stale)} takes differ, e.g. "
                             f"{(stale or ['-'])[0]}): the set was rebuilt; run python -m ac_train.quality {set_name}")
    out = []
    moved = []
    for name, n in zip(takes, frames):
        s = by_take.get(name)
        t = Take(name, n, source="scan" if scanned else "none")
        if s is not None:
            t.cuts = [(int(a), int(b)) for a, b, _ in s["cuts"]]
            t.drop = f"scan: {s['reject']}" if s["reject"] else ""
        d = decisions.get(f"{set_name}/{name}")
        if d is not None and d.get("frames", n) != n:
            moved.append(f"{set_name}/{name} ({d.get('frames')} frames then, {n} now)")
            d = None
        if d is not None:
            t.source = "decision"
            t.kind = str(d.get("type") or "")
            t.tags = [str(x) for x in d.get("tags", [])]
            who = d.get("by") or "someone"
            if d["status"] == "ok":
                t.cuts, t.drop = [], ""
            elif d["status"] == "cut":
                t.cuts = [(int(a), int(b)) for a, b in d.get("cuts", [])]
                t.drop = ""
            else:
                t.cuts, t.drop = [], f"rejected by {who}" + (f": {d['note']}" if d.get("note") else "")
        out.append(t)
    if moved:
        notes.append(f"{len(moved)} decisions were made on a different length of take and are set aside "
                     f"(the scan applies; review them again): {', '.join(moved[:3])}{' ...' if len(moved) > 3 else ''}")
    unknown = sorted(k for k in decisions if k.split("/", 1)[0] == set_name and k.split("/", 1)[-1] not in set(takes))
    if unknown:
        notes.append(f"{len(unknown)} decisions name takes {set_name} doesn't have (renamed or no longer built): "
                     f"{', '.join(unknown[:3])}{' ...' if len(unknown) > 3 else ''}")
    return Plan(set_name, out, notes)


def starts(take: Take, length: int) -> np.ndarray:
    """The first frames of every ``length``-frame window of the take that touches no cut."""
    if take.drop or take.frames < length:
        return np.zeros(0, np.int64)
    bad = np.concatenate([[0], np.cumsum(take.bad())])
    s = np.arange(take.frames - length + 1)
    return s[bad[s + length] - bad[s] == 0]


def plan_set(set_name: str, rots_len: int, starts_arr, take_names, decisions=None, scanned=True,
             rig: pathlib.Path | None = None, npz=None) -> Plan:
    """:func:`plan` for a set as stored in its npz (``starts``, ``takes``, total frames). With the
    ``npz``, its build stamp is checked too (:func:`ac_train.stamp.check`)."""
    built = stamp.check(set_name, npz) if npz is not None else []
    ends = [*list(starts_arr[1:]), rots_len]
    frames = [int(e - s) for s, e in zip(starts_arr, ends)]
    scan = load_scan(set_name, rig) if scanned else None
    p = plan(set_name, take_names, frames, decisions, scan, scanned)
    p.notes[:0] = built
    return p


def main(argv=None):
    ap = argparse.ArgumentParser(description="What training would use of each set (nothing is trained)")
    ap.add_argument("sets", nargs="*", help="sets (default: every data/rig/*.npz)")
    ap.add_argument("--decisions", default=str(DECISIONS))
    ap.add_argument("--dropped", action="store_true", help="list the takes left out and why")
    args = ap.parse_args(argv)
    decisions = load_decisions(pathlib.Path(args.decisions))
    print(f"{len(decisions)} decisions in {args.decisions}")
    for n in args.sets or sorted(p.stem for p in RIG.glob("*.npz")):
        d = np.load(RIG / f"{n}.npz")
        p = plan_set(n, len(d["roots"]), d["starts"], d["takes"], decisions, npz=d)
        print(p.summary())
        for note in p.notes:
            print(f"  {note}")
        if args.dropped:
            for t in p.takes:
                if t.drop:
                    print(f"  - {t.name} ({t.frames} frames): {t.drop}")


if __name__ == "__main__":
    main()
