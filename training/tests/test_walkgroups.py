import numpy as np
import pytest

pytest.importorskip("torch")

from ac_train import walkgroups as W  # noqa: E402


def test_first_change_finds_a_stop_and_ignores_a_blip():
    moving, still = np.full(60, 1.2), np.zeros(60)
    assert 55 <= W.first_change(np.concatenate([moving, still])) <= 65
    blip = moving.copy()
    blip[30:34] = 0.0  # a stride's dip, no stop
    assert W.first_change(blip) is None
    assert W.first_change(moving) is None


def test_kind():
    assert W.kind("backwards or sideways", 12) == W.OTHER_WAY
    assert W.kind("plain", 12) == W.CHANGES
    assert W.kind("plain", None) == W.FORWARDS


def test_groups_keep_performers_apart_and_ignore_take_numbers():
    a = W.group_key("cmu/walk_cmu_35_01", "walk", W.FORWARDS)
    assert a == W.group_key("cmu/walk_cmu_35_09", "walk", W.FORWARDS)
    assert a != W.group_key("cmu/walk_cmu_36_01", "walk", W.FORWARDS)
    assert a != W.group_key("cmu/walk_cmu_35_01", "walk", W.CHANGES)
    female = W.group_key("accad/walk_accad_female1_b03_walk1", "Female1 B03 Walk1", W.FORWARDS)
    male = W.group_key("accad/walk_accad_male1_b3_walk", "Male1 B3 Walk", W.FORWARDS)
    assert female != male
    assert female[2] == male[2] == "walk"


def test_pick_shows_the_most_unusual_take():
    centre = {m: (0.0, 1.0) for m in W.gaitcheck.MEASURES}
    calm = {m: 0.5 for m in W.gaitcheck.MEASURES}
    takes = [{"take": "a", "measures": calm, "minutes": 9.0}, {"take": "b", "measures": {**calm, "swing": 3.0}, "minutes": 1.0},
             {"take": "c", "measures": {}, "minutes": 20.0}]
    assert W.pick(takes, centre)["take"] == "b"


def test_span_centres_on_the_change_and_stays_on_clean_frames():
    allowed = np.arange(0, 245)
    start, length = W.span(300, 200, W.CHANGES, allowed)
    assert length == int(W.CHANGE_SECONDS * W.FPS)
    assert start == 200 - length // 2
    assert W.span(300, None, W.FORWARDS, np.array([], dtype=np.int64)) is None


def test_styled_descriptions_and_plain_ones():
    assert W.STYLED.search("Drunk Wait")
    assert W.STYLED.search("Old Man Wait")
    assert W.STYLED.search("SC trunksway4 segment 0")
    assert not W.STYLED.search("wait for bus")
    assert not W.STYLED.search("Female1 C02 Run To Stand")


def test_other_types_show_longer_idles_and_split_addbiomechanics_by_description():
    assert W.span(300, None, W.IDLE, np.arange(0, 245))[1] == int(W.CHANGE_SECONDS * W.FPS)
    assert W.span(300, None, W.JUMP, np.arange(0, 245))[1] == int(W.STEADY_SECONDS * W.FPS)
    a = W.group_key("addbiomechanics/jump_addbiomechanics_han2023_s001", "han2023 s001_split1: 20220513 balletsmalljump 1 segment 0", W.JUMP)
    b = W.group_key("addbiomechanics/jump_addbiomechanics_han2023_s002", "han2023 s002_split1: 20220520 balletsmalljump 2 segment 1", W.JUMP)
    c = W.group_key("addbiomechanics/jump_addbiomechanics_han2023_s002", "han2023 s002_split1: 20220520 countermovement 2 segment 1", W.JUMP)
    assert a == b != c
