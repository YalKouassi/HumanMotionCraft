"""The helper's protocol on the fake engine (no GPU, no numpy):

    uv run --with pytest python -m pytest tools/posefill/tests        (from the repo root)
    uv run --with pytest python -m pytest ../tools/posefill/tests     (from training/)
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys

import pytest

HERE = pathlib.Path(__file__).resolve().parent
TOOL = HERE.parent
REPO = TOOL.parent.parent
RIG = str(REPO / "assets" / "rig" / "default_rig.json")
sys.path.insert(0, str(TOOL))

import fake_engine  # noqa: E402
import helper  # noqa: E402

BONES = len(json.loads(pathlib.Path(RIG).read_text(encoding="utf-8"))["names"])
REST = {"rotations": [[0.0, 0.0, 0.0, 1.0]] * BONES, "root": [0.0, 0.0, 0.94]}
UP = {"rotations": [[0.0, 0.0, 0.0, 1.0]] * BONES, "root": [0.0, 0.0, 1.94]}


def ask(requests: list[dict]) -> list[dict]:
    """Every reply to the requests, through `serve` on the fake engine."""
    out: list[dict] = []
    helper.serve([json.dumps(r) for r in requests], out.append, lambda say: fake_engine.Engine(say=say), fake_engine.describe)
    return out


def results(replies: list[dict], rid: int):
    return [r for r in replies if r.get("id") == rid and "result" in r]


def errors(replies: list[dict], rid: int):
    return [r["error"] for r in replies if r.get("id") == rid and "error" in r]


def test_info_before_and_after_a_load():
    replies = ask([{"id": 1, "op": "info"}, {"id": 2, "op": "load"}, {"id": 3, "op": "info"}])
    first = results(replies, 1)[0]["result"]
    assert first["protocol"] == helper.PROTOCOL and first["loaded"] is False and first["ops"] == ["info", "load", "fill"]
    assert first["min_frames"] == 18 and first["max_frames"] == 58 and first["context"] == 4
    assert results(replies, 2)[0]["result"]["loaded"] is True
    assert results(replies, 3)[0]["result"]["loaded"] is True
    # the load's progress line carries its id
    assert any(r.get("id") == 2 and "progress" in r for r in replies)


def test_a_fill_meets_both_keys_and_rounds_its_numbers():
    replies = ask([{"id": 7, "op": "fill", "rig": RIG, "frames": 31, "fps": 30, "context": [REST] * 4, "targets": [UP]}])
    res = results(replies, 7)[0]["result"]
    assert len(res["rotations"]) == 31 and len(res["rotations"][0]) == BONES and len(res["rotations"][0][0]) == 4
    assert len(res["roots"]) == 31
    assert res["roots"][0] == [0.0, 0.0, 0.94] and res["roots"][-1] == [0.0, 0.0, 1.94]
    assert res["roots"][15] == [0.0, 0.0, 1.44]
    assert res["notes"] == ["made by the fake engine, not the model"]
    assert res["model_frames"] == 31 and res["tokens"] == 9
    assert any(r.get("id") == 7 and r.get("progress") == "fake fill" for r in replies)


def test_a_short_context_is_padded_and_said():
    replies = ask([{"id": 1, "op": "fill", "rig": RIG, "frames": 31, "context": [REST], "targets": [UP]}])
    assert "1 context pose(s) given; the first was repeated to make 4" in results(replies, 1)[0]["result"]["notes"]


@pytest.mark.parametrize("frames", [2, 17, 59, 120])
def test_a_gap_outside_the_model_range_is_refused_with_the_limits(frames):
    replies = ask([{"id": 1, "op": "fill", "rig": RIG, "frames": frames, "context": [REST] * 4, "targets": [UP]}])
    (err,) = errors(replies, 1)
    assert "18 to 58" in err and str(frames) in err


def test_bad_requests_are_refused_and_the_helper_keeps_serving():
    bad = [
        {"id": 1, "op": "fly"},
        {"id": 2, "op": "fill", "rig": RIG, "frames": 31, "context": [], "targets": [UP]},
        {"id": 3, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 5, "targets": [UP]},
        {"id": 4, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [{"rotations": [[0, 0, 0, 1]] * 3, "root": [0, 0, 1]}]},
        {"id": 5, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [{"rotations": [[0, 0, 0, 1]] * BONES, "root": [0, 0]}]},
        {"id": 6, "op": "fill", "rig": RIG, "frames": "31", "context": [REST] * 4, "targets": [UP]},
        {"id": 8, "op": "fill", "rig": RIG, "frames": 31, "fps": 0, "context": [REST] * 4, "targets": [UP]},
        {"id": 9, "op": "fill", "frames": 31, "context": [REST] * 4, "targets": [UP]},
        {"id": 10, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [{"rotations": [[1e400, 0, 0, 1]] * BONES, "root": [0, 0, 1]}]},
    ]
    replies = ask(bad + [{"id": 11, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [UP]}])
    for req in bad:
        assert errors(replies, req["id"]), req
    assert "unknown op 'fly'" in errors(replies, 1)[0]
    assert "1 to 4 poses" in errors(replies, 2)[0] and "1 to 4 poses" in errors(replies, 3)[0]
    assert f"{BONES} finite rotations" in errors(replies, 4)[0]
    assert "finite root" in errors(replies, 5)[0]
    assert "frames" in errors(replies, 6)[0] and "fps" in errors(replies, 8)[0] and "rig" in errors(replies, 9)[0]
    assert results(replies, 11)


def test_non_finite_and_wrong_shape_engine_output_is_refused():
    nan = {"rotations": REST["rotations"], "root": [9.0, 9.0, 9.0]}
    short = {"rotations": REST["rotations"], "root": [8.0, 8.0, 8.0]}
    replies = ask([{"id": 1, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [nan]},
                   {"id": 2, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [short]}])
    assert "non-finite" in errors(replies, 1)[0]
    assert "wrong shape" in errors(replies, 2)[0] and "31 x" in errors(replies, 2)[0]


def test_a_line_that_is_not_json_is_an_error_without_an_id():
    out: list[dict] = []
    helper.serve(["not json", ""], out.append, lambda say: fake_engine.Engine(say=say), fake_engine.describe)
    assert out == [{"id": None, "error": out[0]["error"]}] and "JSONDecodeError" in out[0]["error"]


def test_the_process_speaks_json_lines_on_stdout_only():
    env = dict(os.environ, AC_POSEFILL_ENGINE=str(HERE / "fake_engine.py"), PYTHONUTF8="1")
    lines = "\n".join(json.dumps(r) for r in [
        {"id": 1, "op": "info"},
        {"id": 2, "op": "fill", "rig": RIG, "frames": 31, "context": [REST] * 4, "targets": [UP]},
    ]) + "\n"
    p = subprocess.run([sys.executable, "-I", str(TOOL / "helper.py")], input=lines, capture_output=True, text=True,
                       env=env, timeout=120, check=False)
    assert p.returncode == 0, p.stderr
    replies = [json.loads(line) for line in p.stdout.splitlines()]
    assert all(set(r) <= {"id", "result", "error", "progress"} for r in replies)
    assert results(replies, 1)[0]["result"]["model"] == "fake"
    assert results(replies, 2)[0]["result"]["roots"][-1] == [0.0, 0.0, 1.94]
    assert "fake engine ready" in [r.get("progress") for r in replies]
