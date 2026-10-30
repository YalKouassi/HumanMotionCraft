"""ac_train.train: training windows look like what the app sends."""

import pytest

torch = pytest.importorskip("torch")

from ac_train import train  # noqa: E402
from ac_train.model import BONES, PART_GROUPS  # noqa: E402


def test_part_masks_are_what_a_part_layer_sends():
    """A row is no mask, a part group, or everything but a part group (a left-arm layer: 16 bones
    given, the arm's 3 free), the last most often."""
    torch.manual_seed(0)
    g = train.given_mask(4000, "cpu")
    groups = [set(v) for v in PART_GROUPS.values()]
    kinds = {"none": 0, "group": 0, "complement": 0}
    for row in g:
        on = {i for i in range(len(BONES)) if row[i] == 1}
        if not on:
            kinds["none"] += 1
        elif on in groups and set(range(len(BONES))) - on not in groups:
            kinds["group"] += 1
        else:
            assert set(range(len(BONES))) - on in groups, sorted(on)
            kinds["complement"] += 1
    assert abs(kinds["none"] / 4000 - (1 - train.GIVEN[0])) < 0.03
    assert kinds["complement"] > kinds["group"] > 0


def test_gaps_cover_every_length_the_app_fills():
    torch.manual_seed(0)
    gaps = [train.sample_gap() for _ in range(3000)]
    assert min(gaps) == 1 and max(gaps) == 45
    short = sum(g <= 4 for g in gaps) / len(gaps)
    assert abs(short - train.GAP_MIX[0]) < 0.03


def test_root_path_is_given_on_the_gap_frames_of_half_the_windows():
    """141000: a window gets the path on every gap frame or on none, never on a known frame."""
    torch.manual_seed(0)
    N, T = 2000, 20
    truth = torch.randn(N, T, 4)
    mask = torch.zeros(N, T, 1)
    mask[:, :10] = 1
    mask[:, -1] = 1
    path, pm = train.path_given(truth, mask, widths=(1,))
    assert path.shape == (N, T, 3) and pm.shape == (N, T, 1)
    assert torch.equal(path, truth[..., -3:])  # width 1: the recording itself
    assert float((pm * mask).abs().sum()) == 0
    per = pm[:, 10:-1, 0]
    on = per[:, 0] == 1
    assert torch.equal(per, on[:, None].expand_as(per).float())
    assert abs(float(on.float().mean()) - train.PATH_GIVEN[0]) < 0.03
