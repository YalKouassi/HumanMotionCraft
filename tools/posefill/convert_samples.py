# SPDX-License-Identifier: Apache-2.0
"""Retarget the optional sample clips' BVH files onto the rig, for `setup_samples.py`:

    <posefill>/.venv/python -I convert_samples.py --posefill <dir> --rig <rig.json> --motions <dir> --out <dir>

Run on the install's own Python, with the rig maths the install holds. Each file becomes one
`<folder>__<name>.json` under `--out`: {name, fps, rotations, root}. The app turns those into
library clips marked evaluation-only (`posefill.samples`, action add); nothing here reads or writes
anything else.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

FPS = 30.0
CM = 0.01  # the files are in centimetres, Y up
# landmark -> SOMA joint ("Leg" is the thigh, "Shin" the knee), as training's SOMA sets
SOMA = {"hips": "Hips", "spine": "Spine1", "neck": "Neck1", "head": "Head",
        "l_shoulder": "LeftArm", "l_elbow": "LeftForeArm", "l_wrist": "LeftHand",
        "l_hip": "LeftLeg", "l_knee": "LeftShin", "l_ankle": "LeftFoot", "l_toe": "LeftToeBase",
        "r_shoulder": "RightArm", "r_elbow": "RightForeArm", "r_wrist": "RightHand",
        "r_hip": "RightLeg", "r_knee": "RightShin", "r_ankle": "RightFoot", "r_toe": "RightToeBase"}
FEET = ("foot.L", "toe3-1.L", "foot.R", "toe3-1.R")


def floor_offset(np, skel, quat_to_mat, rots, roots) -> float:
    """Where the take's floor is against the rig's (m): a low percentile of the lowest foot point
    over the height it has standing flat, as training levels its sets."""
    lift = skel.hip_height - float(skel.heads[0][2])
    flat = np.array([skel.heads[skel.index[b]][2] + lift for b in FEET])
    R, P = {}, {}
    for i in sorted({j for b in FEET for j in skel.chain(b)}):
        L = quat_to_mat(rots[:, i] / np.maximum(np.linalg.norm(rots[:, i], axis=-1, keepdims=True), 1e-12))
        p = int(skel.parents[i])
        if p < 0:
            R[i], P[i] = L, skel.heads[i] + roots * skel.hip_height
        else:
            R[i] = R[p] @ L
            P[i] = P[p] + np.einsum("fij,j->fi", R[p], skel.heads[i] - skel.heads[p])
    heights = np.stack([P[skel.index[b]][:, 2] for b in FEET], 1)
    return float(np.percentile((heights - flat).min(1), 5))


def convert(path: pathlib.Path, skel) -> dict:
    """One BVH on the rig at 30 fps (every n-th frame: the files are at 120)."""
    import numpy as np

    from ac_rig.bvh import load
    from ac_rig.rig import quat_to_mat
    from ac_rig.retarget import retarget, scale_to

    m = load(path)
    every = max(1, round(m.fps / FPS))
    missing = [j for j in SOMA.values() if j not in m.names]
    if missing:
        raise ValueError(f"{path.name}: joints not found {missing}")
    pos = m.positions[::every] * CM
    src = {k: np.stack([pos[:, m.names.index(j), 0], -pos[:, m.names.index(j), 2], pos[:, m.names.index(j), 1]], -1)
           for k, j in SOMA.items()}
    rots, roots = retarget(skel, scale_to(skel, src))
    roots = np.array(roots, dtype=np.float64)
    roots[:, 2] -= floor_offset(np, skel, quat_to_mat, np.asarray(rots, dtype=np.float64), roots) / skel.hip_height
    return {"name": path.stem, "fps": round(m.fps / every, 2),
            "rotations": np.asarray(rots).tolist(), "root": roots.tolist()}


def main(argv=None) -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--posefill", type=pathlib.Path, required=True, help="the pose fill install, for its ac_rig")
    ap.add_argument("--rig", type=pathlib.Path, required=True)
    ap.add_argument("--motions", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    args = ap.parse_args(argv)
    sys.path.insert(0, str(args.posefill))
    from ac_rig.rig import Skeleton

    skel = Skeleton.load(args.rig)
    args.out.mkdir(parents=True, exist_ok=True)
    files = sorted(args.motions.rglob("*.bvh"))
    for n, f in enumerate(files):
        out = args.out / f"{f.parent.name}__{f.stem}.json"
        if out.exists():
            continue
        clip = convert(f, skel)
        tmp = out.with_suffix(".tmp")
        tmp.write_text(json.dumps(clip), encoding="utf-8")
        tmp.replace(out)
        print(f"  {n + 1}/{len(files)} {clip['name']}: {len(clip['root'])} frames", flush=True)


if __name__ == "__main__":
    main()
