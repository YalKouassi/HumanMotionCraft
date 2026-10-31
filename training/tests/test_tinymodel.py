"""The tiny test models under fixtures/data/models are byte for byte what ac_train.tinymodel writes."""

import json

from ac_train import tinymodel


def test_committed_tiny_models_match_the_generator(tmp_path):
    hashes = tinymodel.write(tmp_path)
    for tag in tinymodel.models():
        for name in ("weights.json", "weights.bin"):
            committed = (tinymodel.OUT / tag / name).read_bytes()
            assert committed == (tmp_path / tag / name).read_bytes(), (
                f"{tag}/{name}: regenerate with python -m ac_train.tinymodel"
            )
        assert len(committed) < 20_000
    assert hashes["tiny"] != hashes["stale"]


def test_the_stale_model_names_a_base_parts_never_had():
    meta = json.loads((tinymodel.OUT / "stale" / "weights.json").read_text(encoding="utf-8"))
    assert meta["card"]["base"] == "parts"
    assert meta["card"]["base_hash"] == "0000000000000000"
    tiny = json.loads((tinymodel.OUT / "tiny" / "weights.json").read_text(encoding="utf-8"))
    assert "base" not in tiny["card"]


def test_the_shapes_are_what_the_loader_needs():
    # the input layer takes known + interp (feat each), the mask and one time value
    meta, _ = tinymodel.files("tiny")
    index = json.loads(meta)["tensors"]
    feat = len(tinymodel.BONES) * 6 + 3
    assert index["inp.weight"]["shape"] == [tinymodel.WIDTH, feat * 2 + 2]
    assert index["pos"]["shape"] == [tinymodel.CONTEXT + tinymodel.MAX_GAP + 1, tinymodel.WIDTH]
    assert index["out.1.weight"]["shape"] == [feat, tinymodel.WIDTH]
    stale, _ = tinymodel.files("stale")
    assert json.loads(stale)["card"]["bones"] == tinymodel.parts_bones()
