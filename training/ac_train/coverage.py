"""How much training motion each evaluation slice has: the minutes of the registered sets that carry
each motion the evaluation scores apart (roadmap 134750), beside the windows the slice got.

    python -m ac_train.coverage                                  # every set in data/rig
    python -m ac_train.coverage accad kimodo --card parts        # some sets, one card's windows
    python -m ac_train.coverage --card parts-lowweights:best --card parts

The slices are the evaluation's (:mod:`ac_train.evaluate`), read from the recording's own motion
with the same thresholds, but on the training takes frame by frame rather than on drawn windows:

- ``start`` / ``stop``: the frames inside a span of at most ``MAX_GAP`` frames from hips under
  ``STILL_MS`` to hips over ``MOVING_MS`` (or back): exactly the gap frames of every window
  :func:`ac_train.evaluate.scenarios` would call a start or a stop;
- ``crouch``: hips between ``CROUCH_SHARE`` of their standing height with both feet on the floor;
- ``flight``: a run of at least ``FLIGHT_FRAMES`` frames with both feet off the floor (the frames
  in the air themselves, not every window around them);
- ``turn`` (the hard slice) and ``turn_step``: the gap frames of every window of at most ``MAX_GAP``
  frames whose root turns more than ``TURN_DEG`` at over ``TURN_SPEED`` at both ends (for
  ``turn_step``, with a foot planting or lifting inside it too);
- ``plant_change``: frames where a foot plants or lifts (events; nearly every walking window has one);
- ``still``: hips under ``STILL_MS`` (what the still context asks for at key A);
- the motion types (``type:<name>``) from the takes' labels, as training sees them.

Minutes are split into what training uses (takes not held out, cut frames left out), what
validation holds out, and what the scan or the review leaves out (``out``). On the held-out side the
script also counts, for every gap, the windows each scenario would get at the evaluation's uniform
draw, so it says how many windows a slice can expect from ``--windows`` and how many it would need to
be trusted (:data:`TRUSTED_WINDOWS`).

With ``--card <tag>[:<checkpoint>]`` the table carries the card's windows and numbers for each slice
(the first card is the model; a later card fills rows it lacks, and the row says whose numbers they
are), and ranks the slices by the model's share of the interpolation's joint error. The table is
printed and written to ``data/coverage.json`` (gitignored).
"""

from __future__ import annotations

import argparse
import json
import pathlib
from collections import Counter, defaultdict

import numpy as np

from . import MODELS, OUT, labels, review, split
from .evaluate import BINS, CROUCH_SHARE, FLIGHT_FRAMES, MIN_WINDOWS, MOVING_MS, STILL_MS, TURN_DEG, TURN_SPEED
from .model import CONTEXT, MAX_GAP, WINDOW
from .quality import FPS, RIG, THRESHOLDS, feet, flat_heights
from .rig import Skeleton

#: a slice's numbers are trusted with this many windows. Per-window joint error varies about as
#: much as its mean (the by_type rows span 9 to 74 mm), so a 100-window mean is known to about
#: 10 %, well inside the 20-60 % gaps between slices; :data:`ac_train.evaluate.MIN_WINDOWS` (30)
#: only says when a row is noise (about 20 %), and 17-25 windows (126500's start, stop, crouch)
#: are worse than the differences being ranked
TRUSTED_WINDOWS = 100
#: the evaluation's default windows a bin, to say what a slice can expect
DEFAULT_WINDOWS = 512
SLICES = ("start", "stop", "crouch", "flight", "turn_step", "turn", "plant_change", "still")
OUTPUT = OUT / "coverage.json"


# ---- one take ------------------------------------------------------------------------------------

def motion(skel: Skeleton, rots: np.ndarray, roots: np.ndarray, flat: np.ndarray) -> dict:
    """The per-frame facts the slices read: hip speed between frames ``v`` (F - 1,), the root's
    floor velocity ``vel`` (F - 1, 2), ``share`` of standing height (F,), ``air`` both feet off the
    floor (F,), and ``change`` a foot planting or lifting (F,), the evaluation's way."""
    F = len(rots)
    pts, _ = feet(skel, rots, roots)
    height = pts[..., 2] - flat
    air = height.min(1) > THRESHOLDS["contact_m"]
    down = height < THRESHOLDS["contact_m"]
    flat_down = down[:, 0:2] & down[:, 2:4]
    sp = np.linalg.norm(np.diff(pts[..., :2], axis=0), axis=-1) * FPS  # (F - 1, 4)
    slow = np.minimum(sp[:, 0:2], sp[:, 2:4])
    planted = np.zeros((F, 2), bool)
    planted[1:] = flat_down[1:] & flat_down[:-1] & (slow < THRESHOLDS["skate_ms"])
    change = np.zeros(F, bool)
    change[1:] = (planted[1:] != planted[:-1]).any(1)
    vel = np.diff(np.asarray(roots[:, :2], np.float64), axis=0) * skel.hip_height * FPS
    return {"v": np.linalg.norm(vel, axis=-1), "vel": vel, "share": np.asarray(roots[:, 2], np.float64),
            "air": air, "change": change}


def span_frames(first: np.ndarray, second: np.ndarray, F: int, max_gap: int = MAX_GAP) -> np.ndarray:
    """(F,) the gap frames of every window (gap at most ``max_gap``) whose key A has ``first`` and
    whose last gap frame has ``second`` (both over the F - 1 between-frame speeds): frame t is in
    when the nearest ``first`` before it and the nearest ``second`` from it on are close enough."""
    n = len(first)
    idx = np.arange(n)
    last_first = np.maximum.accumulate(np.where(first, idx, -1))
    first_second = np.minimum.accumulate(np.where(second, idx, n)[::-1])[::-1]
    out = np.zeros(F, bool)
    if n < 2:
        return out
    t = np.arange(1, n)
    a, m = last_first[t - 1], first_second[t]
    out[t] = (a >= 0) & (m < n) & (m - a <= max_gap)
    return out


def window_frames(ok_at, F: int, max_gap: int = MAX_GAP) -> np.ndarray:
    """(F,) the gap frames of every window whose key A at ``a`` and gap ``g`` pass ``ok_at(g)``
    (a bool array over a = 0 .. F - 2 - g)."""
    diff = np.zeros(F + 1, np.int64)
    for g in range(1, max_gap + 1):
        if F - 1 - g <= 0:
            break
        a = np.nonzero(ok_at(g))[0]
        np.add.at(diff, a + 1, 1)
        np.add.at(diff, a + g + 1, -1)
    return np.cumsum(diff)[:F] > 0


def window_tests(m: dict):
    """Per scenario, ``ok(g)`` -> bool over key-A frames a = 0 .. F - 2 - g: whether the window
    with gap ``g`` there is one (:func:`ac_train.evaluate.scenarios`, in numpy)."""
    v, vel, share, air, change = m["v"], m["vel"], m["share"], m["air"], m["change"]
    F = len(share)
    fast = v > TURN_SPEED
    cos_turn = np.cos(np.radians(TURN_DEG))
    cum_share = np.concatenate([[0.0], np.cumsum(share)])
    cum_air = np.concatenate([[0], np.cumsum(air)])
    pair = np.zeros(F, bool)
    pair[1:] = air[1:] & air[:-1]
    cum_pair = np.concatenate([[0], np.cumsum(pair)])
    cum_change = np.concatenate([[0], np.cumsum(change)])

    def rng(g):
        return np.arange(0, max(F - 1 - g, 0))

    def start(g):
        a = rng(g)
        return (v[a] < STILL_MS) & (v[a + g] > MOVING_MS)

    def stop(g):
        a = rng(g)
        return (v[a] > MOVING_MS) & (v[a + g] < STILL_MS)

    def crouch(g):
        a = rng(g)
        mean = (cum_share[a + g + 1] - cum_share[a + 1]) / g
        no_air = (cum_air[a + g + 1] - cum_air[a + 1]) == 0
        return (mean > CROUCH_SHARE[0]) & (mean < CROUCH_SHARE[1]) & no_air

    def flight(g):
        a = rng(g)
        if g < FLIGHT_FRAMES:
            return np.zeros(len(a), bool)
        return (cum_pair[a + g + 1] - cum_pair[a + 2]) > 0

    def turn(g):
        a = rng(g)
        v0, v1 = vel[a], vel[a + g]
        cos = (v0 * v1).sum(-1) / np.maximum(np.linalg.norm(v0, axis=-1) * np.linalg.norm(v1, axis=-1), 1e-9)
        return fast[a] & fast[a + g] & (cos < cos_turn)

    def turn_step(g):
        a = rng(g)
        return turn(g) & ((cum_change[a + g + 2] - cum_change[a + 1]) > 0)

    return {"start": start, "stop": stop, "crouch": crouch, "flight": flight, "turn_step": turn_step, "turn": turn}


def slice_frames(m: dict) -> dict[str, np.ndarray]:
    """(F,) bool per slice: the frames of a take that carry each motion (the module doc)."""
    v, share, air, change = m["v"], m["share"], m["air"], m["change"]
    F = len(share)
    tests = window_tests(m)
    pair = np.zeros(F, bool)
    pair[1:] = air[1:] & air[:-1]
    in_run = pair.copy()
    in_run[:-1] |= pair[1:]  # both frames of every airborne pair
    still = np.zeros(F, bool)
    still[:-1] = v < STILL_MS
    return {
        "start": span_frames(v < STILL_MS, v > MOVING_MS, F),
        "stop": span_frames(v > MOVING_MS, v < STILL_MS, F),
        "crouch": (share > CROUCH_SHARE[0]) & (share < CROUCH_SHARE[1]) & ~air,
        "flight": in_run if FLIGHT_FRAMES == 2 else window_frames(tests["flight"], F) & in_run,
        "turn_step": window_frames(tests["turn_step"], F),
        "turn": window_frames(tests["turn"], F),
        "plant_change": change,
        "still": still,
    }


# ---- the sets -----------------------------------------------------------------------------------

def per_gap_windows(windows: int = DEFAULT_WINDOWS) -> dict[int, int]:
    """Gap -> windows the evaluation draws with that gap (``windows`` a bin, spread over its gaps)."""
    out = {}
    for lo, hi in BINS:
        per = max(1, windows // (hi - lo + 1))
        for g in range(lo, hi + 1):
            out[g] = per
    return out


def scan(names, decisions, skel: Skeleton, progress=print) -> dict:
    """Every take of ``names``: its slice frames by status, and the held-out windows per gap."""
    flat = flat_heights(skel)
    takes = []
    hits = {k: np.zeros(MAX_GAP + 1) for k in ("start", "stop", "crouch", "flight", "turn_step", "turn")}
    starts_per_gap = np.zeros(MAX_GAP + 1)
    for n in names:
        path = RIG / f"{n}.npz"
        if not path.exists():
            progress(f"set {n} not built ({path} is missing): run python -m ac_train.datasets {n}")
            continue
        d = np.load(path)
        rots, roots, starts, names_ = d["rotations"], d["roots"], d["starts"], d["takes"]
        own = d["labels"] if "labels" in d.files else names_
        plan = review.plan_set(n, len(rots), starts, names_, decisions, npz=d)
        for note in plan.notes:
            progress(f"  {n}: {note}")
        ends = [*starts[1:], len(rots)]
        for s, e, take, own_label, t in zip(starts, ends, names_, own, plan.takes):
            take = str(take)
            held = split.held_out(n, take)
            status = "out" if t.drop else ("holdout" if held else "train")
            m = motion(skel, rots[s:e], roots[s:e], flat)
            frames = slice_frames(m)
            bad = t.bad()
            good = ~bad
            rec = {"set": n, "take": take, "type": labels.type_of(take, t.kind) or "untyped",
                   "labels": labels.of(n, take, str(own_label), t.kind, t.tags), "status": status,
                   "frames": int(e - s), "used": 0 if t.drop else int(good.sum()), "cut": int(bad.sum()),
                   "slices": {k: int((f & good).sum()) if not t.drop else 0 for k, f in frames.items()},
                   "slices_out": {k: int(f.sum()) - (int((f & good).sum()) if not t.drop else 0) for k, f in frames.items()}}
            takes.append(rec)
            if status == "holdout":
                fine = review.starts(t, WINDOW)
                if len(fine):
                    a = fine + CONTEXT - 1
                    tests = window_tests(m)
                    for g in range(1, MAX_GAP + 1):
                        starts_per_gap[g] += len(fine)
                        for k, test in tests.items():
                            ok = test(g)
                            at = a[a < len(ok)]
                            hits[k][g] += int(ok[at].sum())
        progress(f"  {n}: {len(plan.takes)} takes")
    return {"takes": takes, "holdout_hits": {k: v.tolist() for k, v in hits.items()}, "holdout_starts": starts_per_gap.tolist()}


def expected_windows(hits: dict, starts: list, windows: int = DEFAULT_WINDOWS) -> dict[str, float]:
    """Scenario -> windows it would get from ``windows`` a bin at the evaluation's uniform draw."""
    per = per_gap_windows(windows)
    out = {}
    for k, h in hits.items():
        out[k] = float(sum(per[g] * h[g] / starts[g] for g in per if starts[g]))
    return out


def totals(takes: list[dict]) -> dict:
    """Minutes per slice and per type, by status and by set."""
    by_status = defaultdict(lambda: defaultdict(float))
    by_set = defaultdict(lambda: defaultdict(float))
    by_type = defaultdict(lambda: defaultdict(float))
    for t in takes:
        st = t["status"]
        by_status[st]["all"] += t["used"]
        by_status["out"]["all"] += t["cut"] if st != "out" else t["frames"]
        for k, f in t["slices"].items():
            by_status[st][k] += f
            by_status["out"][k] += t["slices_out"][k]
            if st == "train":
                by_set[t["set"]][k] += f
                by_type[t["type"]][k] += f
        if st == "train":
            by_set[t["set"]]["all"] += t["used"]
            by_set[t["set"]][f"type:{t['type']}"] += t["used"]
            by_type[t["type"]]["all"] += t["used"]
        by_status[st][f"type:{t['type']}"] += t["used"]
        by_status["out"][f"type:{t['type']}"] += t["cut"] if st != "out" else t["frames"]
    mins = lambda d: {k: round(v / FPS / 60, 2) for k, v in d.items()}  # noqa: E731
    return {"by_status": {s: mins(d) for s, d in by_status.items()}, "by_set": {s: mins(d) for s, d in by_set.items()},
            "by_type": {s: mins(d) for s, d in by_type.items()}}


# ---- the cards ----------------------------------------------------------------------------------

def load_card(spec: str) -> tuple[str, dict]:
    """``tag[:checkpoint]`` -> (name, its evaluation block)."""
    tag, _, ck = spec.partition(":")
    path = MODELS / tag / "model.json"
    card = json.loads(path.read_text(encoding="utf-8"))
    key = "evaluation_best" if ck == "best" else "evaluation"
    ev = card.get(key)
    if not ev:
        raise SystemExit(f"{path} has no {key}: run python -m ac_train.evaluate --tag {tag}" + (" --checkpoint best" if ck == "best" else ""))
    return f"{tag}:{ck or 'model'}", ev


def card_rows(cards: list[tuple[str, dict]]) -> dict[str, dict]:
    """Slice -> {windows, model, interp, card}: the first card that has the row gives it."""
    rows: dict[str, dict] = {}

    def put(name, r, who):
        if name in rows or not r or "windows" not in r:
            return
        rows[name] = {"windows": r["windows"], "model": r["model"], "interp": r["interp"], "card": who}

    for who, ev in cards:
        for k, r in ev.get("scenarios", {}).items():
            put(k, r, who)
        for k, r in ev.get("hard", {}).items():
            put(k, r, who)
        for k, r in ev.get("by_type", {}).items():
            put(f"type:{k}", r, who)
        for k, r in ev.get("bins", {}).items():
            put(f"gap:{k}", r, who)
        for k, r in ev.get("inputs", {}).items():
            put(f"context:{k}", r.get("all"), who)
    return rows


def ranking(rows: dict[str, dict]) -> list[dict]:
    """Slices by the model's share of the interpolation's joint error, worst first, with trust."""
    out = []
    for k, r in rows.items():
        m, i = r["model"].get("joint_m"), r["interp"].get("joint_m")
        if m is None or not i:
            continue
        out.append({"slice": k, "windows": r["windows"], "ratio": round(m / i, 3), "model_mm": round(m * 1000, 1),
                    "interp_mm": round(i * 1000, 1), "trusted": r["windows"] >= TRUSTED_WINDOWS,
                    "noise": r["windows"] < MIN_WINDOWS, "card": r["card"]})
    return sorted(out, key=lambda r: (-r["trusted"], -r["ratio"]))


# ---- the report ---------------------------------------------------------------------------------

def report(result: dict) -> str:
    t = result["totals"]
    st = t["by_status"]
    exp = result["expected_windows"]
    rows = result.get("card_rows", {})
    sets = sorted(t["by_set"])
    lines = [f"{'slice':16}{'train min':>10}{'holdout':>9}{'out':>8}" + "".join(f"{s[:12]:>13}" for s in sets)
             + f"{'eval win':>10}{'expect':>8}{'trusted':>9}{'model/interp':>13}{'mm':>12}  card"]

    def line(k, label=None):
        r = rows.get(k, {})
        w = r.get("windows", "")
        e = exp.get(k)
        trust = "" if w == "" else ("yes" if w >= TRUSTED_WINDOWS else ("noise" if w < MIN_WINDOWS else "no"))
        ratio = mm = ""
        if r:
            m, i = r["model"].get("joint_m"), r["interp"].get("joint_m")
            if m is not None and i:
                ratio, mm = f"{m / i:.3f}", f"{m * 1000:.1f}/{i * 1000:.0f}"
        per_set = "".join(f"{t['by_set'][s].get(k, 0.0):13.1f}" for s in sets)
        return (f"{(label or k):16}{st.get('train', {}).get(k, 0.0):10.1f}{st.get('holdout', {}).get(k, 0.0):9.1f}"
                f"{st.get('out', {}).get(k, 0.0):8.1f}{per_set}{w:>10}{'' if e is None else f'{e:.0f}':>8}{trust:>9}"
                f"{ratio:>13}{mm:>12}  {r.get('card', '')}")

    lines.append(line("all", "every frame"))
    for k in SLICES:
        lines.append(line(k))
    types = sorted(k for k in st.get("train", {}) if k.startswith("type:"))
    for k in types:
        lines.append(line(k))
    for k in rows:
        if k.startswith("gap:") or k.startswith("context:"):
            lines.append(line(k, k))
    lines.append("")
    lines.append(f"minutes: 'train' is what training uses (cut frames left out), 'holdout' what validation holds out, 'out' what "
                 f"the scan or the review leaves out; 'expect' is the windows a scenario gets from {DEFAULT_WINDOWS} windows a bin "
                 f"at the evaluation's draw; trusted at {TRUSTED_WINDOWS} windows, noise under {MIN_WINDOWS}")
    need = {k: (DEFAULT_WINDOWS * TRUSTED_WINDOWS / e if e else None) for k, e in exp.items()}
    small = [f"{k}: --windows {int(np.ceil(v / 100) * 100)}" for k, v in need.items() if v and v > DEFAULT_WINDOWS]
    if small:
        lines.append("to be trusted at the uniform draw: " + "; ".join(small))
    if result.get("ranking"):
        lines.append("")
        lines.append(f"{'rank by model/interp joint error (worst first)':48}{'windows':>8}{'ratio':>7}{'model mm':>10}{'interp mm':>10}  trust")
        for r in result["ranking"]:
            tag = "trusted" if r["trusted"] else ("noise" if r["noise"] else "thin")
            lines.append(f"{r['slice'] + ' (' + r['card'] + ')':48}{r['windows']:>8}{r['ratio']:>7.3f}{r['model_mm']:>10}{r['interp_mm']:>10}  {tag}")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Training minutes per evaluation slice, beside each slice's windows")
    ap.add_argument("sets", nargs="*", help="sets (default: every data/rig/*.npz)")
    ap.add_argument("--card", action="append", default=[], help="training/data/models/<tag>[:best] whose windows to show (repeatable)")
    ap.add_argument("--decisions", default=str(review.DECISIONS))
    ap.add_argument("--out", default=str(OUTPUT), help="where the JSON goes")
    args = ap.parse_args(argv)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    names = args.sets or sorted(p.stem for p in RIG.glob("*.npz"))
    skel = Skeleton.load()
    scanned = scan(names, decisions, skel, progress=lambda s: print(s, flush=True))
    result = {"sets": names, "thresholds": {"still_ms": STILL_MS, "moving_ms": MOVING_MS, "crouch_share": list(CROUCH_SHARE),
                                            "flight_frames": FLIGHT_FRAMES, "turn_deg": TURN_DEG, "turn_speed": TURN_SPEED,
                                            "max_gap": MAX_GAP, "trusted_windows": TRUSTED_WINDOWS, "min_windows": MIN_WINDOWS},
              "totals": totals(scanned["takes"]),
              "expected_windows": expected_windows(scanned["holdout_hits"], scanned["holdout_starts"]),
              "takes": scanned["takes"]}
    if args.card:
        cards = [load_card(c) for c in args.card]
        result["cards"] = [c for c, _ in cards]
        result["card_rows"] = card_rows(cards)
        result["ranking"] = ranking(result["card_rows"])
    counts = Counter(t["status"] for t in scanned["takes"])
    print(f"{len(scanned['takes'])} takes: {counts['train']} train, {counts['holdout']} held out, {counts['out']} left out")
    print(report(result))
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=1), encoding="utf-8")
    print(f"written {out}")


if __name__ == "__main__":
    main()
