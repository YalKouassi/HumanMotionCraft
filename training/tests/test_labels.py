"""Each take's labels, and the styles a model learns from them."""

from collections import Counter

import numpy as np
import pytest

from ac_train import labels as L


def test_words_are_normalised():
    assert L.word("Old Man") == L.word("old_man") == L.word("OldMan") == "old-man"
    assert L.word("  Walk ") == "walk"
    assert L.word("CrossOver") == "cross-over"
    assert L.word("any") == "" and L.word(" - ") == ""


def test_a_take_name_gives_its_type():
    assert L.type_of("walk_cmu_07_01") == "walk"
    assert L.type_of("fallAndGetUp1_subject1") == "fall"
    assert L.type_of("Jump 3") == "jump"
    assert L.type_of("something else") == ""
    assert L.type_of("dance_cmu_05_02") == "dance"  # its own type, not acrobatic
    assert L.type_of("fightAndSports1_subject1") == "combat"  # the first type in the name wins
    assert L.type_of("sports_cmu_06_02") == "sports"
    assert L.type_of("stretch_cmu_12_04") == "exercise"
    assert L.type_of("gesture_cmu_13_26") == "gesture"
    assert L.type_of("carry_cmu_02_06") == "carry"
    assert L.type_of("sit_cmu_13_01") == L.type_of("Sit11") == L.type_of("Armchair1") == "sit"
    # "sit" starts a word: a transition is no sit
    assert L.type_of("braking_kimodo_transition_run_to_stop_s1") == "run"
    assert L.type_of("dancing") == "" and L.type_of("x", "Dancing") == "dance"


def test_the_review_type_wins_and_plain_synonyms_join_the_automatic_type():
    assert L.type_of("walk_cmu_07_01", "Fight") == "combat"
    assert L.type_of("walk_cmu_07_01", "dance") == "dance"  # a person's own word stands
    assert L.type_of("walk_cmu_07_01", "running") == "run"


def test_labels_are_the_type_then_the_style_words():
    take, own = "walk_100style_FW_034", "Heavyset: Walking as a muscle man tensed biceps (forwards walk)"
    assert L.of("100style", take, own) == ["walk", "muscle-man"]
    assert L.of("100style", take, own, tags=["Tired", "muscle man"]) == ["walk", "muscle-man", "tired"]
    assert L.of("100style", take, own, kind="run") == ["run", "muscle-man"]
    # a held pose is no style: the take trains as a walk
    assert L.of("100style", "walk_100style_FW_088", "Teapot: One arm on hip other arm raised") == ["walk"]
    assert L.of("100style", "walk_100style_FW_052", "Neutral: Normal walking") == ["walk", "natural"]
    # only 100STYLE's own label is a style: CMU's is a description
    assert L.of("cmu", "walk_cmu_02_01", "walk with arms out") == ["walk"]
    assert L.of("cmu", "any_cmu_29_25", "monkey (human subject)") == []


def test_styles_are_the_labels_with_enough_motion():
    m = 60 * L.FPS
    frames = Counter({"walk": 50 * m, "zombie": 6 * m, "tired": 2 * m, "acrobatic": 10 * m})
    styles, rare = L.vocabulary(frames, 5)
    assert styles == ["any", "acrobatic", "walk", "zombie"]
    assert rare == {"tired": 2 * m}


def test_rare_types_are_drawn_more_often_and_style_words_dont_count():
    frames = Counter({"walk": 900, "fall": 100, "jump": 400, "zombie": 10})
    styles = ["any", "fall", "jump", "walk", "zombie"]
    w = L.weights(["walk", "fall", "jump", "", "zombie"], frames, styles)
    assert w.tolist() == [1.0, 3.0, 1.5, 1.0, 3.0]
    assert L.weights(["walk"], frames, ["any"]).tolist() == [1.0]


def test_mirroring_swaps_sides():
    assert L.mirrored("limp-left") == "limp-right"
    assert L.mirrored("on-phone-right") == "on-phone-left"
    assert L.mirrored("spin-clock") == "spin-anti-clock"
    assert L.mirrored("spin-anti-clock") == "spin-clock"
    assert L.mirrored("zombie") == "zombie"
    m = L.mirror_matrix(["any", "limp-left", "limp-right", "lean-left", "zombie"])
    style = np.array([[0, 1, 0, 0, 1], [0, 0, 0, 1, 0]], np.float32)
    assert (style @ m).tolist() == [[0, 0, 1, 0, 1], [0, 0, 0, 0, 0]]  # lean-right isn't a style: dropped


def test_styles_add_up_in_the_network():
    torch = pytest.importorskip("torch", reason="the network needs torch")
    from ac_train.model import FEAT, Inbetweener
    from ac_train.train import drop_styles

    torch.manual_seed(0)
    net = Inbetweener(styles=4, given=True).eval()
    torch.nn.init.normal_(net.out[1].weight, std=0.01)
    T = 12
    x = torch.randn(1, T, FEAT)
    mask, ttg = torch.ones(1, T, 1), torch.zeros(1, T, 1)
    one_hot = torch.tensor([[0.0, 0.0, 1.0, 0.0]])
    with torch.no_grad():
        a = net(x, x, mask, ttg, torch.tensor([2]))
        b = net(x, x, mask, ttg, one_hot)
        both = net(x, x, mask, ttg, torch.tensor([[0.0, 1.0, 1.0, 0.0]]))
    assert torch.allclose(a, b, atol=1e-6)  # an index is its one-hot row
    assert not torch.allclose(a, both, atol=1e-4)

    s = drop_styles(torch.tensor([[0.0, 1.0, 1.0, 0.0]] * 2000 + [[1.0, 0.0, 0.0, 0.0]] * 10))
    assert (s.sum(1) >= 1).all()  # never no style at all: "any" when every one was dropped
    assert ((s[:, 0] == 1) == (s[:, 1:].sum(1) == 0)).all()
    assert 0.03 < float((s[:2000, 0] == 1).float().mean()) < 0.1  # both dropped: 0.25 * 0.25
    assert (s[2000:, 0] == 1).all()


def test_the_100style_styles_are_ways_of_moving_with_both_sides():
    import csv

    assert len(L.STYLE100) == 33 and len(set(L.STYLE100.values())) == 33
    assert all(L.word(w) == w for w in L.STYLE100.values())
    assert {"natural", "crouched", "elderly", "limp-left"} <= set(L.STYLE100.values())
    assert not {"Teapot", "Akimbo", "Waving", "SpinClock"} & set(L.STYLE100)
    # a side style's mirror is a style too, so mirrored windows keep their label
    for w in L.STYLE100.values():
        assert L.mirrored(w) in L.STYLE100.values()
    names = L.pathlib.Path(__file__).resolve().parents[1] / "data/raw/100STYLE/Dataset_List.csv"
    if names.exists():  # the set's own names, when it is downloaded
        with open(names, encoding="utf-8-sig", newline="") as fh:
            assert set(L.STYLE100) <= {r["Style Name"] for r in csv.DictReader(fh)}
    assert L.set_style("100style", "Teapot: x") == "teapot" and L.auto_style("100style", "Teapot: x") == ""
