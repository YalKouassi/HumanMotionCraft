"""The walk check (roadmap 153000): the words and the limit, by hand."""

import numpy as np
import pytest

from ac_train import gaitcheck as G
from ac_train.rig import Skeleton

SKEL = Skeleton.load()


@pytest.mark.parametrize(
    "label", ["walk", "Walking001", "SlowWalkRightAngleTurns", "walk forward, turn around, walk back", "walk/wander", "A person walks in a circle clockwise."]
)
def test_a_plain_description_has_no_odd_word(label):
    assert G.odd_word("cmu", label) == ""


@pytest.mark.parametrize(
    "label, word",
    [("walk/hobble", "hobble"), ("ScaredWalk", "Scared"), ("Mickey Walk", "Mickey"), ("walk with anger, frustration", "anger"), ("Mummy Walk", "Mummy")],
)
def test_any_other_word_says_it_is_a_way_of_walking(label, word):
    assert G.odd_word("cmu", label) == word


def test_100style_says_its_style_and_addbiomechanics_names_altered_gaits():
    assert G.odd_word("100style", "Neutral: Normal walking (forwards walk)") == ""
    assert G.odd_word("100style", "Zombie: Arms out") == "Zombie"
    assert G.odd_word("addbiomechanics", "fregly2012 5GC: jw ngait 1 segment 2") == ""
    assert G.odd_word("addbiomechanics", "fregly2012 5GC: jw tsgait 1 segment 2") == "tsgait"
    assert G.odd_word("addbiomechanics", "uhlrich2020 subject2: walkingTS3 segment 1") == "walkingTS"


def values(**over):
    out = dict.fromkeys(G.MEASURES, 0.0)
    return out | {"forwards": 1.0, "speed": 1.3} | over


def test_centre_is_the_median_and_the_scaled_median_deviation():
    rows = [values(hips=x) for x in (1.0, 2.0, 3.0, 4.0, 100.0)]
    mid, spread = G.centres(rows)["hips"]
    assert mid == 3.0
    assert spread == pytest.approx(1.4826)  # deviations 2, 1, 0, 1, 97: their median is 1


def test_a_measure_past_the_limit_stands_out_and_a_sidestep_keeps_its_uneven_steps():
    centre = {m: (0.0, 1.0) for m in G.MEASURES}
    assert G.flags(values(hips=G.LIMIT), centre) == []
    assert G.flags(values(hips=-5.0, swing=9.0), centre) == [("swing", 9.0), ("hips", -5.0)]
    assert G.flags(values(step_uneven=9.0), centre) == [("step_uneven", 9.0)]
    assert G.flags(values(step_uneven=9.0, forwards=0.2), centre) == []


def test_standing_still_has_too_little_walking_to_measure():
    frames = 120
    rots = np.tile(np.array([0.0, 0.0, 0.0, 1.0]), (frames, len(SKEL.names), 1))
    assert G.measure(SKEL, rots, np.zeros((frames, 3)), 30.0) is None


def test_a_body_gliding_forwards_measures_no_unevenness_and_goes_forwards():
    frames = 120
    rots = np.zeros((frames, len(SKEL.names), 4))
    rots[..., 3] = 1.0
    roots = np.zeros((frames, 3))
    roots[:, 1] = -np.arange(frames) * 1.3 / 30  # the rig faces -Y
    m = G.measure(SKEL, rots, roots, 30.0)
    assert m["forwards"] == pytest.approx(1.0)
    assert m["stance_uneven"] == pytest.approx(0.0)
    assert m["swing"] == pytest.approx(0.0, abs=1e-9)
    assert m["lean_side"] == pytest.approx(0.0, abs=1.0)
