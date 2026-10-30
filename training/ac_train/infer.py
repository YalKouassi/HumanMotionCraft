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
    interpolate_known,
    known_times,
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


def load(tag: str, checkpoint: str = "model"):
    """(network on the CPU in eval mode, its model card); reloads when training writes a new one.
    ``checkpoint``: ``model`` (the last step) or ``best`` (the best by the evaluation's score)."""
    return _load(tag, checkpoint, (MODELS / tag / f"{checkpoint}.pt").stat().st_mtime_ns)


@functools.lru_cache(maxsize=4)
def _load(tag: str, checkpoint: str, stamp: int):
    d = MODELS / tag
    card = json.loads((d / "model.json").read_text(encoding="utf-8"))
    net = Inbetweener(styles=len(card.get("styles", [])), given=bool(card.get("given")), contacts=bool(card.get("contacts")),
                      path=bool(card.get("path")), context_after=int(card.get("context_after", 0)),
                      multi_key=bool(card.get("multi_key", False)))
    net.load_state_dict(torch.load(d / f"{checkpoint}.pt", map_location="cpu", weights_only=True))
    net.eval()
    return net, card


def _keys_path(clip: Clip, a: int, b: int) -> np.ndarray:
    """The keys' own root path (the clip's spline) on frames a+1 .. b-1, never a Hold: a Hold key
    kept the root still and dropped it into the next key in one frame (2026-10-06, "Carry 4")."""
    import copy

    smooth = copy.deepcopy(clip)
    for k in smooth.keys:
        if k.interp == "step":
            k.interp = "smooth"
    return smooth.sample(list(range(a + 1, b)))[1]


def window_sides(context_after: int, b: int, length: int, loop: bool) -> tuple[int, int]:
    """How many context frames a fill ending on key ``b`` takes before key A and after B: ``after``
    is the model's ``context_after``, fewer where the clip ends first (its frames stop at
    ``length - 1``; a loop goes on), and the frames before A make up :data:`CONTEXT`."""
    after = context_after if loop else max(0, min(context_after, length - 1 - b))
    return CONTEXT - after, after


@torch.no_grad()
def fill(clip: Clip, a: int, b: int, skel: Skeleton, tag: str, style: str | list[str] = "any", style_force: float = 1.0,
         ai_force: float = 1.0, trajectory: float = 0.0, head: float = 0.0, given=None,
         checkpoint: str = "model", keys=None) -> tuple[np.ndarray, np.ndarray]:
    """Rotations (b-a-1, B, 4) and root offsets (b-a-1, 3) for frames a+1 .. b-1.

    ``keys``: frames strictly between ``a`` and ``b`` known too (keys of the clip; a model trained
    with several keys in a window, ``multi_key``), so the gaps between them fill in one pass;
    those frames come back as the clip has them.

    The network sets the solved bones (:data:`BONES`); the others follow the keys' own spline.
    Forces: ``style`` (one of the card's styles, or a list of them, added up) with ``style_force``
    (guidance: 0 = no style, 1 = the style, >1 = stronger);
    ``ai_force`` scales the network's change to the plain interpolation (0 = interpolation, 1 = the
    network); ``trajectory`` pulls the root onto the keys' own path (a model trained with the path,
    141000, is also given it, so the legs and lean follow it; the blend afterwards still puts the
    root on it); ``head`` keeps the neck and head on the keys' spline. ``given``: names of solved
    bones another layer drives; they keep the clip's own motion and the network fills the rest around them. Deterministic: CPU, float32, eval mode."""
    gap = b - a - 1
    if gap < 1 or gap > MAX_GAP:
        raise ValueError(f"AI fill needs 1..{MAX_GAP} frames between the keys, not {gap}")
    net, cd = load(tag, checkpoint)
    inner = sorted(set(int(k) for k in (keys or ())))
    if inner and not net.multi_key:
        raise ValueError(f"model {tag} fills one gap at a time: it was not trained with several keys in a window")
    if any(k <= a or k >= b for k in inner):
        raise ValueError(f"keys {inner}: inside {a}..{b} only")
    before, after = window_sides(net.context_after, b, clip.length, clip.loop)
    frames = [max(0, f) for f in range(a - before + 1, a + 1)] + list(range(a + 1, b + 1)) + list(range(b + 1, b + after + 1))
    rots, roots = clip.sample(frames)
    idx = [skel.index[n] for n in BONES]
    q = torch.tensor(rots[:, idx], dtype=torch.float32)[None]
    pos = torch.tensor(roots * skel.hip_height, dtype=torch.float32)[None]
    rot, pos_c, yaw, origin = canonical(quat_to_mat(q), pos, before - 1)
    truth = features(rot, pos_c, skel.hip_height)
    ia = torch.tensor([before - 1])
    ib = ia + gap + 1
    T = q.shape[1]
    mask = torch.zeros(1, T, 1)
    mask[:, :before] = 1
    mask[:, before + gap :] = 1  # key B and the frames after it
    for k in inner:
        mask[:, before + k - a - 1] = 1
    if net.multi_key:
        rot_i, pos_i = interpolate_known(mat_to_quat(rot), pos_c, mask)
        ttg = known_times(mask)
    else:
        rot_i, pos_i = interpolate(mat_to_quat(rot), pos_c, ia, ib)
        ttg, _ = time_to_go(ia, ib, T, "cpu")
    interp = features(rot_i, pos_i, skel.hip_height)
    gmask = torch.tensor([[1.0 if b in (given or ()) else 0.0 for b in BONES]]) if given else None
    path = pmask = None
    if trajectory > 0:
        keys_path = _keys_path(clip, a, b)
        if net.path:
            pm = (keys_path * skel.hip_height - origin[0].double().numpy()) @ yaw_mat(-yaw)[0].double().numpy().T
            path = torch.zeros(1, T, 3)
            path[0, before : before + gap] = torch.tensor(pm / skel.hip_height, dtype=torch.float32)
            pmask = torch.zeros(1, T, 1)
            pmask[0, before : before + gap] = 1
    styles = cd.get("styles", [])
    chosen = [s for s in ([style] if isinstance(style, str) else style) if s != "any"]
    unknown = [s for s in chosen if s not in styles]
    if unknown:
        raise ValueError(f"styles {unknown}: model {tag} knows {styles}")
    if chosen and style_force != 0:
        w = torch.zeros(1, len(styles))
        w[0, [styles.index(s) for s in chosen]] = 1.0
        cond = net(truth, interp, mask, ttg, w, gmask=gmask, path=path, pmask=pmask)
        free = net(truth, interp, mask, ttg, torch.tensor([0]), gmask=gmask, path=path, pmask=pmask)
        pred = free + style_force * (cond - free)
    else:
        pred = net(truth, interp, mask, ttg, torch.tensor([0]) if styles else None, gmask=gmask, path=path, pmask=pmask)
    pred = interp + ai_force * (pred - interp)
    if gmask is not None:  # given bones stay exactly as the clip has them (also with an older model)
        from .model import given_features

        fm = given_features(gmask)
        pred = pred * (1 - fm) + truth * fm
    R, P = unfeatures(pred, skel.hip_height)
    Y = yaw_mat(yaw)[:, None]  # back to the clip's frame
    R[:, :, 0] = Y @ R[:, :, 0]
    P = (P.unsqueeze(-1).transpose(-1, -2) @ Y.transpose(-1, -2)).squeeze(-2) + origin[:, None]
    sl = slice(before, before + gap)
    out_r = rots[before : before + gap].copy()
    spline_r = out_r.copy()
    out_r[:, idx] = mat_to_quat(R[0, sl]).double().numpy()
    out_o = (P[0, sl] / skel.hip_height).double().numpy()
    for k in inner:  # a key inside the span comes back as the clip has it
        out_r[k - a - 1] = rots[before + k - a - 1]
        out_o[k - a - 1] = roots[before + k - a - 1]
    if trajectory > 0:
        out_o = out_o + trajectory * (keys_path - out_o)
    if head > 0:
        from . import quat

        for n in ("neck01", "neck02", "neck03", "head"):
            i = skel.index[n]
            out_r[:, i] = quat.slerp(out_r[:, i], spline_r[:, i], np.full(gap, head))
    return out_r, out_o
