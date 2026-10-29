"""Coupled joint range-of-motion tables (``assets/rig/ranges.json``).

``python -m ac_train.ranges`` measures anatomical joint angles (:mod:`ac_train.anatomy`, the same
maths as ``ac_core::anatomy``) on the clean frames of the recorded free-licence mocap sets (CMU,
100STYLE, ACCAD, AddBiomechanics; frames cut or rejected by :mod:`ac_train.review` left out; the
hand-made CC0 loops and the generated Kimodo motion left out, since neither is a recorded body) and
writes, for each coupled pair of joint angles:

- ``typical``: the 1st–99th percentile of the vertical angle in bins along the horizontal one,
  left and right pooled;
- ``grid``: a 24 x 24 log-scaled density (0..=100) of the frames, the Range panel's cloud;
- ``hard`` and ``x_hard``: published clinical limits. Every number is one a source states; the
  pair's ``limits_note`` says which, and where two values are joined (a straight line between
  them, or an ellipse between single-plane limits) the joining is ours.

and ``possible``: per joint angle, the limits past which a pose is impossible (a foot turned on the
shin, a knee bent the wrong way). They are ours, not published: past anything in the reviewed
recordings with a margin, and the review (:mod:`ac_train.review`) cuts the frames past them, so each
entry records what the reviewed frames reach (``observed``). Past the clinical limits but inside
these is unusual or athletic, not wrong.

It also writes ``checks``: a few poses with their angles, which the Rust tests recompute.
"""

from __future__ import annotations

import argparse
import json
import pathlib

import numpy as np

from . import ASSETS, anatomy, review, rig

OUT_PATH = ASSETS / "rig" / "ranges.json"
SETS = ("cmu", "100style", "accad", "addbiomechanics")
GRID = 24
MIN_BIN_SAMPLES = 250
CHUNK = 200_000
CHECK_FRAMES = 6

SOURCES = [
    {
        "id": "cdc_soucie_2011",
        "title": "CDC Normal Joint Range of Motion Study: Soucie et al., Haemophilia 17:500-507 (2011), 674 healthy people aged 2-69",
        "url": "https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html",
        "note": "published mean values, ages 20-44, female and male averaged (only the summary values are used, not the restricted raw dataset)",
    },
    {
        "id": "roaas_andersson_1982",
        "title": "Roaas & Andersson, Normal range of motion of the hip, knee and ankle joints in male subjects, 30-40 years of age. Acta Orthop Scand 53:205-208 (1982)",
        "url": "https://actaorthop.org/actao/article/download/28654/33535/82106",
        "note": "210 hips: abduction 38.6 (SD 7), adduction 30.5 (SD 7.3), left and right averaged",
    },
    {
        "id": "baumbach_2014",
        "title": "Baumbach et al., The influence of knee position on ankle dorsiflexion - a biometric study. BMC Musculoskeletal Disorders 15:246 (2014), CC BY 4.0",
        "url": "https://pmc.ncbi.nlm.nih.gov/articles/PMC4118219/",
        "note": "weight-bearing dorsiflexion: 25 deg with the knee straight, 38 at 20 deg knee bend (39 right, 37 left), 39 at 75 deg; the calf stops limiting it by 20 deg",
    },
    {
        "id": "dshs_13_585a",
        "title": "Washington State DSHS 13-585A Range of Joint Motion Evaluation Chart (rev. 03/2014)",
        "url": "https://www.dshs.wa.gov/sites/default/files/forms/pdf/13-585a.pdf",
        "note": "the reference degrees printed on the form",
    },
    {
        "id": "mocap",
        "title": "CMU Graphics Lab Motion Capture Database; 100STYLE (CC BY 4.0); ACCAD Open Motion Project (CC BY 3.0); AddBiomechanics Dataset 1.0 (CC BY 4.0)",
        "url": "https://zenodo.org/record/8127870",
        "note": "the typical band and density: clean frames retargeted onto the rig",
    },
    {
        "id": "akhter_black_2015",
        "title": "Akhter & Black, Pose-Conditioned Joint Angle Limits for 3D Human Pose Reconstruction (CVPR 2015)",
        "url": "https://openaccess.thecvf.com/content_cvpr_2015/papers/Akhter_Pose-Conditioned_Joint_Angle_2015_CVPR_paper.pdf",
        "note": "the idea of limits that depend on the pose (none of its data is used)",
    },
]


def _ellipse(x_lo: float, x_hi: float, y_lo: float, y_hi: float, steps: int = 24) -> list[dict]:
    """Band of the ellipse quadrants through four single-plane limits (x_lo < 0 < x_hi)."""
    t = np.linspace(-1.0, 1.0, 2 * steps + 1)
    xs = np.where(t < 0, -t * x_lo, t * x_hi)  # x_lo is negative
    k = np.sqrt(np.clip(1.0 - t * t, 0.0, 1.0))
    return [{"x": round(float(x), 3), "lo": round(float(y_lo * f), 3), "hi": round(float(y_hi * f), 3)}
            for x, f in zip(xs, k)]


def _ax(joint: str, comp: str, label: str) -> dict:
    return {"joint": joint, "component": comp, "label": label}


KNEE = (-1.3, 139.8)  # CDC: extension 1.3, flexion 139.8
PAIRS = [
    {
        "id": "hip_knee",
        "label": "Hip bend × Knee bend",
        "sided": True,
        "x": _ax("knee", "flexion", "Knee bend (flexion °)"),
        "y": _ax("hip", "flexion", "Hip bend (flexion °)"),
        "x_hard": list(KNEE),
        "hard": [{"x": 0.0, "lo": -17.75, "hi": 100.0}, {"x": 90.0, "lo": -17.75, "hi": 132.1}],
        "x_bins": [0, 10, 20, 30, 45, 60, 75, 90, 105, 120, 140],
        "grid": [[-20.0, 150.0], [-40.0, 150.0]],
        "note": "The hamstrings cross the hip and the knee: with the knee straight they stop the hip bending as far.",
        "limits_note": "Knee: CDC (flexion 139.8, extension 1.3). Hip flexion 100 with the knee straight (DSHS 13-585A) and 132.1 with it bent (CDC, measured knee bent); the straight line between knee 0 and 90 deg is ours. Hip extension 17.75 (CDC).",
    },
    {
        "id": "ankle_knee",
        "label": "Ankle bend × Knee bend",
        "sided": True,
        "x": _ax("knee", "flexion", "Knee bend (flexion °)"),
        "y": _ax("ankle", "flexion", "Ankle upward bend (dorsiflexion °)"),
        "x_hard": list(KNEE),
        "hard": [{"x": 0.0, "lo": -58.35, "hi": 25.0}, {"x": 20.0, "lo": -58.35, "hi": 38.0},
                 {"x": 75.0, "lo": -58.35, "hi": 39.0}],
        "x_bins": [0, 5, 10, 15, 20, 30, 45, 60, 75, 90, 105, 120, 140],
        "grid": [[-20.0, 150.0], [-70.0, 50.0]],
        "note": "The calf muscle (gastrocnemius) crosses the knee and the ankle: with the knee straight the foot can't bend up as far.",
        "limits_note": "Dorsiflexion under body weight 25 / 38 / 39 deg at knee 0 / 20 / 75 (Baumbach 2014); plantarflexion 58.35 (CDC); knee from CDC.",
    },
    {
        "id": "hip_swing",
        "label": "Hip bend × Hip out",
        "sided": True,
        "x": _ax("hip", "abduction", "Leg out (abduction °)"),
        "y": _ax("hip", "flexion", "Hip bend (flexion °)"),
        "x_hard": [-30.5, 38.6],
        "hard": _ellipse(-30.5, 38.6, -17.75, 132.1),
        "x_bins": [-30, -20, -10, -5, 0, 5, 10, 20, 30, 40, 60],
        "grid": [[-45.0, 90.0], [-40.0, 150.0]],
        "note": "Where the thigh can point: forward and out at once is less than either alone.",
        "limits_note": "Abduction 38.6, adduction 30.5 (Roaas & Andersson 1982); flexion 132.1 (knee bent) and extension 17.75 (CDC); the ellipse between them is ours. Splits and high kicks go past this: they are trained ranges.",
    },
    {
        "id": "shoulder_swing",
        "label": "Arm forward × Arm out",
        "sided": True,
        "x": _ax("shoulder", "abduction", "Arm out (abduction °)"),
        "y": _ax("shoulder", "flexion", "Arm forward (flexion °)"),
        "x_hard": [-30.0, 150.0],
        "hard": _ellipse(-30.0, 150.0, -50.0, 170.4),
        "x_bins": [-30, -10, 0, 15, 30, 45, 60, 90, 120, 150, 180],
        "grid": [[-45.0, 180.0], [-70.0, 180.0]],
        "note": "Where the upper arm can point, measured against the chest (shoulder blade included).",
        "limits_note": "Flexion 170.4 (CDC); extension 50, abduction 150, adduction 30 (DSHS 13-585A); the ellipse between them is ours. Rotation has no limit here: no source we have read gives one.",
    },
    {
        "id": "trunk_bend",
        "label": "Trunk forward × Trunk sideways",
        "sided": False,
        "x": _ax("trunk", "abduction", "Bend left + / right − (°)"),
        "y": _ax("trunk", "flexion", "Bend forward (flexion °)"),
        "x_hard": [-25.0, 25.0],
        "hard": _ellipse(-25.0, 25.0, -25.0, 90.0),
        "x_bins": [-30, -20, -10, -5, 0, 5, 10, 20, 30],
        "grid": [[-45.0, 45.0], [-40.0, 110.0]],
        "note": "The chest against the pelvis, all five spine bones together.",
        "limits_note": "Flexion 90, extension 25, side bend 25 each way (DSHS 13-585A); the ellipse between them is ours. Twist has no limit here: no source we have read gives one.",
    },
    {
        "id": "neck_bend",
        "label": "Neck forward × Neck sideways",
        "sided": False,
        "x": _ax("neck", "abduction", "Tilt left + / right − (°)"),
        "y": _ax("neck", "flexion", "Nod forward (flexion °)"),
        "x_hard": [-45.0, 45.0],
        "hard": _ellipse(-45.0, 45.0, -60.0, 50.0),
        "x_bins": [-45, -30, -15, -5, 0, 5, 15, 30, 45],
        "grid": [[-60.0, 60.0], [-80.0, 70.0]],
        "note": "The head against the chest.",
        "limits_note": "Flexion 50, extension 60, side bend 45 each way (DSHS 13-585A); the ellipse between them is ours.",
    },
]


#: (joint, component, lo, hi, max_swing): impossible past lo..hi. ``max_swing``: the rule holds only
#: while the segment is swung less than this from its zero, where the reading is well defined (hip
#: twist at a big swing and the shoulder overhead read differently for the same pose).
POSSIBLE = [
    ("hip", "flexion", -70.0, 170.0, None),
    ("hip", "abduction", -60.0, 125.0, None),
    ("hip", "rotation", -100.0, 100.0, 60.0),
    ("knee", "flexion", -25.0, 170.0, None),
    ("ankle", "flexion", -90.0, 60.0, None),
    ("ankle", "abduction", -80.0, 80.0, None),
    ("shoulder", "flexion", -95.0, 190.0, 150.0),
    ("shoulder", "abduction", -80.0, 190.0, 150.0),
    ("trunk", "flexion", -80.0, 120.0, None),
    ("trunk", "abduction", -60.0, 60.0, None),
    ("trunk", "rotation", -75.0, 75.0, None),
    ("neck", "flexion", -90.0, 90.0, None),
    ("neck", "abduction", -70.0, 70.0, None),
    ("neck", "rotation", -100.0, 100.0, None),
]
POSSIBLE_NOTE = ("Ours, not published: past anything in the reviewed recordings with a margin (a foot turned "
                 "on the shin, a knee bent the wrong way). Past the clinical limits but inside these is unusual "
                 "or athletic. observed: the 0.01th and 99.99th percentile of the reviewed frames.")
#: most of the reviewed frames a possible limit may leave out (a limit that cuts real motion is wrong)
POSSIBLE_MAX_OUTSIDE = 1e-4


def possible_mask(angles: np.ndarray, rule) -> np.ndarray:
    """Frames of one joint's (N, 3) angles past ``rule`` (an entry of POSSIBLE)."""
    joint, comp, lo, hi, max_swing = rule
    v = angles[:, anatomy.COMPONENTS.index(comp)]
    m = (v < lo) | (v > hi)
    if max_swing is not None:
        m &= np.hypot(angles[:, 0], angles[:, 1]) < max_swing
    return m


def build_possible(angles) -> list[dict]:
    out = []
    for rule in POSSIBLE:
        joint, comp, lo, hi, max_swing = rule
        sides = ("L", "R") if anatomy.DEFS[joint][0] else ("L",)
        a = [angles[(joint, s)] for s in sides]
        held = [x if max_swing is None else x[np.hypot(x[:, 0], x[:, 1]) < max_swing] for x in a]
        v = np.concatenate([x[:, anatomy.COMPONENTS.index(comp)] for x in held])
        share = float(np.mean(np.concatenate([possible_mask(x, rule) for x in a]))) if len(v) else 0.0
        if share > POSSIBLE_MAX_OUTSIDE:
            raise SystemExit(f"possible {joint}.{comp} {lo}..{hi} leaves out {share:.4%} of the reviewed frames: "
                             "review those takes (ac-review) or widen the limit")
        obs = np.percentile(v, [0.01, 99.99]) if len(v) else np.array([0.0, 0.0])
        out.append({"joint": joint, "component": comp, "lo": lo, "hi": hi, "max_swing": max_swing,
                    "observed": [round(float(obs[0]), 2), round(float(obs[1]), 2)], "outside": round(share, 7)})
    return out


def _needed() -> list[tuple[str, str]]:
    out = []
    for joint in anatomy.JOINTS:
        sides = ("L", "R") if anatomy.DEFS[joint][0] else ("L",)
        out += [(joint, s) for s in sides]
    return out


def clean_angles(skel: rig.Skeleton, sets=SETS):
    """{(joint, side): (N, 3) degrees} over the clean frames of ``sets``, the takes used, the sets
    read, and CHECK_FRAMES sample poses (rotations) from the first set."""
    decs = review.load_decisions()
    joints = {k: anatomy.Joint(skel, *k) for k in _needed()}
    parts: dict[tuple[str, str], list[np.ndarray]] = {k: [] for k in joints}
    takes_used, used, samples = 0, [], []
    for s in sets:
        path = review.RIG / f"{s}.npz"
        if not path.exists():
            print(f"skipping {s}: {path} is not built")
            continue
        d = np.load(path, allow_pickle=True)
        rots, starts = d["rotations"], d["starts"]
        lengths = np.diff(np.append(starts, len(rots)))
        p = review.plan(s, d["takes"], lengths, decs, review.load_scan(s))
        keep = np.ones(len(rots), bool)
        for t, st in zip(p.takes, starts):
            if t.drop:
                keep[st : st + t.frames] = False
            else:
                takes_used += 1
                keep[st : st + t.frames] &= ~t.bad()
        idx = np.flatnonzero(keep)
        if not samples and len(idx):
            pick = idx[np.linspace(0, len(idx) - 1, CHECK_FRAMES).astype(int)]
            samples = [rots[i].astype(np.float64) for i in pick]
        for a in range(0, len(idx), CHUNK):
            chunk = rots[idx[a : a + CHUNK]].astype(np.float64)
            for k, j in joints.items():
                parts[k].append(j.angles(skel, chunk).astype(np.float32))
        used.append(s)
        print(f"{s}: {len(idx)} clean frames")
    angles = {k: np.concatenate(v) if v else np.zeros((0, 3), np.float32) for k, v in parts.items()}
    return angles, takes_used, used, samples


def _xy(angles, spec):
    c = anatomy.COMPONENTS
    sides = ("L", "R") if spec["sided"] else ("L",)
    xs = [angles[(spec["x"]["joint"], s)][:, c.index(spec["x"]["component"])] for s in sides]
    ys = [angles[(spec["y"]["joint"], s)][:, c.index(spec["y"]["component"])] for s in sides]
    return np.concatenate(xs).astype(np.float64), np.concatenate(ys).astype(np.float64)


def build_pair(angles, spec: dict) -> dict:
    x, y = _xy(angles, spec)
    edges = spec["x_bins"]
    typical = []
    for x0, x1 in zip(edges[:-1], edges[1:]):
        m = (x >= x0) & (x < x1)
        n = int(m.sum())
        if n >= MIN_BIN_SAMPLES:
            lo, hi = np.percentile(y[m], [1.0, 99.0])
            typical.append({"x": round(0.5 * (x0 + x1), 2), "lo": round(float(lo), 2),
                            "hi": round(float(hi), 2), "count": n})
    (gx0, gx1), (gy0, gy1) = spec["grid"]
    hist, _, _ = np.histogram2d(x, y, bins=[GRID, GRID], range=[[gx0, gx1], [gy0, gy1]])
    log_h = np.log1p(hist).T  # rows are y, bottom first
    top = float(log_h.max()) if log_h.size and log_h.max() > 0 else 1.0
    cells = np.round(log_h / top * 100.0).astype(int).tolist()
    out = {k: spec[k] for k in ("id", "label", "sided", "x", "y", "x_hard", "hard", "note", "limits_note")}
    out["typical"] = typical
    out["grid"] = {"x_range": [gx0, gx1], "y_range": [gy0, gy1], "cols": GRID, "rows": GRID, "cells": cells}
    return out


def checks(skel: rig.Skeleton, samples: list[np.ndarray]) -> list[dict]:
    """Sample poses (the rest pose first) and every joint's angles on them."""
    poses = [np.tile(np.array([0.0, 0.0, 0.0, 1.0]), (len(skel.names), 1))] + samples
    stack = np.stack(poses)
    out = [{"rotations": p.tolist(), "angles": {}} for p in poses]
    for joint, side in _needed():
        a = anatomy.Joint(skel, joint, side).angles(skel, stack)
        for o, row in zip(out, a):
            o["angles"][f"{joint}.{side}" if anatomy.DEFS[joint][0] else joint] = [float(v) for v in row]
    return out


def build(out: pathlib.Path = OUT_PATH) -> dict:
    skel = rig.Skeleton.load()
    angles, takes, used, samples = clean_angles(skel)
    frames = len(next(iter(angles.values())))
    if frames == 0:
        raise SystemExit("no retargeted mocap in training/data/rig: run python -m ac_train.datasets first")
    doc = {
        "format": "humanmotioncraft.ranges",
        "version": 3,
        "space": "anatomical_degrees",
        "sets": used,
        "frames": frames,
        "takes": takes,
        "sources": SOURCES,
        "pairs": [build_pair(angles, spec) for spec in PAIRS],
        "possible": build_possible(angles),
        "possible_note": POSSIBLE_NOTE,
        "checks": checks(skel, samples),
    }
    out.write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    return doc


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=pathlib.Path, default=OUT_PATH, help="output JSON path")
    args = ap.parse_args(argv)
    doc = build(args.out)
    print(f"wrote {args.out} ({doc['frames']} frames, {doc['takes']} takes, {len(doc['pairs'])} pairs)")


if __name__ == "__main__":
    main()
