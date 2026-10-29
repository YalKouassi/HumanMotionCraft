"""The training data scan: what it calls bad, and what it must not."""

import numpy as np
import pytest

from ac_train import quality as Q
from ac_train.datasets import leading_junk
from ac_train.rig import Skeleton

STAND = 0.9450101  # the app's standing root height offset (hip heights)


@pytest.fixture(scope="module")
def skel():
    return Skeleton.load()


def standing(skel, n=60):
    """Standing still as a person does: the hips sway a millimetre (exactly still is a dropout)."""
    rots = np.tile([0.0, 0.0, 0.0, 1.0], (n, len(skel.names), 1))
    roots = np.tile([0.0, 0.0, STAND], (n, 1))
    roots[:, 0] += 0.001 * np.sin(np.arange(n) / 5) / skel.hip_height
    return rots, roots


def turn(axis, deg):
    a = np.radians(deg) / 2
    return np.array([*(np.sin(a) * np.asarray(axis, float)), np.cos(a)])


def test_standing_is_clean_and_on_the_floor(skel):
    rots, roots = standing(skel)
    rec = Q.scan_take(skel, rots, roots)
    assert rec["cuts"] == [] and rec["reject"] == ""
    assert abs(rec["floor_m"]) < 0.005


def test_level_puts_a_sunk_take_on_the_floor(skel):
    rots, roots = standing(skel)
    roots[:, 2] -= 0.05 / skel.hip_height  # 5 cm into the floor
    assert Q.floor_of(Q.lowest(skel, rots, roots)) == pytest.approx(-0.05, abs=0.006)
    assert Q.floor_of(Q.lowest(skel, rots, Q.level(skel, rots, roots))) == pytest.approx(0.0, abs=1e-4)


def test_a_one_frame_spike_is_a_glitch_and_fast_motion_is_not(skel):
    rots, roots = standing(skel)
    i = skel.index["lowerarm01.R"]
    for f in range(len(rots)):  # a steady 30 deg a frame: fast, smooth
        rots[f, i] = turn([1, 0, 0], 30 * f)
    assert not Q.glitches(rots, roots, skel.hip_height).any()
    rots[30, i] = turn([0, 1, 0], 90)  # one frame thrown off
    bad = Q.glitches(rots, roots, skel.hip_height)
    assert bad[30] and bad.sum() <= 3


def test_a_sliding_foot_is_skate(skel):
    rots, roots = standing(skel, 40)
    roots[:, 1] = np.arange(40) * 0.03 / skel.hip_height  # 0.9 m/s, both feet flat on the floor
    rec = Q.scan_take(skel, rots, roots)
    assert any("skate" in c[2] for c in rec["cuts"])


def test_a_held_dropout_is_frozen(skel):
    rots, roots = standing(skel, 60)
    roots[:, 0] = 0.0  # not a hair of motion for 2 s
    assert any("frozen" in c[2] for c in Q.scan_take(skel, rots, roots)["cuts"])


def test_a_teleport_is_cut(skel):
    rots, roots = standing(skel)
    roots[30:, 0] += 2.0 / skel.hip_height
    rec = Q.scan_take(skel, rots, roots)
    assert any("teleport" in w and a <= 30 <= b for a, b, w in rec["cuts"])


def test_a_mostly_bad_take_is_rejected(skel):
    rots, roots = standing(skel, 90)
    roots[:, 2] += 0.5 / skel.hip_height  # floating half a metre the whole time
    rec = Q.scan_take(skel, rots, roots)
    assert rec["reject"]


def test_a_setup_frame_at_the_start_is_dropped():
    hips = np.zeros((10, 3))
    hips[1:] += [3.0, 0.0, 0.0]  # frame 0 at the origin, the take 3 m away
    assert leading_junk(hips) == 1
    assert leading_junk(np.cumsum(np.full((10, 3), 0.01), 0)) == 0
