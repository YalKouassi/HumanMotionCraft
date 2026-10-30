"""Training's loss: terms that match how a fill looks, each logged on its own so none takes over
unseen. All of them compare the fill with the recorded motion over the gap; none penalises speed
itself, so a punch, a jump and a slow walk each keep their own timing.

- ``feat``: L1 on the free bones' features (6D rotations and the root position);
- ``pos``: joint position error, the wrists and feet (:data:`ac_train.kinematics.EXTREMITIES`)
  weighted :data:`EXTREMITY_WEIGHT` times the rest;
- ``vel``: velocity error over the gap and across both keys (leaving A, arriving at B);
- ``jerk``: third difference past the recording's own (extra shake only; a real hit keeps its jerk);
- ``rot``: per-bone rotation (geodesic) error;
- ``possible``: degrees past the possible joint limits (``assets/rig/ranges.json``), so a fill
  rarely needs ``keep_possible`` to bend it back;
- with a contact head (:class:`ac_train.model.Inbetweener` ``contacts``): ``contact`` the left and
  right foot contacts against the recording's (the quality scan's test), ``skate`` the fill's foot
  speed where the recording's foot is planted, ``floor`` the fill's feet under the floor.

Positions are in hip heights, so the weights don't depend on the rig's size.
"""

from __future__ import annotations

import torch

from .kinematics import EXTREMITIES, POINTS, Body, foot_speed, geodesic
from .model import given_features, unfeatures
from .quality import FPS

EXTREMITY_WEIGHT = 2.0
#: each term's weight (``train --loss-weights`` changes them)
WEIGHTS = {"feat": 1.0, "pos": 2.0, "vel": 0.5, "jerk": 0.002, "rot": 0.1, "possible": 0.002,
           "contact": 0.1, "skate": 0.5, "floor": 2.0}


def _point_weights(device):
    w = torch.ones(len(POINTS), device=device)
    w[list(EXTREMITIES)] = EXTREMITY_WEIGHT
    return w / w.mean()


def terms(pred, truth, gm, a: int, b: int, body: Body, contacts=None) -> dict[str, torch.Tensor]:
    """The loss terms of a batch. ``pred``, ``truth`` (N, T, FEAT) features of the windows (truth's
    known frames are the inputs), ``gm`` (N, J) the given bones, ``a``, ``b`` key A and key B,
    ``contacts`` (N, T, 2) the contact logits (models with the head)."""
    hip = body.hip_height
    g = b - a - 1
    gap = slice(a + 1, b)
    # the fill as the app keeps it: the network's gap frames between the known ones (its output on
    # a known frame is never used)
    known = torch.ones(pred.shape[1], 1, device=pred.device)
    known[gap] = 0
    pred = pred * (1 - known) + truth * known
    out = {}
    free = 1 - given_features(gm)  # given bones are copied: no loss on them
    out["feat"] = ((pred[:, gap] - truth[:, gap]).abs() * free).sum() / (free.sum() * g).clamp(min=1)
    Rp, Pp = unfeatures(pred, hip)
    Wp = body.world(Rp)
    pp = body.points(Rp, Pp, Wp)
    with torch.no_grad():
        Rt, Pt = unfeatures(truth, hip)
        pt = body.points(Rt, Pt)
    w = _point_weights(pred.device)
    out["pos"] = ((pp[:, gap] - pt[:, gap]).norm(dim=-1) * w).mean() / hip
    # velocity from key A to key B: the gap frames' and both seams'
    span = slice(a, b + 1)
    vp, vt = pp[:, span].diff(dim=1), pt[:, span].diff(dim=1)
    out["vel"] = ((vp - vt).norm(dim=-1) * w).mean() * FPS / hip
    lo = max(a - 1, 0)
    if b + 1 - lo >= 4:
        jp, jt = pp[:, lo : b + 1].diff(n=3, dim=1), pt[:, lo : b + 1].diff(n=3, dim=1)
        out["jerk"] = torch.relu(jp.norm(dim=-1) - jt.norm(dim=-1)).mean() * FPS**3 / hip
    else:
        out["jerk"] = pred.new_zeros(())
    ang = geodesic(Rp[:, gap], Rt[:, gap])  # (N, g, J)
    fb = 1 - gm
    out["rot"] = (ang * fb[:, None]).sum() / (fb.sum() * g).clamp(min=1)
    _, over = body.impossible(Wp[:, gap], hinge=True)
    out["possible"] = over.mean()
    if contacts is not None:
        planted = body.planted(pt)  # (N, T, 2)
        out["contact"] = torch.nn.functional.binary_cross_entropy_with_logits(contacts[:, gap], planted[:, gap].float())
        speed = foot_speed(pp)[:, gap]
        pl = planted[:, gap].float()
        out["skate"] = (speed * pl).sum() / pl.sum().clamp(min=1) / hip
        low = body.foot_heights(pp[:, gap]).min(-1).values
        out["floor"] = torch.relu(-low).mean() / hip
    return out


def total(t: dict[str, torch.Tensor], weights: dict[str, float] = WEIGHTS) -> torch.Tensor:
    return sum(weights.get(k, 0.0) * v for k, v in t.items())
