"""The context before key A as the app gives it, for training and the evaluation.

In the app, the frames before key A are whatever the clip samples there: a spline between the
animator's keys, or frame 0 held before the first key (``crates/ac-ai/src/lib.rs``: the window's
frames before 0 repeat frame 0). Clean mocap context is the easy case. This rebuilds a window's
context from a few of its own frames as keys, with the app's interpolation (:mod:`ac_train.clip`,
ported here batched in torch: ``step``, ``linear`` and ``smooth``), or holds key A's pose still.
Key B is a key too: it shapes the smooth curve into key A, as it does in the app.

Only frames before key A change, and (a window with context after key B, roadmap 140000) the
frames after B, made the same way: sparse keys after B, or B held, as the app holds the last key.
Key A, the gap and key B stay as recorded: the target is always the original motion.
"""

from __future__ import annotations

import torch

#: how a window's context is made (training draws one per window; the evaluation runs each)
MODES = ("clean", "smooth", "linear", "step", "still")
SPLINES = ("smooth", "linear", "step")


# ---- quaternions (torch, x y z w), as ac_train.quat has them ------------------------------------

def _mul(a, b):
    ax, ay, az, aw = a.unbind(-1)
    bx, by, bz, bw = b.unbind(-1)
    return torch.stack([aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
                        aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz], -1)


def _conj(q):
    return q * torch.tensor([-1.0, -1.0, -1.0, 1.0], dtype=q.dtype, device=q.device)


def _norm(q):
    return q / q.norm(dim=-1, keepdim=True).clamp(min=1e-12)


def _align(q, ref):
    s = torch.sign((q * ref).sum(-1, keepdim=True))
    return q * torch.where(s == 0, torch.ones_like(s), s)


def _log(q):
    q = _norm(q)
    v, w = q[..., :3], q[..., 3:].clamp(-1.0, 1.0)
    n = v.norm(dim=-1, keepdim=True)
    angle = torch.atan2(n, w)
    return v * torch.where(n > 1e-12, angle / n.clamp(min=1e-12), torch.ones_like(n))


def _exp(v):
    n = v.norm(dim=-1, keepdim=True)
    s = torch.where(n > 1e-12, torch.sin(n) / n.clamp(min=1e-12), torch.ones_like(n))
    return torch.cat([v * s, torch.cos(n)], -1)


def _slerp(a, b, t):
    b = _align(b, a)
    return _norm(_mul(a, _exp(_log(_mul(_conj(a), b)) * t[..., None])))


def _squad_arc(q0, q1, s0, s1, t):
    inner = _norm(_mul(s0, _exp(_log(_mul(_conj(s0), s1)) * t[..., None])))
    return _slerp(_slerp(q0, q1, t), inner, 2.0 * t * (1.0 - t))


def _slope(d_in, d_out, h_in, h_out):
    """:func:`ac_train.clip._slope`, batched: ``h_in``, ``h_out`` broadcast against the values."""
    s_in = d_in / h_in.clamp(min=1)
    s_out = d_out / h_out.clamp(min=1)
    limit = 3 * torch.minimum(s_in.abs(), s_out.abs())
    m = (d_in + d_out) / (h_in + h_out).clamp(min=1)
    m = torch.maximum(torch.minimum(m, limit), -limit)
    both = torch.where(s_in * s_out > 0, m, torch.zeros_like(m))
    no_in = torch.where(h_out > 0, s_out / 2, torch.zeros_like(m))
    return torch.where(h_in <= 0, no_in, torch.where(h_out <= 0, s_in / 2, both))


# ---- keys -----------------------------------------------------------------------------------------

def sparse_keys(n: int, T: int, a: int, device, gen=None, first: float = 0.7, most: int = 3, b: int | None = None):
    """(n, T) bool: which frames of each window are keys. Key A (frame ``a``) and key B (frame
    ``b``, the last frame by default) always; the window's first frame with probability ``first``
    (else the earliest key is held back to the start, as before a clip's first key); and 0 to
    ``most`` more keys at random frames between, so spacing and placement vary. Frames after B
    (``b`` before the last frame) get keys the same way: the last frame with probability
    ``first`` (else B's last key holds, as the app holds the last key), 0 to ``most`` between."""
    b = T - 1 if b is None else b
    keys = torch.zeros(n, T, dtype=torch.bool, device=device)
    keys[:, a] = True
    keys[:, b] = True
    if a > 0:
        keys[:, 0] = torch.rand(n, device=device, generator=gen) < first
    if a > 1:
        count = torch.randint(0, most + 1, (n,), device=device, generator=gen)
        # a random order of the frames between, the first ``count`` of them become keys
        order = torch.rand(n, a - 1, device=device, generator=gen).argsort(-1)
        pick = order < count[:, None]
        keys[:, 1:a] |= pick
    after = T - 1 - b
    if after > 0:
        keys[:, T - 1] = torch.rand(n, device=device, generator=gen) < first
    if after > 1:
        count = torch.randint(0, most + 1, (n,), device=device, generator=gen)
        order = torch.rand(n, after - 1, device=device, generator=gen).argsort(-1)
        keys[:, b + 1 : T - 1] |= order < count[:, None]
    return keys


def _neighbours(keys):
    """For every frame: the key at or before it (k0), the key after it (k1), the key before k0 (kp)
    and the key after k1 (kn), as :meth:`ac_train.clip.Clip._neighbours` finds them; frames before
    the first key hold it (k0 = k1 = the first key). (n, T) long each."""
    n, T = keys.shape
    idx = torch.arange(T, device=keys.device).expand(n, T)
    big = torch.full_like(idx, T)
    prev = torch.where(keys, idx, torch.full_like(idx, -1)).cummax(-1).values
    nxt = torch.where(keys, idx, big).flip(-1).cummin(-1).values.flip(-1)
    first = nxt[:, :1].expand(n, T)
    last = prev[:, -1:].expand(n, T)
    k0 = torch.where(prev < 0, first, prev)
    # the key after k0 (k0 itself at the last key)
    after = torch.cat([nxt[:, 1:], big[:, :1]], -1)
    k1 = torch.where(prev < 0, first, after.gather(1, k0))
    k1 = torch.where(k1 >= T, last, k1)
    before = torch.cat([torch.full_like(idx[:, :1], -1), prev[:, :-1]], -1)
    kp = before.gather(1, k0)
    kp = torch.where(kp < 0, k0, kp)
    kn = after.gather(1, k1)
    kn = torch.where(kn >= T, k1, kn)
    return k0, k1, kp, kn


def sample(q, root, keys, mode: str):
    """The clip a window's keys make, sampled on every frame of the window.

    q (n, T, J, 4) rotations, root (n, T, 3) positions, keys (n, T) bool, ``mode`` one of
    :data:`SPLINES` for every key. Returns (q, root) like the inputs: :meth:`ac_train.clip.Clip.sample`
    on those keys."""
    n, T = keys.shape
    k0, k1, kp, kn = _neighbours(keys)
    g = lambda x, k: x.gather(1, k.view(n, T, *[1] * (x.dim() - 2)).expand(n, T, *x.shape[2:]))  # noqa: E731
    f = torch.arange(T, device=q.device, dtype=q.dtype).expand(n, T)
    h = (k1 - k0).to(q.dtype)
    t = torch.where(h > 0, (f - k0.to(q.dtype)) / h.clamp(min=1), torch.zeros_like(f))
    q0, r0 = g(q, k0), g(root, k0)
    if mode == "step":
        return q0, r0
    q1, r1 = g(q, k1), g(root, k1)
    if mode == "linear":
        return _slerp(q0, q1, t[..., None].expand(q0.shape[:-1])), r0 + (r1 - r0) * t[..., None]
    if mode != "smooth":
        raise ValueError(f"unknown interpolation {mode!r}")
    qp, qn, rp, rn = g(q, kp), g(q, kn), g(root, kp), g(root, kn)
    h_in = (k0 - kp).to(q.dtype)
    h_out = (kn - k1).to(q.dtype)
    hh = torch.where(h > 0, h, torch.ones_like(h))
    q1 = _align(q1, q0)
    qp = _align(qp, q0)
    qn = _align(qn, q1)
    into = _log(_mul(_conj(qp), q0))
    across = _log(_mul(_conj(q0), q1))
    back = _log(_mul(_conj(q1), q0))
    out = _log(_mul(_conj(q1), qn))
    e = lambda x: x[:, :, None, None]  # noqa: E731
    t0 = _slope(into, across, e(h_in), e(hh)) * e(hh)
    t1 = _slope(-back, out, e(hh), e(h_out)) * e(hh)
    s0 = _norm(_mul(q0, _exp(0.5 * (t0 - across))))
    s1 = _norm(_mul(q1, _exp(-0.5 * (t1 + back))))
    tt = t[..., None].expand(q0.shape[:-1])
    rot = _squad_arc(q0, q1, s0, s1, tt)
    rot = torch.where((t == 0)[..., None, None], q0, rot)
    r = lambda x: x[:, :, None]  # noqa: E731
    m0 = _slope(r0 - rp, r1 - r0, r(h_in), r(hh)) * r(hh)
    m1 = _slope(r1 - r0, rn - r1, r(hh), r(h_out)) * r(hh)
    t1_, t2, t3 = r(t), r(t) ** 2, r(t) ** 3
    pos = (2 * t3 - 3 * t2 + 1) * r0 + (t3 - 2 * t2 + t1_) * m0 + (-2 * t3 + 3 * t2) * r1 + (t3 - t2) * m1
    return rot, pos


# ---- the context a window gets ------------------------------------------------------------------

def rebuild(q, root, a: int, mode: str, gen=None, keys=None, b: int | None = None):
    """q (n, T, J, 4), root (n, T, 3) with the frames before ``a`` (and after ``b``, when it is
    before the last frame) made the way ``mode`` says (:data:`MODES`); the rest unchanged.
    ``keys``: the sparse keys to use (default :func:`sparse_keys`)."""
    n, T = q.shape[:2]
    b = T - 1 if b is None else b
    if mode == "clean" or (a <= 0 and b >= T - 1):
        return q, root
    q, root = q.clone(), root.clone()
    if mode == "still":
        q[:, :a] = q[:, a : a + 1]
        root[:, :a] = root[:, a : a + 1]
        q[:, b + 1 :] = q[:, b : b + 1]
        root[:, b + 1 :] = root[:, b : b + 1]
        return q, root
    if keys is None:
        keys = sparse_keys(n, T, a, q.device, gen, b=b)
    sq, sr = sample(q, root, keys, mode)
    q[:, :a], root[:, :a] = sq[:, :a], sr[:, :a]
    q[:, b + 1 :], root[:, b + 1 :] = sq[:, b + 1 :], sr[:, b + 1 :]
    return q, root


def mixed(q, root, a: int, shares: dict[str, float], gen=None, b: int | None = None):
    """Each window's context made by one mode, drawn with ``shares`` (mode -> weight). Returns
    (q, root, the mode index per window into :data:`MODES`)."""
    n = q.shape[0]
    w = torch.tensor([shares.get(m, 0.0) for m in MODES], device=q.device)
    pick = torch.multinomial(w, n, replacement=True, generator=gen)
    q, root = q.clone(), root.clone()
    for i, m in enumerate(MODES):
        rows = (pick == i).nonzero().flatten()
        if m == "clean" or not len(rows):
            continue
        rq, rr = rebuild(q[rows], root[rows], a, m, gen, b=b)
        q[rows], root[rows] = rq, rr
    return q, root, pick


def noise(q, root, a: int, deg: float, metres: float, hip: float, gen=None, b: int | None = None):
    """Small random turns (about ``deg`` degrees) and root shifts (about ``metres``) on the frames
    before ``a`` (and after ``b``, when the window has frames there): keys an animator set by
    eye, not by mocap. Key A and key B stay exact."""
    T = q.shape[1]
    b = T - 1 if b is None else b
    if (a <= 0 and b >= T - 1) or (deg <= 0 and metres <= 0):
        return q, root
    q, root = q.clone(), root.clone()
    for sl in (slice(0, a), slice(b + 1, T)):
        if sl.stop <= sl.start:
            continue
        shape = q[:, sl].shape[:-1]
        v = torch.randn(*shape, 3, device=q.device, generator=gen) * (torch.pi / 180 * deg / 2)
        q[:, sl] = _norm(_mul(_exp(v), q[:, sl]))
        root[:, sl] = root[:, sl] + torch.randn(root[:, sl].shape, device=q.device, generator=gen) * (metres / hip)
    return q, root

