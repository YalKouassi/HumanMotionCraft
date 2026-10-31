"""ac_train.flag: the worst 1 % rule, the ranking and the flags file, on made-up takes (no model)."""

import json

import pytest

pytest.importorskip("torch")

from ac_train import flag  # noqa: E402


def take(id_, loss, worst=None, windows=10, worst_at=3):
    return {"id": id_, "label": "", "frames": 100, "windows": windows, "loss": loss, "worst": worst or loss,
            "worst_at": worst_at, "terms": {"pos": loss / 2, "feat": loss / 4}, "weighted": {"pos": loss, "feat": loss / 4}}


def test_the_worst_one_percent_at_least_one():
    assert flag.how_many(0) == 0
    assert flag.how_many(1) == 1
    assert flag.how_many(50) == 1
    assert flag.how_many(100) == 1
    assert flag.how_many(101) == 2  # rounded up: 1.01 takes
    assert flag.how_many(5243) == 53
    assert flag.how_many(10, share=0.5) == 5
    assert flag.how_many(3, share=1.0) == 3


def test_ranking_is_worst_first_and_stable():
    takes = [take("s/c", 0.3), take("s/a", 0.9), take("s/b", 0.9), take("s/d", 0.1, worst=5.0)]
    assert [t["id"] for t in flag.rank(takes, share=1.0)] == ["s/a", "s/b", "s/c", "s/d"]
    # the same whatever order they came in; one of four is the worst 1 % (at least one)
    assert [t["id"] for t in flag.rank(takes[::-1])] == ["s/a"]
    # by the worst window instead of the mean
    assert [t["id"] for t in flag.rank(takes, by="worst")] == ["s/d"]
    with pytest.raises(ValueError):
        flag.rank(takes, by="median")


def test_the_file_names_the_run_and_says_why(tmp_path):
    takes = [take(f"s/t{i}", 0.1 + 0.001 * i) for i in range(150)] + [take("s/bad", 0.8, worst=2.5, worst_at=42)]
    sets = {"s": {"takes": 151, "frames": 15100, "crc32": 1, "evaluated": 151, "skipped": 0, "windows": 1510}}
    rec = flag.record({"tag": "parts", "checkpoint": "best"}, sets, takes, gap=15, stride=1, share=0.01, by="loss", elapsed_s=12.34)
    assert rec["format"] == flag.FORMAT and rec["version"] == flag.VERSION
    assert (rec["gap"], rec["stride"], rec["share"], rec["by"]) == (15, 1, 0.01, "loss")
    assert rec["takes"] == 151 and rec["windows"] == 1510 and rec["elapsed_s"] == 12.3
    assert rec["median_loss"] == pytest.approx(0.175)
    # 1 % of 151 rounds up to 2: the bad take first, then the worst of the rest
    ids = [t["id"] for t in rec["flagged"]]
    assert ids == ["s/bad", "s/t149"]
    bad = rec["flagged"][0]
    assert bad["rank"] == 1 and bad["loss"] == 0.8
    assert "mean loss 0.800" in bad["why"] and "gap 15" in bad["why"] and "4.6x the median" in bad["why"]
    assert "worst 2.500 at frame 42" in bad["why"] and "mostly pos" in bad["why"]
    # written as JSON and read back the same; nothing else is written (never decisions.json)
    out = tmp_path / "rig" / "flags.json"
    flag.write(out, rec)
    assert json.loads(out.read_text(encoding="utf-8")) == json.loads(json.dumps(rec))
    assert sorted(p.name for p in tmp_path.rglob("*") if p.is_file()) == ["flags.json"]


def test_no_takes_flags_nothing():
    rec = flag.record({"tag": "parts", "checkpoint": "model"}, {}, [], 15, 1, 0.01, "loss", 0.0)
    assert rec["flagged"] == [] and rec["takes"] == 0 and rec["median_loss"] == 0.0 and rec["unusable"] == []


def test_a_take_with_a_loss_that_is_not_a_number_is_left_out_and_named():
    # a NaN sorts anywhere and json.dumps writes it as NaN, which no JSON reader (the review app) accepts
    takes = [take("s/a", 0.2), take("s/nan", float("nan")), take("s/inf", 0.1, worst=float("inf")), take("s/b", 0.3)]
    rec = flag.record({"tag": "parts", "checkpoint": "best"}, {}, takes, 15, 1, 0.01, "loss", 1.0)
    assert [t["id"] for t in rec["flagged"]] == ["s/b"]
    assert rec["takes"] == 2 and rec["unusable"] == ["s/nan", "s/inf"]
    assert "NaN" not in json.dumps(rec) and "Infinity" not in json.dumps(rec)


def test_the_digest_matches_the_review_apps():
    """The same numbers as crates/ac-review/tests/data/flags.json (its tiny set: walk_a 10 frames, jump_b 6)."""
    assert flag.digest(["walk_a", "jump_b"], [10, 6]) == 408843836
    assert flag.digest(["walk_a", "jump_b"], [11, 6]) == 3654330876
    assert flag.digest([], []) == 0
