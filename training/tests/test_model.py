"""ac_train.model: the root-path input (141000)."""

import pytest

torch = pytest.importorskip("torch")

from ac_train.model import FEAT, PATH, Inbetweener, smooth_path  # noqa: E402


def test_smoothing_keeps_a_steady_path_and_its_middle():
    """A root moving 0.1 a frame along X: a 5-frame box keeps every frame 2 or more from the ends,
    and the first frame becomes (0 + 0 + 0 + 0.1 + 0.2) / 5 = 0.06 (the end repeated)."""
    pos = torch.zeros(1, 12, 3)
    pos[0, :, 0] = torch.arange(12) * 0.1
    s = smooth_path(pos, 5)
    assert torch.allclose(s[0, 2:10], pos[0, 2:10], atol=1e-6)
    assert abs(float(s[0, 0, 0]) - 0.06) < 1e-6
    assert torch.equal(smooth_path(pos, 1), pos)


def _net(path):
    torch.manual_seed(3)
    net = Inbetweener(d=32, layers=1, heads=4, given=True, path=path).eval()
    torch.nn.init.normal_(net.out[1].weight, std=0.1)  # trained-like: not the zero start
    return net


def test_a_path_model_is_wider_by_the_path_and_works_without_one():
    assert _net(True).inp.in_features - _net(False).inp.in_features == PATH
    net = _net(True)
    T = 16
    torch.manual_seed(1)
    known, interp = torch.randn(1, T, FEAT), torch.randn(1, T, FEAT)
    mask = torch.zeros(1, T, 1)
    mask[:, :10] = 1
    mask[:, -1] = 1
    ttg = torch.linspace(1, 0, T)[None, :, None]
    with torch.no_grad():
        none = net(known, interp, mask, ttg)
        zero = net(known, interp, mask, ttg, path=torch.randn(1, T, 3), pmask=torch.zeros(1, T, 1))
        given = net(known, interp, mask, ttg, path=torch.randn(1, T, 3), pmask=1 - mask)
    # no path and a path with every flag 0 are the same input
    assert torch.allclose(none, zero, atol=1e-6)
    assert not torch.allclose(none, given, atol=1e-4)
