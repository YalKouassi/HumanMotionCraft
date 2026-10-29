"""Joint positions of any humanoid skeleton -> the rig's pose-library rotations and root offsets.

The source gives world positions of a few landmarks per frame (hips, neck, head, and per side the
shoulder, elbow, wrist, hip, knee, ankle, toe). The retarget walks the rig's bones parents first, all
frames at once, and turns each solved bone so that its child joint points the way the source's
segment points (the smallest turn). The root and the chest also take their facing from the hip and
shoulder lines, and the spine bends evenly over its five bones. Bones without a source (fingers,
eyes, twist bones) keep the rest rotation.

Result: rotations (frames, 104, 4) and root offsets (frames, 3) in the space of
:class:`ac_train.clip.Clip` (rig world axes: Z up), ready for ``Clip`` keys or training.
"""

from __future__ import annotations

import numpy as np

from .rig import Skeleton

LANDMARKS = ("hips", "neck", "head",
             "l_shoulder", "l_elbow", "l_wrist", "l_hip", "l_knee", "l_ankle", "l_toe",
             "r_shoulder", "r_elbow", "r_wrist", "r_hip", "r_knee", "r_ankle", "r_toe")

# (rig bone, its child joint whose direction is matched, source segment from -> to; from None = the
# bone's own head, i.e. aim at the source point)
AIMS = [
    ("neck01", "head", "neck", "head"),
    # a clavicle starts beside the neck, not at it: it aims at the shoulder point itself (from = None)
    *[(f"clavicle.{s}", f"upperarm01.{s}", None, f"{c}_shoulder") for s, c in (("L", "l"), ("R", "r"))],
    *[(f"upperarm01.{s}", f"lowerarm01.{s}", f"{c}_shoulder", f"{c}_elbow") for s, c in (("L", "l"), ("R", "r"))],
    *[(f"lowerarm01.{s}", f"wrist.{s}", f"{c}_elbow", f"{c}_wrist") for s, c in (("L", "l"), ("R", "r"))],
    *[(f"upperleg01.{s}", f"lowerleg01.{s}", f"{c}_hip", f"{c}_knee") for s, c in (("L", "l"), ("R", "r"))],
    *[(f"lowerleg01.{s}", f"foot.{s}", f"{c}_knee", f"{c}_ankle") for s, c in (("L", "l"), ("R", "r"))],
    *[(f"foot.{s}", f"toe3-1.{s}", f"{c}_ankle", f"{c}_toe") for s, c in (("L", "l"), ("R", "r"))],
]
SPINE = ("spine05", "spine04", "spine03", "spine02", "spine01")
# hinge limbs: (upper bone, its lower bone, the lower's child, source root -> mid -> end). The smallest
# turn leaves the upper bone's twist to chance, so the knee/elbow then bent sideways: plain mocap
# "Carry 1" put up to 76 deg of a knee's turn off its hinge (2026-10-11). The upper bone's twist is
# set so the rest bend plane matches the source's root-mid-end plane; the lower bone's own smallest
# turn then stays on the hinge.
HINGES = [
    *[(f"upperleg01.{s}", f"lowerleg01.{s}", f"foot.{s}", f"{c}_hip", f"{c}_knee", f"{c}_ankle") for s, c in (("L", "l"), ("R", "r"))],
    *[(f"upperarm01.{s}", f"lowerarm01.{s}", f"wrist.{s}", f"{c}_shoulder", f"{c}_elbow", f"{c}_wrist") for s, c in (("L", "l"), ("R", "r"))],
]
PELVIS_SHARE = 0.5  # without a source pelvis: how far it follows the trunk's tilt (the spine bends the rest)
_STRAIGHT = 20.0  # deg: below this bend the source plane is ignored (the roll is interpolated in time)
_TRUST = 35.0  # deg: from this bend on, the source plane sets the roll outright
# Arms reach in proportion: the rig's arm length over the actor's scales the way to the wrist. The
# legs set the scale (scale_to), and 100STYLE's actor has arms 8% longer than the rig's, so his
# straight arm left the rig's locked straight 4 cm short of the wrist; as the elbow bent the rig's
# stayed straight until the wrist came in reach, then bent 45 deg in a frame ("Angry" backwards run,
# the scan's "pops", 2026-10-28). The legs still reach the ankle itself: the feet's contacts matter
# more than their bend, and a scaled reach slides a planted foot as the hips move.
ARM_REACH = True


def _unit(v):
    return v / np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-12)


def _between(a, b):
    """Batched smallest rotation (..., 3, 3) taking direction a to direction b."""
    a, b = _unit(a), _unit(b)
    v = np.cross(a, b)
    c = np.sum(a * b, -1)[..., None, None]
    K = np.zeros(v.shape[:-1] + (3, 3))
    K[..., 0, 1], K[..., 0, 2], K[..., 1, 2] = -v[..., 2], v[..., 1], -v[..., 0]
    K = K - np.swapaxes(K, -1, -2)
    I = np.broadcast_to(np.eye(3), K.shape)
    out = I + K + K @ K / np.maximum(1 + c, 1e-9)
    # opposite directions: half turn about any perpendicular
    flip = (c[..., 0, 0] < -0.999999)
    if np.any(flip):
        perp = _unit(np.cross(a[flip], np.array([1.0, 0, 0])) + 1e-9)
        out[flip] = 2 * perp[:, :, None] * perp[:, None, :] - np.eye(3)
    return out


def _frame(primary, secondary):
    """Batched orthonormal frames with columns (primary, secondary', primary x secondary')."""
    x = _unit(primary)
    y = _unit(secondary - x * np.sum(secondary * x, -1, keepdims=True))
    return np.stack([x, y, np.cross(x, y)], -1)


def _power(R, t):
    """R^t for batched rotations (axis-angle scaling)."""
    ang = np.arccos(np.clip((np.trace(R, axis1=-2, axis2=-1) - 1) / 2, -1, 1))
    axis = np.stack([R[..., 2, 1] - R[..., 1, 2], R[..., 0, 2] - R[..., 2, 0], R[..., 1, 0] - R[..., 0, 1]], -1)
    axis = _unit(axis + 1e-12)
    a = (ang * t)[..., None, None]
    K = np.zeros(R.shape)
    K[..., 0, 1], K[..., 0, 2], K[..., 1, 2] = -axis[..., 2], axis[..., 1], -axis[..., 0]
    K = K - np.swapaxes(K, -1, -2)
    return np.eye(3) + np.sin(a) * K + (1 - np.cos(a)) * K @ K


def _mat_to_quat(R):
    """Batched rotation matrices -> quaternions (x, y, z, w), w >= 0."""
    w = np.sqrt(np.maximum(0, 1 + R[..., 0, 0] + R[..., 1, 1] + R[..., 2, 2])) / 2
    x = np.sqrt(np.maximum(0, 1 + R[..., 0, 0] - R[..., 1, 1] - R[..., 2, 2])) / 2
    y = np.sqrt(np.maximum(0, 1 - R[..., 0, 0] + R[..., 1, 1] - R[..., 2, 2])) / 2
    z = np.sqrt(np.maximum(0, 1 - R[..., 0, 0] - R[..., 1, 1] + R[..., 2, 2])) / 2
    x = np.copysign(x, R[..., 2, 1] - R[..., 1, 2])
    y = np.copysign(y, R[..., 0, 2] - R[..., 2, 0])
    z = np.copysign(z, R[..., 1, 0] - R[..., 0, 1])
    q = np.stack([x, y, z, w], -1)
    return q / np.linalg.norm(q, axis=-1, keepdims=True)


def _rodrigues(axis, angle):
    """Batched rotations (F, 3, 3) by ``angle`` (F,) about unit ``axis`` (F, 3)."""
    K = np.zeros(axis.shape[:-1] + (3, 3))
    K[..., 0, 1], K[..., 0, 2], K[..., 1, 2] = -axis[..., 2], axis[..., 1], -axis[..., 0]
    K = K - np.swapaxes(K, -1, -2)
    a = np.asarray(angle)[..., None, None]
    return np.eye(3) + np.sin(a) * K + (1 - np.cos(a)) * K @ K


def _limb(Rp, A, hinge, H, idx, src, reach=1.0):
    """Two-bone solve of a hinge limb: (upper bone's world rotation (F, 3, 3), where its end goes (F, 3)).

    ``reach`` scales the way from the rig's root joint (hip/shoulder) to the source's end: the rig's
    limb length over the source's makes a straight source limb a straight rig limb (see
    :data:`ARM_REACH`).

    The rig's own lengths reach the source's ankle/wrist (copying segment directions left knees 7 cm and
    feet up to 9.5 cm off: its proportions are not the actor's), in ONE bend plane that both places
    the knee/elbow and rolls the upper bone, so the lower bone's turn stays on its hinge. (Taking the
    smallest turn left the roll to chance, and knees bent up to 76 deg sideways; two separate planes,
    one for the knee and one for the roll, put it back to 100 deg.) The plane's roll about the
    hip->ankle line comes from the source where its limb is clearly bent; a near-straight limb's plane
    is noise that flips side frame to frame, so there the roll is interpolated in time between the
    clearly bent frames (a hinge cannot bend backwards, so a clearly bent frame shows the true side).
    Moderately bent frames pull toward their own plane, on whichever side is nearer. (2026-10-11)"""
    upper, lower, end, a, m, z = hinge
    rest_n = _unit(np.cross(H[idx[lower]] - H[idx[upper]], H[idx[end]] - H[idx[lower]]))
    la = np.linalg.norm(H[idx[lower]] - H[idx[upper]])
    lb = np.linalg.norm(H[idx[end]] - H[idx[lower]])
    to = (src[z] - A) * reach
    u = _unit(to)
    flat = lambda v: _unit(v - u * np.sum(v * u, -1, keepdims=True))  # noqa: E731
    seg1, seg2 = src[m] - src[a], src[z] - src[m]
    want = np.cross(seg1, seg2)
    bend = np.degrees(np.arctan2(np.linalg.norm(want, axis=-1), np.sum(seg1 * seg2, -1)))
    ref = flat(np.einsum("fij,j->fi", Rp, rest_n))  # the parent's idea of the plane: a smooth reference
    want = flat(want)
    theta = np.arctan2(np.sum(u * np.cross(ref, want), -1), np.sum(ref * want, -1))
    strong = np.flatnonzero(bend >= _TRUST)
    if len(strong):
        held = np.interp(np.arange(len(theta)), strong, np.unwrap(theta[strong]))
        tw = np.clip((bend - _STRAIGHT) / (_TRUST - _STRAIGHT), 0.0, 1.0)
        near = np.mod(theta - held + np.pi / 2, np.pi) - np.pi / 2  # to want or -want, whichever is nearer
        angle = held + tw * near
    else:
        angle = np.zeros(len(theta))  # never clearly bent: the parent's plane
    n = np.einsum("fij,fj->fi", _rodrigues(u, angle), ref)
    # the knee/elbow in that plane: n = (K - A) x (T - K) points along -(u x w), so w = u x n
    d = np.clip(np.linalg.norm(to, axis=-1), abs(la - lb) + 1e-6, la + lb - 1e-6)[:, None]
    x = (la * la - lb * lb + d * d) / (2 * d)
    h = np.sqrt(np.maximum(la * la - x * x, 0.0))
    knee = A + u * x + _unit(np.cross(u, n)) * h
    # the upper bone: aim its child at the knee, then roll it about itself until its rest normal is n
    Ri = Rp @ np.eye(3)
    Ri = _between(np.einsum("fij,j->fi", Ri, H[idx[lower]] - H[idx[upper]]), knee - A) @ Ri
    axis = _unit(knee - A)
    cur = np.einsum("fij,j->fi", Ri, rest_n)
    cur = _unit(cur - axis * np.sum(cur * axis, -1, keepdims=True))
    roll = np.arctan2(np.sum(axis * np.cross(cur, n), -1), np.sum(cur * n, -1))
    return _rodrigues(axis, roll) @ Ri, A + to


def _pelvis_up(src):
    """(F, 3) up direction of the pelvis.

    Taking the whole trunk's tilt (hips -> neck) left the spine nothing to bend: "Carry 1" frame 36
    put 152 deg into each hip, the spine bent 3 deg and the skin crushed at the groin. A fixed share
    of it (half the trunk, half the thighs) was a guess and hunched the back into a C (2026-10-11).
    The source records its pelvis: the hips -> first spine joint segment ("spine"). That segment
    leans a little in the source's own rest pose, so its lean against the trunk is measured on the
    take's upright frames and taken back off, about the hip line. Sources without "spine" fall back
    to the fixed share."""
    trunk = _unit(src["neck"] - src["hips"])
    seg = src["spine"] - src["hips"] if "spine" in src else None
    if seg is None or np.median(np.linalg.norm(seg, axis=-1)) < 0.01:  # absent, or no length to aim with
        thighs = _unit(src["l_hip"] + src["r_hip"] - src["l_knee"] - src["r_knee"])  # knees -> hips
        return _unit(PELVIS_SHARE * trunk + (1 - PELVIS_SHARE) * thighs)
    up = _unit(src["spine"] - src["hips"])
    side = _unit(src["l_hip"] - src["r_hip"])
    ang = np.arctan2(np.sum(side * np.cross(trunk, up), -1), np.sum(trunk * up, -1))  # lean about the hip line
    upright = np.degrees(np.arccos(np.clip(trunk[:, 2], -1, 1))) < 15.0
    off = float(np.median(ang[upright])) if upright.sum() >= 10 else float(np.median(ang))
    K = np.zeros(side.shape[:-1] + (3, 3))
    K[..., 0, 1], K[..., 0, 2], K[..., 1, 2] = -side[..., 2], side[..., 1], -side[..., 0]
    K = K - np.swapaxes(K, -1, -2)
    R = np.eye(3) + np.sin(-off) * K + (1 - np.cos(-off)) * K @ K
    return _unit(np.einsum("fij,fj->fi", R, up))


def retarget(skel: Skeleton, src: dict[str, np.ndarray]) -> tuple[np.ndarray, np.ndarray]:
    """Rotations (F, B, 4) and root offsets (F, 3) of the rig following the source landmarks.

    ``src`` maps every name of :data:`LANDMARKS` to (F, 3) positions in the rig's axes (Z up) and
    metres; the hips' rest height should be scaled to the rig's (see :func:`scale_to`).
    """
    F = src["hips"].shape[0]
    B = len(skel.names)
    idx, H, par = skel.index, skel.heads, skel.parents
    R = np.zeros((F, B, 3, 3))
    P = np.zeros((F, B, 3))
    local = np.broadcast_to(np.eye(3), (F, B, 3, 3)).copy()
    aims = {b: (c, a, z) for b, c, a, z in AIMS}
    hinges = {h[0]: h for h in HINGES}
    ends = {}  # lower bone -> where its end goes (the hinge solve's target)

    # root: hips position, facing from the hip line, tilted as the source's own pelvis (see _pelvis_up)
    hip_line = (H[idx["upperleg01.L"]] - H[idx["upperleg01.R"]])[None]
    want = _frame(_pelvis_up(src), src["l_hip"] - src["r_hip"])
    rest = _frame((H[idx["neck01"]] - H[0])[None], hip_line)
    local[:, 0] = want @ np.swapaxes(rest, -1, -2)
    # place the pelvis so its hip joints land on the source's: the rig's root sits ~11.5 cm above them,
    # so pinning the root on the source "hips" joint swung the hip joints (and the legs) 10-15 cm
    # backwards whenever the pelvis tipped forward ("Carry 1" frames 20-40, 2026-10-11)
    mid_rest = (H[idx["upperleg01.L"]] + H[idx["upperleg01.R"]]) / 2
    mid_src = (src["l_hip"] + src["r_hip"]) / 2
    root = (mid_src - np.einsum("fij,j->fi", local[:, 0], mid_rest - H[0]) - H[0]) / skel.hip_height

    spine_turn = None
    for i in range(B):
        p = par[i]
        if p < 0:
            R[:, i], P[:, i] = local[:, i], H[i] + root * skel.hip_height
            continue
        P[:, i] = P[:, p] + np.einsum("fij,j->fi", R[:, p], H[i] - H[p])
        Ri = R[:, p] @ local[:, i]
        name = skel.names[i]
        if name in SPINE:
            if spine_turn is None:  # the chest's turn against the root, spread over the 5 spine bones
                cur = _frame(np.einsum("fij,j->fi", Ri, H[idx["neck01"]] - H[i]),
                             np.einsum("fij,j->fi", Ri, H[idx["upperarm01.L"]] - H[idx["upperarm01.R"]]))
                want_dir = src["neck"] - P[:, i]
                tgt = _frame(want_dir, src["l_shoulder"] - src["r_shoulder"])
                # an even split over bones that do not lie on one line misses the neck once the spine
                # bends (3.9 cm with the pelvis taking half the tilt): re-aim the target at the miss
                chain = [idx[n] for n in SPINE] + [idx["neck01"]]
                for _ in range(4):
                    spine_turn = _power(tgt @ np.swapaxes(cur, -1, -2), 1.0 / len(SPINE))
                    M, end = Ri, np.zeros_like(want_dir)
                    for k0, k1 in zip(chain, chain[1:]):
                        M = spine_turn @ M
                        end = end + np.einsum("fij,j->fi", M, H[k1] - H[k0])
                    tgt = _between(end, want_dir) @ tgt
            Ri = spine_turn @ Ri
        elif name in aims:
            child, a, z = aims[name]
            cur = np.einsum("fij,j->fi", Ri, H[idx[child]] - H[i])
            if name in hinges:  # the thigh/upper arm: two-bone solve in one bend plane (see _limb)
                upper, lower, end, sa, sm, sz = hinges[name]
                reach = 1.0
                if ARM_REACH and name.startswith("upperarm"):
                    own = np.linalg.norm(H[idx[lower]] - H[idx[upper]]) + np.linalg.norm(H[idx[end]] - H[idx[lower]])
                    theirs = np.median(np.linalg.norm(src[sm] - src[sa], axis=-1) + np.linalg.norm(src[sz] - src[sm], axis=-1))
                    reach = own / max(float(theirs), 1e-9)
                Ri, ends[lower] = _limb(R[:, p] @ local[:, i], P[:, i], hinges[name], H, idx, src, reach)
            else:
                # the shin/forearm points at its hinge solve's end, every other bone along its segment
                aim = ends[name] - P[:, i] if name in ends else src[z] - (P[:, i] if a is None else src[a])
                Ri = _between(cur, aim) @ Ri
        local[:, i] = np.swapaxes(R[:, p], -1, -2) @ Ri
        R[:, i] = Ri
    return _mat_to_quat(local), root


def scale_to(skel: Skeleton, src: dict[str, np.ndarray], rest_frame: int = 0) -> dict[str, np.ndarray]:
    """Scale the source so its leg length (hip -> ankle) matches the rig's; positions keep their axes."""
    leg_src = np.median(np.linalg.norm(src["l_hip"] - src["l_knee"], axis=-1) + np.linalg.norm(src["l_knee"] - src["l_ankle"], axis=-1))
    H, i = skel.heads, skel.index
    leg_rig = np.linalg.norm(H[i["upperleg01.L"]] - H[i["lowerleg01.L"]]) + np.linalg.norm(H[i["lowerleg01.L"]] - H[i["foot.L"]])
    s = leg_rig / max(leg_src, 1e-9)
    return {k: v * s for k, v in src.items()}
