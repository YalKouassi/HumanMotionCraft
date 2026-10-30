"""Train the in-betweener on the retargeted sets (``training/data/rig/*.npz``).

    python -m ac_train.train --minutes 180 --tag parts   # the free-licence sets: what ships

Only sets whose licence allows commercial use (``commercial=True`` in the registry) are read, so
the model in ``assets/models`` carries no non-commercial terms. Every set the repo supplies is one.
A set you add yourself under a non-commercial licence is read only with ``--noncommercial``; such
a model stays on your machine (``ac_train.export`` refuses to write it into the repo).

Validation holds out whole subjects (:mod:`ac_train.split`), the same ones however the sets
change, and the card lists them. A real run with nothing held out stops; a quick ``--steps`` run
then validates on its training takes and its card says so. The styles are the labels the training
takes have (:mod:`ac_train.labels`: each take's motion type and style words, with the review's
``type`` and ``tags``) that have at least ``--min-style-minutes`` of motion. What each take gives is
``review.py``'s plan: the quality scan's cuts and rejections, with the review's decisions
(``training/review/decisions.json``) over them; no window touches a cut. Every set must be scanned
first (``python -m ac_train.quality``), unless ``--unscanned``.

What a training window looks like, to match what the app sends:

- **gaps** of every length the app fills (1 to 45 frames), drawn by bin (``--gap-mix``);
- **context** before key A as the app samples it (:mod:`ac_train.context`, ``--context``): clean
  mocap, a spline through a few sparse keys (smooth, linear, step), or key A held still (the clip
  start); some contexts get a little noise, as keys set by eye do;
- **part layers** (``--given``): the bones a layer leaves alone come in as given, as the app sends
  them: every bone outside one part group (a left-arm layer) or a part group alone;
- **root path** (141000): half the windows get the recorded root path across the gap, some of
  them smoothed (the app sends the keys' spline, rounder than a recording); the rest get none, so
  a fill without a path still works;
- **context after key B** (``--context-after N``, roadmap 140000): N of the context frames come
  after key B instead of before key A, the window no longer; a share of the batches
  (:data:`AFTER_END`) get fewer of them, as a clip that ends soon after B gives, and the frames
  before A make up the number;
- **several keys** (``--multi-key``, roadmap 142000): a share of the windows (:data:`MULTI`) also
  know a few frames inside the gap, as two or three neighbouring gaps filled in one pass; the
  time channel is then two wide (to the next known frame, from the last) for every window;
- **augmentation**: mirrored left/right; tumbled (the whole body turned over the window) only for
  motion where a spinning root happens (acrobatic, fall, ground); a mild speed change (0.8 to 1.25
  times) except for jumps, flips and falls, since gravity doesn't scale.

The loss is :mod:`ac_train.losses` (each term logged). Every ``--eval-every`` steps a quick
evaluation (:mod:`ac_train.evaluate`) scores the model; the best is kept as ``best.pt`` beside the
last (``model.pt``). The full evaluation of both goes into the card.

Output: ``training/data/models/<tag>/model.pt``, ``best.pt`` and ``model.json`` (sets and licences,
``commercial``, the held-out subjects, the evaluation).
"""

from __future__ import annotations

import argparse
import json
import pathlib
import time
from collections import Counter

import numpy as np
import torch

from . import MODELS, context, evaluate, labels, losses, review, split
from .datasets import REGISTRY, RIG
from .kinematics import Body
from .model import (
    BONES,
    CONTEXT,
    MAX_GAP,
    WINDOW,
    Inbetweener,
    canonical,
    features,
    interpolate,
    interpolate_known,
    known_times,
    layout,
    mat_to_quat,
    quat_to_mat,
    slerp,
    smooth_path,
    time_to_go,
)

#: share of training windows per gap bin (:data:`ac_train.evaluate.BINS`)
GAP_MIX = (0.15, 0.2, 0.2, 0.2, 0.25)
#: share of training windows per context (:data:`ac_train.context.MODES`)
CONTEXT_MIX = {"clean": 0.4, "smooth": 0.25, "linear": 0.1, "step": 0.1, "still": 0.15}
#: share of the spline contexts that get noise, and how much (degrees, metres)
NOISE = (0.5, 2.0, 0.01)
#: motion types whose windows may be tumbled, and how often
TUMBLE_TYPES = ("acrobatic", "fall", "ground")
TUMBLE = 0.25
#: motion types kept at their own speed (gravity doesn't scale)
STEADY_TYPES = ("jump", "acrobatic", "fall")
WARP = (0.8, 1.25)
#: share of windows with a part mask, and of those, the share given as a complement (a part layer)
GIVEN = (0.5, 0.6)
#: share of windows given the root path, and the smoothing widths (frames) it is drawn with
PATH_GIVEN = (0.5, (1, 1, 5, 9, 15))
#: with context after key B: share of the batches whose clip ends before all of it (fewer frames
#: after B, drawn 0 to N-1, the rest before A: the window the app builds at a clip's end)
AFTER_END = 0.2
#: with --multi-key: share of the windows with known frames inside the gap, and at most how many
MULTI = (0.5, 3)


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
        #: per set, the groups (subjects, sessions) validation holds out
        self.held: dict[str, list[str]] = {}
        offset = 0
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
            self.held.update(split.holdout({n: [str(t) for t in takes]}))
            for s, take, own_label, t in zip(starts, takes, own, plan.takes):
                held = split.held_out(n, str(take))
                e = s + t.frames
                fine = review.starts(t, WINDOW)  # none for a take left out or too short
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
        if not q:
            raise ValueError("no takes: " + ("nothing is held out for validation" if val else "every take is held out or left out"))
        self.q = torch.tensor(np.concatenate(q), device=device)
        self.root = torch.tensor(np.concatenate(r), device=device)
        self.starts = torch.tensor(np.concatenate(ok), device=device)
        self.take = torch.tensor(np.concatenate(tk), device=device)
        self.tumbles = torch.tensor([t in TUMBLE_TYPES for t in self.types], device=device)
        self.steady = torch.tensor([t in STEADY_TYPES for t in self.types], device=device)

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

    def batch(self, n, gap, gen=None, warp=None):
        """``n`` windows of ``CONTEXT + gap + 1`` frames: rotations, root offsets, style weights and
        each window's take. ``gen``: the evaluation's plain uniform windows, seeded (else drawn by
        the takes' weights). ``warp`` (lo, hi): play each window at a random speed in that range
        (takes of :data:`STEADY_TYPES` at their own)."""
        if gen is None:
            s = self.starts[torch.multinomial(self.weights, n, replacement=True)]
        else:
            s = self.starts[torch.randint(len(self.starts), (n,), device=self.starts.device, generator=gen)]
        T = CONTEXT + gap + 1
        take = self.take[s]
        if warp is None:
            i = s[:, None] + torch.arange(T, device=s.device)
            return self.q[i], self.root[i], self.style[take], take
        lo, hi = warp
        top = (WINDOW - 1) / max(T - 1, 1)  # the window must stay inside the clean frames
        rate = (lo + torch.rand(n, device=s.device) * (hi - lo)).clamp(max=top)
        rate = torch.where(self.steady[take], torch.ones_like(rate), rate)
        at = s[:, None].float() + torch.arange(T, device=s.device)[None].float() * rate[:, None]
        i0 = at.floor().long()
        i1 = torch.minimum(i0 + 1, s[:, None] + WINDOW - 1)
        f = (at - i0.float())
        q = slerp(self.q[i0], self.q[i1], f[..., None, None])
        root = self.root[i0] + (self.root[i1] - self.root[i0]) * f[..., None]
        return q, root, self.style[take], take


def qmul(a, b):
    ax, ay, az, aw = a.unbind(-1)
    bx, by, bz, bw = b.unbind(-1)
    return torch.stack([aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
                        aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz], -1)


def augment(q, root, style, mirror, device, tumbles=None):
    """Mirror half the windows left <-> right (their styles too: ``style @ mirror``), and tumble
    some of those whose motion has a spinning root (``tumbles``, (N,) bool; every window when
    ``None``): the whole body turned about a horizontal axis over the window, so a flip or a fall
    with any rotation does not fight the fill. A tumbled walk is motion no body makes."""
    from .model import MIRROR

    N, T = q.shape[:2]
    flip = torch.rand(N, device=device) < 0.5
    mq = q[:, :, list(MIRROR)] * torch.tensor([1.0, -1.0, -1.0, 1.0], device=device)
    mr = root * torch.tensor([-1.0, 1.0, 1.0], device=device)
    q = torch.where(flip[:, None, None, None], mq, q)
    root = torch.where(flip[:, None, None], mr, root)
    style = torch.where(flip[:, None], style @ mirror, style)
    tumble = torch.rand(N, device=device) < TUMBLE
    if tumbles is not None:
        tumble &= tumbles
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


def given_mask(n, device, p=GIVEN[0], complement=GIVEN[1]):
    """Part masks as the app makes them. A share ``p`` of the windows get one: a part group
    (:data:`ac_train.model.PART_GROUPS`, the unions too) picked at random, and with probability
    ``complement`` every bone *outside* it is given (a layer that animates that part: the app
    passes the rest as given), else the group itself is (the rest of the body animated over a
    layer that drives the part)."""
    from .model import PART_GROUPS

    groups = list(PART_GROUPS.values())
    inside = torch.zeros(len(groups), len(BONES), device=device)
    for i, grp in enumerate(groups):
        inside[i, list(grp)] = 1.0
    pick = torch.randint(len(groups), (n,), device=device)
    g = inside[pick]
    flip = torch.rand(n, device=device) < complement
    g = torch.where(flip[:, None], 1 - g, g)
    on = torch.rand(n, device=device) < p
    return g * on[:, None]


def path_given(truth, mask, p=PATH_GIVEN[0], widths=PATH_GIVEN[1]):
    """The root path the app sends with ``trajectory`` (141000), for a share ``p`` of the windows:
    the recorded root (features' last 3, hip heights), box-smoothed by a width drawn from
    ``widths``, on the gap frames. Returns path (N, T, 3) and its mask (N, T, 1)."""
    N, device = truth.shape[0], truth.device
    root = truth[..., -3:]
    w = widths[int(torch.randint(len(widths), (1,)))]
    path = smooth_path(root, w)
    on = (torch.rand(N, device=device) < p).to(truth.dtype)
    return path, (1 - mask) * on[:, None, None]


def sample_gap(mix=GAP_MIX) -> int:
    """A gap length: a bin by ``mix``, then a length in it (uniform)."""
    b = int(torch.multinomial(torch.tensor(mix, dtype=torch.float), 1))
    lo, hi = evaluate.BINS[b]
    return int(torch.randint(lo, hi + 1, (1,)))


def prepare(q, root, gap, sub, device, after: int = 0, known=None, multi_key: bool = False):
    """Canonical window, inputs and truth for a batch of quaternion windows (``CONTEXT + gap + 1``
    frames each). ``sub``: anything with the rig's ``hip_height``. ``after``: context frames after
    key B instead of before key A (:func:`ac_train.model.layout`); ``known`` (N, T, 1): frames
    inside the gap known too (several keys); ``multi_key``: the model's two-wide time channel and
    the interpolation between neighbouring known frames. The defaults make the window as it
    always was, bit for bit (``tests/data/prepare-reference.npz``)."""
    N, T = q.shape[:2]
    ia, ib, _ = layout(gap, after)
    a = torch.full((N,), ia, device=device)
    b = a + gap + 1
    rot, pos, _, _ = canonical(quat_to_mat(q), root * sub.hip_height, ia)
    truth = features(rot, pos, sub.hip_height)
    mask = torch.zeros(N, T, 1, device=device)
    mask[:, : ia + 1] = 1
    mask[:, ib:] = 1
    if known is not None:
        mask = torch.maximum(mask, known)
    if multi_key:
        rot_i, pos_i = interpolate_known(mat_to_quat(rot), pos, mask)
        ttg = known_times(mask)
    else:
        if known is not None:
            raise ValueError("known frames inside the gap need a multi-key model (--multi-key)")
        rot_i, pos_i = interpolate(mat_to_quat(rot), pos, a, b)
        ttg, _ = time_to_go(a, b, T, device)
    interp = features(rot_i, pos_i, sub.hip_height)
    return truth, interp, mask, ttg, rot, pos


def multi_known(n: int, T: int, a: int, b: int, device, p: float = MULTI[0], most: int = MULTI[1]):
    """(N, T, 1) frames inside the gap ``a+1 .. b-1`` that a share ``p`` of the windows know too:
    1 to ``most`` of them at random (never every gap frame), as the keys between two or three
    neighbouring gaps the app fills in one pass. The other windows know none."""
    known = torch.zeros(n, T, 1, device=device)
    g = b - a - 1
    if g < 2:
        return known
    count = torch.randint(1, min(most, g - 1) + 1, (n,), device=device)
    order = torch.rand(n, g, device=device).argsort(-1)
    pick = (order < count[:, None]) & (torch.rand(n, 1, device=device) < p)
    known[:, a + 1 : b, 0] = pick.float()
    return known


def _floats(text: str, n: int, what: str) -> tuple[float, ...]:
    v = tuple(float(x) for x in text.split(","))
    if len(v) != n or any(x < 0 for x in v) or not sum(v):
        raise SystemExit(f"{what}: {n} shares, not negative, not all 0")
    return v


def _named(text: str, known, what: str) -> dict[str, float]:
    out = {}
    for part in filter(None, text.split(",")):
        k, _, v = part.partition("=")
        if k not in known:
            raise SystemExit(f"{what}: {k!r} is not one of {', '.join(known)}")
        out[k] = float(v)
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
    ap.add_argument("--eval-windows", type=int, default=512, help="validation windows per gap bin at the end")
    ap.add_argument("--eval-every", type=int, default=1000, help="steps between quick evaluations (and the best checkpoint)")
    ap.add_argument("--decisions", default=str(review.DECISIONS), help="the review's decisions file")
    ap.add_argument("--min-style-minutes", type=float, default=labels.MIN_MINUTES,
                    help="a label becomes a style with this much training motion (minutes)")
    ap.add_argument("--unscanned", action="store_true",
                    help="train on sets that have no quality scan, every frame (says so for each)")
    ap.add_argument("--gap-mix", default=",".join(map(str, GAP_MIX)),
                    help="shares of the gap bins " + " ".join(evaluate.bin_name(*b) for b in evaluate.BINS))
    ap.add_argument("--context", default=",".join(f"{k}={v}" for k, v in CONTEXT_MIX.items()),
                    help="shares of the contexts: " + ", ".join(context.MODES))
    ap.add_argument("--given", type=float, default=GIVEN[0], help="share of the windows with a part mask")
    ap.add_argument("--loss-weights", default="", help="change loss weights: pos=2,vel=0.5 (" + ", ".join(losses.WEIGHTS) + ")")
    ap.add_argument("--no-contacts", action="store_true", help="no foot-contact head (the network's output as before)")
    ap.add_argument("--context-after", type=int, default=0, metavar="N",
                    help=f"of the {CONTEXT} context frames, N come after key B instead of before key A (140000; default 0)")
    ap.add_argument("--multi-key", action="store_true",
                    help="train with several known frames in a window, so neighbouring gaps fill in one pass (142000)")
    ap.add_argument("--no-warp", action="store_true", help="no speed change")
    args = ap.parse_args(argv)

    from .rig import Skeleton

    gap_mix = _floats(args.gap_mix, len(evaluate.BINS), "--gap-mix")
    ctx_mix = _named(args.context, context.MODES, "--context")
    weights = {**losses.WEIGHTS, **_named(args.loss_weights, losses.WEIGHTS, "--loss-weights")}
    contacts = not args.no_contacts
    warp = None if args.no_warp else WARP
    if not 0 <= args.context_after < CONTEXT:
        ap.error(f"--context-after: 0 to {CONTEXT - 1}")

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
    skel = Skeleton.load()
    body = Body(skel, device)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    if args.unscanned:
        print("--unscanned: no set's scan is read; every frame of every take is used, glitches and all", flush=True)
    train = Sets(names, device, decisions=decisions, scanned=not args.unscanned)
    for plan in train.plans:
        print(plan.summary(), flush=True)
        for note in plan.notes:
            print(f"  {note}", flush=True)
    validated_on = "held-out subjects"
    try:
        val = Sets(names, device, val=True, decisions=decisions, scanned=not args.unscanned)
    except ValueError:
        if args.steps is None:
            raise SystemExit("no validation takes: these sets hold out no subject (ac_train.split). A real run "
                             "must validate on takes it doesn't train on; add a set with more subjects")
        print("no validation takes in these sets: a quick run validates on its training takes (the numbers flatter it)", flush=True)
        val, validated_on = train, "training takes (a quick --steps run: the numbers flatter it)"
    print(f"sets {train.used}: {len(train.q)} train frames, {len(val.q)} val frames on {device}", flush=True)
    print(f"held out: {json.dumps(train.held)}", flush=True)
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

    tag = args.tag or ("parts-own" if args.noncommercial else "parts")
    out = MODELS / tag
    out.mkdir(parents=True, exist_ok=True)
    net_args = dict(styles=len(styles), given=True, contacts=contacts, path=True,
                    context_after=args.context_after, multi_key=args.multi_key)
    model = Inbetweener(**net_args).to(device)
    opt = torch.optim.AdamW(model.parameters(), lr=3e-4, weight_decay=1e-4)
    t0, step, log = time.time(), 0, []
    best = {"step": None, "score": None}
    total = args.minutes * 60
    running: Counter = Counter()
    while time.time() - t0 < total and (args.steps is None or step < args.steps):
        frac = (time.time() - t0) / total
        if args.steps:
            frac = max(frac, step / args.steps)
        for grp in opt.param_groups:
            grp["lr"] = 3e-4 * min(1.0, step / 500) * (0.5 * (1 + np.cos(np.pi * frac)))
        g = sample_gap(gap_mix)
        after = args.context_after
        if after and float(torch.rand(())) < AFTER_END:
            after = int(torch.randint(0, after, (1,)))  # the clip ends soon after B
        a, b, T = layout(g, after)
        q, root, style, take = train.batch(args.batch, g, warp=warp)
        q, root, style = augment(q, root, style, mirror, device, train.tumbles[take])
        style = drop_styles(style)
        q, root, mode = context.mixed(q, root, a, ctx_mix, b=b)
        noisy = (mode != context.MODES.index("clean")) & (torch.rand(len(mode), device=device) < NOISE[0])
        if noisy.any():
            rows = noisy.nonzero().flatten()
            q[rows], root[rows] = context.noise(q[rows], root[rows], a, NOISE[1], NOISE[2], skel.hip_height, b=b)
        known = multi_known(q.shape[0], T, a, b, device) if args.multi_key else None
        truth, interp, mask, ttg, _, _ = prepare(q, root, g, body, device, after, known, args.multi_key)
        gm = given_mask(q.shape[0], device, p=args.given)
        path, pmask = path_given(truth, mask)
        pred, logits = model(truth, interp, mask, ttg, style, gmask=gm, with_contacts=True, path=path, pmask=pmask)
        parts = losses.terms(pred, truth, gm, a, b, body, logits, mask)
        loss = losses.total(parts, weights)
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        opt.step()
        step += 1
        for k, v in parts.items():
            running[k] += float(v.detach())
        running["n"] += 1
        if step % args.eval_every == 0:
            ev = evaluate.run(model, val, body, device, windows=64, inputs=("clean", "smooth", "still"), parts=False, breakdown=False)
            n = running.pop("n")
            entry = {"step": step, "minutes": round((time.time() - t0) / 60, 1), "loss": float(loss),
                     "terms": {k: round(v / n, 6) for k, v in running.items()}, "score": ev["score"],
                     "joint_mm": {k: round(r["model"]["joint_m"] * 1000, 2) for k, r in ev["bins"].items()}}
            running.clear()
            log.append(entry)
            print(json.dumps(entry), flush=True)
            if ev["score"] is not None and (best["score"] is None or ev["score"] < best["score"]):
                best = {"step": step, "score": ev["score"]}
                torch.save(model.state_dict(), out / "best.pt")

    torch.save(model.state_dict(), out / "model.pt")
    if best["step"] is None:
        torch.save(model.state_dict(), out / "best.pt")
        best = {"step": step, "score": None}
    print(f"evaluating the last step ({step}) and the best ({best['step']}) on {validated_on}", flush=True)
    evaluation = evaluate.run(model, val, body, device, windows=args.eval_windows)
    best_model = Inbetweener(**net_args).to(device)
    best_model.load_state_dict(torch.load(out / "best.pt", map_location=device, weights_only=True))
    evaluation_best = evaluate.run(best_model, val, body, device, windows=args.eval_windows)
    print(evaluate.report(evaluation), flush=True)
    for ev in (evaluation, evaluation_best):
        ev.update({"checkpoint": "model" if ev is evaluation else "best", "holdout": train.held, "sets": val.used})
    commercial = all(REGISTRY[n].commercial for n in train.used)
    card = {
        "tag": tag, "steps": step, "minutes": round((time.time() - t0) / 60, 1), "bones": list(BONES),
        "context": CONTEXT - args.context_after, "context_after": args.context_after, "max_gap": MAX_GAP,
        "multi_key": args.multi_key, "styles": styles, "style_frames": style_frames,
        "styles_left_out": rare, "min_style_minutes": args.min_style_minutes,
        "given": True, "contacts": contacts, "path": True,
        "augment": ["mirror 0.5 (side styles swapped)", f"tumble {TUMBLE} ({', '.join(TUMBLE_TYPES)} only)",
                    f"speed {WARP[0]}-{WARP[1]}x (not {', '.join(STEADY_TYPES)})" if warp else "no speed change",
                    f"given parts {args.given} ({GIVEN[1]} of them the complement of a part, as a part layer sends)",
                    f"root path {PATH_GIVEN[0]} (smoothed over {', '.join(map(str, PATH_GIVEN[1]))} frames)",
                    f"context {json.dumps(ctx_mix)}", f"noise on {NOISE[0]} of the spline contexts ({NOISE[1]} deg, {NOISE[2]} m)",
                    *([f"context after key B {args.context_after} ({AFTER_END} of the batches fewer, the clip ending)"]
                      if args.context_after else []),
                    *([f"several keys: {MULTI[0]} of the windows know 1 to {MULTI[1]} gap frames"] if args.multi_key else []),
                    f"gaps by bin {dict(zip((evaluate.bin_name(*x) for x in evaluate.BINS), gap_mix))}",
                    "each style dropped 0.25", "rare motion types up to x3"],
        "loss_weights": weights,
        "sets": [{"name": n, "title": REGISTRY[n].title, "licence": REGISTRY[n].licence, "commercial": REGISTRY[n].commercial} for n in train.used],
        "commercial": commercial,
        "warning": "" if commercial else "Trained on non-commercial data: do not ship this model or its output in a commercial product.",
        "data": {"scanned": not args.unscanned, "decisions": len(decisions),
                 "plans": [p.summary() for p in train.plans]},
        "split": "subjects", "holdout": train.held, "validated_on": validated_on,
        "best": best,
        "metrics": {k: {"model_m": r["model"]["joint_m"], "interp_m": r["interp"]["joint_m"]} for k, r in evaluation["bins"].items()},
        "evaluation": evaluation, "evaluation_best": evaluation_best, "log": log,
    }
    (out / "model.json").write_text(json.dumps(card, indent=1), encoding="utf-8")
    print(json.dumps({"saved": str(out), "score": evaluation["score"], "best": best, "commercial": commercial}))


if __name__ == "__main__":
    main()
