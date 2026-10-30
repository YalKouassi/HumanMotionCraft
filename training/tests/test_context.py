"""ac_train.context: the batched spline is the app's interpolation (ac_train.clip), key for key."""

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from ac_train import context  # noqa: E402
from ac_train.clip import Clip, Key  # noqa: E402


def random_window(n, T, J, seed):
    rng = np.random.default_rng(seed)
    q = rng.normal(size=(n, T, J, 4))
    q /= np.linalg.norm(q, axis=-1, keepdims=True)
    root = rng.normal(size=(n, T, 3))
    return q, root


@pytest.mark.parametrize("mode", context.SPLINES)
def test_sample_matches_clip_sample(mode):
    n, T, J, a = 6, 20, 3, 9
    q, root = random_window(n, T, J, 1)
    keys = context.sparse_keys(n, T, a, "cpu", torch.Generator().manual_seed(2))
    keys[0] = False  # one window with a single key in the context besides a and b
    keys[0, [a, T - 1]] = True
    got_q, got_r = context.sample(torch.tensor(q), torch.tensor(root), keys, mode)
    for w in range(n):
        frames = keys[w].nonzero().flatten().tolist()
        clip = Clip(bones=[f"b{j}" for j in range(J)], keys=[Key(f, q[w, f], root[w, f], interp=mode) for f in frames])
        want_q, want_r = clip.sample(np.arange(T))
        dot = np.abs((got_q[w].numpy() * want_q).sum(-1))  # q and -q are one rotation
        assert np.allclose(dot, 1.0, atol=1e-9), (mode, w)
        assert np.allclose(got_r[w].numpy(), want_r, atol=1e-9), (mode, w)


def test_rebuild_changes_only_the_frames_before_key_a():
    n, T, J, a = 4, 16, 2, 9
    q, root = (torch.tensor(x) for x in random_window(n, T, J, 3))
    for mode in context.MODES:
        rq, rr = context.rebuild(q, root, a, mode, torch.Generator().manual_seed(0))
        assert torch.equal(rq[:, a:], q[:, a:]) and torch.equal(rr[:, a:], root[:, a:])
    sq, sr = context.rebuild(q, root, a, "still")
    assert torch.equal(sq[:, :a], q[:, a : a + 1].expand(-1, a, -1, -1))
    assert torch.equal(sr[:, :a], root[:, a : a + 1].expand(-1, a, -1))


def test_keys_always_hold_key_a_and_key_b():
    keys = context.sparse_keys(64, 30, 9, "cpu", torch.Generator().manual_seed(4))
    assert keys[:, 9].all() and keys[:, 29].all()
    assert not keys[:, 10:29].any()
    between = keys[:, 1:9].sum(1)
    assert between.min() == 0 and between.max() <= 3
