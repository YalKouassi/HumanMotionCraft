"""Which walks look unnatural (roadmap 153000): a person reviews what this leaves, not all 1,687.

    python -m ac_train.gaitcheck --out data/walk-check
    python -m ac_train.gaitcheck --out data/walk-check --write-takes review/natural-takes-proposed.json

The style filter only understands 100STYLE's labels, so "walk/hobble" and a zombie walk from CMU pass
it. Two checks here: a description with any word outside the plain walk's words (``PLAIN_WORDS``), and
the motion, for what a description doesn't say: how unevenly the feet stay down and step, how the
trunk leans, where the wrists are held and how much they swing, and how low the hips ride. A measure
stands out when it is more than ``LIMIT`` robust deviations from the walks' median. Walking backwards
or sideways is reported apart, and takes with under ``MIN_MOVING`` s of walking can't be measured.

100STYLE's styled walks say what they are, so they test this: the same limit is run on them and the
share it catches is printed beside the share of plain walks it flags. Nothing here removes a take:
the proposed list is for the owner to accept.
"""

from __future__ import annotations

import argparse
import csv
import json
import pathlib
import re

import numpy as np

from . import OUT, review, selection
from .posetargets import facing, hips_path, path_of
from .rig import Skeleton

MOVING = 0.4  # m/s: frames slower than this say nothing about a gait
MIN_MOVING = 1.5  # seconds of walking a take needs to be measured
LIMIT = 3.5  # robust deviations
#: at most this many seconds of a take are measured
MOST = 60.0
#: read along the way the body goes, so they say nothing of a sidestep, where one foot always leads
GAIT_MEASURES = ("stance_uneven", "step_uneven")
MEASURES = ("stance_uneven", "step_uneven", "lean_forward", "lean_side", "wrist_ahead", "wrist_out", "wrist_height", "swing", "hips")
#: The only words a plain walk's description uses: any other word ("ScaredWalk", "walk/hobble", "Mickey Walk")
#: says it is a way of walking. A list of bad words would never end.
PLAIN_WORDS = frozenset("""
    a an and the to of in on at then with from for by around about back again both each one two three
    person people someone they he she walks walk walking walked walkway stroll strolls straight forward forwards
    ahead normal normally regular casual casually natural naturally steady steadily slow slowly slower fast faster quick
    quickly medium speed pace repeated cyclic start starts stop stops stand stands standing still stationary to wander
    wanders turn turns turning around left right opposite direction directions degree degrees corner corners angle angles
    veer veers curve curves circle circles clockwise anticlockwise counterclockwise figure eight digital path paths follow
    follows navigate navigates obstacle obstacles backward backwards sideway sideways sidestep sidesteps side step steps
    stepping short large long small male female subject subjects take takes clean cleaned grs trial trials
    diagonal place frequent brisk ninety change random fix adjust link
""".split())
#: AddBiomechanics labels are study file names; these name gaits altered on purpose (medial thrust, trunk sway, ...)
ALTERED = re.compile(r"mtgait|mtpgait|tsgait|wpgait|medialthrust|bouncy|walkingTS|smooth og", re.I)
#: 100STYLE's styles that are clearly not a normal walk (the words before the colon in its labels), the test
TEST_STYLES = ("Zombie", "Drunk", "DragLeftLeg", "DragRightLeg", "Old", "Dinosaur", "Chicken", "Monk", "Stiff", "Crouched", "Robot")


def measure(skel: Skeleton, rots: np.ndarray, roots: np.ndarray, fps: float) -> dict[str, float] | None:
    """A take's measures, or None when it has too little walking to tell. The trunk and the arms are
    read against where the body faces, so walking backwards doesn't look like leaning."""
    most = int(MOST * fps)
    rots, roots = rots[:most].astype(np.float64), roots[:most].astype(np.float64)
    if len(roots) < MIN_MOVING * fps + 2:
        return None
    P = hips_path(skel, roots)
    v = np.diff(P[:, :2], axis=0) * fps
    # the walking direction over a third of a second, which a stride's sway doesn't turn
    width = max(1, int(fps / 3))
    kernel = np.ones(width) / width
    smooth = np.stack([np.convolve(v[:, c], kernel, mode="same") for c in range(2)], -1)
    speed = np.linalg.norm(smooth, axis=-1)
    on = speed > MOVING
    if on.sum() < MIN_MOVING * fps:
        return None
    going = smooth / np.maximum(speed, 1e-9)[:, None]
    ahead = facing(rots)[:-1]
    side = np.stack([-ahead[:, 1], ahead[:, 0]], -1)
    hips3 = P[:-1]
    of = {b: path_of(skel, rots, roots, b)[:-1] for b in ("neck01", "foot.L", "foot.R", "wrist.L", "wrist.R")}

    def along(p, axis):
        return np.sum((p - hips3)[:, :2] * axis, -1)

    # uneven stance: the feet's time on the ground, which a limp shortens on one side
    def down(foot):
        step = np.linalg.norm(np.diff(of[foot][:, :2], axis=0), axis=-1) * fps
        return float((step[on[:-1]] < 0.35 * np.median(speed[on])).mean())

    stance_l, stance_r = down("foot.L"), down("foot.R")
    gap = np.sum((of["foot.L"] - of["foot.R"])[:, :2] * going, -1)[on]
    lead_l, lead_r = np.percentile(gap, 90), -np.percentile(gap, 10)
    size = np.maximum(np.linalg.norm(of["neck01"] - hips3, axis=-1), 1e-9)
    wrists = [(along(of[w], ahead)[on], along(of[w], side)[on], (of[w] - hips3)[on, 2]) for w in ("wrist.L", "wrist.R")]
    return {
        "stance_uneven": abs(stance_l - stance_r),
        "step_uneven": abs(lead_l - lead_r) / max(abs(lead_l) + abs(lead_r), 1e-3),
        "lean_forward": float(np.degrees(np.arcsin(np.clip(along(of["neck01"], ahead) / size, -1, 1)))[on].mean()),
        "lean_side": float(np.degrees(np.arcsin(np.clip(np.abs(along(of["neck01"], side)) / size, -1, 1)))[on].mean()),
        "wrist_ahead": float(max(w[0].mean() for w in wrists)),
        "wrist_out": float(max(np.abs(w[1]).mean() for w in wrists)),
        "wrist_height": float(max(w[2].mean() for w in wrists)),
        "swing": float(np.mean([w[0].std() for w in wrists])),
        "hips": float(P[:-1, 2][on].mean()),
        "speed": float(speed[on].mean()),
        "forwards": float((np.sum(ahead * going, -1)[on] > 0.5).mean()),
    }


def flags(values: dict[str, float], centre: dict[str, tuple[float, float]]) -> list[tuple[str, float]]:
    """The measures standing out, largest first, with their robust deviations."""
    out = []
    for m in MEASURES:
        if m in GAIT_MEASURES and values["forwards"] < 0.5:
            continue
        mid, spread = centre[m]
        z = (values[m] - mid) / max(spread, 1e-9)
        if abs(z) > LIMIT:
            out.append((m, round(float(z), 1)))
    return sorted(out, key=lambda x: -abs(x[1]))


def centres(rows: list[dict[str, float]]) -> dict[str, tuple[float, float]]:
    """Each measure's median and robust spread."""
    out = {}
    for m in MEASURES:
        xs = np.array([r[m] for r in rows])
        mid = float(np.median(xs))
        out[m] = (mid, 1.4826 * float(np.median(np.abs(xs - mid))))
    return out


def odd_word(set_name: str, label: str) -> str:
    """The word in a take's description that says it isn't a plain walk, or ""."""
    if set_name == "addbiomechanics":
        found = ALTERED.search(label)
        return found.group(0) if found else ""
    if set_name == "100style":
        return "" if label.startswith("Neutral:") else label.split(":")[0]
    spaced = re.sub(r"(?<=[a-z])(?=[A-Z0-9])", " ", label)
    for word in re.findall(r"[A-Za-z]+", spaced):
        if len(word) > 1 and word.lower() not in PLAIN_WORDS:
            return word
    return ""


def takes_of(set_name: str, decisions, select) -> list[tuple[selection.Record, dict[str, float] | None]]:
    """The set's walks the review allows, kept by ``select`` or left out by style, with their measures."""
    z = np.load(OUT / "rig" / f"{set_name}.npz")
    _, recs = selection.records(set_name, z, decisions, select)
    # an npz reads a whole array at each z[name]: once a set, not once a take
    starts, rotations, roots, fps = z["starts"], z["rotations"], z["roots"], float(z["fps"])
    skel = Skeleton.load()
    out = []
    for i, r in enumerate(recs):
        if r.type != "walk" or not (r.kept or r.why.startswith("style:")):
            continue
        a, b = int(starts[i]), int(starts[i + 1]) if i + 1 < len(starts) else len(roots)
        out.append((r, measure(skel, rotations[a:b], roots[a:b], fps)))
    return out


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--sets", default="100style,cmu,kimodo,accad,addbiomechanics")
    ap.add_argument("--out", default="review/walk-check")
    ap.add_argument("--write-takes", type=pathlib.Path, default=None, help="the natural take list with the flagged walks excluded")
    args = ap.parse_args(argv)
    decisions = review.load_decisions()
    natural = selection.Selection(frozenset(selection.NATURAL_TYPES), frozenset(selection.NATURAL_STYLES))
    rows = []
    rates = {s: float(np.load(OUT / "rig" / f"{s}.npz")["fps"]) for s in args.sets.split(",")}
    styled: dict[str, list[dict[str, float]]] = {}
    for s in args.sets.split(","):
        for r, m in takes_of(s, decisions, natural):
            if r.kept:
                rows.append((r, m))
            if s == "100style" and m is not None:
                styled.setdefault(r.label.split(":")[0], []).append(m)
    measured = [m for _, m in rows if m is not None]
    centre = centres(measured)
    report = []
    for r, m in rows:
        why = [] if m is None else [f"{k} {z:+.1f}" for k, z in flags(m, centre)]
        word = odd_word(r.set, r.label)
        back = m is not None and m["forwards"] < 0.5
        report.append({
            "take": r.id, "label": r.label, "minutes": round(r.used / rates[r.set] / 60, 2), "held_out": r.held,
            "group": "words" if word else "motion" if why else "backwards or sideways" if back else "too little walking" if m is None else "plain",
            "word": word, "why": why,
            "measures": {k: round(v, 3) for k, v in (m or {}).items()},
        })
    total = {}
    for row in report:
        g = total.setdefault(row["group"], [0, 0.0])
        g[0] += 1
        g[1] += row["minutes"]
    print(f"{len(report)} natural walks, {sum(x[1] for x in total.values()):.0f} min:")
    for g, (n, mins) in sorted(total.items(), key=lambda kv: -kv[1][0]):
        print(f"  {g:20s} {n:5d} takes {mins:6.1f} min")
    print("the test, 100STYLE's walks caught by the motion measures alone (three takes a style: forwards, backwards, sideways):")
    caught = seen = 0
    for w in (*TEST_STYLES, "Neutral"):
        ms = styled.get(w, [])
        hit = sum(bool(flags(m, centre)) for m in ms)
        if w != "Neutral":
            caught, seen = caught + hit, seen + len(ms)
        print(f"  {w:14s} {hit:3d} of {len(ms)}")
    if seen:
        print(f"  the odd styles {caught} of {seen} ({100 * caught / seen:.0f}%)")
    every = {w: sum(bool(flags(m, centre)) for m in ms) / len(ms) for w, ms in styled.items()}
    print("  caught least:", ", ".join(f"{w} {v:.0%}" for w, v in sorted(every.items(), key=lambda kv: kv[1])[:12]))
    print(f"natural walks flagged by the same limit: {sum(bool(flags(m, centre)) for m in measured)} of {len(measured)}")
    out = pathlib.Path(args.out)
    out.with_suffix(".json").write_text(json.dumps({"limit": LIMIT, "centre": centre, "takes": report}, indent=1), encoding="utf-8")
    with out.with_suffix(".csv").open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["take", "group", "minutes", "word", "why", "label"])
        for row in sorted(report, key=lambda x: (x["group"], x["take"])):
            w.writerow([row["take"], row["group"], row["minutes"], row["word"], "; ".join(row["why"]), row["label"]])
    print(f"wrote {out.with_suffix('.json')} and {out.with_suffix('.csv')}")
    if args.write_takes:
        out_list = [row["take"] for row in report if row["group"] in ("words", "motion")]
        base = json.loads((review.DECISIONS.parent / "natural-takes.json").read_text(encoding="utf-8"))
        args.write_takes.write_text(json.dumps({
            "note": "proposed from ac_train.gaitcheck: natural-takes.json with the walks whose words or motion stand out excluded",
            "include": [t for t in base["include"] if t not in set(out_list)],
            "exclude": sorted(out_list),
        }, indent=1), encoding="utf-8")
        print(f"wrote {args.write_takes}: {len(out_list)} walks excluded")


if __name__ == "__main__":
    main()
