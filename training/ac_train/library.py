"""Cut built-in animation clips from the retargeted mocap (``training/data/rig/*.npz``).

    python -m ac_train.library --sets cmu,100style,kimodo --out assets/library   # ships

Each take type (fight, sit, dance, walk, ...) maps onto one of the library's groups; for each type
the most active 3-second moments of the chosen sets (by joint motion, never overlapping, at most
one a take, and one a 100STYLE style, so four walks are four different walks) become read-only clips named "Fight 1", "Fight 2", ... A moment never touches the scan's cuts or comes from a
rejected take (``review.py``: the review's decisions win, and their type, when given, picks the
group). Numbering carries on after the clips already in ``assets/library``, the local-only
``training/data/library`` and ``--out``, so a bake only adds clips, never renames one a fixture
names, and never takes a name a local clip has in someone's own library. Every clip records the set
it was cut from in ``dataset``.

Only sets whose licence allows commercial use are cut into the repo's ``assets`` (every set the
repo supplies is one). A non-commercial set you add yourself goes to ``training/data/library``, and
``library.install`` puts it in your own app library.
"""

from __future__ import annotations

import argparse
import pathlib
import re

import numpy as np

from . import ASSETS, REPO, labels, review
from .clip import Clip, Key
from .datasets import REGISTRY, RIG

#: clips that stay on this machine (non-commercial sets you add), installed with library.install
LOCAL = REPO / "training" / "data" / "library"
SECONDS = 3.0
PER_TYPE = 4
KEY_EVERY = 2  # dense enough that the smooth curve between keys cannot whip fingers and toes

# (take-name pattern, group, clip name); the first match wins, None = skip the take
TYPES = [
    (r"^flip", "Action", "Acrobatics"),
    (r"^stretch", "Sports and fitness", "Stretch"),
    (r"^sports", "Sports and fitness", "Ball sports"),
    (r"rom|identity|multipleactions|vr_motions|controllers|tall|^box", None, None),
    (r"fight", "Martial arts", "Fight"),
    (r"baseball", "Sports and fitness", "Baseball swing"),
    (r"beat_saber", "Sports and fitness", "Saber swings"),
    (r"pushandfall|pushandstumble", "Falls", "Pushed"),
    (r"fallandgetup", "Falls", "Fall and get up"),
    (r"dance", "Dance", "Dance"),
    (r"armchair", "Sitting", "Armchair"),
    (r"desk", "Sitting", "Desk"),
    (r"sitting_and_viewing", "Sitting", "Sit and watch"),
    (r"ground_sit_stand", "Sitting", "Sit down and stand up"),
    (r"^sit", "Sitting", "Sit"),
    (r"ground", "On the floor", "On the ground"),
    (r"leg_jump", "Action", "Jump transitions"),
    (r"jump", "Action", "Jump"),
    (r"obstacles", "Action", "Vault"),
    (r"aiming|shooting", "Action", "Aim"),
    (r"crouch", "Locomotion", "Crouch walk"),
    (r"sprint", "Locomotion", "Sprint"),
    (r"run", "Locomotion", "Run"),
    (r"braking", "Locomotion", "Stop"),
    (r"turning_in_place", "Locomotion", "Turn in place"),
    (r"side_?step|walksideback", "Locomotion", "Side step"),
    (r"small_steps", "Locomotion", "Small steps"),
    (r"circular", "Locomotion", "Walk in a circle"),
    (r"walk|loco|avoid", "Locomotion", "Walk"),
    (r"gesture", "Gestures", "Gesture"),
    (r"speech", "Gestures", "Talk"),
    (r"whiteboard", "Gestures", "Whiteboard"),
    (r"transitions_motions_to_standing|standing|idle", "Standing", "Stand"),
    (r"transitions_between", "Action", "Transitions"),
    (r"carry|pickup", "Interaction", "Carry"),
    (r"door", "Interaction", "Open door"),
    (r"cooking", "Interaction", "Cook"),
    (r"gardening", "Interaction", "Garden"),
]


def classify(take: str):
    # a generated, ACCAD or AddBiomechanics take is "<type>_<set>_<stem>": only its type says what it is
    t = re.split(r"_(?:kimodo|accad|addbiomechanics)_", take.lower())[0]
    for pattern, group, name in TYPES:
        if re.search(pattern, t):
            return group, name
    return None, None


def _activity(rots: np.ndarray) -> np.ndarray:
    """Per-frame joint motion: how far the rotations move from one frame to the next."""
    d = np.abs(np.sum(rots[1:] * rots[:-1], -1)).clip(0, 1)
    return np.r_[0.0, np.arccos(d).sum(-1)]


MAX_SPEED = 15.0  # m/s: any joint faster than this in a window is a capture glitch
MAX_DROP = 0.4  # m: the lowest point may not fall further below the start (no ledges)


def _clean(skel, rots: np.ndarray, roots: np.ndarray) -> bool:
    """A window without glitches: no joint faster than MAX_SPEED, no big drop below the start."""
    P = np.array([skel.fk(r, o)[1] for r, o in zip(rots, roots)])
    if not np.isfinite(P).all():
        return False
    speed = np.linalg.norm(np.diff(P, axis=0), axis=-1).max() * 30
    feet = [skel.index[f"foot.{s}"] for s in "LR"]
    drop = P[0, feet, 2].min() - P[:, :, 2].min()
    return speed <= MAX_SPEED and drop <= MAX_DROP


def _taken(dirs, group: str, name: str) -> int:
    """The highest N of "<name> N" in ``group`` under any of ``dirs`` (0 when none)."""
    top = 0
    for d in dirs:
        for f in (d / "animations" / group).glob(f"{name} *.clip.json"):
            m = re.fullmatch(re.escape(name) + r" (\d+)\.clip\.json", f.name)
            if m:
                top = max(top, int(m.group(1)))
    return top


def build(sets: list[str], out: pathlib.Path, per_type: int = PER_TYPE, decisions: dict | None = None,
          scanned: bool = True) -> int:
    from .rig import Skeleton

    skel = Skeleton.load()
    bones = list(skel.names)
    n_win = int(SECONDS * 30)
    candidates: dict[tuple[str, str], list] = {}
    data = {}
    for set_name in sets:
        path = RIG / f"{set_name}.npz"
        if not path.exists():
            raise FileNotFoundError(f"{path} is missing: run python -m ac_train.datasets {set_name}")
        d = data[set_name] = np.load(path)
        rot, starts, takes = d["rotations"], d["starts"], d["takes"]
        own = d["labels"] if "labels" in d.files else takes
        plan = review.plan_set(set_name, len(rot), starts, takes, decisions, scanned, npz=d)
        print(plan.summary(), flush=True)
        for note in plan.notes:
            print(f"  {note}", flush=True)
        for s, take, own_label, t in zip(starts, takes, own, plan.takes):
            group, name = classify(t.kind) if t.kind and classify(t.kind)[0] else classify(str(take))
            fine = review.starts(t, n_win)  # 3 s windows clear of every cut
            if group is None or not len(fine):
                continue
            act = _activity(rot[s : s + t.frames].astype(np.float64))
            win = np.full(t.frames - n_win + 1, -np.inf)
            win[fine] = np.convolve(act, np.ones(n_win), "valid")[fine]  # motion in each 3 s window
            source = (set_name, labels.set_style(set_name, str(own_label)) or str(take))
            for i in np.argsort(-win)[: 40]:
                if not np.isfinite(win[i]):
                    break
                candidates.setdefault((group, name), []).append((float(win[i]), set_name, int(s + i), source))
    count = 0
    for (group, name), cands in sorted(candidates.items()):
        cands.sort(reverse=True)
        picked = []
        for score, set_name, at, source in cands:  # the most active clean moments, one a take (or style)
            if all(source != p[3] for p in picked):
                d = data[set_name]
                if not _clean(skel, d["rotations"][at : at + n_win].astype(np.float64), d["roots"][at : at + n_win].astype(np.float64)):
                    continue
                picked.append((score, set_name, at, source))
            if len(picked) == per_type:
                break
        k = _taken([ASSETS / "library", LOCAL, out], group, name)
        for _, set_name, at, _ in picked:
            d = data[set_name]
            r = d["rotations"][at : at + n_win].astype(np.float64)
            o = d["roots"][at : at + n_win].astype(np.float64)
            o = o - np.array([o[0, 0], o[0, 1], 0.0])  # starts on the spot
            clip = Clip(bones=bones, fps=30.0, name=f"{name} {k + 1}", group=group, origin="mocap", dataset=set_name)
            for f in range(0, n_win, KEY_EVERY):
                clip.set_key(Key(f, r[f], o[f], "smooth"))
            clip.set_key(Key(n_win - 1, r[-1], o[-1], "smooth"))
            rr, oo = clip.sample_all()  # the clip as it will play, not only the raw frames
            if not _clean(skel, rr, oo):
                continue
            k += 1
            dest = out / "animations" / group / f"{clip.name}.clip.json"
            dest.parent.mkdir(parents=True, exist_ok=True)
            clip.save(dest)
            print(f"  {group}/{clip.name} <- {set_name} frame {at}", flush=True)
            count += 1
    print(f"{count} clips -> {out}")
    return count


def main(argv=None):
    ap = argparse.ArgumentParser(description="Cut built-in animation clips from the retargeted sets")
    ap.add_argument("--sets", required=True, help=f"comma list of sets ({', '.join(n for n in REGISTRY if n != 'builtin_loops')})")
    ap.add_argument("--out", required=True, help="a library root (clips go to <out>/animations/<group>/)")
    ap.add_argument("--per-type", type=int, default=PER_TYPE)
    ap.add_argument("--decisions", default=str(review.DECISIONS), help="the review's decisions file")
    ap.add_argument("--unscanned", action="store_true", help="sets with no quality scan: every frame may be cut from")
    args = ap.parse_args(argv)
    sets = [n for n in args.sets.split(",") if n]
    nc = [n for n in sets if n in REGISTRY and not REGISTRY[n].commercial]
    if nc and pathlib.Path(args.out).resolve().is_relative_to(ASSETS.resolve()):
        ap.error(f"{', '.join(nc)} only allow non-commercial use: cut them into {LOCAL} (yours only), not the shipped assets")
    if args.unscanned:
        print("--unscanned: no set's scan is read; clips may come from glitches and rejected takes", flush=True)
    build(sets, pathlib.Path(args.out), args.per_type,
          review.load_decisions(pathlib.Path(args.decisions)), not args.unscanned)


if __name__ == "__main__":
    main()
