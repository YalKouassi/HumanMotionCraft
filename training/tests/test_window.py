"""The training window's contract (roadmap 140000 context after key B, 142000 several keys):
default flags give today's window bit for bit; the options lay it out as the card says."""

import pathlib

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from ac_train import context, train  # noqa: E402
from ac_train.model import (  # noqa: E402
    BONES,
    CONTEXT,
    FEAT,
    MAX_GAP,
    Inbetweener,
    interpolate,
    interpolate_known,
    known_times,
    layout,
    mat_to_quat,
    quat_to_mat,
    time_to_go,
)

REFERENCE = pathlib.Path(__file__).parent / "data" / "prepare-reference.npz"


class Sub:
    hip_height = 0.87


def random_windows(n, T, seed):
    g = torch.Generator().manual_seed(seed)
    q = torch.randn(n, T, len(BONES), 4, generator=g)
    q = q / q.norm(dim=-1, keepdim=True)
    return q, torch.randn(n, T, 3, generator=g)


def test_default_flags_give_the_window_as_it_was_bit_for_bit():
    """``tests/data/prepare-reference.npz`` was written by ``prepare`` before the options existed
    (2026-10-30): the same inputs give the same bytes."""
    ref = np.load(REFERENCE)
    q, root, gap = torch.tensor(ref["q"]), torch.tensor(ref["root"]), int(ref["gap"])
    truth, interp, mask, ttg, _, _ = train.prepare(q, root, gap, Sub, "cpu")
    for name, got in (("truth", truth), ("interp", interp), ("mask", mask), ("ttg", ttg)):
        assert np.array_equal(got.numpy(), ref[name]), name
    assert mask[0, :, 0].tolist() == [1.0] * CONTEXT + [0.0] * gap + [1.0]


def test_layout_trades_frames_before_a_for_frames_after_b():
    assert layout(5) == (9, 15, 16)
    assert layout(5, 4) == (5, 11, 16)  # 6 before and on A, 5 gap, B, 4 after: still 16
    assert layout(45, 4) == (5, 51, 56)
    with pytest.raises(ValueError):
        layout(5, CONTEXT)


def test_context_after_puts_known_frames_after_key_b_with_signed_time():
    gap, after = 5, 4
    a, b, T = layout(gap, after)
    q, root = random_windows(2, T, 1)
    truth, interp, mask, ttg, _, _ = train.prepare(q, root, gap, Sub, "cpu", after=after)
    assert truth.shape == (2, T, FEAT) and interp.shape == (2, T, FEAT)
    assert mask[0, :, 0].tolist() == [1.0] * (a + 1) + [0.0] * gap + [1.0] * (1 + after)
    want = [(b - i) / MAX_GAP for i in range(T)]
    assert ttg[0, :, 0].tolist() == pytest.approx(want)
    assert (ttg[0, b + 1 :, 0] < 0).all()  # after B: negative, so the network can tell them apart
    # key A is the window's origin: its root is at 0, 0 and the known frames are the recording
    assert truth[0, a, -3:-1].abs().max() < 1e-6
    assert torch.allclose(interp[:, : a + 1], truth[:, : a + 1], atol=1e-4)  # the recording (a float32 quaternion round trip)
    assert torch.allclose(interp[:, b:], truth[:, b:], atol=1e-4)


def test_the_fallback_at_the_clip_end_is_the_window_as_it_was():
    """A clip that ends on B gives no frames after it: ``after`` 0 is today's layout."""
    q, root = random_windows(1, 16, 2)
    a = train.prepare(q, root, 5, Sub, "cpu", after=0)
    b = train.prepare(q, root, 5, Sub, "cpu")
    for x, y in zip(a[:4], b[:4]):
        assert torch.equal(x, y)


def test_known_times_count_to_the_next_known_frame_and_from_the_last():
    mask = torch.zeros(1, 10, 1)
    mask[0, [0, 1, 4, 9], 0] = 1
    t = (known_times(mask) * MAX_GAP).round()
    assert t[0, :, 0].tolist() == [0, 0, 2, 1, 0, 4, 3, 2, 1, 0]
    assert t[0, :, 1].tolist() == [0, 0, 1, 2, 0, 1, 2, 3, 4, 0]


def test_interpolate_known_is_the_plain_interpolation_between_neighbouring_known_frames():
    gap = 6
    a, b, T = layout(gap)
    q, root = random_windows(3, T, 3)
    mask = torch.zeros(3, T, 1)
    mask[:, : a + 1] = 1
    mask[:, b:] = 1
    R1, P1 = interpolate(q, root, torch.full((3,), a), torch.full((3,), b))
    R2, P2 = interpolate_known(q, root, mask)
    assert torch.allclose(R1, R2, atol=1e-6) and torch.allclose(P1, P2, atol=1e-6)
    # a key inside the gap: the interpolation goes through it, in two pieces
    k = a + 3
    mask[:, k] = 1
    R3, P3 = interpolate_known(q, root, mask)
    assert torch.allclose(R3[:, k], quat_to_mat(q[:, k]), atol=1e-6) and torch.equal(P3[:, k], root[:, k])
    Ra, Pa = interpolate(q, root, torch.full((3,), a), torch.full((3,), k))
    assert torch.allclose(R3[:, a : k + 1], Ra[:, a : k + 1], atol=1e-6)
    assert torch.allclose(P3[:, a : k + 1], Pa[:, a : k + 1], atol=1e-6)
    assert torch.allclose(mat_to_quat(R3[:, k - 1]), mat_to_quat(Ra[:, k - 1]), atol=1e-5)


def test_multi_key_windows_know_a_few_gap_frames_and_the_time_channel_is_two_wide():
    torch.manual_seed(0)
    gap = 8
    a, b, T = layout(gap)
    known = train.multi_known(4000, T, a, b, "cpu")
    assert known.shape == (4000, T, 1)
    assert float(known[:, : a + 1].sum()) == 0 and float(known[:, b:].sum()) == 0
    per = known[:, a + 1 : b, 0].sum(1)
    assert per.max() <= train.MULTI[1] and (per < gap).all()
    assert abs(float((per > 0).float().mean()) - train.MULTI[0]) < 0.03
    assert float(train.multi_known(10, 12, 9, 11, "cpu").sum()) == 0  # a gap of 1 has no frame to spare
    q, root = random_windows(2, T, 4)
    known = torch.zeros(2, T, 1)
    known[:, a + 3] = 1
    truth, interp, mask, ttg, _, _ = train.prepare(q, root, gap, Sub, "cpu", known=known, multi_key=True)
    assert ttg.shape == (2, T, 2)
    assert mask[0, :, 0].tolist() == [1.0] * (a + 1) + [0.0, 0.0, 1.0] + [0.0] * (gap - 3) + [1.0]
    assert torch.allclose(interp[:, a + 3], truth[:, a + 3], atol=1e-4)  # the key inside comes through unchanged
    with pytest.raises(ValueError):
        train.prepare(q, root, gap, Sub, "cpu", known=known)  # known frames need a multi-key model


def test_the_network_takes_the_time_channel_its_card_says():
    torch.manual_seed(1)
    plain = Inbetweener(d=32, layers=1, heads=4, given=True, path=True)
    multi = Inbetweener(d=32, layers=1, heads=4, given=True, path=True, multi_key=True, context_after=4)
    assert multi.inp.in_features - plain.inp.in_features == 1
    assert (multi.context_after, multi.multi_key, multi.times) == (4, True, 2)
    assert (plain.context_after, plain.multi_key, plain.times) == (0, False, 1)
    T = 16
    known, interp = torch.randn(1, T, FEAT), torch.randn(1, T, FEAT)
    mask = torch.zeros(1, T, 1)
    mask[:, :10] = 1
    mask[:, -1] = 1
    one, _ = time_to_go(torch.tensor([9]), torch.tensor([15]), T, "cpu")
    with torch.no_grad():
        assert plain(known, interp, mask, one).shape == (1, T, FEAT)
        assert multi(known, interp, mask, known_times(mask)).shape == (1, T, FEAT)
        with pytest.raises(ValueError):
            multi(known, interp, mask, one)
        with pytest.raises(ValueError):
            plain(known, interp, mask, known_times(mask))
    with pytest.raises(ValueError):
        Inbetweener(d=32, layers=1, heads=4, context_after=CONTEXT)


def test_context_after_b_is_rebuilt_like_the_context_before_a():
    n, T, a, b = 4, 20, 5, 15
    q, root = random_windows(n, T, 5)
    keys = context.sparse_keys(n, T, a, "cpu", torch.Generator().manual_seed(1), b=b)
    assert keys[:, a].all() and keys[:, b].all() and not keys[:, a + 1 : b].any()
    for mode in context.MODES:
        rq, rr = context.rebuild(q, root, a, mode, torch.Generator().manual_seed(0), b=b)
        assert torch.equal(rq[:, a : b + 1], q[:, a : b + 1]) and torch.equal(rr[:, a : b + 1], root[:, a : b + 1])
    sq, sr = context.rebuild(q, root, a, "still", b=b)
    assert torch.equal(sq[:, b + 1 :], q[:, b : b + 1].expand(-1, T - b - 1, -1, -1))
    assert torch.equal(sr[:, :a], root[:, a : a + 1].expand(-1, a, -1))
    rq, _ = context.rebuild(q, root, a, "linear", torch.Generator().manual_seed(0), b=b)
    assert not torch.equal(rq[:, b + 1 :], q[:, b + 1 :])
    nq, nr = context.noise(q, root, a, 2.0, 0.01, 0.87, torch.Generator().manual_seed(2), b=b)
    assert torch.equal(nq[:, a : b + 1], q[:, a : b + 1]) and torch.equal(nr[:, a : b + 1], root[:, a : b + 1])
    assert not torch.equal(nq[:, b + 1 :], q[:, b + 1 :])
    # without b the window ends on B, as before
    keys = context.sparse_keys(n, T, a, "cpu", torch.Generator().manual_seed(1))
    assert keys[:, T - 1].all()
