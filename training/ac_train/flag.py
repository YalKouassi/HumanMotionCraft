"""Flag the takes the model fails on, for a person to look at (roadmap 137000).

    python -m ac_train.flag --tag parts-lowweights --checkpoint best   # writes data/rig/flags.json

Every take of every set the registry allows is filled by the model window by window (clean
context, a gap of ``--gap`` frames, nothing given, the take's own styles), and each window gets
training's loss (:func:`ac_train.losses.window_terms` with the card's weights). A take's loss is
the mean over its windows (``--by worst``: its worst window). The worst :data:`SHARE` of the takes
(at least one) go into ``data/rig/flags.json`` with their loss, the worst window's frame and a line
saying why, and the review app (``ac-review``, ``review.list {"show": "model"}``) lists them. That
is all flagging does: it writes nothing to ``training/review/decisions.json``, because a high loss
is often just hard motion (acrobatics, a fall), not bad data. A person decides.

Every window of every take is filled (about 15,000 windows a second on an RTX 3080 Ti: the
repo's six sets, 3.2 million windows, take a few minutes; ``--stride n`` keeps every n-th window
of a take, deterministic, for a quicker look). Windows keep clear of the scan's cuts and the
review's decisions as training does (:mod:`ac_train.review`): a cut is known bad already. Takes left out of training (rejected by the
scan or the review) or too short for one window are counted as skipped, not evaluated: they are in
the review queue already.

The file names each set's takes and lengths by a CRC-32 (:func:`digest`), so the review app can
tell flags made on a set since rebuilt (stale) from current ones.
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import time
import zlib

import numpy as np

from . import labels, losses, review
from .quality import RIG

FORMAT = "ac-train.flags"
VERSION = 1
#: the share of the takes flagged (the worst), at least one
SHARE = 0.01
#: the gap filled (frames): the middle of the app's range
GAP = 15
#: a window every this many frames of a take (1: every window)
STRIDE = 1
FILE = "flags.json"


def digest(takes, frames) -> int:
    """CRC-32 over ``"<take>\\t<frames>\\n"`` for each take in order: what the set's takes are. The
    review app computes the same (``crates/ac-review/src/flags.rs``)."""
    text = "".join(f"{t}\t{int(f)}\n" for t, f in zip(takes, frames))
    return zlib.crc32(text.encode("utf-8"))


def how_many(n: int, share: float = SHARE) -> int:
    """How many of ``n`` takes are flagged: the worst ``share`` rounded up, at least one (none of none)."""
    if n <= 0:
        return 0
    return max(1, min(n, math.ceil(share * n)))


def rank(takes: list[dict], share: float = SHARE, by: str = "loss") -> list[dict]:
    """The worst ``share`` of ``takes`` (dicts with ``loss`` and ``worst``), worst first by ``by``
    (``loss``: the take's mean window loss; ``worst``: its worst window's). Ties by id, so the
    list is the same whatever the order they came in."""
    if by not in ("loss", "worst"):
        raise ValueError(f"by {by!r}: loss or worst")
    order = sorted(takes, key=lambda t: (-float(t[by]), str(t["id"])))
    return order[: how_many(len(takes), share)]


def why(t: dict, median: float, gap: int) -> str:
    """One line a reviewer reads first: the loss against the median take, the worst window, the
    term that weighs most."""
    ratio = f", {t['loss'] / median:.1f}x the median take" if median > 0 else ""
    terms = t.get("weighted") or {}
    top = max(terms, key=terms.get) if terms else ""
    top = f"; mostly {top} ({terms[top]:.3f})" if top else ""
    return (f"mean loss {t['loss']:.3f} over {t['windows']} windows of gap {gap}{ratio}; "
            f"worst {t['worst']:.3f} at frame {t['worst_at']}{top}")


def record(model: dict, sets: dict, takes: list[dict], gap: int, stride: int, share: float, by: str,
           elapsed_s: float) -> dict:
    """The flags file's content: the worst takes of ``takes`` (every evaluated take's numbers),
    with the run's settings and each set's digest."""
    losses_ = sorted(float(t["loss"]) for t in takes)
    median = losses_[len(losses_) // 2] if losses_ else 0.0
    flagged = []
    for i, t in enumerate(rank(takes, share, by)):
        flagged.append({**t, "rank": i + 1, "why": why(t, median, gap)})
    return {
        "format": FORMAT, "version": VERSION, "model": model, "gap": gap, "stride": stride, "share": share, "by": by,
        "sets": sets, "takes": len(takes), "windows": int(sum(int(t["windows"]) for t in takes)),
        "median_loss": round(median, 6), "elapsed_s": round(elapsed_s, 1), "flagged": flagged,
    }


def write(path: pathlib.Path, rec: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(rec, indent=1), encoding="utf-8")


def _styles(card: dict, set_name: str, takes, own_labels, plan: review.Plan, device):
    """(takes, S) style weights as training gives them: the take's labels that are the card's
    styles, "any" when none is."""
    import torch

    styles = card.get("styles") or ["any"]
    table = np.zeros((len(takes), len(styles)), np.float32)
    for i, (take, own, t) in enumerate(zip(takes, own_labels, plan.takes)):
        for w in labels.of(set_name, str(take), str(own), t.kind, t.tags):
            if w in styles:
                table[i, styles.index(w)] = 1.0
        if not table[i].any():
            table[i, 0] = 1.0
    return torch.tensor(table, device=device)


def evaluate_set(model, card: dict, name: str, body, device, decisions: dict, gap: int = GAP, stride: int = STRIDE,
                 batch: int = 1024, weights: dict | None = None, log=print) -> tuple[list[dict], dict]:
    """Every take of ``data/rig/<name>.npz``: its numbers (``id``, ``label``, ``frames``,
    ``windows``, ``loss``, ``worst``, ``worst_at``, ``terms``, ``weighted``) for the takes with a
    window, and the set's record (takes, frames, digest, evaluated, skipped)."""
    import torch

    from .model import BONES, CONTEXT
    from .rig import Skeleton
    from .train import prepare

    weights = weights or {**losses.WEIGHTS, **(card.get("loss_weights") or {})}
    bones = list(Skeleton.load().names)
    idx = [bones.index(b) for b in BONES]
    d = np.load(RIG / f"{name}.npz")
    rot_np, roots_np, starts, takes = d["rotations"], d["roots"], d["starts"], d["takes"]
    own = d["labels"] if "labels" in d.files else takes
    plan = review.plan_set(name, len(rot_np), starts, takes, decisions, npz=d)
    for note in plan.notes:
        log(f"  {note}")
    ends = [*list(starts[1:]), len(rot_np)]
    frames = [int(e - s) for s, e in zip(starts, ends)]
    T = CONTEXT + gap + 1
    a, b = CONTEXT - 1, CONTEXT + gap
    win_take, win_start = [], []
    for i, (s, t) in enumerate(zip(starts, plan.takes)):
        fine = review.starts(t, T)[::stride]
        win_take.append(np.full(len(fine), i, np.int64))
        win_start.append(int(s) + fine)
    win_take = np.concatenate(win_take) if win_take else np.zeros(0, np.int64)
    win_start = np.concatenate(win_start) if win_start else np.zeros(0, np.int64)
    rot = torch.tensor(rot_np[:, idx].astype(np.float32), device=device)
    root = torch.tensor(roots_np, device=device)
    style = _styles(card, name, takes, own, plan, device)
    n = len(takes)
    total = np.zeros(n)
    worst = np.full(n, -np.inf)
    worst_at = np.zeros(n, np.int64)
    count = np.zeros(n, np.int64)
    term_sum: dict[str, np.ndarray] = {}
    ar = torch.arange(T, device=device)
    with torch.no_grad():
        for k in range(0, len(win_start), batch):
            st = torch.tensor(win_start[k : k + batch], device=device)
            tk = torch.tensor(win_take[k : k + batch], device=device)
            i = st[:, None] + ar
            truth, interp, mask, ttg, _, _ = prepare(rot[i], root[i], gap, body, device)
            out = model(truth, interp, mask, ttg, style[tk], with_contacts=True)
            pred, logits = out if isinstance(out, tuple) else (out, None)
            terms = losses.window_terms(pred, truth, a, b, body, logits)
            loss = losses.total(terms, weights).double().cpu().numpy()
            tk_np = win_take[k : k + batch]
            np.add.at(total, tk_np, loss)
            np.add.at(count, tk_np, 1)
            for name_, v in terms.items():
                term_sum.setdefault(name_, np.zeros(n))
                np.add.at(term_sum[name_], tk_np, v.double().cpu().numpy())
            for j in np.unique(tk_np):
                rows = np.nonzero(tk_np == j)[0]
                r = rows[np.argmax(loss[rows])]
                if loss[r] > worst[j]:
                    worst[j] = loss[r]
                    worst_at[j] = int(win_start[k + r]) - int(starts[j]) + a
    out = []
    for i, (take, lab, f) in enumerate(zip(takes, own, frames)):
        if not count[i]:
            continue
        terms_mean = {k_: float(v[i] / count[i]) for k_, v in term_sum.items()}
        out.append({
            "id": f"{name}/{take}", "label": str(lab) if str(lab) != str(take) else "", "frames": f,
            "windows": int(count[i]), "loss": round(float(total[i] / count[i]), 6), "worst": round(float(worst[i]), 6),
            "worst_at": int(worst_at[i]), "terms": {k_: round(v, 6) for k_, v in terms_mean.items()},
            "weighted": {k_: round(v * weights.get(k_, 0.0), 6) for k_, v in terms_mean.items()},
        })
    rec = {"takes": n, "frames": int(sum(frames)), "crc32": digest(takes, frames), "evaluated": len(out),
           "skipped": n - len(out), "windows": int(count.sum())}
    del rot, root
    return out, rec


def main(argv=None):
    import torch

    from . import infer
    from .datasets import REGISTRY
    from .kinematics import Body
    from .rig import Skeleton

    ap = argparse.ArgumentParser(description="Flag the takes the model fails on, for the review app (nothing is decided)")
    ap.add_argument("--tag", default="parts", help="training/data/models/<tag>")
    ap.add_argument("--checkpoint", default="model", choices=("model", "best"), help="model.pt (the last) or best.pt")
    ap.add_argument("--sets", default=None, help="comma list of sets (default: every set the registry allows)")
    ap.add_argument("--gap", type=int, default=GAP, help="the gap filled (frames)")
    ap.add_argument("--stride", type=int, default=STRIDE, help="a window every this many frames of a take (1: every window)")
    ap.add_argument("--share", type=float, default=SHARE, help="the share of takes flagged (the worst)")
    ap.add_argument("--by", default="loss", choices=("loss", "worst"), help="rank takes by their mean window loss, or their worst window's")
    ap.add_argument("--batch", type=int, default=1024)
    ap.add_argument("--decisions", default=str(review.DECISIONS), help="the review's decisions file")
    ap.add_argument("--out", default=str(RIG / FILE), help="the flags file (the review app reads <data>/flags.json)")
    args = ap.parse_args(argv)
    if not 1 <= args.gap <= 45 or args.stride < 1 or not 0 < args.share <= 1:
        ap.error("--gap 1..45, --stride at least 1, --share in (0, 1]")

    t0 = time.time()
    net, card = infer.load(args.tag, args.checkpoint)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    net = net.to(device)
    names = [n for n in args.sets.split(",") if n] if args.sets else [n for n in REGISTRY if REGISTRY[n].commercial]
    body = Body(Skeleton.load(), device)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    all_takes, sets = [], {}
    for n in names:
        if not (RIG / f"{n}.npz").exists():
            print(f"set {n} not built: run python -m ac_train.datasets {n}", flush=True)
            continue
        t1 = time.time()
        takes, rec = evaluate_set(net, card, n, body, device, decisions, args.gap, args.stride, args.batch)
        sets[n] = rec
        all_takes += takes
        print(f"{n}: {rec['evaluated']} of {rec['takes']} takes, {rec['windows']} windows, {rec['skipped']} skipped "
              f"(left out or too short), {time.time() - t1:.0f} s", flush=True)
    model = {"tag": args.tag, "checkpoint": args.checkpoint, "file": str(infer.MODELS / args.tag / f"{args.checkpoint}.pt"),
             "steps": card.get("steps"), "best": card.get("best")}
    rec = record(model, sets, all_takes, args.gap, args.stride, args.share, args.by, time.time() - t0)
    write(pathlib.Path(args.out), rec)
    print(f"{len(rec['flagged'])} of {rec['takes']} takes flagged (median loss {rec['median_loss']:.3f}) into {args.out}, "
          f"{rec['elapsed_s']:.0f} s; nothing decided (review them: ac-review, or review.list {{\"show\": \"model\"}})")
    for t in rec["flagged"][:20]:
        print(f"  {t['rank']:3} {t['id']:48} {t['why']}")


if __name__ == "__main__":
    main()
