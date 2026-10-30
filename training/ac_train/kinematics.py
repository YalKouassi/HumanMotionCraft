"""Batched (torch) measurements of the solved bones' motion: joint and end points, the feet on the
floor, foot contacts and anatomical joint angles. Training's losses and the evaluation share them.

Everything works on the network's bones (:data:`ac_train.model.BONES`) as the data has them: every
other bone at rest (identity), so a bone's world rotation is its nearest solved ancestor's, and an
end point (a wrist, the ball of a foot, the head) hangs off that ancestor by the rest offset.

Positions are in the sub-skeleton's frame: the root at ``root offset * hip height`` (the rest head
of the root is left out), so a height here is the world height minus the root's rest head height.
The canonical window (:func:`ac_train.model.canonical`) only turns about Z and moves along the
floor, so heights survive it.

Foot contact follows the quality scan's skate test (:mod:`ac_train.quality`): a foot is down when
its ankle and its ball are both within ``contact_m`` of their flat-standing heights, and planted
when it is down on two frames running and the slower of the two points moves less than
``skate_ms``. A foot pivoting on its ball or rolling off its heel is planted: one point stays put.
"""

from __future__ import annotations

import numpy as np
import torch

from . import anatomy
from .model import BONES
from .quality import FEET, FPS, THRESHOLDS, flat_heights

#: points beyond the solved bones' heads: the wrists, the balls of the feet, the head
ENDS = ("wrist.L", "wrist.R", "toe2-1.L", "toe2-1.R", "head")
POINTS = BONES + ENDS
#: the feet's points in :data:`POINTS`, in :data:`ac_train.quality.FEET` order (ankles, then balls)
FOOT_POINTS = tuple(POINTS.index(b) for b in FEET)
#: hands and feet: the points an animator watches most (the position loss weights them up)
EXTREMITIES = tuple(POINTS.index(b) for b in ("wrist.L", "wrist.R", "foot.L", "foot.R", "toe2-1.L", "toe2-1.R"))


def solved_ancestor(skel, i: int) -> int:
    """Index in :data:`BONES` of bone ``i`` or its nearest solved ancestor."""
    while skel.names[i] not in BONES:
        i = int(skel.parents[i])
        if i < 0:
            raise ValueError("a bone with no solved ancestor")
    return BONES.index(skel.names[i])


class Body:
    """FK of the solved bones, with the end points, the flat-foot heights and the joint angles."""

    def __init__(self, skel, device="cpu"):
        idx = [skel.index[b] for b in BONES]
        self.parents = [solved_ancestor(skel, int(skel.parents[i])) if skel.parents[i] >= 0 else -1 for i in idx]
        H = skel.heads
        heads = [H[i] for i in idx]
        self.offsets = torch.tensor(np.array([heads[j] - (heads[p] if p >= 0 else 0) for j, p in enumerate(self.parents)]),
                                    dtype=torch.float32, device=device)
        self.end_parent = [solved_ancestor(skel, skel.index[e]) for e in ENDS]
        self.end_offsets = torch.tensor(np.array([H[skel.index[e]] - heads[a] for e, a in zip(ENDS, self.end_parent)]),
                                        dtype=torch.float32, device=device)
        self.hip_height = float(skel.hip_height)
        #: ankles' and balls' heights standing flat, in this frame (:data:`FEET` order)
        self.flat = torch.tensor(flat_heights(skel) - H[0][2], dtype=torch.float32, device=device)
        self.joints = _Joints(skel, _possible_rules(), device)
        self.device = device

    def world(self, local):
        """local (..., J, 3, 3) -> world rotations (..., J, 3, 3)."""
        R = [None] * len(BONES)
        for j, p in enumerate(self.parents):
            R[j] = local[..., j, :, :] if p < 0 else R[p] @ local[..., j, :, :]
        return torch.stack(R, -3)

    def points(self, local, root, world=None):
        """local (..., J, 3, 3), root positions (..., 3) -> :data:`POINTS` positions (..., J + E, 3)."""
        R = self.world(local) if world is None else world
        P = [None] * len(BONES)
        for j, p in enumerate(self.parents):
            P[j] = root if p < 0 else P[p] + (R[..., p, :, :] @ self.offsets[j].unsqueeze(-1)).squeeze(-1)
        E = [P[a] + (R[..., a, :, :] @ self.end_offsets[k].unsqueeze(-1)).squeeze(-1) for k, a in enumerate(self.end_parent)]
        return torch.stack(P + E, -2)

    def foot_heights(self, pts):
        """(..., P, 3) points -> (..., 4) ankles' and balls' heights over standing flat (m)."""
        return pts[..., list(FOOT_POINTS), 2] - self.flat

    def planted(self, pts):
        """(N, T, P, 3) points -> (N, T, 2) left and right foot planted on frame t (and t - 1).
        Frame 0 has no frame before it: never planted."""
        h = self.foot_heights(pts)
        down = h < THRESHOLDS["contact_m"]
        flat = down[..., 0:2] & down[..., 2:4]
        speed = foot_speed(pts)
        out = torch.zeros_like(flat)
        out[:, 1:] = flat[:, 1:] & flat[:, :-1] & (speed[:, 1:] < THRESHOLDS["skate_ms"])
        return out

    def angles(self, world):
        """(..., J, 3, 3) world rotations -> {(joint, side): (..., 3) degrees} for the joints the
        possible limits name."""
        v = self.joints.angles(world)
        return {k: v[..., i, :] for i, k in enumerate(self.joints.keys)}

    def impossible(self, world, hinge=False):
        """Frames past the possible limits (``assets/rig/ranges.json``, :data:`ac_train.ranges.POSSIBLE`).
        (..., J, 3, 3) world rotations -> (...,) bool any joint past; with ``hinge`` also the summed
        degrees past (..., ) for a loss."""
        jt = self.joints
        a = jt.angles(world)  # (..., K, 3)
        v = a[..., jt.rule_joint, jt.rule_comp]  # (..., M)
        swing = torch.hypot(a[..., jt.rule_joint, 0], a[..., jt.rule_joint, 1])
        gate = swing < jt.rule_swing
        past = torch.relu(jt.rule_lo - v) + torch.relu(v - jt.rule_hi)
        bad = (gate & (past > 0)).any(-1)
        return (bad, (past * gate).sum(-1)) if hinge else bad


def geodesic(Ra, Rb):
    """(..., 3, 3) pairs -> the angle between them (radians), from both the sine and the cosine so
    a small angle reads true in float32 (``acos`` alone floors near zero) and keeps a gradient."""
    D = Ra.transpose(-1, -2) @ Rb
    c = (D.diagonal(dim1=-2, dim2=-1).sum(-1) - 1) / 2
    v = torch.stack([D[..., 2, 1] - D[..., 1, 2], D[..., 0, 2] - D[..., 2, 0], D[..., 1, 0] - D[..., 0, 1]], -1)
    s = (v.pow(2).sum(-1) + 1e-12).sqrt() / 2
    return torch.atan2(s, c)


def foot_speed(pts):
    """(N, T, P, 3) -> (N, T, 2) left and right foot speed along the floor (m/s) from the frame
    before: the slower of ankle and ball (0 on frame 0)."""
    f = pts[..., list(FOOT_POINTS), :2]
    v = torch.zeros(f.shape[:-1], dtype=f.dtype, device=f.device)
    v[:, 1:] = (f[:, 1:] - f[:, :-1]).norm(dim=-1) * FPS
    return torch.minimum(v[..., 0:2], v[..., 2:4])


def _possible_rules():
    from .ranges import POSSIBLE

    seen = []
    for joint, *_ in POSSIBLE:
        for side in ("L", "R") if anatomy.DEFS[joint][0] else ("L",):
            if (joint, side) not in seen:
                seen.append((joint, side))
    return [(j, s) for j, s in seen]


# ---- quaternions (torch, x y z w) and the anatomical angles -------------------------------------

def qmul(a, b):
    ax, ay, az, aw = a.unbind(-1)
    bx, by, bz, bw = b.unbind(-1)
    return torch.stack([aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
                        aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz], -1)


def qconj(q):
    return q * torch.tensor([-1.0, -1.0, -1.0, 1.0], dtype=q.dtype, device=q.device)


def mat_to_quat_smooth(R):
    """Rotation matrix -> unit quaternion, differentiable everywhere it matters (Shepperd's method,
    the branch picked per element): :func:`ac_train.model.mat_to_quat` puts square roots of
    numbers that can be 0 on every component, so its gradient is not finite at a right angle."""
    m00, m11, m22 = R[..., 0, 0], R[..., 1, 1], R[..., 2, 2]
    tr = m00 + m11 + m22
    eps = 1e-8
    cands = [
        torch.stack([R[..., 2, 1] - R[..., 1, 2], R[..., 0, 2] - R[..., 2, 0], R[..., 1, 0] - R[..., 0, 1], 1 + tr], -1),
        torch.stack([1 + m00 - m11 - m22, R[..., 0, 1] + R[..., 1, 0], R[..., 0, 2] + R[..., 2, 0], R[..., 2, 1] - R[..., 1, 2]], -1),
        torch.stack([R[..., 0, 1] + R[..., 1, 0], 1 - m00 + m11 - m22, R[..., 1, 2] + R[..., 2, 1], R[..., 0, 2] - R[..., 2, 0]], -1),
        torch.stack([R[..., 0, 2] + R[..., 2, 0], R[..., 1, 2] + R[..., 2, 1], 1 - m00 - m11 + m22, R[..., 1, 0] - R[..., 0, 1]], -1),
    ]
    pick = torch.stack([tr, m00, m11, m22], -1).argmax(-1)
    q = torch.stack(cands, -2).gather(-2, pick[..., None, None].expand(*pick.shape, 1, 4)).squeeze(-2)
    q = q / (q.pow(2).sum(-1, keepdim=True) + eps).sqrt()
    return torch.where(q[..., 3:] < 0, -q, q)


class _Joints:
    """:class:`ac_train.anatomy.Joint` for several joints at once, on the solved bones, in torch."""

    def __init__(self, skel, keys, device):
        from .ranges import POSSIBLE

        self.keys = list(keys)
        js = [anatomy.Joint(skel, joint, side) for joint, side in self.keys]
        f = lambda v: torch.tensor(np.asarray(v, dtype=np.float64), dtype=torch.float32, device=device)  # noqa: E731
        self.u = f([j.u for j in js])
        self.zero_turn = f([j.zero_turn for j in js])
        self.flex_axis, self.abd_axis = f([j.flex_axis for j in js]), f([j.abd_axis for j in js])
        self.twist_sign = f([j.twist_sign for j in js])
        self.proximal = [solved_ancestor(skel, j.proximal) for j in js]
        self.distal = [solved_ancestor(skel, j.distal) for j in js]
        rj, rc, lo, hi, sw = [], [], [], [], []
        for joint, comp, l, h, max_swing in POSSIBLE:
            for k, (jn, _side) in enumerate(self.keys):
                if jn == joint:
                    rj.append(k)
                    rc.append(anatomy.COMPONENTS.index(comp))
                    lo.append(l)
                    hi.append(h)
                    sw.append(np.inf if max_swing is None else max_swing)
        self.rule_joint = torch.tensor(rj, device=device)
        self.rule_comp = torch.tensor(rc, device=device)
        self.rule_lo, self.rule_hi, self.rule_swing = f(lo), f(hi), f(sw)

    def angles(self, world):
        """(..., J, 3, 3) world rotations -> (..., K, 3) [flexion, abduction, rotation] degrees."""
        rel = world[..., self.proximal, :, :].transpose(-1, -2) @ world[..., self.distal, :, :]
        t = qmul(mat_to_quat_smooth(rel), self.zero_turn.expand(rel.shape[:-2] + (4,)))
        u = self.u
        along = (t[..., :3] * u).sum(-1)
        w = torch.cat([u * along[..., None], t[..., 3:4]], -1)
        w = w / (w.pow(2).sum(-1, keepdim=True) + 1e-12).sqrt()
        swing = qmul(t, qconj(w))
        swing = torch.where(swing[..., 3:4] < 0, -swing, swing)
        v = swing[..., :3]
        n = (v.pow(2).sum(-1, keepdim=True) + 1e-12).sqrt()
        s = 2.0 * v * torch.atan2(n, swing[..., 3:4].clamp(-1.0, 1.0)) / n
        twist = 2.0 * torch.atan2((w[..., :3] * u).sum(-1), w[..., 3])
        twist = torch.where(twist > np.pi, twist - 2 * np.pi, twist)
        twist = torch.where(twist <= -np.pi, twist + 2 * np.pi, twist)
        deg = 180.0 / np.pi
        return torch.stack([(s * self.flex_axis).sum(-1) * deg, (s * self.abd_axis).sum(-1) * deg,
                            twist * self.twist_sign * deg], -1)
