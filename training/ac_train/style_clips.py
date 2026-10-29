"""Built-in clips from 100STYLE: realistic locomotion for travel, and one walk for every style.

    python -m ac_train.style_clips --out assets/library

- **Locomotion** (what travel walks with): the *Neutral* style ("normal walking") walking forwards
  and sideways as "Walk N" and "Side step N", standing as "Standing/Stand N", and the *Crouched*
  style walking forwards as "Crouch walk N". These are plain, real steps, so travel's are too.
  100STYLE's runs are jogs round a small room, never 3 straight seconds, so travel's "Run N" come
  from CMU's few straight runs instead (:data:`CMU_RUNS`).
- **Walk styles**: every other style walking forwards, one clip named after its style ("Zombie",
  "Arms above head"; an AI style by its AI name, a cryptic one by what it shows:
  :func:`style_title`), in a group of its own that travel never uses.

Each clip is the most active clean 3 seconds of its take that walks somewhere (2026-10-28 review:
the most active moments were the turns at the ends of the room, so "walks" turned round):

- it turns at most :data:`MAX_TURN` degrees, heading smoothed over a second (a spinning style
  keeps its spin);
- a forwards walk goes forwards: it drifts sideways at most :data:`MAX_DRIFT` of the way it goes;
- it keeps moving: no half second without :data:`MIN_STEP` metres (except the stop-start style).

A style walk with no such window may curve a little more (:data:`CURVED`), and says so; one with
none at all gets no clip, and says so (its takes still train the style). Then
:mod:`ac_train.library`'s rules: no scan cut or rejected take, no glitch, numbering carried on after
the clips already there (here and in the local ``training/data/library``); a clip already there is
left alone. Every clip is turned to face the front (-Y) on its first frame, so a contact sheet's
front view shows its front.
"""

from __future__ import annotations

import argparse
import math
import pathlib

import numpy as np

from . import labels, quat, review
from .clip import Clip, Key
from .datasets import RIG
from .library import KEY_EVERY, LOCAL, SECONDS, _activity, _clean, _taken

SET = "100style"
#: (style, gait code, group, clip name, how many, metres it must travel in 3 s, which way):
#: the plain clips travel uses
PLAIN = [
    ("Neutral", "FW", "Locomotion", "Walk", 4, 1.5, "forward"),
    ("Neutral", "SW", "Locomotion", "Side step", 4, 1.0, "side"),
    ("Neutral", "ID", "Standing", "Stand", 1, 0.0, None),
    ("Crouched", "FW", "Locomotion", "Crouch walk", 4, 1.0, "forward"),
]
#: CMU takes with 3 clean, nearly straight seconds of plain running (2026-10-28 review: the
#: others start, stop or turn), one "Run N" each; runs may turn up to RUN_TURN degrees
CMU_RUNS = ["run_cmu_113_07", "run_cmu_111_23"]
RUN_TURN = 30.0
STYLED = "Walk styles"
STYLED_GAIT = "FW"
STYLED_TRAVEL = 0.5
#: the most a clip may turn (degrees, over its 3 seconds), unless the style is a spin
MAX_TURN = 20.0
#: a forwards walk may drift sideways this much of the way it goes forwards
MAX_DRIFT = 0.35
#: a style walk with no straight window may curve this much (turn degrees, drift), and says so
CURVED = (35.0, 0.6)
#: metres a moving clip covers in every half second
MIN_STEP = 0.05
#: styles that stand still on purpose
PAUSING = ("StartStop",)


#: clip names that say what you will see where 100STYLE's name doesn't (2026-10-29, owner's
#: decision); a style that is an AI style is named as the app names the style (``labels.STYLE100``)
CLEAR_NAMES = {
    "Akimbo": "Hands on hips", "Balance": "Balancing (arms out)",
    "BouncyLeft": "Spring off left foot", "BouncyRight": "Spring off right foot", "Cat": "Cat paws",
    "FairySteps": "Heel-to-toe (tightrope)", "GracefulArms": "Ballet arms", "LawnMower": "Disco hand roll",
    "Monk": "Hands clasped, head bowed", "Morris": "Morris dance skip", "Roadrunner": "High knees, arms back",
    "Rocket": "Bent forward, arms back", "SpinAntiClock": "Spinning walk (anticlockwise)",
    "SpinClock": "Spinning walk (clockwise)", "Star": "Star shape (arms and legs wide)",
    "Superman": "Right arm forward", "Swat": "Rifle walk", "Teapot": "Hand on hip, arm out",
    "TogetherStep": "Step-together", "WildLegs": "High kicks",
}


def style_title(style: str) -> str:
    """A 100STYLE style as a clip name: its AI style's name ("Quail" is "Small quick steps"), a
    clearer name (:data:`CLEAR_NAMES`), else its own words ("ArmsAboveHead" is "Arms above head")."""
    if style in CLEAR_NAMES:
        return CLEAR_NAMES[style]
    w = labels.STYLE100.get(style) or labels.word(style)
    w = w.replace("-", " ")
    return w[:1].upper() + w[1:]


#: styles whose 3 seconds are chosen by what the arms do, not the whole body's activity (the
#: most active seconds missed the style, 2026-10-28 review): Aeroplane holds one arm out at a
#: time, so its clip catches a change of arm; Beat chest's hands barely move, so its clip has the
#: most hand motion; Waving's waves are short, so its clip has the most time with a hand raised
ARM_STYLES = ("Aeroplane", "BeatChest", "Waving")


def arm_scores(skel, rot, roots, start: int, frames: int, n_win: int, style: str) -> np.ndarray:
    """A score for each 3-second window of the take (higher shows the style better)."""
    r = rot[start : start + frames].astype(np.float64)
    o = roots[start : start + frames].astype(np.float64)
    P = np.array([skel.fk(a, b)[1] for a, b in zip(r, o)])
    lw, rw = P[:, skel.index["wrist.L"]], P[:, skel.index["wrist.R"]]
    shoulder = P[:, skel.index["upperarm01.L"], 2]
    win = lambda x: np.convolve(x, np.ones(n_win), "valid")  # noqa: E731
    if style == "Aeroplane":  # frames each arm is out level with the shoulder: both, not one
        return np.minimum(win(lw[:, 2] - shoulder > -0.12), win(rw[:, 2] - shoulder > -0.12))
    if style == "BeatChest":
        speed = np.r_[0.0, np.linalg.norm(np.diff(lw, axis=0), axis=1) + np.linalg.norm(np.diff(rw, axis=0), axis=1)]
        return win(speed)
    raised = np.maximum(lw[:, 2], rw[:, 2]) - shoulder  # Waving
    return win(np.clip(raised + 0.3, 0.0, None))


def heading(q: np.ndarray) -> np.ndarray:
    """(T, 4) root rotations (x, y, z, w) -> where the figure's left (+X) points in the floor
    plane, radians, unwrapped: how the body turns over the frames."""
    x, y, z, w = q[:, 0], q[:, 1], q[:, 2], q[:, 3]
    return np.unwrap(np.arctan2(2 * (x * y + z * w), 1 - 2 * (y * y + z * z)))


def windows(skel, rot, roots, start: int, take: review.Take, n_win: int, count: int,
            travel: float = 0.0, max_turn: float | None = MAX_TURN, way: str | None = "forward",
            moving: bool = True, max_drift: float = MAX_DRIFT, scores: np.ndarray | None = None) -> list[int]:
    """Up to ``count`` starts (in the set's frames) of the take's most active clean 3 seconds that
    turn at most ``max_turn`` degrees, travel at least ``travel`` metres ``way`` ("forward",
    "side" or None: any way) and, when ``moving``, never stand still; never overlapping and never
    touching a cut. ``scores``: each window's score in place of its activity."""
    fine = review.starts(take, n_win)
    if not len(fine):
        return []
    end = start + take.frames
    act = _activity(rot[start:end].astype(np.float64))
    score = np.convolve(act, np.ones(n_win), "valid") if scores is None else scores
    # the body's way over a second: a hip-swinging style twists the root back and forth as it goes
    raw = heading(rot[start:end, 0].astype(np.float64))
    yaw = np.convolve(np.pad(raw, 15, mode="edge"), np.ones(31) / 31, "valid")
    win_yaw = np.lib.stride_tricks.sliding_window_view(yaw, n_win)
    turn = np.degrees(win_yaw.max(1) - win_yaw.min(1))
    r = roots[start:end, :2].astype(np.float64) * skel.hip_height
    d = r[n_win - 1 :] - r[: len(r) - n_win + 1]
    h = yaw[: len(d)]
    left = np.stack([np.cos(h), np.sin(h)], -1)
    fwd = np.stack([np.sin(h), -np.cos(h)], -1)  # left turned a quarter clockwise: facing
    along, side = (d * fwd).sum(-1), np.abs((d * left).sum(-1))
    ok = np.zeros(len(score), bool)
    ok[fine] = True
    if way == "forward":
        ok &= (along >= travel) & (side <= max_drift * np.maximum(along, 1e-9))
    elif way == "side":
        ok &= side >= travel
    else:
        ok &= np.hypot(*d.T) >= travel
    if max_turn is not None:
        ok &= turn <= max_turn
    if moving:
        half = 15
        step = np.linalg.norm(r[half:] - r[:-half], axis=1)
        slow = np.lib.stride_tricks.sliding_window_view(step, n_win - half).min(1)
        ok &= slow[: len(ok)] >= MIN_STEP
    win = np.where(ok, score, -np.inf)
    picked: list[int] = []
    for i in np.argsort(-win):
        if not np.isfinite(win[i]) or len(picked) == count:
            break
        at = start + int(i)
        if any(abs(at - p) < n_win for p in picked):
            continue
        if _clean(skel, rot[at : at + n_win].astype(np.float64), roots[at : at + n_win].astype(np.float64)):
            picked.append(at)
    return picked


def face_front(skel, r: np.ndarray, o: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Rotations and root offsets turned about the vertical so the hips face -Y on frame 0."""
    P = skel.fk(r[0], o[0])[1]
    lr = P[skel.index["upperleg01.L"]] - P[skel.index["upperleg01.R"]]
    a = -math.atan2(lr[1], lr[0])  # brings the figure's left onto +X
    qz = np.array([0.0, 0.0, math.sin(a / 2), math.cos(a / 2)])
    r = r.copy()
    r[:, 0] = quat.mul(np.broadcast_to(qz, r[:, 0].shape), r[:, 0])
    c, s = math.cos(a), math.sin(a)
    o = o.copy()
    o[:, :2] = o[:, :2] @ np.array([[c, s], [-s, c]])
    return r, o


def cut(skel, rot, roots, at: int, n_win: int, group: str, name: str, dataset: str = SET) -> Clip | None:
    """The clip of frames ``at`` .. ``at + n_win - 1``, starting on the spot and facing the front;
    None when the keyed clip (as it plays) is not clean."""
    r = rot[at : at + n_win].astype(np.float64)
    o = roots[at : at + n_win].astype(np.float64)
    o = o - np.array([o[0, 0], o[0, 1], 0.0])
    r, o = face_front(skel, r, o)
    clip = Clip(bones=list(skel.names), fps=30.0, name=name, group=group, origin="mocap", dataset=dataset)
    for f in range(0, n_win, KEY_EVERY):
        clip.set_key(Key(f, r[f], o[f], "smooth"))
    clip.set_key(Key(n_win - 1, r[-1], o[-1], "smooth"))
    rr, oo = clip.sample_all()
    return clip if _clean(skel, rr, oo) else None


def build(out: pathlib.Path, decisions: dict | None = None, only: set[str] | None = None) -> int:
    from .rig import Skeleton

    skel = Skeleton.load()
    path = RIG / f"{SET}.npz"
    if not path.exists():
        raise SystemExit(f"{path} is missing: run python -m ac_train.datasets {SET}")
    d = np.load(path)
    rot, roots, starts, takes = d["rotations"], d["roots"], d["starts"], d["takes"]
    plan = review.plan_set(SET, len(rot), starts, takes, decisions, npz=d)
    print(plan.summary(), flush=True)
    for note in plan.notes:
        print(f"  {note}", flush=True)
    # (style name as 100STYLE spells it, gait code) -> (start, plan take)
    by = {}
    for s, take, lab, t in zip(starts, takes, d["labels"], plan.takes):
        by[(str(lab).split(":", 1)[0], str(take).split("_")[2])] = (int(s), t)
    n_win = int(SECONDS * 30)
    count = 0

    def save(clip: Clip, at: int, why: str):
        nonlocal count
        dest = out / "animations" / clip.group / f"{clip.name}.clip.json"
        dest.parent.mkdir(parents=True, exist_ok=True)
        clip.save(dest)
        print(f"  {clip.group}/{clip.name} <- {why} frame {at}", flush=True)
        count += 1

    for style, gait, group, name, n, travel, way in PLAIN:
        if only and style_title(style) not in only:
            continue
        s, t = by.get((style, gait), (None, None))
        if t is None or t.drop:
            print(f"  {style} {gait}: no usable take ({t.drop if t else 'missing'})", flush=True)
            continue
        k = _taken([out, LOCAL], group, name)
        got = 0
        for at in windows(skel, rot, roots, s, t, n_win, n, travel, MAX_TURN, way, moving=way is not None):
            clip = cut(skel, rot, roots, at, n_win, group, f"{name} {k + 1}")
            if clip is not None:
                k += 1
                got += 1
                save(clip, at, f"{style} {gait}")
        if got < n:
            print(f"  {style} {gait}: {got} of {n} clips (too few clean straight windows)", flush=True)
    count += cmu_runs(skel, out, decisions, n_win, only)
    plain = {p[0] for p in PLAIN}
    for style in sorted({k[0] for k in by} - plain):
        title = style_title(style)
        if only and title not in only:
            continue
        dest = out / "animations" / STYLED / f"{title}.clip.json"
        s, t = by.get((style, STYLED_GAIT), (None, None))
        if dest.exists():
            continue
        if t is None or t.drop:
            print(f"  {title}: no usable forwards walk ({t.drop if t else 'missing'})", flush=True)
            continue
        spin = style.lower().startswith("spin")
        tries = [(None, MAX_DRIFT)] if spin else [(MAX_TURN, MAX_DRIFT), CURVED]
        done = False
        for i, (turn, drift) in enumerate(tries):
            arms = arm_scores(skel, rot, roots, s, t.frames, n_win, style) if style in ARM_STYLES else None
            for at in windows(skel, rot, roots, s, t, n_win, 3, STYLED_TRAVEL, turn, None if spin else "forward",
                              moving=style not in PAUSING, max_drift=drift, scores=arms):
                clip = cut(skel, rot, roots, at, n_win, STYLED, title)
                if clip is not None:
                    save(clip, at, f"{style} {STYLED_GAIT}" + (f" (no straight window: curves up to {turn:g} degrees)" if i else ""))
                    done = True
                    break
            if done:
                break
        if not done:
            print(f"  {title}: no clean 3 seconds walking on: no clip (the style still trains)", flush=True)
    print(f"{count} clips -> {out}")
    return count


def cmu_runs(skel, out: pathlib.Path, decisions, n_win: int, only) -> int:
    """One "Locomotion/Run N" from each of :data:`CMU_RUNS`: 3 straight seconds of running."""
    if only and "Run" not in only:
        return 0
    path = RIG / "cmu.npz"
    if not path.exists():
        print(f"  runs: {path} is missing (python -m ac_train.datasets cmu): no runs", flush=True)
        return 0
    d = np.load(path)
    rot, roots, starts, takes = d["rotations"], d["roots"], d["starts"], [str(t) for t in d["takes"]]
    plan = review.plan_set("cmu", len(rot), starts, takes, decisions, npz=d)
    k = _taken([out, LOCAL], "Locomotion", "Run")
    n = 0
    for name in CMU_RUNS:
        if name not in takes:
            print(f"  runs: cmu has no take {name}", flush=True)
            continue
        i = takes.index(name)
        t = plan.takes[i]
        if t.drop:
            print(f"  runs: {name} is left out ({t.drop})", flush=True)
            continue
        for at in windows(skel, rot, roots, int(starts[i]), t, n_win, 1, 2.0, RUN_TURN, "forward"):
            clip = cut(skel, rot, roots, at, n_win, "Locomotion", f"Run {k + 1}", dataset="cmu")
            if clip is not None:
                k += 1
                n += 1
                dest = out / "animations" / "Locomotion" / f"{clip.name}.clip.json"
                clip.save(dest)
                print(f"  Locomotion/{clip.name} <- cmu {name} frame {at}", flush=True)
                break
        else:
            print(f"  runs: {name} has no clean straight 3 seconds", flush=True)
    return n


def main(argv=None):
    ap = argparse.ArgumentParser(description="Cut 100STYLE's built-in clips: plain locomotion and a walk per style")
    ap.add_argument("--out", required=True, help="a library root (clips go to <out>/animations/<group>/)")
    ap.add_argument("--only", default="", help="comma list of clip titles to cut (\"Zombie,Natural\"); default all")
    ap.add_argument("--decisions", default=str(review.DECISIONS), help="the review's decisions file")
    args = ap.parse_args(argv)
    only = {x.strip() for x in args.only.split(",") if x.strip()} or None
    build(pathlib.Path(args.out), review.load_decisions(pathlib.Path(args.decisions)), only)


if __name__ == "__main__":
    main()
