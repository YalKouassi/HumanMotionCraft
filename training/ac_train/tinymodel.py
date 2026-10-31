"""Tiny test models for the fixtures that choose a model (roadmap 155000).

    python -m ac_train.tinymodel            # writes fixtures/data/models/{tiny,stale}

A copy of the shipped weights is far too big for the repo, so the fixtures load these instead: the
smallest network the app's loader takes (one solved bone, width 8, one encoder layer), with every
weight zero but the output bias. The network then adds a fixed amount to the root's height on every
frame it fills, so a fill with it is the plain interpolation with the root raised by ``lift`` hip
heights: a number worked out by hand in the fixture, not copied from the program.

``tiny`` is a base model of its own (no styles, no contacts, no given mask). ``stale`` is the same kind
of network with the shipped model's bones, whose card names ``parts`` as its base with a hash
``parts`` never had, so the app's checks refuse it for that one reason and say so. Both are deterministic: ``tests/test_tinymodel.py`` checks
the committed files are byte for byte what this writes. Nothing here is trained; pure Python,
no torch.
"""

from __future__ import annotations

import json
import pathlib
import struct

from . import REPO

OUT = REPO / "fixtures" / "data" / "models"

BONES = ["root"]  # the first solved bone must be the root (the fill turns the window by it)
WIDTH = 8
HEADS = 2
FF = 8
CONTEXT = 10
MAX_GAP = 45
WINDOW = CONTEXT + MAX_GAP + 1


def parts_bones() -> list[str]:
    """The shipped model's solved bones (its card), so an add-on's weights have its shape."""
    card = json.loads((REPO / "assets" / "models" / "parts" / "weights.json").read_text(encoding="utf-8"))["card"]
    return list(card["bones"])


def tensors(bones: list[str], lift: float) -> list[tuple[str, list[int], list[float]]]:
    """Every tensor the loader reads, in file order: zeros but the output bias on the root's z."""
    feat = len(bones) * 6 + 3
    inp_w = feat * 2 + 1 + 1  # known, interp, mask, one time value; no given mask, no path

    def zeros(name: str, shape: list[int]):
        n = 1
        for s in shape:
            n *= s
        return (name, shape, [0.0] * n)

    out_bias = [0.0] * feat
    out_bias[len(bones) * 6 + 2] = lift
    p = "body.layers.0"
    return [
        zeros("pos", [WINDOW, WIDTH]),
        zeros("inp.weight", [WIDTH, inp_w]),
        zeros("inp.bias", [WIDTH]),
        zeros(f"{p}.self_attn.in_proj_weight", [3 * WIDTH, WIDTH]),
        zeros(f"{p}.self_attn.in_proj_bias", [3 * WIDTH]),
        zeros(f"{p}.self_attn.out_proj.weight", [WIDTH, WIDTH]),
        zeros(f"{p}.self_attn.out_proj.bias", [WIDTH]),
        zeros(f"{p}.linear1.weight", [FF, WIDTH]),
        zeros(f"{p}.linear1.bias", [FF]),
        zeros(f"{p}.linear2.weight", [WIDTH, FF]),
        zeros(f"{p}.linear2.bias", [WIDTH]),
        zeros(f"{p}.norm1.weight", [WIDTH]),
        zeros(f"{p}.norm1.bias", [WIDTH]),
        zeros(f"{p}.norm2.weight", [WIDTH]),
        zeros(f"{p}.norm2.bias", [WIDTH]),
        zeros("out.0.weight", [WIDTH]),
        zeros("out.0.bias", [WIDTH]),
        zeros("out.1.weight", [feat, WIDTH]),
        ("out.1.bias", [feat], out_bias),
    ]


def card(tag: str, bones: list[str], lift: float) -> dict:
    return {
        "tag": tag,
        "bones": bones,
        "context": CONTEXT,
        "max_gap": MAX_GAP,
        "heads": HEADS,
        "styles": [],
        "commercial": True,
        "lift": lift,
        "note": (
            "A test model for the fixtures that choose a model (roadmap 155000), written by "
            "python -m ac_train.tinymodel: every weight is zero but the output bias, so a fill is "
            "the plain interpolation with the root raised by `lift` hip heights on every filled "
            "frame. Not trained on anything."
        ),
    }


def models() -> dict[str, dict]:
    """The cards: ``tiny`` on its own, ``stale`` with the shipped model's bones and a base hash it never had."""
    return {
        "tiny": dict(card("tiny", BONES, 0.25), name="Tiny test model"),
        "stale": dict(
            card("stale", parts_bones(), 0.5),
            name="Stale add-on (test)",
            base="parts",
            base_hash="0000000000000000",
        ),
    }


def files(tag: str) -> tuple[bytes, bytes]:
    """``weights.json`` and ``weights.bin`` for ``tag``, as bytes (the format of ac_train.export)."""
    c = models()[tag]
    index, blobs, offset = {}, [], 0
    for name, shape, values in tensors(c["bones"], c["lift"]):
        index[name] = {"offset": offset, "shape": shape}
        blobs.append(struct.pack(f"<{len(values)}f", *values))
        offset += len(values)
    meta = {"format": "ac-inbetweener", "version": 1, "tensors": index, "card": c}
    return json.dumps(meta, indent=1).encode("utf-8"), b"".join(blobs)


def fnv1a(*parts: bytes) -> str:
    """FNV-1a, 64 bits, over the parts in order, as 16 hex digits: what ``session.info`` reports."""
    h = 0xCBF29CE484222325
    for part in parts:
        for b in part:
            h ^= b
            h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def write(out: pathlib.Path = OUT) -> dict[str, str]:
    """Write every model under ``out``; returns each tag's hash (``weights.json`` then ``weights.bin``)."""
    hashes = {}
    for tag in models():
        meta, blob = files(tag)
        (out / tag).mkdir(parents=True, exist_ok=True)
        (out / tag / "weights.json").write_bytes(meta)
        (out / tag / "weights.bin").write_bytes(blob)
        hashes[tag] = fnv1a(meta, blob)
    return hashes


if __name__ == "__main__":
    for tag, h in write().items():
        print(f"{tag}: {h} -> {OUT / tag}")
