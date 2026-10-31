from ac_train import judge as J


def test_span_keeps_the_context_and_stays_in_the_take():
    assert J.span(100, 30, 300) == (84, 115)
    assert J.span(5, 30, 300) == (10, 41)
    a, b = J.span(295, 30, 300)
    assert b < 300
    assert b - a - 1 == 30


def test_every_scene_fits_the_models_gap():
    from ac_train.model import MAX_GAP

    assert all(1 <= s.gap <= MAX_GAP for s in J.SCENES)
    assert len({s.name for s in J.SCENES}) == len(J.SCENES)
