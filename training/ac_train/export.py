"""Export a trained in-betweener for the app, and the goldens that pin the Rust port to it.

    python -m ac_train.export parts
    python -m ac_train.export parts-lowweights --checkpoint best --as parts   # a training run shipped under the app's tag

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
from .model import BONES, FEAT, PART_GROUPS
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


def export(tag: str, out: pathlib.Path | None = None, gold: pathlib.Path | None = None, checkpoint: str = "model",
           as_tag: str | None = None) -> None:
    """Write ``tag``'s weights to ``out`` (default ``assets/models/<tag>``) and its goldens to
    ``gold`` (default ``crates/ac-ai/tests/golden``) as ``<tag>-net.json`` and ``<tag>-fill.json``.
    ``checkpoint``: ``model`` (training's last step) or ``best`` (its best evaluation score).
    ``as_tag``: the tag the app sees (the card, the folder and the goldens' names) when a training
    run (``parts-lowweights``) ships as the app's model (``parts``, as ``assets/models/MANIFEST.json`` records)."""
    net, card = infer.load(tag, checkpoint)
    shipped = as_tag or tag
    card = {**card, "tag": shipped, "checkpoint": checkpoint}
    out = out or ASSETS / "models" / shipped
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

    # 1. the network alone: smooth made-up inputs, a style, two given bones (and a path)
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
    # a model with the root path: a made-up path on the gap frames
    path = torch.tensor([[[math.cos(0.21 * i + k) * 0.3 for k in range(3)] for i in range(T)]], dtype=torch.float32)
    pmask = 1 - mask
    with torch.no_grad():
        y, contacts = net(known, interp, mask, ttg, torch.tensor([style]) if style is not None else None,
                          gmask if card.get("given") else None, with_contacts=True,
                          path=path if card.get("path") else None, pmask=pmask if card.get("path") else None)
    (gold / f"{shipped}-net.json").write_text(json.dumps({
        "T": T, "style": style, "given": [3, 7] if card.get("given") else [],
        "known": known[0].tolist(), "interp": interp[0].tolist(), "mask": mask[0, :, 0].tolist(), "ttg": ttg[0, :, 0].tolist(),
        **({"path": path[0].tolist(), "pmask": pmask[0, :, 0].tolist()} if card.get("path") else {}),
        "out": y[0].tolist(),
        # the foot-contact logits (a model with the contact head)
        **({"contacts": contacts[0].tolist()} if contacts is not None else {}),
    }), encoding="utf-8", newline="\n")

    # 2. the pipeline: fills of a library walk (a 33-frame loop, so a long gap wraps through it)
    skel = Skeleton.load()
    clip = Clip.load(ASSETS / "library" / "animations" / (GOLDEN_CLIP + ".clip.json"))
    cases = []
    styles = card.get("styles", [])
    styled = "walk" if "walk" in styles else styles[1] if len(styles) > 1 else "any"
    # a part layer driving the left arm: the model's other bones are given, as the app builds the
    # list (crates/ac-core/src/ai.rs, AiFiller::fill)
    arm_layer = [b for i, b in enumerate(BONES) if i not in PART_GROUPS["left_arm"]] if card.get("given") else []
    for a, b, style, sf, af, tr, head, given in [
        (20, 39, "any", 1.0, 1.0, 0.0, 0.0, []),  # the plain fill, the gap past the loop's end
        (5, 12, styled, 1.5, 0.8, 0.0, 0.0, []),  # a style, pushed, the network's change scaled down
        (8, 30, "any", 1.0, 1.0, 1.0, 0.0, []),  # the root put on the keys' own path
        (10, 12, "any", 1.0, 1.0, 0.0, 0.0, []),  # a gap of 1: the shortest the model takes (127000)
        (4, 50, "any", 1.0, 1.0, 0.0, 0.0, []),  # a gap of 45: the longest, more than one lap of the loop
        (6, 22, "any", 1.0, 1.0, 0.0, 0.0, arm_layer),  # a part layer: every bone but the left arm given
        (14, 36, "any", 1.0, 1.0, 0.5, 0.0, []),  # half way to the keys' path
        (16, 40, "any", 1.0, 1.0, 0.0, 0.5, []),  # the neck and head half way back to the keys' spline
        (20, 39, "any", 1.0, 0.0, 0.0, 0.0, []),  # ai_force 0: the plain interpolation, exactly
        (0, 9, "any", 1.0, 1.0, 0.0, 0.0, []),  # a fill at the clip's start: the context repeats frame 0
    ]:
        if given and not card.get("given"):
            continue  # a model without the given mask cannot fill around a part layer
        r, o = infer.fill(clip, a, b, skel, tag, style=style, style_force=sf, ai_force=af, trajectory=tr, head=head,
                          given=given or None, checkpoint=checkpoint)
        cases.append({"a": a, "b": b, "style": style, "style_force": sf, "ai_force": af, "trajectory": tr, "head": head,
                      "given": given, "rotations": np.round(r, 7).tolist(), "roots": np.round(o, 7).tolist()})
    (gold / f"{shipped}-fill.json").write_text(json.dumps({"clip": GOLDEN_CLIP, "cases": cases}), encoding="utf-8", newline="\n")
    print(f"goldens written -> {gold}")


def main(argv=None):
    ap = argparse.ArgumentParser(description="Export a trained in-betweener to assets/models and write its goldens")
    ap.add_argument("tag", nargs="?", default="parts", help="training/data/models/<tag>")
    ap.add_argument("--out", type=pathlib.Path, help="weights folder (default assets/models/<tag>)")
    ap.add_argument("--golden", type=pathlib.Path, help="goldens folder (default crates/ac-ai/tests/golden)")
    ap.add_argument("--checkpoint", default="model", choices=("model", "best"),
                    help="model.pt (the last step) or best.pt (the best evaluation score; the card says which step)")
    ap.add_argument("--as", dest="as_tag", metavar="TAG",
                    help="ship under this tag (the card, assets/models/<TAG>, the goldens' names); default: the training tag")
    args = ap.parse_args(argv)
    export(args.tag, args.out, args.golden, args.checkpoint, args.as_tag)


if __name__ == "__main__":
    main()
