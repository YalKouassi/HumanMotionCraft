"""Target pose sets for live motion control (roadmap 158000): ac_train.posetargets, the shipped set,
and the helper-side session in tools/posefill/live.py that reads it."""

import importlib.util
import json
import math
import pathlib
import sys

import numpy as np
import pytest

import ac_train
from ac_train import ASSETS, quat, retarget
from ac_train import posetargets as T
from ac_train.rig import Skeleton

# live.py runs in the install beside `ac_rig`, the setup's copy of these modules
sys.modules.setdefault("ac_rig", ac_train)
sys.modules.setdefault("ac_rig.quat", quat)
sys.modules.setdefault("ac_rig.retarget", retarget)
_spec = importlib.util.spec_from_file_location("posefill_live", pathlib.Path(__file__).resolve().parents[2] / "tools" / "posefill" / "live.py")
live = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(live)

SKEL = Skeleton.load()
BONES = len(SKEL.names)


def identity(frames=1):
    q = np.zeros((frames, BONES, 4))
    q[..., 3] = 1.0
    return q


def cand(gait="run", set_name="cmu", take="a", frame=0, score=0.0, lean=0.0, height=0.8):
    return T.Candidate(gait, set_name, take, "", frame, 3.6, score, lean, height, identity(4), np.zeros((4, 3)))


# -- choosing ---------------------------------------------------------------------------------------


def test_among_the_same_speed_the_most_typical_posture_wins():
    bent = cand(take="a", lean=27.0, height=0.76)  # the set's order and the take's name both favour it
    typical = cand(take="b", lean=10.0, height=0.80)
    others = [cand(take=f"c{i}", lean=10.0 + i, height=0.80, score=0.5) for i in range(5)]
    assert T.ranked([bent, typical, *others], ("cmu",))[0] is typical


def test_a_speed_nearer_the_gaits_beats_a_better_posture():
    near = cand(take="a", lean=20.0, score=0.0)
    far = cand(take="b", lean=10.0, score=0.3)
    other = cand(take="c", lean=10.0, score=0.3)
    assert T.ranked([far, other, near], ("cmu",))[0] is near


def test_ties_go_by_set_order_then_take_then_frame():
    a, b, c = cand("walk", "cmu", "z"), cand("walk", "100style", "z"), cand("walk", "100style", "a", frame=9)
    d = cand("walk", "100style", "a", frame=2)
    assert T.ranked([a, b, c, d], ("100style", "cmu")) == [d, c, b, a]
    assert T.best([a, b, c, d], ("100style", "cmu")) == {"walk": d}
    assert T.ranked([], ("cmu",)) == []


# -- the maths --------------------------------------------------------------------------------------


def test_path_of_is_the_rigs_forward_kinematics():
    rng = np.random.default_rng(3)
    rots = quat.normalize(identity(3) + rng.normal(0.0, 0.2, (3, BONES, 4)))
    roots = rng.normal(0.0, 0.2, (3, 3))
    for bone in ("root", "neck01", "foot.L"):
        got = T.path_of(SKEL, rots, roots, bone)
        for f in range(3):
            assert got[f] == pytest.approx(SKEL.fk(rots[f], roots[f])[1][SKEL.index[bone]], abs=1e-9)


def test_normalised_sends_the_heading_along_minus_y_from_the_origin():
    rots = identity(4)
    start = T.hips_path(SKEL, np.zeros((1, 3)))[0]
    # hips at (1, 2) heading +X at 0.05 m a frame
    P = start + np.array([[1.0 + 0.05 * k, 2.0, 0.0] for k in range(4)]) - np.array([start[0], start[1], 0.0])
    roots = (P - SKEL.heads[SKEL.index["root"]][None]) / SKEL.hip_height
    r, o = T.normalised(SKEL, rots, roots, np.array([1.0, 0.0]))
    out = T.hips_path(SKEL, o)
    assert out[:, 0] == pytest.approx(np.zeros(4), abs=1e-9)
    assert out[:, 1] == pytest.approx(np.array([0.0, -0.05, -0.10, -0.15]), abs=1e-9)
    assert out[:, 2] == pytest.approx(P[:, 2], abs=1e-9)
    # the root turned by -90 degrees about the vertical, nothing else moved
    assert r[0, 0] == pytest.approx([0.0, 0.0, -math.sqrt(0.5), math.sqrt(0.5)], abs=1e-9)
    assert r[0, 1:] == pytest.approx(rots[0, 1:])


# -- the shipped set --------------------------------------------------------------------------------


def shipped():
    return json.loads((ASSETS / "posefill-targets" / "locomotion.json").read_text(encoding="utf-8"))


def test_the_shipped_set_reads_and_its_targets_are_where_the_format_says():
    doc = shipped()
    assert doc["format"] == T.FORMAT and doc["version"] == T.VERSION and doc["bones"] == BONES
    assert live.Targets(doc, BONES).names == ["stand", "walk", "jog", "run"]
    for t in doc["targets"]:
        assert len(t["poses"]) == T.POSES
        P = T.hips_path(SKEL, np.array([p["root"] for p in t["poses"]]))
        assert P[0, :2] == pytest.approx([0.0, 0.0], abs=1e-5)
        if t["name"] != "stand":
            assert P[-1, 1] < -0.05, "the motion goes along -Y"
            assert abs(P[-1, 0]) < 0.05


def test_the_shipped_set_comes_from_free_sets_only_and_no_target_is_bent_over():
    for t in shipped()["targets"]:
        assert t["from"]["set"] in T.DEFAULT_SETS, "ATTRIBUTION.md names these; never the engine's sample clips"
        p = t["poses"][0]
        _, P = SKEL.fk(np.array(p["rotations"]), np.array(p["root"]))
        trunk = P[SKEL.index["neck01"]] - P[SKEL.index["root"]]
        assert math.degrees(math.acos(trunk[2] / np.linalg.norm(trunk))) < 15.0, t["name"]


# -- the helper's session ---------------------------------------------------------------------------


def group(speed, name):
    """Four identity poses whose hips move along -Y at ``speed`` m/s."""
    poses = []
    for k in range(4):
        root = (np.array([0.0, -speed * k / 30.0, 0.0])) / SKEL.hip_height
        poses.append({"rotations": identity()[0].tolist(), "root": root.tolist()})
    return {"name": name, "speed": speed, "poses": poses}


DOC = {"format": T.FORMAT, "version": 1, "bones": BONES, "targets": [group(3.0, "run"), group(0.0, "stand"), group(1.0, "walk")]}


def test_blend_takes_the_two_gaits_around_the_speed():
    t = live.Targets(DOC, BONES)
    assert t.names == ["stand", "walk", "run"] and t.max_speed == 3.0
    rots, roots, what = t.blend(2.0, None)
    assert what == "walk-run 0.50" and rots.shape == (4, BONES, 4)
    # halfway between 1/30 and 3/30 m a frame, in hip heights
    assert roots[3, 1] * SKEL.hip_height == pytest.approx(-3 * 2.0 / 30.0)
    assert t.blend(9.0, None)[2] == "run"
    assert t.blend(0.0, None)[2].startswith("stand")
    assert t.blend(0.5, "walk")[2] == "walk"
    with pytest.raises(ValueError, match="moonwalk"):
        t.blend(1.0, "moonwalk")


@pytest.mark.parametrize("doc, says", [
    ({"format": "other", "version": 1, "bones": BONES, "targets": []}, "not a target pose set"),
    ({"format": T.FORMAT, "version": 2, "bones": BONES, "targets": []}, "version 2"),
    ({"format": T.FORMAT, "version": 1, "bones": 3, "targets": []}, "3-bone"),
    ({"format": T.FORMAT, "version": 1, "bones": BONES, "targets": []}, "no targets"),
    ({"format": T.FORMAT, "version": 1, "bones": BONES, "targets": [{"name": "x", "speed": 1.0, "poses": []}]}, "4 poses"),
])
def test_a_bad_target_set_is_refused(doc, says):
    with pytest.raises(ValueError, match=says):
        live.Targets(doc, BONES)


def test_place_scales_the_travel_turns_it_and_stands_it_over_the_spot():
    rots, roots, _ = live.Targets(DOC, BONES).blend(0.5, "walk")  # 1 m/s along -Y
    # twice the speed, facing +X (a quarter turn left from -Y), first hips over (1, 2)
    r, o = live.place(SKEL, rots, roots, 2.0, np.array([1.0, 2.0]), np.array([1.0, 0.0]))
    P = live.hips(SKEL, o)
    assert P[:, 0] == pytest.approx([1.0 + 2.0 * k / 30.0 for k in range(4)])
    assert P[:, 1] == pytest.approx([2.0] * 4)
    assert P[:, 2] == pytest.approx(live.hips(SKEL, roots)[:, 2])
    assert r[0, 0] == pytest.approx([0.0, 0.0, math.sqrt(0.5), math.sqrt(0.5)])
    assert r[0, 1:] == pytest.approx(rots[0, 1:])


def test_place_leaves_a_standing_target_where_it_stands():
    rots, roots, _ = live.Targets(DOC, BONES).blend(0.0, "stand")
    _, o = live.place(SKEL, rots, roots, 3.0, np.array([1.0, 2.0]), np.array([0.0, -1.0]))
    P = live.hips(SKEL, o)
    assert P[:, :2] == pytest.approx(np.array([[1.0, 2.0]] * 4))


class StubBody:
    J, leg = 30, 1.0

    def rig(self, path):
        return SKEL, []

    def from_rig(self, skel, table, rotations, root, scale):
        return np.zeros((self.J, 7))


class StubEngine:
    body = StubBody()

    @staticmethod
    def leg_of(skel):
        return 1.0

    def generate(self, ctx, tgt, tokens, say=print):
        raise ValueError("the engine fell over")


def session():
    return live.Session(StubEngine(), "rig", DOC, (identity()[0], np.zeros(3)))


def test_shown_frames_join_the_history_and_the_range_is_checked():
    s = session()
    assert len(s.history) == live.CONTEXT
    s.shown(0)
    with pytest.raises(ValueError, match="no segment"):
        s.shown(1)
    s.segment = np.zeros((10, 30, 7))
    s.shown(9)
    assert len(s.history) == live.CONTEXT + 9
    with pytest.raises(ValueError, match="0 to 9"):
        s.shown(10)


@pytest.mark.parametrize("style, says", [(None, "fell over"), ("moonwalk", "moonwalk")])
def test_a_refused_step_keeps_the_history_so_a_retry_shows_no_frame_twice(style, says):
    s = session()
    s.segment = np.zeros((10, 30, 7))
    before = list(s.history)
    for _ in range(2):
        with pytest.raises(ValueError, match=says):
            s.next(5, np.array([0.0, -1.0]), np.array([0.0, -1.0]), style, live.TOKENS, say=lambda _: None)
        assert s.history == before
