"""ac_train.selection: the take filter by type and style word, and the explicit take list."""

import json

import numpy as np
import pytest

from ac_train import selection as S


def test_style_words_are_the_sets_style_and_the_tags_that_are_not_types_or_data_tags():
    assert S.style_words("100style", "Neutral: Normal walking") == ["natural"]
    assert S.style_words("100style", "Zombie: ...") == ["zombie"]
    # a pose or prop style is no model label, but it is a styled walk: its own name is the word
    assert S.style_words("100style", "Teapot: One arm on hip") == ["teapot"]
    assert S.style_words("cmu", "walk with arms out") == []
    assert S.style_words("cmu", "walk", ["Tired", "running", "range-of-motion", "duplicate"]) == ["tired"]
    assert S.style_words("100style", "Old: ...", ["elderly"]) == ["elderly"]


def test_the_natural_filter_keeps_natural_types_with_no_style_or_the_natural_one():
    sel = S.Selection(frozenset(S.NATURAL_TYPES), frozenset(S.NATURAL_STYLES))
    assert sel.why_out("cmu/walk_cmu_02_01", "walk", []) == ""
    assert sel.why_out("100style/walk_100style_FW_052", "walk", ["natural"]) == ""
    assert sel.why_out("100style/walk_100style_FW_001", "walk", ["zombie"]) == "style: zombie"
    assert sel.why_out("100style/walk_100style_FW_088", "walk", ["teapot"]) == "style: teapot"
    assert sel.why_out("cmu/fight_cmu_02_05", "combat", []) == "type: combat"
    assert sel.why_out("cmu/any_cmu_29_25", "", []) == "type: untyped"
    assert sel.active() and "walk" in sel.describe() and "natural" in sel.describe()
    assert not S.Selection().active()


def test_styles_none_keeps_unstyled_takes_only():
    sel = S.Selection(None, frozenset())
    assert sel.why_out("a/b", "walk", []) == "" and sel.why_out("a/c", "dance", []) == ""
    assert sel.why_out("a/d", "walk", ["natural"]) == "style: natural"


def test_an_include_list_replaces_the_filters_and_exclude_wins():
    sel = S.Selection(frozenset(S.NATURAL_TYPES), frozenset(S.NATURAL_STYLES),
                      include=frozenset({"cmu/a", "cmu/b"}), exclude=frozenset({"cmu/b"}), takes_file="takes.json")
    assert sel.why_out("cmu/a", "combat", ["zombie"]) == ""  # listed: the type and style filters don't apply
    assert sel.why_out("cmu/b", "walk", []) == "list: excluded"
    assert sel.why_out("cmu/c", "walk", []) == "list: not included"
    only = S.Selection(exclude=frozenset({"cmu/b"}))
    assert only.why_out("cmu/c", "walk", []) == "" and only.why_out("cmu/b", "walk", []) == "list: excluded"
    assert "2 takes listed in takes.json" in sel.describe() and "don't apply" in sel.describe()


def test_the_take_list_file_is_checked(tmp_path):
    p = tmp_path / "takes.json"
    p.write_text(json.dumps({"include": ["cmu/a"], "exclude": ["cmu/b"], "note": "x"}), encoding="utf-8")
    assert S.load_takes(p) == (frozenset({"cmu/a"}), frozenset({"cmu/b"}))
    p.write_text(json.dumps({"include": ["cmu/a"], "exclude": ["cmu/a"]}), encoding="utf-8")
    with pytest.raises(SystemExit, match="both included and excluded"):
        S.load_takes(p)
    p.write_text(json.dumps({"include": ["no-slash"]}), encoding="utf-8")
    with pytest.raises(SystemExit, match="set/take ids"):
        S.load_takes(p)
    p.write_text(json.dumps({"takes": []}), encoding="utf-8")
    with pytest.raises(SystemExit, match="a take list is"):
        S.load_takes(p)
    p.write_text("{", encoding="utf-8")
    with pytest.raises(SystemExit):
        S.load_takes(p)


def test_the_arguments_build_the_selection(tmp_path):
    import argparse

    ap = argparse.ArgumentParser()
    S.add_arguments(ap)
    sel = S.from_args(ap.parse_args(["--natural"]))
    assert sel.types == frozenset(S.NATURAL_TYPES) and sel.styles == frozenset(S.NATURAL_STYLES)
    sel = S.from_args(ap.parse_args(["--types", "walk, Run", "--styles", "none"]))
    assert sel.types == frozenset({"walk", "run"}) and sel.styles == frozenset()
    sel = S.from_args(ap.parse_args(["--natural", "--styles", "elderly"]))
    assert sel.styles == frozenset({"natural", "elderly"})
    assert not S.from_args(ap.parse_args([])).active()
    p = tmp_path / "t.json"
    p.write_text(json.dumps({"include": [], "exclude": ["cmu/x"]}), encoding="utf-8")
    sel = S.from_args(ap.parse_args(["--takes", str(p)]))
    assert sel.exclude == frozenset({"cmu/x"}) and not sel.include and sel.takes_file == str(p)


class Npz(dict):
    """Enough of an npz for :func:`selection.records`: ``files`` and item access."""

    @property
    def files(self):
        return list(self)


def fake_set():
    takes = ["walk_s_01", "walk_s_02", "fight_s_03", "walk_s_04", "stand_s_05"]
    frames = [100, 60, 80, 90, 40]
    starts = np.cumsum([0, *frames[:-1]])
    labels = ["Neutral: normal", "Zombie: dead", "Neutral: x", "Neutral: x", "Neutral: x"]
    npz = Npz(starts=np.array(starts), takes=np.array(takes), labels=np.array(labels),
              roots=np.zeros((sum(frames), 3), np.float32))
    scan = {"set": "100style", "takes": [{"take": n, "frames": f, "cuts": [[0, 9, "glitch"]] if n == "walk_s_01" else [],
                                   "reject": "float" if n == "walk_s_04" else ""} for n, f in zip(takes, frames)]}
    return npz, scan


def test_records_say_what_each_take_is_and_why_it_is_out(monkeypatch):
    from ac_train import review, stamp

    npz, scan = fake_set()
    monkeypatch.setattr(review, "load_scan", lambda name, rig=None: scan)
    monkeypatch.setattr(stamp, "check", lambda name, d: [])
    decisions = {"100style/stand_s_05": {"status": "ok", "frames": 40, "type": "idle", "tags": ["nervous"]}}
    sel = S.Selection(frozenset(S.NATURAL_TYPES), frozenset(S.NATURAL_STYLES))
    _, recs = S.records("100style", npz, decisions, sel)
    by = {r.take: r for r in recs}
    assert by["walk_s_01"].kept and by["walk_s_01"].used == 90 and by["walk_s_01"].styles == ["natural"]
    assert by["walk_s_02"].why == "style: zombie"
    assert by["fight_s_03"].why == "type: combat" and by["fight_s_03"].type == "combat"
    assert by["walk_s_04"].why.startswith("left out: scan: float") and by["walk_s_04"].used == 0
    assert by["stand_s_05"].type == "idle" and by["stand_s_05"].why == "style: nervous"
    assert by["stand_s_05"].kind == "idle" and by["stand_s_05"].tags == ["nervous"]
    assert by["walk_s_01"].id == "100style/walk_s_01"
    # the review's rejection wins over an include list
    listed = S.Selection(include=frozenset({"100style/walk_s_04", "100style/walk_s_02"}))
    _, recs = S.records("100style", npz, {}, listed)
    by = {r.take: r for r in recs}
    assert by["walk_s_02"].kept and by["walk_s_04"].why.startswith("left out")
    assert by["walk_s_01"].why == "list: not included"


def test_a_listed_take_no_set_has_is_an_error(capsys):
    recs = [S.Record("s", "a", "", "walk", [], "", [], 10, 10, False, ""),
            S.Record("s", "b", "", "walk", [], "", [], 10, 0, False, "left out: scan: float")]
    with pytest.raises(SystemExit, match="takes no read set has"):
        S.check_listed(S.Selection(exclude=frozenset({"s/zz"}), takes_file="t.json"), recs, ["s"])
    S.check_listed(S.Selection(include=frozenset({"s/a", "s/b", "other/q"})), recs, ["s"])
    out = capsys.readouterr().out
    assert "sets not read: other" in out and "rejected by the scan or the review: s/b" in out


def test_the_minutes_table_counts_kept_train_and_held_out_minutes_by_type_and_set():
    m = 30 * 60
    recs = [S.Record("cmu", "a", "", "walk", [], "", [], 2 * m, 2 * m, False, ""),
            S.Record("cmu", "b", "", "walk", [], "", [], m, m, True, ""),
            S.Record("accad", "c", "", "idle", [], "", [], m, m, False, ""),
            S.Record("cmu", "d", "", "combat", [], "", [], 3 * m, 3 * m, False, "type: combat"),
            S.Record("cmu", "e", "", "walk", ["zombie"], "", [], m, m, False, "style: zombie"),
            S.Record("cmu", "f", "", "walk", [], "", [], m, 0, False, "left out: scan: float")]
    d = S.minutes(recs)
    assert d["train"] == {"walk": {"cmu": 2.0}, "idle": {"accad": 1.0}}
    assert d["held"] == {"walk": 1.0}
    assert d["out"] == {"type": 3.0, "style": 1.0, "left out": 1.0}
    assert d["takes"] == {"walk": 2, "idle": 1}
    text = S.table(recs, S.Selection(frozenset({"walk", "idle"})))
    lines = text.splitlines()
    assert lines[0].startswith("takes read: types idle, walk")
    assert lines[1].split() == ["type", "takes", "train", "min", "accad", "cmu", "held", "out"]
    assert lines[2].split() == ["idle", "1", "1.0", "1.0", "0.0", "0.0"]
    assert lines[3].split() == ["walk", "2", "2.0", "0.0", "2.0", "1.0"]
    assert lines[4].split() == ["all", "3", "3.0", "1.0", "2.0", "1.0"]
    assert lines[5] == "left out (minutes): type 3.0, style 1.0, left out 1.0"
