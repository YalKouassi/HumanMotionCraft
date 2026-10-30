"""ac_train.split: validation holds out whole subjects, the same ones whatever else changes."""

from ac_train import split


def test_groups_are_subjects_and_sessions():
    assert split.group_of("cmu", "walk_cmu_02_01") == "02"
    assert split.group_of("cmu", "run_cmu_02_03") == "02"
    assert split.group_of("accad", "standing_accad_female1_a01_stand") == "female1_a"
    assert split.group_of("accad", "standing_accad_female1_a02_swayt2") == "female1_a"
    assert split.group_of("accad", "fight_accad_male2_e12_punch") == "male2_e"
    assert split.group_of("accad", "fight_accad_male2_extended_1") == "fight_accad_male2_extended_1"
    assert split.group_of("accad", "walk_accad_swagger") == "walk_accad_swagger"
    assert split.group_of("addbiomechanics", "walk_addbiomechanics_falisse2017_subject_0_gait_7_segment_0") == "falisse2017_subject_0"
    assert split.group_of("addbiomechanics", "walk_addbiomechanics_han2023_s001_split1_20220513_x_2_segment_0") == "han2023_s001"
    assert split.group_of("kimodo", "jump_kimodo_jump_side_s2") == "jump_kimodo_jump_side"
    assert split.group_of("100style", "run_100style_BR_001") == "run_100style_BR_001"


def test_a_subject_is_held_out_whole():
    takes = [f"walk_cmu_{s:02d}_{t:02d}" for s in range(1, 120) for t in range(1, 6)]
    held = {t for t in takes if split.held_out("cmu", t)}
    assert held, "some of 119 subjects are held out"
    for t in held:
        subject = split.group_of("cmu", t)
        assert all(split.held_out("cmu", u) for u in takes if split.group_of("cmu", u) == subject)


def test_the_split_stays_when_a_set_is_added_or_a_take_dropped():
    sets = {"cmu": [f"walk_cmu_{s:02d}_01" for s in range(1, 120)],
            "kimodo": [f"walk_kimodo_p{p}_s{s}" for p in range(40) for s in (1, 2, 3)]}
    before = split.holdout(sets)
    after = split.holdout({"accad": ["walk_accad_male1_a01_x"], **sets, "addbiomechanics": ["walk_addbiomechanics_x_subject1_t1"]})
    assert {k: after[k] for k in before} == before
    fewer = split.holdout({"cmu": sets["cmu"][::2], "kimodo": sets["kimodo"]})
    assert set(fewer["cmu"]) <= set(before["cmu"]) and fewer["kimodo"] == before["kimodo"]
