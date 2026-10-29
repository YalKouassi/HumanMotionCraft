"""Generate training clips: one SOMA BVH per prompt and seed, and the index training reads.

    <install>/.venv/Scripts/python <install>/generate.py --prompts training/kimodo/prompts.tsv \
        --out training/data/raw/kimodo [--seeds 1,2,3] [--only name,name]

<install> is where setup_textmotion.py put the model (it prints it); the install's settings are
read from its install.json.

`prompts.tsv` has the columns name, type, seconds, prompt. Each clip is written as
`<name>_s<seed>.bvh` (Kimodo's own BVH export of its 77-joint skeleton, 30 fps), and the out folder
gets a `prompts.tsv` naming every clip with its type and prompt (`ac_train.datasets kimodo` reads it).
Clips already there are skipped, so a run can be resumed or extended with more seeds.
"""

from __future__ import annotations

import argparse
import csv
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import engine as kc  # noqa: E402


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description="Generate training clips with Kimodo")
    ap.add_argument("--prompts", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--seeds", default="1", help="comma list of seeds; each prompt is made once a seed")
    ap.add_argument("--only", default="", help="comma list of prompt names")
    args = ap.parse_args(argv)
    seeds = [int(s) for s in args.seeds.split(",") if s.strip()]
    only = {s.strip() for s in args.only.split(",") if s.strip()}
    with open(args.prompts, encoding="utf-8-sig", newline="") as fh:
        rows = [r for r in csv.DictReader(fh, delimiter="\t") if not only or r["name"] in only]
    if not rows:
        sys.exit(f"no prompts in {args.prompts}" + (f" named {sorted(only)}" if only else ""))
    args.out.mkdir(parents=True, exist_ok=True)

    import torch

    from kimodo.exports.bvh import save_motion_bvh
    from kimodo.skeleton import global_rots_to_local_rots
    from kimodo.tools import seed_everything

    gen = kc.Generator()
    sk77 = gen.skel77
    index = []
    for row in rows:
        frames = int(round(float(row["seconds"]) * kc.FPS))
        if not 2 <= frames <= kc.MAX_FRAMES:
            print(f"skip {row['name']}: {row['seconds']} s is outside 0.1-10 s", flush=True)
            continue
        for seed in seeds:
            stem = f"{row['name']}_s{seed}"
            index.append({"name": stem, "type": row["type"], "seconds": row["seconds"], "prompt": row["prompt"]})
            path = args.out / f"{stem}.bvh"
            if path.exists():
                continue
            seed_everything(seed)
            out = gen.model([row["prompt"]], [frames], num_denoising_steps=kc.STEPS, num_samples=1,
                            multi_prompt=True, post_processing=True, return_numpy=True, progress_bar=kc._quiet)
            pos = torch.from_numpy(out["posed_joints"][0]).to(gen.device)
            grot = torch.from_numpy(out["global_rot_mats"][0]).to(gen.device)
            save_motion_bvh(path, global_rots_to_local_rots(grot, sk77), pos[:, sk77.root_idx, :],
                            skeleton=sk77, fps=kc.FPS)
            print(f"{stem}: {row['prompt']}", flush=True)
    # the index covers every clip in the folder, from this run and earlier ones
    old = {}
    idx_path = args.out / "prompts.tsv"
    if idx_path.exists():
        with open(idx_path, encoding="utf-8-sig", newline="") as fh:
            old = {r["name"]: r for r in csv.DictReader(fh, delimiter="\t")}
    old.update({r["name"]: r for r in index})
    with open(idx_path, "w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=["name", "type", "seconds", "prompt"], delimiter="\t", lineterminator="\n")
        w.writeheader()
        for name in sorted(old):
            w.writerow({k: old[name][k] for k in ("name", "type", "seconds", "prompt")})
    print(f"{len(index)} clips in {args.out}")


if __name__ == "__main__":
    main()
