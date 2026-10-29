"""The registry, and the retargeted sets when they have been built (``training/data/rig``)."""

import numpy as np
import pytest

from ac_train.datasets import REGISTRY, RIG
from ac_train.rig import Skeleton


def test_every_set_states_its_licence():
    for name, ds in REGISTRY.items():
        assert ds.name == name
        assert ds.licence and ds.title
        assert isinstance(ds.commercial, bool)
    # the repo supplies free-licence sets only, each one shareable (a non-commercial set is yours to add)
    for ds in REGISTRY.values():
        assert ds.commercial and "NC" not in ds.licence and ds.share == "yes", ds.name


def test_100style_takes_are_named_by_gait_not_style():
    from ac_train.datasets import style100_take

    index = {"Kick": {"number": 7, "description": "Kick out each leg", "cuts": {"FW": (300, 900), "ID": (400, 600)}}}
    take, label, cut = style100_take("Kick_FW", index)
    assert take == "walk_100style_FW_007"  # "Kick" would make it a combat take
    assert label == "Kick: Kick out each leg (forwards walk)"
    assert cut == (300, 900)
    assert style100_take("Kick_ID", index)[0] == "standing_100style_ID_007"
    assert style100_take("Kick_SR", index) is None  # no frame cut
    assert style100_take("Unknown_FW", index) is None


def test_kimodo_takes_take_their_type_from_the_index_not_the_prompt():
    from ac_train.datasets import kimodo_take
    from ac_train.library import classify

    index = {"carry_box": {"type": "carry", "prompt": "A person picks up a box from the floor and carries it."},
             "untyped": {"type": "", "prompt": "A person waves."}}
    take, label = kimodo_take("carry_box", index)
    assert take == "carry_kimodo_carry_box"
    assert label == "A person picks up a box from the floor and carries it."
    assert classify(take) == ("Interaction", "Carry")  # "from the floor" would make it a floor take
    assert kimodo_take("untyped", index) is None
    assert kimodo_take("missing", index) is None
    # nor does the prompt's name: "run" in it would make a stop a run
    assert classify("braking_kimodo_transition_run_to_stop_s1") == ("Locomotion", "Stop")


def test_accad_takes_are_named_and_classified():
    from ac_train import labels as L
    from ac_train.datasets import accad_take
    from ac_train.library import classify

    take, label = accad_take("Female1_C21_RunToPickUpBox")
    assert take == "carry_accad_female1_c21_runtopickupbox"
    assert label == "Female1 C21 Run To Pick Up Box"
    assert L.type_of(take) == "carry"  # "box" in the stem would otherwise match combat
    assert classify(take) == ("Interaction", "Carry")  # "run" in the stem would otherwise match Run

    take, _ = accad_take("Male2_G17_PushKickLeft")
    assert take == "fight_accad_male2_g17_pushkickleft"
    assert L.type_of(take) == "combat"  # "push" in the stem would otherwise match carry
    assert classify(take) == ("Martial arts", "Fight")

    assert accad_take("flip")[0] == "flip_accad_flip"
    assert L.type_of("flip_accad_flip") == "acrobatic"
    assert accad_take("Female1_D4_RandomStuff")[0] == "any_accad_female1_d4_randomstuff"
    assert L.type_of("any_accad_female1_d4_randomstuff") == ""


def test_addbiomechanics_takes_and_b3d_landmarks():
    from ac_train import b3d
    from ac_train import labels as L
    from ac_train.datasets import addbiomechanics_take
    from ac_train.library import classify

    # Soccer kick is sports, not combat (despite "kick" in the trial name)
    take, label = addbiomechanics_take("Han2023_Formatted_With_Arm", "s004_split1", "20220524_soccerkick_3_segment_0")
    assert take == "sports_addbiomechanics_han2023_s004_split1_20220524_soccerkick_3_segment_0"
    assert label == "han2023 s004_split1: 20220524 soccerkick 3 segment 0"
    assert L.type_of(take) == "sports"
    assert classify(take) == ("Sports and fitness", "Ball sports")

    # Drop jumps and sit-to-stand in Uhlrich2023
    dj, _ = addbiomechanics_take("Uhlrich2023_Formatted_With_Arm", "subject2", "DJ1_segment_1")
    assert dj == "jump_addbiomechanics_uhlrich2023_subject2_dj1_segment_1"
    assert L.type_of(dj) == "jump"
    assert classify(dj) == ("Action", "Jump")

    sts, _ = addbiomechanics_take("Uhlrich2023_Formatted_With_Arm", "subject2", "STS1_segment_0")
    assert L.type_of(sts) == "sit"
    assert classify(sts) == ("Sitting", "Sit")

    # Stairs, treadmill (tmf) and quadruped trials are skipped
    assert addbiomechanics_take("Falisse2017_Formatted_With_Arm", "subject_1", "StairUp_4_segment_0") is None
    assert addbiomechanics_take("Fregly2012_Formatted_With_Arm", "5GC", "PS_ngait_tmf_ss1HS_segment_0") is None
    assert addbiomechanics_take("Han2023_Formatted_With_Arm", "s003", "20220520_dog_1_segment_0") is None
    assert addbiomechanics_take("Han2023_Formatted_With_Arm", "s003", "_segment_0") is None

    # Synthetic 20-joint Rajagopal B3dTrial converts to the 17 rig landmarks in Z-up
    joints = [
        "ground_pelvis", "hip_r", "walker_knee_r", "ankle_r", "subtalar_r", "mtp_r",
        "hip_l", "walker_knee_l", "ankle_l", "subtalar_l", "mtp_l", "back",
        "acromial_r", "elbow_r", "radioulnar_r", "radius_hand_r",
        "acromial_l", "elbow_l", "radioulnar_l", "radius_hand_l",
    ]
    jcs = np.zeros((4, 20, 3), dtype=np.float64)
    jcs[:, joints.index("back"), 1] = 1.0
    jcs[:, joints.index("acromial_l"), 1] = 1.4
    jcs[:, joints.index("acromial_r"), 1] = 1.4
    trial = b3d.B3dTrial("Gait_1", 100.0, joints, jcs, ["calcn_r", "calcn_l"], np.zeros((4, 2, 3)))
    src, fps = b3d.landmarks(trial)
    assert fps == 100.0
    assert set(src) == set(REGISTRY["cmu"].joints)
    assert np.all(src["head"][:, 2] > src["neck"][:, 2])
    assert np.all(src["neck"][:, 2] > src["spine"][:, 2])


BUILT = sorted(RIG.glob("*.npz")) if RIG.is_dir() else []


@pytest.mark.skipif(not BUILT, reason=f"no retargeted sets under {RIG} (python -m ac_train.datasets builds them)")
@pytest.mark.parametrize("path", BUILT, ids=[p.stem for p in BUILT])
def test_a_built_set_is_well_formed(path):
    d = np.load(path)
    rots, roots, starts = d["rotations"], d["roots"], d["starts"]
    assert rots.shape[1:] == (len(Skeleton.load().names), 4)
    assert roots.shape == (len(rots), 3)
    assert starts[0] == 0 and np.all(np.diff(starts) > 0) and starts[-1] < len(rots)
    n = np.linalg.norm(rots[::97].astype(np.float32), axis=-1)
    assert np.allclose(n, 1.0, atol=2e-3)  # float16 storage


def test_ranges_json_is_well_formed():
    import json

    from ac_train.ranges import OUT_PATH, PAIRS, SETS

    assert OUT_PATH.is_file()
    data = json.loads(OUT_PATH.read_text(encoding="utf-8"))
    assert data["version"] == 2
    assert set(data["sets"]).issubset(set(SETS))
    assert not {"kimodo", "builtin_loops"} & set(data["sets"])  # recorded bodies only
    assert len(data["checks"]) >= 2
    assert len(data["sources"]) >= 4
    pairs = {p["id"]: p for p in data["pairs"]}
    assert set(pairs) == {p["id"] for p in PAIRS}
    for p in pairs.values():
        assert len(p["hard"]) >= 2
        assert len(p["typical"]) >= 2
        assert all(pt["lo"] <= pt["hi"] for pt in p["hard"])
        assert all(pt["lo"] <= pt["hi"] for pt in p["typical"])
        assert len(p["grid"]["cells"]) == p["grid"]["rows"]
        assert all(len(row) == p["grid"]["cols"] for row in p["grid"]["cells"])
