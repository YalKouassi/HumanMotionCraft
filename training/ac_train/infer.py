"""Fill the frames between two keys of a clip with the trained in-betweener."""

from __future__ import annotations

import functools
import json
import pathlib

import numpy as np
import torch

from . import MODELS
from .clip import Clip
from .model import (
    BONES,
    CONTEXT,
    MAX_GAP,
    Inbetweener,
    canonical,
    features,
    interpolate,
    mat_to_quat,
    quat_to_mat,
    time_to_go,
    unfeatures,
    yaw_mat,
)
from .rig import Skeleton


def available() -> list[str]:
    return sorted(p.parent.name for p in MODELS.glob("*/model.pt"))


def card(tag: str) -> dict:
    """The tag's model.json ({} when unreadable); cached on its stamp, no network is loaded."""
    p = MODELS / tag / "model.json"
    try:
        return _card(str(p), p.stat().st_mtime_ns)
    except OSError:
        return {}


@functools.lru_cache(maxsize=32)
def _card(path: str, stamp: int) -> dict:
    try:
        return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def load(tag: str):
    """(network on the CPU in eval mode, its model card); reloads when training writes a new one."""
    return _load(tag, (MODELS / tag / "model.pt").stat().st_mtime_ns)


@functools.lru_cache(maxsize=4)
def _load(tag: str, stamp: int):
    d = MODELS / tag
    card = json.loads((d / "model.json").read_text(encoding="utf-8"))
    net = Inbetweener(styles=len(card.get("styles", [])), given=bool(card.get("given")))
    net.load_state_dict(torch.load(d / "model.pt", map_location="cpu", weights_only=True))
    net.eval()
    return net, card


@torch.no_grad()
def fill(clip: Clip, a: int, b: int, skel: Skeleton, tag: str, style: str | list[str] = "any", style_force: float = 1.0,
         ai_force: float = 1.0, trajectory: float = 0.0, head: float = 0.0, given=None) -> tuple[np.ndarray, np.ndarray]:
    """Rotations (b-a-1, B, 4) and root offsets (b-a-1, 3) for frames a+1 .. b-1.

    The network sets the solved bones (:data:`BONES`); the others follow the keys' own spline.
    Forces: ``style`` (one of the card's styles, or a list of them, added up) with ``style_force``
    (guidance: 0 = no style, 1 = the style, >1 = stronger);
    ``ai_force`` scales the network's change to the plain interpolation (0 = interpolation, 1 = the
    network); ``trajectory`` pulls the root onto the keys' own path; ``head`` keeps the neck and head
    on the keys' spline. ``given``: names of solved bones another layer drives; they keep the
    clip's own motion and the network fills the rest around them. Deterministic: CPU, float32, eval mode."""
    gap = b - a - 1
    if gap < 1 or gap > MAX_GAP:
        raise ValueError(f"AI fill needs 1..{MAX_GAP} frames between the keys, not {gap}")
    net, _ = load(tag)
    frames = [max(0, f) for f in range(a - CONTEXT + 1, a + 1)] + list(range(a + 1, b)) + [b]
    rots, roots = clip.sample(frames)
    idx = [skel.index[n] for n in BONES]
    q = torch.tensor(rots[:, idx], dtype=torch.float32)[None]
    pos = torch.tensor(roots * skel.hip_height, dtype=torch.float32)[None]
    rot, pos_c, yaw, origin = canonical(quat_to_mat(q), pos, CONTEXT - 1)
    truth = features(rot, pos_c, skel.hip_height)
    ia = torch.tensor([CONTEXT - 1])
    ib = ia + gap + 1
    rot_i, pos_i = interpolate(mat_to_quat(rot), pos_c, ia, ib)
    interp = features(rot_i, pos_i, skel.hip_height)
    T = q.shape[1]
    mask = torch.zeros(1, T, 1)
    mask[:, :CONTEXT] = 1
    mask[:, -1] = 1
    ttg, _ = time_to_go(ia, ib, T, "cpu")
    _, cd = load(tag)
    gmask = torch.tensor([[1.0 if b in (given or ()) else 0.0 for b in BONES]]) if given else None
    styles = cd.get("styles", [])
    chosen = [s for s in ([style] if isinstance(style, str) else style) if s != "any"]
    unknown = [s for s in chosen if s not in styles]
    if unknown:
        raise ValueError(f"styles {unknown}: model {tag} knows {styles}")
    if chosen and style_force != 0:
        w = torch.zeros(1, len(styles))
        w[0, [styles.index(s) for s in chosen]] = 1.0
        cond = net(truth, interp, mask, ttg, w, gmask=gmask)
        free = net(truth, interp, mask, ttg, torch.tensor([0]), gmask=gmask)
        pred = free + style_force * (cond - free)
    else:
        pred = net(truth, interp, mask, ttg, torch.tensor([0]) if styles else None, gmask=gmask)
    pred = interp + ai_force * (pred - interp)
    if gmask is not None:  # given bones stay exactly as the clip has them (also with an older model)
        from .model import given_features

        fm = given_features(gmask)
        pred = pred * (1 - fm) + truth * fm
    R, P = unfeatures(pred, skel.hip_height)
    Y = yaw_mat(yaw)[:, None]  # back to the clip's frame
    R[:, :, 0] = Y @ R[:, :, 0]
    P = (P.unsqueeze(-1).transpose(-1, -2) @ Y.transpose(-1, -2)).squeeze(-2) + origin[:, None]
    sl = slice(CONTEXT, CONTEXT + gap)
    out_r = rots[CONTEXT : CONTEXT + gap].copy()
    spline_r = out_r.copy()
    out_r[:, idx] = mat_to_quat(R[0, sl]).double().numpy()
    out_o = (P[0, sl] / skel.hip_height).double().numpy()
    if trajectory > 0:  # the keys' own root path (the clip's spline), never a Hold: a Hold key
        # kept the root still and dropped it into the next key in one frame (2026-10-06, "Carry 4")
        import copy

        smooth = copy.deepcopy(clip)
        for k in smooth.keys:
            if k.interp == "step":
                k.interp = "smooth"
        path = smooth.sample(list(range(a + 1, b)))[1]
        out_o = out_o + trajectory * (path - out_o)
    if head > 0:
        from . import quat

        for n in ("neck01", "neck02", "neck03", "head"):
            i = skel.index[n]
            out_r[:, i] = quat.slerp(out_r[:, i], spline_r[:, i], np.full(gap, head))
    return out_r, out_o
