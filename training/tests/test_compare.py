"""ac_train.compare: model cards side by side."""

import json

import pytest

from ac_train import compare as C


def ev(joint, slide, windows=100, types=("walk",)):
    row = {"windows": windows, "model": {"joint_m": joint, "slide_ms": slide}, "interp": {"joint_m": 2 * joint, "slide_ms": 2 * slide}}
    return {"bins": {"1-4": row, "5-10": row}, "all": row, "by_type": {t: row for t in types}}


def test_cards_are_read_by_tag_checkpoint_and_subset(tmp_path):
    (tmp_path / "a").mkdir()
    card = {"evaluation": ev(0.01, 0.05), "evaluation_best": ev(0.02, 0.06), "evaluation_best_natural": ev(0.03, 0.07)}
    (tmp_path / "a" / "model.json").write_text(json.dumps(card), encoding="utf-8")
    assert C.load("a", tmp_path) == ("a:model", card["evaluation"])
    assert C.load("a:best", tmp_path) == ("a:best", card["evaluation_best"])
    assert C.load("a:best:natural", tmp_path) == ("a:best:natural", card["evaluation_best_natural"])
    with pytest.raises(SystemExit, match="has no evaluation_natural: run python -m ac_train.evaluate --tag a --subset natural"):
        C.load("a:model:natural", tmp_path)
    with pytest.raises(SystemExit, match="no model of that tag"):
        C.load("zz", tmp_path)


def test_the_table_has_a_row_per_bin_and_type_and_a_pair_of_columns_per_card():
    text = C.table([("new:best:natural", ev(0.02, 0.05, types=("walk", "run"))), ("old:best:natural", ev(0.03, 0.04, 80))])
    lines = text.splitlines()
    assert lines[0].split() == ["new:best:natural", "new:best:natural", "old:best:natural", "old:best:natural"]
    names = [ln.split("  ")[0].strip() for ln in lines[2:-1]]
    assert names == ["gap 1-4", "gap 5-10", "all", "type run", "type walk"]
    walk = next(ln for ln in lines if ln.startswith("type walk"))
    assert "20 / 40 (100)" in walk and "0.05 / 0.1 (100)" in walk and "30 / 60 (80)" in walk
    run = next(ln for ln in lines if ln.startswith("type run"))
    assert run.rstrip().endswith("-") and "20 / 40 (100)" in run  # the old card has no run row
