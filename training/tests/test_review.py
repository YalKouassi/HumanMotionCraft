"""What training may use of each take: the scan's verdict, the review's decisions over it."""

import json

import numpy as np
import pytest

from ac_train import review as R


def scan(*takes):
    return {"set": "s", "takes": [{"take": n, "frames": f, "cuts": c, "reject": rej} for n, f, c, rej in takes]}


SCAN = scan(("walk", 100, [[10, 12, "glitch"]], ""), ("fall", 50, [[0, 40, "float"]], "82% cut"), ("run", 80, [], ""))


def by_name(plan):
    return {t.name: t for t in plan.takes}


def test_with_no_decisions_the_scan_decides():
    p = by_name(R.plan("s", ["walk", "fall", "run"], [100, 50, 80], {}, SCAN))
    assert p["walk"].cuts == [(10, 12)] and not p["walk"].drop
    assert p["fall"].drop.startswith("scan: 82% cut")
    assert p["run"].cuts == [] and not p["run"].drop


def test_a_decision_wins_over_the_scan():
    d = {
        "s/walk": {"status": "ok", "frames": 100, "by": "person"},
        "s/fall": {"status": "cut", "cuts": [[0, 5]], "frames": 50, "type": "jump"},
        "s/run": {"status": "reject", "frames": 80, "by": "agent", "note": "retarget bug"},
    }
    plan = R.plan("s", ["walk", "fall", "run"], [100, 50, 80], d, SCAN)
    p = by_name(plan)
    assert p["walk"].cuts == [] and not p["walk"].drop
    assert p["fall"].cuts == [(0, 5)] and not p["fall"].drop and p["fall"].kind == "jump"
    assert p["run"].drop == "rejected by agent: retarget bug"
    assert all(t.source == "decision" for t in plan.takes) and not plan.notes
    assert "2 of 3 takes used" in plan.summary() and "3 reviewed" in plan.summary()


def test_a_decision_on_another_length_of_take_is_set_aside_and_said():
    d = {"s/walk": {"status": "ok", "frames": 120}, "s/gone": {"status": "reject"}}
    plan = R.plan("s", ["walk", "fall", "run"], [100, 50, 80], d, SCAN)
    assert by_name(plan)["walk"].cuts == [(10, 12)]  # the scan's again
    assert any("different length" in n and "s/walk" in n for n in plan.notes)
    assert any("doesn't have" in n and "s/gone" in n for n in plan.notes)


def test_a_missing_or_stale_scan_is_an_error_unless_unscanned():
    with pytest.raises(SystemExit, match="not been scanned"):
        R.plan("s", ["walk"], [100], {}, None)
    with pytest.raises(SystemExit, match="rebuilt"):
        R.plan("s", ["walk", "fall", "run"], [100, 51, 80], {}, SCAN)
    p = R.plan("s", ["walk"], [100], {}, None, scanned=False)
    assert p.takes[0].cuts == [] and p.takes[0].source == "none"


def test_windows_keep_clear_of_every_cut():
    t = R.Take("walk", 100, cuts=[(10, 12), (50, 50)])
    s = R.starts(t, 20)
    assert len(s) and all(not t.bad()[a : a + 20].any() for a in s)
    # every clean window is there: 13..30 fit before frame 50, 51..80 after it
    assert list(s) == [*range(13, 31), *range(51, 81)]
    assert len(R.starts(R.Take("x", 100, drop="rejected"), 20)) == 0
    assert len(R.starts(R.Take("x", 10), 20)) == 0
    assert list(R.starts(R.Take("x", 20), 20)) == [0]


def test_the_decisions_file_reads_and_a_bad_one_stops(tmp_path):
    assert R.load_decisions(tmp_path / "none.json") == {}
    good = {"format": "x.review", "version": 1, "takes": {"s/walk": {"status": "cut", "cuts": [[1, 2]]}}}
    (tmp_path / "d.json").write_text(json.dumps(good))
    assert R.load_decisions(tmp_path / "d.json")["s/walk"]["cuts"] == [[1, 2]]
    for bad in ["{ broken", json.dumps({"format": "x.clip", "version": 1}),
                json.dumps({**good, "version": 2}), json.dumps({**good, "takes": {"s/a": {"status": "maybe"}}})]:
        (tmp_path / "b.json").write_text(bad)
        with pytest.raises(SystemExit):
            R.load_decisions(tmp_path / "b.json")


def test_the_checked_in_decisions_read():
    assert isinstance(R.load_decisions(), dict)


def test_a_set_as_stored_plans_by_its_starts(tmp_path):
    (tmp_path / "s.quality.json").write_text(json.dumps(SCAN))
    p = R.plan_set("s", 230, np.array([0, 100, 150]), np.array(["walk", "fall", "run"]), {}, rig=tmp_path)
    assert [t.frames for t in p.takes] == [100, 50, 80]
