"""tools/community_check.py (roadmap 150000): the app's checks run on the clip in one script, a
refusal names the frame and the joints, the download's caps and its hash come before any parse,
and a problem of the checker's own leaves a clip pending instead of failing it."""

import gzip
import hashlib
import io
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[2] / "tools"))

import community_check as cc  # noqa: E402


def test_the_checks_open_the_clip_then_measure_the_same_document():
    # two `ac run` calls would be two sessions, and clip.possible would measure an empty document
    steps = cc.script_steps()
    assert [s["command"] for s in steps] == ["clip.open", "clip.possible"]
    assert steps[0]["params"] == {"id": cc.CLIP_ID}


def test_an_impossible_pose_names_the_frame_and_the_joints():
    lines = [
        {"command": "clip.open", "result": {"id": cc.CLIP_ID}},
        {"command": "clip.possible", "result": {
            "level": "impossible", "impossible": {"worst_frame": 12},
            "breaches": [{"joint": "knee.L", "past_deg": 30}, {"joint": "ankle.R"}, {"joint": "knee.L"}],
        }},
    ]
    with pytest.raises(ValueError, match=r"impossible poses on frame 12 \(knee.L, ankle.R\)"):
        cc.judge(lines)
    cc.judge([{"command": "clip.possible", "result": {"level": "unusual"}}])
    # no result from the app is our problem, not the clip's
    with pytest.raises(cc.CheckerProblem):
        cc.judge([{"command": "clip.open", "result": {}}])
    with pytest.raises(cc.CheckerProblem):
        cc.judge([{"command": "clip.possible", "result": {"frames": 3}}])


def test_the_download_is_capped_and_must_be_a_bounded_gzip():
    assert cc.read_capped(io.BytesIO(b"x" * 100), 100) == b"x" * 100
    with pytest.raises(ValueError, match="larger than 100 bytes"):
        cc.read_capped(io.BytesIO(b"x" * 101), 100)
    with pytest.raises(ValueError, match="not gzipped"):
        cc.inflate(b'{"format": "x.clip"}')
    small = gzip.compress(b"a" * 200)
    assert cc.inflate(small, most=200) == b"a" * 200
    with pytest.raises(ValueError, match="larger than 100 bytes unzipped"):
        cc.inflate(small, most=100)
    with pytest.raises(ValueError, match="cut short"):
        cc.inflate(small[:-8])
    with pytest.raises(ValueError, match="not a valid gzip"):
        cc.inflate(b"\x1f\x8b" + b"\xff" * 20)


def test_the_hash_is_checked_before_anything_parses():
    raw = b"not gzip, not json"
    cc.verify_sha256(raw, hashlib.sha256(raw).hexdigest().upper())
    with pytest.raises(ValueError, match="sha256 mismatch"):
        cc.verify_sha256(raw, "00" * 32)
    # check_item: the bytes don't match the record, so the gzip and JSON never get a look
    item = {"id": "c1", "url": "u", "sha256": "00" * 32}
    cc_download, cc_check = cc.download, cc.check_clip_text
    cc.download = lambda url, most=cc.MAX_GZ: raw
    cc.check_clip_text = lambda *a, **k: pytest.fail("the app ran on bytes whose hash is wrong")
    try:
        with pytest.raises(ValueError, match="sha256 mismatch"):
            cc.check_item(item, pathlib.Path("ac"), pathlib.Path("assets"))
    finally:
        cc.download, cc.check_clip_text = cc_download, cc_check


def test_a_problem_of_the_checkers_own_leaves_the_clip_pending():
    posted = []

    def post(verdict):
        posted.append(verdict)
        return {"status": "published" if verdict["ok"] else "failed"}

    def check(item):
        if item["id"] == "bad":
            raise ValueError("the clip has impossible poses on frame 3")
        if item["id"] == "offline":
            raise cc.CheckerProblem("could not download the file")

    clips = [
        {"id": "good", "url": "u", "sha256": "ab"},
        {"id": "bad", "url": "u", "sha256": "ab"},
        {"id": "offline", "url": "u", "sha256": "ab"},
        {"id": "nohash", "url": "u"},
    ]
    assert cc.process(clips, post, check) == 2
    assert posted == [
        {"action": "result", "id": "good", "ok": True},
        {"action": "result", "id": "bad", "ok": False, "reason": "the clip has impossible poses on frame 3"},
    ]
    # a verdict that can't be posted is never turned into a refusal
    posted.clear()

    def post_down(verdict):
        posted.append(verdict)
        raise cc.CheckerProblem("HTTP 503 from the checker endpoint")

    assert cc.process(clips[:1], post_down, check) == 1
    assert posted == [{"action": "result", "id": "good", "ok": True}]


def test_a_missing_secret_stops_the_run(monkeypatch, capsys):
    monkeypatch.delenv("CHECKER_SECRET", raising=False)
    assert cc.main([]) == 1
    assert "CHECKER_SECRET" in capsys.readouterr().err
