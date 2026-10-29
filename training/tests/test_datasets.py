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
