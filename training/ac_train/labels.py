"""Each take's labels, and the styles a model learns: whatever labels the training data has.

A take's labels are its motion type first, then its style words:

- **type**: the review's ``type`` when it gives one, else a word from the take name (:data:`AUTO`:
  ``walk_cmu_02_01`` is a walk, ``fallAndGetUp1_subject1`` a fall). The types are walk, run, jump,
  idle, combat, ground, acrobatic, fall, dance, gesture, sit, carry, sports and exercise. A name
  that matches nothing has no type.
- **style words**: 100STYLE's style name when it is a way of moving (:data:`STYLE100`:
  ``Neutral`` -> ``natural``, ``Old`` -> ``elderly``), then the review's ``tags``, added. A
  100STYLE style that is a held pose or prop (``Teapot``, ``ArmsFolded``), an arm action you
  would key (``Waving``) or a path (``SpinClock``) is no style word: the keys already say it.
  Its takes still train under their type.

Words are normalised (:func:`word`): lower case, ``-`` between words, so "Old Man", "old_man" and
"OldMan" are one label. A person's word that only says an automatic type another way
(:data:`ALIASES`: "fight", "running") is that type, so the two don't split a style. "any" is not a
label: it is what the model does with no style given.

The styles a model learns (:func:`vocabulary`) are the labels with at least ``min_minutes`` of
training frames, "any" first and the rest in name order. A rarer label teaches the network noise:
it is left out and said so. The model card lists the styles, which is all the app offers.

``python -m ac_train.labels [set ...]`` prints the labels and their minutes without training;
with a take filter (:mod:`ac_train.selection`: ``--natural``, ``--types``, ``--styles``,
``--takes``) it prints what that filter's run would learn.
"""

from __future__ import annotations

import argparse
import pathlib
import re
from collections import Counter

import numpy as np

#: motion types from take names: the first type with a word (a regular expression) in the
#: lower-case name wins, so the order matters: "fightAndSports" is combat, "ground_sit_stand"
#: sits. "sit" must start a word, or every "transition" would sit
AUTO = [
    ("fall", ("fall", "stumble", "getup", "push")),
    ("dance", ("dance",)),
    ("combat", ("fight", "punch", "kick", "aim", "box", "sword", "combat", "shoot")),
    ("sports", ("sport",)),
    ("exercise", ("stretch", "exercise", "squat", "yoga")),
    ("sit", (r"(?<![a-z])sit", "chair", "desk")),
    ("ground", ("ground", "crawl", "floor", "lying", "crouch")),
    ("acrobatic", ("obstacle", "acrobat", "flip", "vault", "cartwheel")),
    ("jump", ("jump", "hop", "leap")),
    ("run", ("run", "sprint", "jog")),
    ("carry", ("carry", "pickup", "pick_up", "door")),
    ("walk", ("walk", "sidestep", "turn", "step", "avoid", "loco")),
    ("gesture", ("gesture", "speech", "talk", "whiteboard", "wave")),
    ("idle", ("idle", "stand", "rom", "wait", "identity")),
]
#: a person's word for an automatic type -> that type (only plain synonyms)
ALIASES = {
    "falling": "fall", "fight": "combat", "fighting": "combat", "acrobatics": "acrobatic",
    "jumping": "jump", "running": "run", "sprint": "run", "jog": "run", "walking": "walk", "idling": "idle",
    "dancing": "dance", "sitting": "sit", "carrying": "carry", "sport": "sports", "gestures": "gesture",
    "gesturing": "gesture",
}
#: the 100STYLE styles that are AI styles -> their label (the app shows "-" as a space). A style
#: is a way of moving the keys don't give; the other 67 are poses, actions or paths
STYLE100 = {
    # natural
    "Neutral": "natural",
    # mood and character
    "Angry": "angry", "Depressed": "depressed", "Elated": "happy", "Proud": "proud", "Strutting": "swagger",
    "Old": "elderly", "Drunk": "drunk", "Zombie": "zombie", "Robot": "robot", "Stiff": "stiff",
    "Rushed": "hurried", "Tiptoe": "tiptoe", "InTheDark": "feeling-in-the-dark", "Followed": "nervous",
    "Heavyset": "muscle-man",
    # crouched and bent
    "Crouched": "crouched", "BentKnees": "bent-knees", "OnToesCrouched": "crouched-on-toes",
    # injured
    "LimpLeft": "limp-left", "LimpRight": "limp-right", "DragLeftLeg": "drag-left-leg",
    "DragRightLeg": "drag-right-leg",
    # gait
    "Skip": "skip", "March": "march", "HighKnees": "high-knees", "BigSteps": "big-steps",
    "Quail": "small-quick-steps", "SlideFeet": "shuffle", "CrossOver": "catwalk", "Penguin": "waddle",
    "DuckFoot": "toes-out", "PigeonToed": "toes-in",
}
ANY = "any"
#: a style needs this much training motion by default (minutes)
MIN_MINUTES = 5.0
FPS = 30


def word(s: str) -> str:
    """A label as stored: lower case, words joined by ``-``, CamelCase split ("" for nothing)."""
    s = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "-", str(s).strip())
    w = re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")
    return "" if w == ANY else w


def auto_type(take: str) -> str:
    """The motion type a take name gives ("" when it gives none)."""
    # an ACCAD or AddBiomechanics take is "<type>_<set>_<stem>": only its type word says what it is
    t = re.split(r"_(?:accad|addbiomechanics)_", take.lower())[0]
    for name, words in AUTO:
        if any(re.search(w, t) for w in words):
            return name
    return ""


def type_of(take: str, kind: str = "") -> str:
    """A take's motion type: the review's ``kind`` (a person's word), else the name's."""
    w = word(kind)
    return ALIASES.get(w, w) or auto_type(take)


def mirrored(w: str) -> str:
    """The label of the same motion mirrored left <-> right: "limp-left" is "limp-right",
    "spin-clock" is "spin-anti-clock"; a label with no side is itself."""
    parts = w.split("-")
    out, i = [], 0
    while i < len(parts):
        p = parts[i]
        if p in ("anti", "counter") and i + 1 < len(parts) and parts[i + 1] in ("clock", "clockwise"):
            out.append(parts[i + 1])
            i += 2
            continue
        out += {"left": ["right"], "right": ["left"], "clock": ["anti", "clock"], "clockwise": ["anti", "clockwise"],
                "anticlockwise": ["clockwise"], "counterclockwise": ["clockwise"]}.get(p, [p])
        i += 1
    return "-".join(out)


def set_style(set_name: str, label: str) -> str:
    """The style a set's own label names, every one of them: 100STYLE's style name as a word
    ("teapot"; "" for other sets). The library keeps one clip per such style."""
    if set_name != "100style":
        return ""
    return word(str(label).split(":", 1)[0])


def auto_style(set_name: str, label: str) -> str:
    """The style word a set's own label gives: a 100STYLE style in :data:`STYLE100` ("" for the
    rest and for other sets)."""
    if set_name != "100style":
        return ""
    return STYLE100.get(str(label).split(":", 1)[0].strip(), "")


def of(set_name: str, take: str, label: str = "", kind: str = "", tags=()) -> list[str]:
    """A take's labels: its type (the review's ``kind``, else the name's), then its style words."""
    out = [type_of(take, kind), auto_style(set_name, label), *(ALIASES.get(word(t), word(t)) for t in tags)]
    return list(dict.fromkeys(w for w in out if w))


def vocabulary(frames: Counter, min_minutes: float = MIN_MINUTES) -> tuple[list[str], dict[str, int]]:
    """(styles, left out): the labels with enough frames ("any" first, then by name), and the
    rarer ones with their frame counts."""
    need = min_minutes * 60 * FPS
    kept = sorted(w for w, n in frames.items() if n >= need)
    rare = {w: int(n) for w, n in sorted(frames.items()) if n < need}
    return [ANY, *kept], rare


def weights(types: list[str], frames: Counter, styles: list[str]) -> np.ndarray:
    """Sampling weight per take from its motion type's rarity: sqrt(the commonest type's frames /
    its type's frames), clamped to 1..3. Falls and flips come up about as often as walks; a take
    with no learned type weighs 1. Only types count, so a hundred small style words don't all
    weigh 3."""
    learned = {t for t in types if t and t in styles}
    if not learned:
        return np.ones(len(types))
    top = max(frames[t] for t in learned)
    return np.array([min(3.0, (top / frames[t]) ** 0.5) if t in learned else 1.0 for t in types])


def mirror_matrix(styles: list[str]) -> np.ndarray:
    """(S, S): style weights @ it are the mirrored window's ("limp-left" -> "limp-right"). A side
    label whose other side wasn't learned goes (row of zeros): the mirrored motion isn't it."""
    m = np.zeros((len(styles), len(styles)), np.float32)
    for i, w in enumerate(styles):
        j = styles.index(mirrored(w)) if mirrored(w) in styles else -1
        if j >= 0:
            m[i, j] = 1.0
    return m


def main(argv=None):
    from . import review, selection
    from .quality import RIG

    ap = argparse.ArgumentParser(description="The labels in the training data and the styles a model would learn")
    ap.add_argument("sets", nargs="*", help="sets (default: every data/rig/*.npz)")
    ap.add_argument("--decisions", default=str(review.DECISIONS))
    ap.add_argument("--min-minutes", type=float, default=MIN_MINUTES)
    selection.add_arguments(ap)
    args = ap.parse_args(argv)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    select = selection.from_args(args)
    frames: Counter = Counter()
    takes: Counter = Counter()
    names = args.sets or sorted(p.stem for p in RIG.glob("*.npz"))
    for rec in selection.plan_sets(names, decisions, select):
        if not rec.kept:
            continue
        for w in of(rec.set, rec.take, rec.label, rec.kind, rec.tags):
            frames[w] += rec.used
            takes[w] += 1
    styles, rare = vocabulary(frames, args.min_minutes)
    print(f"takes: {select.describe()}")
    print(f"{len(styles) - 1} styles with {args.min_minutes:g}+ minutes (all takes, before the 5 % held out):")
    for w in styles[1:]:
        print(f"  {w:24s} {frames[w] / FPS / 60:8.1f} min  {takes[w]:5d} takes")
    if rare:
        print(f"{len(rare)} labels left out (too little motion):")
        for w, f in rare.items():
            print(f"  {w:24s} {f / FPS / 60:8.1f} min  {takes[w]:5d} takes")


if __name__ == "__main__":
    main()
