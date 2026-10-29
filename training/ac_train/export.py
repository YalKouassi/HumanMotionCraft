"""Export a trained in-betweener for the app, and the goldens that pin the Rust port to it.

    python -m ac_train.export parts

Writes, under the repo:
- assets/models/<tag>/weights.bin   every tensor of the state dict, float32 little-endian, in order;
- assets/models/<tag>/weights.json  {name: {offset, shape}} (offset in floats) and the card;
- crates/ac-ai/tests/golden/<tag>-net.json   one forward pass on fixed inputs (the network alone);
- crates/ac-ai/tests/golden/<tag>-fill.json  :func:`ac_train.infer.fill` on a library walk (the pipeline).

The goldens pin the port to PyTorch numerically; nothing here runs at app time.

A model trained on a non-commercial set you added (``train --noncommercial``) is never written into the repo:
give ``--out`` outside it (your app's own models folder, ``<app data>/models/<tag>``) and
``--golden`` somewhere temporary.
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib

import numpy as np
import torch

from . import ASSETS, REPO, infer
from .clip import Clip
from .model import BONES, FEAT
from .rig import Skeleton

GOLDEN_CLIP = "Locomotion/Walk"  # a CC0 built-in loop, so the golden carries no dataset's terms


def write_weights(state: dict, card: dict, out: pathlib.Path) -> int:
    """``weights.bin`` and ``weights.json`` for a state dict under ``out``; returns the float count."""
    out.mkdir(parents=True, exist_ok=True)
    index, blobs, offset = {}, [], 0
    for name, t in state.items():
        a = t.detach().to(torch.float32).contiguous().cpu().numpy()
        index[name] = {"offset": offset, "shape": list(a.shape)}
        blobs.append(a.astype("<f4").tobytes())
        offset += a.size
    (out / "weights.bin").write_bytes(b"".join(blobs))
    (out / "weights.json").write_text(json.dumps({"format": "ac-inbetweener", "version": 1, "tensors": index, "card": card}, indent=1), encoding="utf-8", newline="\n")
    return offset


def read_weights(folder: pathlib.Path) -> tuple[dict[str, np.ndarray], dict]:
    """The tensors (name -> float32 array) and card of an exported model: what the app reads."""
    meta = json.loads((folder / "weights.json").read_text(encoding="utf-8"))
    flat = np.frombuffer((folder / "weights.bin").read_bytes(), dtype="<f4")
    tensors = {}
    for name, t in meta["tensors"].items():
        n = int(np.prod(t["shape"])) if t["shape"] else 1
        tensors[name] = flat[t["offset"]:t["offset"] + n].reshape(t["shape"])
    return tensors, meta["card"]


def export(tag: str, out: pathlib.Path | None = None, gold: pathlib.Path | None = None) -> None:
    """Write ``tag``'s weights to ``out`` (default ``assets/models/<tag>``) and its goldens to
    ``gold`` (default ``crates/ac-ai/tests/golden``) as ``<tag>-net.json`` and ``<tag>-fill.json``."""
    net, card = infer.load(tag)
    out = out or ASSETS / "models" / tag
    gold = gold or REPO / "crates" / "ac-ai" / "tests" / "golden"
    if not card.get("commercial"):
        inside = [p for p in (out, gold) if p.resolve().is_relative_to(REPO.resolve())]
        if inside:
            sets = ", ".join(s["name"] for s in card.get("sets", []) if not s.get("commercial"))
            raise SystemExit(f"{tag} was trained on non-commercial sets ({sets}): it stays out of the repo. "
                             f"Export it with --out <app data>/models/{tag} --golden <a temporary folder>")
    offset = write_weights(net.state_dict(), card, out)
    print(f"{tag}: {offset} floats -> {out}")

    gold.mkdir(parents=True, exist_ok=True)

    # 1. the network alone: smooth made-up inputs, a style, two given bones
    T, J = 16, len(BONES)
    f = lambda k: [[math.sin(0.37 * i + 0.11 * j + k) * 0.5 for j in range(FEAT)] for i in range(T)]  # noqa: E731
    known = torch.tensor([f(0.0)], dtype=torch.float32)
    interp = torch.tensor([f(0.3)], dtype=torch.float32)
    mask = torch.zeros(1, T, 1)
    mask[:, :10] = 1
    mask[:, -1] = 1
    ttg = ((torch.tensor(T - 1.0) - torch.arange(T)[None].float()) / 45).clamp(min=0)[..., None]
    style = min(2, len(card["styles"]) - 1) if card.get("styles") else None
    gmask = torch.zeros(1, J)
    gmask[0, [3, 7]] = 1
    with torch.no_grad():
        y = net(known, interp, mask, ttg, torch.tensor([style]) if style is not None else None, gmask if card.get("given") else None)
    (gold / f"{tag}-net.json").write_text(json.dumps({
        "T": T, "style": style, "given": [3, 7] if card.get("given") else [],
        "known": known[0].tolist(), "interp": interp[0].tolist(), "mask": mask[0, :, 0].tolist(), "ttg": ttg[0, :, 0].tolist(),
        "out": y[0].tolist(),
    }), encoding="utf-8", newline="\n")

    # 2. the pipeline: fill frames 21..38 of a library walk from its keys 20 and 39
    skel = Skeleton.load()
    clip = Clip.load(ASSETS / "library" / "animations" / (GOLDEN_CLIP + ".clip.json"))
    cases = []
    styles = card.get("styles", [])
    styled = "walk" if "walk" in styles else styles[1] if len(styles) > 1 else "any"
    for a, b, style, sf, af in [(20, 39, "any", 1.0, 1.0), (5, 12, styled, 1.5, 0.8)]:
        r, o = infer.fill(clip, a, b, skel, tag, style=style, style_force=sf, ai_force=af)
        cases.append({"a": a, "b": b, "style": style, "style_force": sf, "ai_force": af,
                      "rotations": np.round(r, 7).tolist(), "roots": np.round(o, 7).tolist()})
    (gold / f"{tag}-fill.json").write_text(json.dumps({"clip": GOLDEN_CLIP, "cases": cases}), encoding="utf-8", newline="\n")
    print(f"goldens written -> {gold}")


def main(argv=None):
    ap = argparse.ArgumentParser(description="Export a trained in-betweener to assets/models and write its goldens")
    ap.add_argument("tag", nargs="?", default="parts", help="training/data/models/<tag>")
    ap.add_argument("--out", type=pathlib.Path, help="weights folder (default assets/models/<tag>)")
    ap.add_argument("--golden", type=pathlib.Path, help="goldens folder (default crates/ac-ai/tests/golden)")
    args = ap.parse_args(argv)
    export(args.tag, args.out, args.golden)


if __name__ == "__main__":
    main()
