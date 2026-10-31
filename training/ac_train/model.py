"""The in-betweening network and its features.

Features cover the bones the retarget solves (:data:`BONES`: root, spine, neck, clavicles, arms,
legs, feet). Other bones (fingers, toes, twist bones, head, eyes) keep the keys' own spline: the
network never touches a hand pose an animator set.

A window is ``CONTEXT`` frames that end on key A, the gap, and key B. Everything is put in key A's
frame (root at the origin, facing +Y about Z), so the network sees the same motion wherever and
whichever way the character stands. The network starts from the plain interpolation between the
keys and predicts a correction for every frame; the known frames are given back unchanged.

Two options a model card carries change that window (both off for the shipped model, whose
inputs stay as they were, bit for bit):

- ``context_after`` (roadmap 140000): that many of the ``CONTEXT`` frames come *after* key B
  instead of before key A (the window stays ``WINDOW`` frames), so the network sees how the
  motion leaves B. Where the clip ends before them, the window falls back to the frames before A
  (:func:`layout`). Time to go is signed: negative after B.
- ``multi_key`` (roadmap 142000): any set of frames in the window may be known (several keys, so
  two neighbouring gaps fill in one pass and agree at the key they share). The plain
  interpolation then runs between neighbouring known frames (:func:`interpolate_known`), and the
  time channel becomes two: time to the next known frame and from the last one
  (:func:`known_times`).
"""

from __future__ import annotations

import math

import numpy as np
import torch
from torch import nn

BONES = ("root", "spine05", "spine04", "spine03", "spine02", "spine01", "neck01",
         "clavicle.L", "upperarm01.L", "lowerarm01.L", "clavicle.R", "upperarm01.R", "lowerarm01.R",
         "upperleg01.L", "lowerleg01.L", "foot.L", "upperleg01.R", "lowerleg01.R", "foot.R")
# the solved bones' left/right partner (mirroring) and the part groups a layer can hand over
MIRROR = tuple(BONES.index(b[:-2] + (".R" if b.endswith(".L") else ".L")) if b[-2:] in (".L", ".R") else i
               for i, b in enumerate(BONES))
PART_GROUPS = {
    "upper": tuple(range(1, 13)),  # spine, neck, both arms
    "lower": (0, 13, 14, 15, 16, 17, 18),  # hips and both legs
    "spine": (1, 2, 3, 4, 5, 6),
    "left_arm": (7, 8, 9), "right_arm": (10, 11, 12), "arms": tuple(range(7, 13)),
    "left_leg": (13, 14, 15), "right_leg": (16, 17, 18), "legs": tuple(range(13, 19)),
}
#: context frames a window holds: before key A, or (``context_after``) some of them after key B
CONTEXT = 10
MAX_GAP = 45
WINDOW = CONTEXT + MAX_GAP + 1
FEAT = len(BONES) * 6 + 3
#: the root-path channel's width: x, y, z and the flag
PATH = 4


# ---- rotations (torch, batched) -----------------------------------------------------------------

def quat_to_mat(q):
    q = q / q.norm(dim=-1, keepdim=True)
    x, y, z, w = q.unbind(-1)
    return torch.stack([1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w),
                        2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w),
                        2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)], -1).reshape(q.shape[:-1] + (3, 3))


def mat_to_6d(m):
    return m[..., :, :2].transpose(-1, -2).reshape(m.shape[:-2] + (6,))


def sixd_to_mat(d):
    a, b = d[..., :3], d[..., 3:]
    x = nn.functional.normalize(a, dim=-1)
    y = nn.functional.normalize(b - (x * b).sum(-1, keepdim=True) * x, dim=-1)
    return torch.stack([x, y, torch.cross(x, y, dim=-1)], -1)


def mat_to_quat(R):
    w = torch.sqrt(torch.clamp(1 + R[..., 0, 0] + R[..., 1, 1] + R[..., 2, 2], min=0)) / 2
    x = torch.sqrt(torch.clamp(1 + R[..., 0, 0] - R[..., 1, 1] - R[..., 2, 2], min=0)) / 2
    y = torch.sqrt(torch.clamp(1 - R[..., 0, 0] + R[..., 1, 1] - R[..., 2, 2], min=0)) / 2
    z = torch.sqrt(torch.clamp(1 - R[..., 0, 0] - R[..., 1, 1] + R[..., 2, 2], min=0)) / 2
    x = torch.copysign(x, R[..., 2, 1] - R[..., 1, 2])
    y = torch.copysign(y, R[..., 0, 2] - R[..., 2, 0])
    z = torch.copysign(z, R[..., 1, 0] - R[..., 0, 1])
    q = torch.stack([x, y, z, w], -1)
    return q / q.norm(dim=-1, keepdim=True)


def slerp(a, b, t):
    """Batched quaternion slerp; t broadcasts over the leading axes."""
    d = (a * b).sum(-1, keepdim=True)
    b = torch.where(d < 0, -b, b)
    d = d.abs().clamp(max=1 - 1e-7)
    th = torch.acos(d)
    s = torch.sin(th).clamp(min=1e-7)
    wa, wb = torch.sin((1 - t) * th) / s, torch.sin(t * th) / s
    near = d > 0.9995
    wa, wb = torch.where(near, 1 - t, wa), torch.where(near, t, wb)
    q = wa * a + wb * b
    return q / q.norm(dim=-1, keepdim=True)


# ---- the sub-skeleton for the position loss --------------------------------------------------------

class SubSkeleton:
    """FK of :data:`BONES` alone: bones in between keep their rest rotation (as in the data), so a
    bone's offset to its nearest solved ancestor is the sum of the rest offsets."""

    def __init__(self, skel, device="cpu"):
        idx = [skel.index[b] for b in BONES]
        self.parents = []
        for i in idx:
            p = int(skel.parents[i])
            while p >= 0 and p not in idx:
                p = int(skel.parents[p])
            self.parents.append(idx.index(p) if p >= 0 else -1)
        H = skel.heads
        off = [H[i] - (H[idx[p]] if p >= 0 else 0) for i, p in zip(idx, self.parents)]
        self.offsets = torch.tensor(np.array(off), dtype=torch.float32, device=device)
        self.hip_height = float(skel.hip_height)

    def positions(self, local, root):
        """local (..., J, 3, 3), root positions (..., 3) -> joint positions (..., J, 3)."""
        R, P = [None] * len(BONES), [None] * len(BONES)
        for j, p in enumerate(self.parents):
            if p < 0:
                R[j], P[j] = local[..., j, :, :], root
            else:
                R[j] = R[p] @ local[..., j, :, :]
                P[j] = P[p] + (R[p] @ self.offsets[j].unsqueeze(-1)).squeeze(-1)
        return torch.stack(P, -2)


# ---- canonical frame ---------------------------------------------------------------------------------

def heading(root_rot):
    """Yaw (radians about Z) of the body's facing, from the hip line (the root's +X, the figure's left).

    The hip line stays level when a person bends over; the root's forward axis does not (it points at
    the floor), and taking the facing from it rotated a bent-over window by up to 180 degrees
    (2026-10-06, "Carry 4"). At rest the left is +X and the facing is -Y: yaw 0."""
    left = root_rot[..., :, 0]
    return torch.atan2(left[..., 1], left[..., 0])


def yaw_mat(a):
    c, s = torch.cos(a), torch.sin(a)
    z, o = torch.zeros_like(a), torch.ones_like(a)
    return torch.stack([c, -s, z, s, c, z, z, z, o], -1).reshape(a.shape + (3, 3))


def canonical(rot, pos, at):
    """Turn and move a window so frame ``at`` stands at the origin facing -Y.

    rot (N, T, J, 3, 3) local rotations (index 0 = root), pos (N, T, 3) root positions."""
    a = heading(rot[:, at, 0])
    Y = yaw_mat(-a)[:, None]
    rot = rot.clone()
    rot[:, :, 0] = Y @ rot[:, :, 0]
    origin = pos[:, at].clone()
    origin[:, 2] = 0
    pos = ((pos - origin[:, None]).unsqueeze(-1).transpose(-1, -2) @ Y.transpose(-1, -2)).squeeze(-2)
    return rot, pos, a, origin


# ---- the network ----------------------------------------------------------------------------------

def given_features(gmask):
    """Per-bone given mask (N, J) -> per-feature (N, 1, FEAT): 6 numbers a bone, the root position
    goes with the root bone."""
    J = len(BONES)
    f = gmask[:, :, None].expand(-1, J, 6).reshape(gmask.shape[0], J * 6)
    return torch.cat([f, gmask[:, :1].expand(-1, 3)], -1)[:, None]


def path_inputs(path, pmask):
    """The root-path channel (141000): (N, T, 3) root positions / hip height and (N, T, 1) 1 where
    one is given -> (N, T, 4), the positions zeroed where none is."""
    return torch.cat([path * pmask, pmask], -1)


def smooth_path(pos, width):
    """Root positions (N, T, 3) box-smoothed over ``width`` frames (odd; the ends repeated): a path
    rounder than the recording, as a spline through the keys is."""
    if width <= 1:
        return pos
    h = width // 2
    x = torch.cat([pos[:, :1].expand(-1, h, -1), pos, pos[:, -1:].expand(-1, h, -1)], 1)
    return x.unfold(1, width, 1).mean(-1)


class Inbetweener(nn.Module):
    def __init__(self, d=256, layers=6, heads=8, styles=0, given=False, contacts=False, path=False,
                 context_after=0, multi_key=False):
        super().__init__()
        self.given = given  # takes a per-bone mask of bones another layer drives (known in the gap)
        # takes the root path across the gap (141000): positions and a per-frame "given" flag
        self.path = path
        if not 0 <= int(context_after) < CONTEXT:
            raise ValueError(f"context_after {context_after}: 0 to {CONTEXT - 1} of the {CONTEXT} context frames")
        #: of the CONTEXT frames, how many come after key B (140000); the rest come before key A
        self.context_after = int(context_after)
        #: trained with any set of known frames in the window (142000): the time channel is two
        #: wide (to the next known frame, from the last), else one (time to key B)
        self.multi_key = bool(multi_key)
        self.times = 2 if multi_key else 1
        # also says, per frame, whether each foot is planted (left, right: logits); the app plants
        # the feet the network says are down inside the gap
        self.contacts = contacts
        self.style = nn.Embedding(styles, d) if styles else None
        self.inp = nn.Linear(FEAT * 2 + 1 + self.times + (len(BONES) if given else 0) + (PATH if path else 0), d)
        self.pos = nn.Parameter(torch.randn(WINDOW, d) * 0.02)
        enc = nn.TransformerEncoderLayer(d, heads, 4 * d, dropout=0.1, batch_first=True, norm_first=True)
        self.body = nn.TransformerEncoder(enc, layers)
        self.out = nn.Sequential(nn.LayerNorm(d), nn.Linear(d, FEAT))
        nn.init.zeros_(self.out[1].weight)
        nn.init.zeros_(self.out[1].bias)
        self.contact = nn.Linear(d, 2) if contacts else None

    def forward(self, known, interp, mask, ttg, style=None, gmask=None, with_contacts=False, path=None, pmask=None):
        """known/interp (N, T, FEAT), mask (N, T, 1) 1 = known frame, ttg (N, T, 1) time to key B
        (a ``multi_key`` model: (N, T, 2), :func:`known_times`), style (N,) style indices or (N, S) weights over the styles (several styles add up; 0 = any;
        ignored by a model trained without styles), gmask (N, J)
        1 = a bone known on every frame (another layer drives it; given back unchanged).
        ``with_contacts``: also return the contact logits (N, T, 2), left and right foot (a model
        with the head; ``None`` without it). ``path`` (N, T, 3) root positions / hip height the root
        should follow and ``pmask`` (N, T, 1) 1 on the frames that have one (a model with ``path``;
        none given: no path)."""
        if ttg.shape[-1] != self.times:
            raise ValueError(f"time channel {ttg.shape[-1]} wide: this model takes {self.times} "
                             f"({'known_times' if self.multi_key else 'time_to_go'})")
        if self.given:
            g = gmask if gmask is not None else torch.zeros(known.shape[0], len(BONES), device=known.device)
            fm = given_features(g)
            m = torch.maximum(mask, fm)
            x = torch.cat([known * m, interp, mask, ttg, g[:, None].expand(-1, known.shape[1], -1)], -1)
        else:
            x = torch.cat([known * mask, interp, mask, ttg], -1)
        if self.path:
            if path is None or pmask is None:
                path, pmask = torch.zeros_like(known[..., :3]), torch.zeros_like(mask)
            x = torch.cat([x, path_inputs(path, pmask)], -1)
        t = self.inp(x) + self.pos[: x.shape[1]]
        if self.style is not None:
            st = style if style is not None else torch.zeros(x.shape[0], dtype=torch.long, device=x.device)
            e = st.to(self.style.weight.dtype) @ self.style.weight if st.dim() == 2 else self.style(st)
            t = t + e[:, None]
        h = self.out[0](self.body(t))
        out = interp + self.out[1](h)
        if self.given and gmask is not None:
            out = out * (1 - fm) + known * fm
        if with_contacts:
            return out, (self.contact(h) if self.contact is not None else None)
        return out


def features(local, pos, hip):
    """(N, T, J, 3, 3), (N, T, 3) -> (N, T, FEAT): 6D rotations and the root position / hip height."""
    return torch.cat([mat_to_6d(local).flatten(-2), pos / hip], -1)


def unfeatures(f, hip):
    J = len(BONES)
    return sixd_to_mat(f[..., : J * 6].reshape(f.shape[:-1] + (J, 6))), f[..., J * 6 :] * hip


def interpolate(q, pos, a, b):
    """Plain interpolation between frames a and b (the same for the whole batch) of quaternion
    windows (N, T, J, 4) and root positions (N, T, 3): slerp / lerp inside the gap, the rest kept.
    Returns local matrices and positions."""
    a, b = int(a[0]), int(b[0])
    g = b - a
    q_i, p_i = q.clone(), pos.clone()
    t = torch.arange(1, g, device=q.device, dtype=q.dtype) / g
    q_i[:, a + 1 : b] = slerp(q[:, a : a + 1], q[:, b : b + 1], t[None, :, None, None])
    p_i[:, a + 1 : b] = pos[:, a : a + 1] + (pos[:, b : b + 1] - pos[:, a : a + 1]) * t[None, :, None]
    return quat_to_mat(q_i), p_i


def time_to_go(a, b, T, device):
    """(N, T, 1) time to key B: ``(b - i) / MAX_GAP``, negative on the frames after B
    (``context_after``; a window without them has no frame past B, so its values are as they
    always were), and the gap length (N, 1)."""
    i = torch.arange(T, device=device)[None].float()
    g = (b - a).float()[:, None].clamp(min=1)
    return ((b[:, None].float() - i) / MAX_GAP)[..., None], g


def layout(gap: int, after: int = 0) -> tuple[int, int, int]:
    """Where key A and key B sit in a window of ``gap`` frames with ``after`` context frames after
    B: ``(a, b, T)``. The window is always ``CONTEXT + gap + 1`` frames: ``CONTEXT - after``
    before and on A, the gap, B, then ``after`` more. ``after`` 0 is the window as it always was."""
    if not 0 <= after < CONTEXT:
        raise ValueError(f"after {after}: 0 to {CONTEXT - 1}")
    a = CONTEXT - after - 1
    return a, a + gap + 1, CONTEXT + gap + 1


def _known_neighbours(mask):
    """For every frame of (N, T, 1) known masks: the index of the last known frame at or before it
    and of the next known frame at or after it (N, T) each. A frame before the first known frame
    gets the first; one after the last gets the last."""
    N, T = mask.shape[:2]
    known = mask[..., 0] > 0.5
    idx = torch.arange(T, device=mask.device).expand(N, T)
    prev = torch.where(known, idx, torch.full_like(idx, -1)).cummax(-1).values
    nxt = torch.where(known, idx, torch.full_like(idx, T)).flip(-1).cummin(-1).values.flip(-1)
    first = nxt[:, :1].expand(N, T)
    last = prev[:, -1:].expand(N, T)
    prev = torch.where(prev < 0, first, prev)
    nxt = torch.where(nxt >= T, last, nxt)
    return prev, nxt


def known_times(mask):
    """The two-wide time channel of a ``multi_key`` model (N, T, 2): frames to the next known
    frame and from the last one, each over ``MAX_GAP``; 0 and 0 on a known frame. The window's
    first and last frames are always known (context, key B or the frames after it)."""
    prev, nxt = _known_neighbours(mask)
    i = torch.arange(mask.shape[1], device=mask.device).expand_as(prev)
    return torch.stack([(nxt - i).float() / MAX_GAP, (i - prev).float() / MAX_GAP], -1)


def interpolate_known(q, pos, mask):
    """:func:`interpolate` for any set of known frames (N, T, 1): slerp / lerp from the last known
    frame to the next on every unknown frame, the known ones kept. With the known frames being the
    context and key B it is the same interpolation. Returns local matrices and positions."""
    N, T = mask.shape[:2]
    prev, nxt = _known_neighbours(mask)
    i = torch.arange(T, device=q.device).expand(N, T)
    h = (nxt - prev).to(q.dtype)
    t = torch.where(h > 0, (i - prev).to(q.dtype) / h.clamp(min=1), torch.zeros_like(h))
    g = lambda x, k: x.gather(1, k.view(N, T, *[1] * (x.dim() - 2)).expand(N, T, *x.shape[2:]))  # noqa: E731
    q0, q1 = g(q, prev), g(q, nxt)
    p0, p1 = g(pos, prev), g(pos, nxt)
    known = mask > 0.5
    q_i = torch.where(known[..., None], q, slerp(q0, q1, t[:, :, None, None]))
    p_i = torch.where(known, pos, p0 + (p1 - p0) * t[..., None])
    return quat_to_mat(q_i), p_i


def positional(n, d):
    p = torch.arange(n)[:, None]
    i = torch.arange(0, d, 2)
    w = torch.exp(-math.log(10000.0) * i / d)
    out = torch.zeros(n, d)
    out[:, 0::2], out[:, 1::2] = torch.sin(p * w), torch.cos(p * w)
    return out
