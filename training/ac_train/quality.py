"""Training data quality: find the frames of a retargeted set that would teach the model wrong.

``python -m ac_train.quality [set ...]`` scans ``data/rig/<set>.npz`` take by take and writes
``data/rig/<set>.quality.json``: each take's numbers, the frame ranges to cut and why, and whether
the whole take is rejected. It prints a report per set. Nothing is removed from the npz: training
and the library skip the cuts (``review.py``, where the review app's decisions win over the scan's),
and the review app shows them.

What counts as bad (frames at 30 fps, heights against a flat floor at z = 0):

- ``glitch``: a joint or the hips off the path its neighbours draw (a one-frame spike: a marker
  swap, a solver flip). Fast motion is not a glitch: a fast arm's middle frame sits between its
  neighbours.
- ``teleport``: the hips move further in a frame than a sprinter does.
- ``sink``: a foot well below the floor.
- ``skate``: a foot flat on the floor (ankle and ball both down) sliding along it, ankle and ball
  both moving, for several frames. A foot pivoting on its ball or rolling off its heel is not
  sliding: one of the two stays put.
- ``float``: nothing near the floor for a long time (the take needs scenery the rig hasn't: a
  chair, a box, a bar).
- ``frozen``: the data stops changing (a dropout held), not a person holding still.

A take is rejected when most of it is cut, or when its floor can't be found.

:func:`level` also runs at build time (``datasets.build``): it puts each take's floor at z = 0.
"""

from __future__ import annotations

import argparse
import json

import numpy as np

from . import OUT
from .rig import Skeleton, quat_to_mat

RIG = OUT / "rig"
FPS = 30.0
FEET = ("foot.L", "foot.R", "toe2-1.L", "toe2-1.R")

THRESHOLDS = {
    # a joint this far off the midpoint of its neighbours' rotations. 20 caught clean fast motion
    # (an arm swing turning round, a foot landing: LaFAN1's flags sat at 22-25); CMU's real foot
    # twitches are 40 and up
    "glitch_deg": 30.0,
    "glitch_hips_m": 0.05,  # the hips this far off the midpoint of their neighbours'
    "teleport_m": 0.5,  # hips moving this far in one frame (15 m/s)
    "sink_m": 0.05,  # a foot this far below the floor
    "contact_m": 0.02,  # a foot this close to its flat height is on the floor
    "skate_ms": 0.5,  # ...and moving faster than this along it is sliding
    "skate_frames": 4,  # for this many frames running
    "float_m": 0.15,  # the lowest foot point this high...
    "float_s": 1.5,  # ...for this long
    "frozen_s": 1.0,  # no change at all for this long
    # frames cut either side of a glitch, teleport or skate. Training already skips every window
    # that touches a cut, so a wide pad only threw good frames away (10 made a one-frame twitch
    # 0.7 s of red)
    "pad": 2,
    "reject_share": 0.5,  # a take this much cut is rejected
    "floor_most_m": 0.2,  # a floor further off than this is not levelled: the take is rejected
}


def _stand_lift(skel: Skeleton) -> float:
    """How far the rest pose rises to stand on the floor: the root sits ``hip_height`` above the
    rest mesh's lowest point."""
    return skel.hip_height - float(skel.heads[0][2])


def flat_heights(skel: Skeleton) -> np.ndarray:
    """(4,) heights of the ankles and balls of the feet when standing flat on the floor."""
    lift = _stand_lift(skel)
    return np.array([skel.heads[skel.index[b]][2] + lift for b in FEET])


def feet(skel: Skeleton, rots: np.ndarray, roots: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """World positions of the ankles and balls (F, 4, 3) and the hips (F, 3), all frames at once."""
    need = sorted({j for b in FEET for j in skel.chain(b)})
    q = np.asarray(rots, dtype=np.float64)
    R, P = {}, {}
    for i in need:
        L = quat_to_mat(q[:, i] / np.maximum(np.linalg.norm(q[:, i], axis=-1, keepdims=True), 1e-12))
        p = int(skel.parents[i])
        if p < 0:
            R[i], P[i] = L, skel.heads[i] + np.asarray(roots, dtype=np.float64) * skel.hip_height
        else:
            R[i] = R[p] @ L
            P[i] = P[p] + np.einsum("fij,j->fi", R[p], skel.heads[i] - skel.heads[p])
    return np.stack([P[skel.index[b]] for b in FEET], 1), P[0]


def lowest(skel: Skeleton, rots: np.ndarray, roots: np.ndarray) -> np.ndarray:
    """(F,) how far the lowest foot point is above where it would be standing flat (m)."""
    pts, _ = feet(skel, rots, roots)
    return (pts[..., 2] - flat_heights(skel)).min(1)


def floor_of(low: np.ndarray) -> float:
    """Where a take's floor is: the feet are down on most frames, so a low percentile of their
    lowest point (a few frames below it are noise or a foot through the floor)."""
    return float(np.percentile(low, 5)) if len(low) else 0.0


def level(skel: Skeleton, rots: np.ndarray, roots: np.ndarray) -> np.ndarray:
    """Root offsets with the take's floor moved to z = 0 (LaFAN1 walked 5 cm into it, 100STYLE 2 cm
    above it). A floor more than ``floor_most_m`` off is left: the take is odd, and the scan says so."""
    off = floor_of(lowest(skel, rots, roots))
    if abs(off) > THRESHOLDS["floor_most_m"]:
        return roots
    out = np.array(roots, dtype=np.float32, copy=True)
    out[:, 2] -= off / skel.hip_height
    return out


def _runs(mask: np.ndarray, at_least: int = 1) -> list[tuple[int, int]]:
    """[(first, last)] of the runs of True at least ``at_least`` long."""
    out, start = [], None
    for i, m in enumerate(np.append(mask, False)):
        if m and start is None:
            start = i
        elif not m and start is not None:
            if i - start >= at_least:
                out.append((start, i - 1))
            start = None
    return out


def _merge(ranges: list[tuple[int, int, str]]) -> list[tuple[int, int, str]]:
    """Overlapping or touching ranges joined; the reasons listed together."""
    out: list[list] = []
    for a, b, why in sorted(ranges):
        if out and a <= out[-1][1] + 1:
            out[-1][1] = max(out[-1][1], b)
            if why not in out[-1][2].split("+"):
                out[-1][2] += "+" + why
        else:
            out.append([a, b, why])
    return [(a, b, w) for a, b, w in out]


def glitches(rots: np.ndarray, roots: np.ndarray, hip_height: float) -> np.ndarray:
    """(F,) frames off the path their neighbours draw (see the module doc)."""
    F = len(rots)
    bad = np.zeros(F, bool)
    if F < 3:
        return bad
    q = np.asarray(rots, dtype=np.float32)
    q = q / np.maximum(np.linalg.norm(q, axis=-1, keepdims=True), 1e-12)
    a, m, b = q[:-2], q[1:-1], q[2:]
    b = b * np.sign((a * b).sum(-1, keepdims=True) + 1e-12)
    mid = a + b
    mid /= np.maximum(np.linalg.norm(mid, axis=-1, keepdims=True), 1e-12)
    off = np.degrees(2 * np.arccos(np.abs((mid * m).sum(-1)).clip(0, 1))).max(1)
    o = np.asarray(roots, dtype=np.float64) * hip_height
    hips = np.linalg.norm(o[1:-1] - (o[:-2] + o[2:]) / 2, axis=-1)
    bad[1:-1] = (off > THRESHOLDS["glitch_deg"]) | (hips > THRESHOLDS["glitch_hips_m"])
    return bad


def scan_take(skel: Skeleton, rots: np.ndarray, roots: np.ndarray) -> dict:
    """One take's numbers, the ranges to cut ``[[first, last, why]]`` and whether it is rejected."""
    T = THRESHOLDS
    F = len(rots)
    pts, hips = feet(skel, rots, roots)
    height = pts[..., 2] - flat_heights(skel)
    low = height.min(1)
    floor = floor_of(low)
    pad = T["pad"]
    cuts: list[tuple[int, int, str]] = []

    def cut(mask, why, at_least=1, padded=True):
        for a, b in _runs(mask, at_least):
            p = pad if padded else 0
            cuts.append((max(0, a - p), min(F - 1, b + p), why))

    g = glitches(rots, roots, skel.hip_height)
    cut(g, "glitch")
    step = np.linalg.norm(np.diff(hips, axis=0), axis=-1)
    tele = np.zeros(F, bool)
    tele[1:] = step > T["teleport_m"]
    cut(tele, "teleport")
    cut(low < -T["sink_m"], "sink", padded=False)
    # a foot flat down (ankle and ball) on two frames running, both moving along the floor
    down = height < T["contact_m"]
    flat = down[:, 0:2] & down[:, 2:4]  # (F, 2): left, right
    on = flat[1:] & flat[:-1]
    speed = np.linalg.norm(np.diff(pts[..., :2], axis=0), axis=-1) * FPS
    moving = np.minimum(speed[:, 0:2], speed[:, 2:4])  # the slower of ankle and ball
    slide = np.zeros(F, bool)
    slide[1:] = (on & (moving > T["skate_ms"])).any(1)
    cut(slide, "skate", T["skate_frames"])
    cut(low > T["float_m"], "float", int(T["float_s"] * FPS), padded=False)
    still = np.zeros(F, bool)
    if F > 1:
        still[1:] = (np.abs(np.diff(np.asarray(rots, dtype=np.float32), axis=0)).max((1, 2)) < 1e-6) & (step < 1e-6)
    cut(still, "frozen", int(T["frozen_s"] * FPS), padded=False)

    cuts = _merge(cuts)
    cut_frames = sum(b - a + 1 for a, b, _ in cuts)
    contacts = on.any(1).sum()
    why = []
    if abs(floor) > T["floor_most_m"]:
        why.append(f"floor {floor:+.2f} m")
    if F and cut_frames / F > T["reject_share"]:
        why.append(f"{100 * cut_frames / F:.0f}% cut")
    return {
        "frames": F,
        "floor_m": round(floor, 4),
        "glitches": int(g.sum()),
        "teleports": int(tele.sum()),
        "sink_frames": int((low < -T["sink_m"]).sum()),
        "skate_share": round(float(slide[1:].sum() / max(contacts, 1)), 4),
        "float_s": round(max((b - a + 1 for a, b in _runs(low > T["float_m"])), default=0) / FPS, 2),
        "cut_frames": int(cut_frames),
        "cuts": [[int(a), int(b), w] for a, b, w in cuts],
        "reject": "; ".join(why),
    }


def scan_set(name: str, skel: Skeleton | None = None) -> dict:
    """Scan ``data/rig/<name>.npz``; writes and returns its quality record."""
    skel = skel or Skeleton.load()
    d = np.load(RIG / f"{name}.npz", allow_pickle=True)
    rots, roots, starts, takes = d["rotations"], d["roots"], d["starts"], d["takes"]
    labels = d["labels"] if "labels" in d.files else takes
    ends = [*starts[1:], len(rots)]
    out = []
    for take, label, a, b in zip(takes, labels, starts, ends):
        rec = scan_take(skel, rots[a:b], roots[a:b])
        out.append({"take": str(take), "label": str(label), **rec})
    record = {"set": name, "fps": FPS, "thresholds": THRESHOLDS, "takes": out}
    (RIG / f"{name}.quality.json").write_text(json.dumps(record, indent=1), encoding="utf-8")
    return record


def summary(record: dict) -> dict:
    """A set's totals: hours, hours cut (by reason), takes rejected."""
    takes = record["takes"]
    frames = sum(t["frames"] for t in takes)
    rejected = [t for t in takes if t["reject"]]
    gone = sum(t["frames"] for t in rejected) + sum(t["cut_frames"] for t in takes if not t["reject"])
    reasons: dict[str, int] = {}
    for t in takes:
        if t["reject"]:
            continue
        for a, b, why in t["cuts"]:
            for w in why.split("+"):
                reasons[w] = reasons.get(w, 0) + (b - a + 1)
    h = 1 / FPS / 3600
    return {"set": record["set"], "takes": len(takes), "hours": frames * h, "rejected": len(rejected),
            "hours_out": gone * h, "by_reason_h": {k: v * h for k, v in sorted(reasons.items())}}


def report(records: list[dict]) -> str:
    lines = [f"{'set':16}{'takes':>6}{'hours':>7}{'rejected':>9}{'hours out':>10}{'share':>7}  cut by reason (hours, overlaps count twice)"]
    for r in records:
        s = summary(r)
        reasons = ", ".join(f"{k} {v:.2f}" for k, v in s["by_reason_h"].items())
        share = 100 * s["hours_out"] / max(s["hours"], 1e-9)
        lines.append(f"{s['set']:16}{s['takes']:6}{s['hours']:7.2f}{s['rejected']:9}{s['hours_out']:10.2f}{share:6.0f}%  {reasons}")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("sets", nargs="*", help="sets to scan (default: every data/rig/*.npz)")
    args = ap.parse_args(argv)
    names = args.sets or sorted(p.stem for p in RIG.glob("*.npz"))
    skel = Skeleton.load()
    records = []
    for n in names:
        records.append(scan_set(n, skel))
        print(f"  {n}: {RIG / (n + '.quality.json')}", flush=True)
    print(report(records))


if __name__ == "__main__":
    main()
