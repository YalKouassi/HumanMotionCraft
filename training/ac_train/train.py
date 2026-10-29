"""Train the in-betweener on the retargeted sets (``training/data/rig/*.npz``).

    python -m ac_train.train --minutes 180 --tag parts   # the free-licence sets: what ships

Only sets whose licence allows commercial use (``commercial=True`` in the registry) are read, so
the model in ``assets/models`` carries no non-commercial terms. Every set the repo supplies is one.
A set you add yourself under a non-commercial licence is read only with ``--noncommercial``; such
a model stays on your machine (``ac_train.export`` refuses to write it into the repo).

Validation holds out 5 % of each set's takes. The styles are the labels the training takes have (:mod:`ac_train.labels`: each take's
motion type and style words, with the review's ``type`` and ``tags``) that have at least
``--min-style-minutes`` of motion. What each take gives is ``review.py``'s plan: the quality scan's cuts and rejections, with
the review's decisions (``training/review/decisions.json``) over them; no window touches a cut.
Every set must be scanned first (``python -m ac_train.quality``), unless ``--unscanned``. The report compares the network with the plain interpolation it starts from: mean joint
position error over the gap frames (metres) at gaps of 5, 15 and 30 frames.

Output: ``training/data/models/<tag>/model.pt`` and ``model.json`` (sets and licences, ``commercial``, metrics).
"""

from __future__ import annotations

import argparse
import json
import pathlib
import time
from collections import Counter

import numpy as np
import torch

from . import MODELS, labels, review
from .datasets import REGISTRY, RIG
from .model import (
    BONES,
    CONTEXT,
    MAX_GAP,
    Inbetweener,
    SubSkeleton,
    canonical,
    features,
    given_features,
    interpolate,
    mat_to_quat,
    quat_to_mat,
    time_to_go,
    unfeatures,
)


class Sets:
    """All frames of the chosen sets on the device, where a window may start, and each take's
    labels. :meth:`learn` then gives the takes their style weights over a list of styles."""

    def __init__(self, names, device, val=False, decisions=None, scanned=True):
        from .rig import Skeleton

        bones = list(Skeleton.load().names)
        idx = [bones.index(b) for b in BONES]
        q, r, ok, tk = [], [], [], []
        self.device = device
        #: per take kept: its labels and motion type; frames of each label (no cut frames)
        self.labels: list[list[str]] = []
        self.types: list[str] = []
        self.label_frames: Counter = Counter()
        self.used = []
        self.plans: list[review.Plan] = []
        window = CONTEXT + MAX_GAP + 1
        offset = 0
        rng = np.random.default_rng(0)
        for n in names:
            path = RIG / f"{n}.npz"
            if not path.exists():
                print(f"set {n} not built ({path} is missing): run python -m ac_train.datasets {n}", flush=True)
                continue
            d = np.load(path)
            rot, root, starts, takes = d["rotations"][:, idx].astype(np.float32), d["roots"], d["starts"], d["takes"]
            own = d["labels"] if "labels" in d.files else takes
            plan = review.plan_set(n, len(rot), starts, takes, decisions, scanned, npz=d)
            self.plans.append(plan)
            for s, take, own_label, t in zip(starts, takes, own, plan.takes):
                held = rng.random() < 0.05
                e = s + t.frames
                fine = review.starts(t, window)  # none for a take left out or too short
                if held != val or not len(fine):
                    continue
                q.append(rot[s:e])
                r.append(root[s:e])
                words = labels.of(n, str(take), str(own_label), t.kind, t.tags)
                for w in words:
                    self.label_frames[w] += t.frames - int(t.bad().sum())
                tk.append(np.full(e - s, len(self.labels), dtype=np.int64))
                self.labels.append(words)
                self.types.append(labels.type_of(str(take), t.kind))
                ok.append(offset + fine)
                offset += e - s
            self.used.append(n)
        self.q = torch.tensor(np.concatenate(q), device=device)
        self.root = torch.tensor(np.concatenate(r), device=device)
        self.starts = torch.tensor(np.concatenate(ok), device=device)
        self.take = torch.tensor(np.concatenate(tk), device=device)

    def learn(self, styles: list[str], counts: Counter):
        """Each take's style weights over ``styles`` (its labels that are styles; "any" when none
        is), and how often its windows are drawn (:func:`labels.weights`, from ``counts``)."""
        table = np.zeros((len(self.labels), len(styles)), np.float32)
        for i, words in enumerate(self.labels):
            for w in words:
                if w in styles:
                    table[i, styles.index(w)] = 1.0
            if not table[i].any():
                table[i, 0] = 1.0
        self.style = torch.tensor(table, device=self.device)
        w = torch.tensor(labels.weights(self.types, counts, styles), dtype=torch.float32, device=self.device)
        self.weights = w[self.take[self.starts]]

    def batch(self, n, gap, gen=None):
        if gen is None:
            s = self.starts[torch.multinomial(self.weights, n, replacement=True)]
        else:  # evaluation: plain uniform windows, seeded
            s = self.starts[torch.randint(len(self.starts), (n,), device=self.starts.device, generator=gen)]
        T = CONTEXT + gap + 1
        i = s[:, None] + torch.arange(T, device=s.device)
        return self.q[i], self.root[i], self.style[self.take[s]]


def qmul(a, b):
    ax, ay, az, aw = a.unbind(-1)
    bx, by, bz, bw = b.unbind(-1)
    return torch.stack([aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
                        aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz], -1)


def augment(q, root, style, mirror, device):
    """Mirror half the windows left <-> right (their styles too: ``style @ mirror``), and tumble
    some: the whole body turned about a horizontal axis over the window (flips and falls with any
    rotation, so the fill does not fight a spinning root)."""
    from .model import MIRROR

    N, T = q.shape[:2]
    flip = torch.rand(N, device=device) < 0.5
    mq = q[:, :, list(MIRROR)] * torch.tensor([1.0, -1.0, -1.0, 1.0], device=device)
    mr = root * torch.tensor([-1.0, 1.0, 1.0], device=device)
    q = torch.where(flip[:, None, None, None], mq, q)
    root = torch.where(flip[:, None, None], mr, root)
    style = torch.where(flip[:, None], style @ mirror, style)
    tumble = torch.rand(N, device=device) < 0.15
    a = torch.rand(N, device=device) * 2 * np.pi
    axis = torch.stack([torch.cos(a), torch.sin(a), torch.zeros_like(a)], -1)  # horizontal
    turns = (torch.rand(N, device=device) * 2 - 1) * 2 * np.pi * tumble  # up to a full turn
    ang = turns[:, None] * torch.linspace(0, 1, T, device=device)[None]
    r = torch.cat([axis[:, None] * torch.sin(ang / 2)[..., None], torch.cos(ang / 2)[..., None]], -1)
    q = q.clone()
    q[:, :, 0] = qmul(r, q[:, :, 0])
    return q, root, style


def drop_styles(style, p=0.25):
    """Each of a window's styles left out with probability ``p``, "any" when none is left: the
    network learns every style alone, the styles added up, and no style at all."""
    keep = style * (torch.rand(style.shape, device=style.device) >= p)
    keep[:, 0] = 0
    keep[:, 0] = (keep.sum(1) == 0).to(keep.dtype)
    return keep


def given_mask(n, device, p=0.5):
    """Random part masks: half the windows hand a part group over as given (another layer)."""
    from .model import PART_GROUPS

    g = torch.zeros(n, len(BONES), device=device)
    groups = list(PART_GROUPS.values())
    pick = torch.randint(len(groups), (n,), device=device)
    on = torch.rand(n, device=device) < p
    for i, grp in enumerate(groups):
        rows = on & (pick == i)
        g[rows.nonzero().flatten()[:, None], torch.tensor(grp, device=device)[None]] = 1.0
    return g


def prepare(q, root, gap, sub, device):
    """Canonical window, inputs and truth for a batch of quaternion windows."""
    N, T = q.shape[:2]
    a = torch.full((N,), CONTEXT - 1, device=device)
    b = a + gap + 1
    rot, pos, _, _ = canonical(quat_to_mat(q), root * sub.hip_height, CONTEXT - 1)
    truth = features(rot, pos, sub.hip_height)
    rot_i, pos_i = interpolate(mat_to_quat(rot), pos, a, b)
    interp = features(rot_i, pos_i, sub.hip_height)
    mask = torch.zeros(N, T, 1, device=device)
    mask[:, :CONTEXT] = 1
    mask[:, -1] = 1
    ttg, _ = time_to_go(a, b, T, device)
    return truth, interp, mask, ttg, rot, pos


def positions_error(f_pred, rot_true, pos_true, sub, gapslice):
    R, P = unfeatures(f_pred, sub.hip_height)
    a = sub.positions(R[:, gapslice], P[:, gapslice])
    b = sub.positions(rot_true[:, gapslice], pos_true[:, gapslice])
    return (a - b).norm(dim=-1).mean()


@torch.no_grad()
def evaluate(model, sets, sub, device, gaps=(5, 15, 30), n=512):
    model.eval()
    out = {}
    gen = torch.Generator(device=device).manual_seed(1)
    for g in gaps:
        q, root, style = sets.batch(n, g, gen)
        truth, interp, mask, ttg, rot, pos = prepare(q, root, g, sub, device)
        pred = model(truth, interp, mask, ttg, style)
        sl = slice(CONTEXT, CONTEXT + g)
        out[g] = {"model_m": float(positions_error(pred, rot, pos, sub, sl)), "interp_m": float(positions_error(interp, rot, pos, sub, sl))}
    model.train()
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description="Train the in-betweener")
    ap.add_argument("--sets", default=None, help="comma list of training/data/rig sets (default: every set allowed)")
    ap.add_argument("--noncommercial", action="store_true",
                    help="also sets you added under a non-commercial licence: the model is for your own use only, never shipped")
    ap.add_argument("--minutes", type=float, default=40)
    ap.add_argument("--batch", type=int, default=128)
    ap.add_argument("--tag", default=None)
    ap.add_argument("--steps", type=int, default=None, help="stop after this many steps (a quick check)")
    ap.add_argument("--eval-windows", type=int, default=2048, help="validation windows per gap at the end")
    ap.add_argument("--decisions", default=str(review.DECISIONS), help="the review's decisions file")
    ap.add_argument("--min-style-minutes", type=float, default=labels.MIN_MINUTES,
                    help="a label becomes a style with this much training motion (minutes)")
    ap.add_argument("--unscanned", action="store_true",
                    help="train on sets that have no quality scan, every frame (says so for each)")
    args = ap.parse_args(argv)

    from .rig import Skeleton

    torch.manual_seed(0)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    if args.sets:
        names = [n for n in args.sets.split(",") if n]
        unknown = [n for n in names if n not in REGISTRY]
        if unknown:
            ap.error(f"sets {unknown}: the registry has {', '.join(REGISTRY)}")
        nc = [n for n in names if not REGISTRY[n].commercial]
        if nc and not args.noncommercial:
            ap.error(f"{', '.join(nc)} only allow non-commercial use: add --noncommercial (a model for your own use)")
    else:
        names = [n for n in REGISTRY if REGISTRY[n].commercial or args.noncommercial]
    print(f"sets: {', '.join(names)}" + ("" if args.noncommercial else " (the free-licence sets; --noncommercial adds non-commercial sets you added)"),
          flush=True)
    sub = SubSkeleton(Skeleton.load(), device)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    if args.unscanned:
        print("--unscanned: no set's scan is read; every frame of every take is used, glitches and all", flush=True)
    train = Sets(names, device, decisions=decisions, scanned=not args.unscanned)
    for plan in train.plans:
        print(plan.summary(), flush=True)
        for note in plan.notes:
            print(f"  {note}", flush=True)
    try:
        val = Sets(names, device, val=True, decisions=decisions, scanned=not args.unscanned)
    except ValueError:
        # a handful of takes (a quick run) can leave no take held out
        print("no validation takes in these sets: validating on the training takes (the metrics flatter it)", flush=True)
        val = train
    print(f"sets {train.used}: {len(train.q)} train frames, {len(val.q)} val frames on {device}", flush=True)
    styles, rare = labels.vocabulary(train.label_frames, args.min_style_minutes)
    train.learn(styles, train.label_frames)
    if val is not train:
        val.learn(styles, train.label_frames)
    mirror = torch.tensor(labels.mirror_matrix(styles), device=device)
    style_frames = {w: int(train.label_frames[w]) for w in styles[1:]}
    print(f"{len(styles) - 1} styles (train frames): {style_frames}", flush=True)
    if rare:
        print(f"{len(rare)} labels left out, under {args.min_style_minutes:g} minutes: {rare}", flush=True)
    lost = [w for w in styles[1:] if labels.mirrored(w) != w and labels.mirrored(w) not in styles]
    if lost:
        print(f"mirrored windows drop {lost}: their other side isn't a style", flush=True)

    model = Inbetweener(styles=len(styles), given=True).to(device)
    opt = torch.optim.AdamW(model.parameters(), lr=3e-4, weight_decay=1e-4)
    t0, step, log = time.time(), 0, []
    total = args.minutes * 60
    while time.time() - t0 < total and (args.steps is None or step < args.steps):
        frac = (time.time() - t0) / total
        if args.steps:
            frac = max(frac, step / args.steps)
        for grp in opt.param_groups:
            grp["lr"] = 3e-4 * min(1.0, step / 500) * (0.5 * (1 + np.cos(np.pi * frac)))
        g = int(torch.randint(5, MAX_GAP + 1, (1,)))
        q, root, style = train.batch(args.batch, g)
        q, root, style = augment(q, root, style, mirror, device)
        style = drop_styles(style)
        truth, interp, mask, ttg, rot, pos = prepare(q, root, g, sub, device)
        gm = given_mask(q.shape[0], device)
        pred = model(truth, interp, mask, ttg, style, gmask=gm)
        sl = slice(CONTEXT, CONTEXT + g)
        free = 1 - given_features(gm)  # the given bones are copied: no loss on them
        loss = ((pred[:, sl] - truth[:, sl]).abs() * free).sum() / (free.sum() * g).clamp(min=1) \
            + 2.0 * positions_error(pred, rot, pos, sub, sl) / sub.hip_height
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        opt.step()
        step += 1
        if step % 500 == 0:
            m = evaluate(model, val, sub, device)
            log.append({"step": step, "minutes": round((time.time() - t0) / 60, 1), "loss": float(loss), "val": m})
            print(json.dumps(log[-1]), flush=True)

    metrics = evaluate(model, val, sub, device, n=args.eval_windows)
    commercial = all(REGISTRY[n].commercial for n in train.used)
    tag = args.tag or ("parts-own" if args.noncommercial else "parts")
    out = MODELS / tag
    out.mkdir(parents=True, exist_ok=True)
    torch.save(model.state_dict(), out / "model.pt")
    card = {
        "tag": tag, "steps": step, "minutes": round((time.time() - t0) / 60, 1), "bones": list(BONES),
        "context": CONTEXT, "max_gap": MAX_GAP, "styles": styles, "style_frames": style_frames,
        "styles_left_out": rare, "min_style_minutes": args.min_style_minutes,
        "given": True, "augment": ["mirror 0.5 (side styles swapped)", "tumble 0.15", "given parts 0.5",
                                   "each style dropped 0.25", "rare motion types up to x3"],
        "sets": [{"name": n, "title": REGISTRY[n].title, "licence": REGISTRY[n].licence, "commercial": REGISTRY[n].commercial} for n in train.used],
        "commercial": commercial,
        "warning": "" if commercial else "Trained on non-commercial data: do not ship this model or its output in a commercial product.",
        "data": {"scanned": not args.unscanned, "decisions": len(decisions),
                 "plans": [p.summary() for p in train.plans]},
        "metrics": metrics, "log": log,
    }
    (out / "model.json").write_text(json.dumps(card, indent=1), encoding="utf-8")
    print(json.dumps({"saved": str(out), "metrics": metrics, "commercial": commercial}))


if __name__ == "__main__":
    main()
