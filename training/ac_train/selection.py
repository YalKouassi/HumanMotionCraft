"""Which takes a run reads: a filter by motion type and style word, and an explicit take list.

    python -m ac_train.selection --natural                      # the minutes that survive, by type and set
    python -m ac_train.selection --natural --write-takes review/natural-takes.json

Training (``ac_train.train``), the evaluation (``ac_train.evaluate``) and the label listing
(``ac_train.labels``) take the same arguments (:func:`add_arguments`, :func:`from_args`):

- ``--types idle,walk,run,jump``: only takes of those motion types (:func:`ac_train.labels.type_of`:
  the review's type, else the take name's);
- ``--styles natural``: only takes whose style words are all in the list; a take with no style word
  always passes, ``--styles none`` keeps the unstyled takes alone. A take's style words
  (:func:`style_words`) are the set's own style (100STYLE's: ``Neutral`` is ``natural``, the model's
  label when the style is one, else the style's own name, so ``Teapot`` is ``teapot`` and a walk
  with a teapot arm is a styled walk too) and the review's tags that are neither a motion type
  nor a data tag (:data:`DATA_TAGS`: ``range-of-motion`` and ``duplicate`` say what was cut, not
  how the body moves);
- ``--natural``: both at once, :data:`NATURAL_TYPES` and :data:`NATURAL_STYLES` (roadmap 153000);
- ``--takes <json>``: ``{"include": ["cmu/walk_cmu_02_01", ...], "exclude": [...]}``. A non-empty
  ``include`` lists exactly the takes to use and the type and style filters are not applied (the
  list is the owner's word, from the candidate videos); ``exclude`` takes out, whatever else says.
  An id no read set has is an error, never ignored (a misspelt exclusion would silently stay in).

The quality scan and the review come first: a rejected take stays out however it is listed, and a
take's cut frames stay cut. :func:`records` gives every take of a set its :class:`Record` (type,
style words, frames, held out or not, kept or why not); :func:`table` prints the minutes that
survive by type and by set, which training prints before any step (``--dry-run`` stops there).
"""

from __future__ import annotations

import argparse
import json
import pathlib
from collections import defaultdict
from dataclasses import dataclass

import numpy as np

from . import labels, review, split
from .quality import FPS, RIG

#: roadmap 153000: the natural motion types, and the style words a natural take may carry
NATURAL_TYPES = ("idle", "walk", "run", "jump")
NATURAL_STYLES = ("natural",)
#: the review's tags that describe the data, not a way of moving: no style word
DATA_TAGS = ("range-of-motion", "duplicate")
#: ``--styles none``: unstyled takes only
NONE = "none"


def style_words(set_name: str, label: str = "", tags=()) -> list[str]:
    """A take's style words, for the filter: the set's own style (100STYLE's, as the model's label
    when it is one, else its own name as a word), then the review's tags that are not a motion
    type (or its synonym) and not a data tag."""
    types = {name for name, _ in labels.AUTO}
    own = labels.auto_style(set_name, label) or labels.set_style(set_name, label)
    out = [own, *(labels.word(t) for t in tags)]
    return list(dict.fromkeys(w for w in out if w and labels.ALIASES.get(w, w) not in types and w not in DATA_TAGS))


@dataclass
class Record:
    """One take as the filter sees it."""
    set: str
    take: str
    label: str
    type: str
    styles: list[str]
    #: the review's type word and tags (what :func:`ac_train.labels.of` reads)
    kind: str
    tags: list[str]
    frames: int
    #: frames training may use: the take without its cuts (0 when it is left out)
    used: int
    held: bool
    #: "" when the take is kept, else why it is out
    why: str

    @property
    def id(self) -> str:
        return f"{self.set}/{self.take}"

    @property
    def kept(self) -> bool:
        return not self.why


@dataclass(frozen=True)
class Selection:
    types: frozenset | None = None
    styles: frozenset | None = None
    include: frozenset = frozenset()
    exclude: frozenset = frozenset()
    #: where the take list came from, for the report
    takes_file: str = ""

    def active(self) -> bool:
        return bool(self.types is not None or self.styles is not None or self.include or self.exclude)

    def why_out(self, take_id: str, kind: str, styles) -> str:
        """"" when the take passes, else why it is out: "type: ...", "style: ...", "list: ..."."""
        if take_id in self.exclude:
            return "list: excluded"
        if self.include:
            return "" if take_id in self.include else "list: not included"
        if self.types is not None and kind not in self.types:
            return f"type: {kind or 'untyped'}"
        if self.styles is not None:
            bad = [w for w in styles if w not in self.styles]
            if bad:
                return "style: " + ", ".join(bad)
        return ""

    def describe(self) -> str:
        if not self.active():
            return "every take the review allows"
        parts = []
        if self.include:
            parts.append(f"the {len(self.include)} takes listed in {self.takes_file or 'the take list'}"
                         + (" (the type and style filters don't apply)" if self.types is not None or self.styles is not None else ""))
        else:
            if self.types is not None:
                parts.append("types " + ", ".join(sorted(self.types)))
            if self.styles is not None:
                parts.append("styles " + (", ".join(sorted(self.styles)) or "none (unstyled takes only)"))
        if self.exclude:
            parts.append(f"{len(self.exclude)} excluded by {self.takes_file or 'the take list'}")
        return "; ".join(parts)


def load_takes(path: pathlib.Path) -> tuple[frozenset, frozenset]:
    """The take list file's (include, exclude). A file that isn't one is an error."""
    try:
        d = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise SystemExit(f"{path}: {e}") from e
    if not isinstance(d, dict) or not set(d) <= {"include", "exclude", "note"}:
        raise SystemExit(f'{path}: a take list is {{"include": [set/take, ...], "exclude": [...]}}')
    out = []
    for key in ("include", "exclude"):
        ids = d.get(key, [])
        if not isinstance(ids, list) or not all(isinstance(x, str) and x.count("/") == 1 and all(x.split("/")) for x in ids):
            raise SystemExit(f"{path}: {key} must be a list of set/take ids")
        out.append(frozenset(ids))
    both = out[0] & out[1]
    if both:
        raise SystemExit(f"{path}: {len(both)} takes are both included and excluded, e.g. {sorted(both)[0]}")
    return out[0], out[1]


def add_arguments(ap: argparse.ArgumentParser) -> None:
    g = ap.add_argument_group("which takes")
    g.add_argument("--types", default=None, help="comma list of motion types to read (idle,walk,run,jump,...)")
    g.add_argument("--styles", default=None,
                   help=f"comma list of style words a take may have (an unstyled take always passes; '{NONE}': unstyled only)")
    g.add_argument("--natural", action="store_true",
                   help=f"types {','.join(NATURAL_TYPES)} with styles {','.join(NATURAL_STYLES)} (roadmap 153000)")
    g.add_argument("--takes", default=None, metavar="JSON",
                   help='a take list {"include": [set/take, ...], "exclude": [...]}: a non-empty include is exactly the '
                        "takes to read (the type and style filters then don't apply); exclude takes out")


def from_args(args) -> Selection:
    types = set(NATURAL_TYPES) if args.natural else None
    styles = set(NATURAL_STYLES) if args.natural else None
    if args.types:
        types = (types or set()) | {labels.word(t) for t in args.types.split(",") if labels.word(t)}
    if args.styles is not None:
        words = {labels.word(t) for t in args.styles.split(",") if labels.word(t)} - {NONE}
        styles = (styles or set()) | words
    include, exclude, name = frozenset(), frozenset(), ""
    if args.takes:
        include, exclude = load_takes(pathlib.Path(args.takes))
        name = args.takes
    return Selection(None if types is None else frozenset(types), None if styles is None else frozenset(styles),
                     include, exclude, name)


def records(set_name: str, npz, decisions, select: Selection | None = None, scanned: bool = True) -> tuple[review.Plan, list[Record]]:
    """The set's plan and each take's :class:`Record`, in the set's order."""
    starts, takes = npz["starts"], npz["takes"]
    own = npz["labels"] if "labels" in npz.files else takes
    plan = review.plan_set(set_name, len(npz["roots"]), starts, takes, decisions, scanned, npz=npz)
    out = []
    for take, label, t in zip(takes, own, plan.takes):
        take, label = str(take), str(label)
        kind = labels.type_of(take, t.kind)
        styles = style_words(set_name, label, t.tags)
        rec = Record(set_name, take, label, kind, styles, t.kind, list(t.tags), t.frames,
                     0 if t.drop else t.frames - int(t.bad().sum()), split.held_out(set_name, take), "")
        if t.drop:
            rec.why = f"left out: {t.drop}"
        elif select is not None:
            rec.why = select.why_out(rec.id, kind, styles)
        out.append(rec)
    return plan, out


def check_listed(select: Selection, recs: list[Record], sets) -> None:
    """Every listed take of a read set exists, or it is an error; listed takes of sets not read
    are said. Rejected takes the list includes are said too."""
    sets = set(sets)
    known = {r.id for r in recs}
    listed = select.include | select.exclude
    missing = sorted(i for i in listed if i.split("/", 1)[0] in sets and i not in known)
    if missing:
        raise SystemExit(f"{select.takes_file or 'the take list'}: {len(missing)} takes no read set has, e.g. "
                         f"{', '.join(missing[:3])}: fix the id (set/take, as the review names it)")
    elsewhere = sorted({i.split('/', 1)[0] for i in listed if i.split("/", 1)[0] not in sets})
    if elsewhere:
        print(f"the take list names takes of sets not read: {', '.join(elsewhere)}", flush=True)
    out = [r.id for r in recs if r.id in select.include and r.why.startswith("left out")]
    if out:
        print(f"{len(out)} included takes stay out, rejected by the scan or the review: {', '.join(out[:3])}"
              f"{' ...' if len(out) > 3 else ''}", flush=True)


def plan_sets(names, decisions, select: Selection | None = None, scanned: bool = True, progress=None) -> list[Record]:
    """Every take of ``names`` (sets in ``data/rig``), without loading any rotation."""
    recs: list[Record] = []
    read = []
    for n in names:
        path = RIG / f"{n}.npz"
        if not path.exists():
            if progress:
                progress(f"set {n} not built ({path} is missing): run python -m ac_train.datasets {n}")
            continue
        d = np.load(path)
        plan, r = records(n, d, decisions, select, scanned)
        if progress:
            progress(plan.summary())
            for note in plan.notes:
                progress(f"  {note}")
        recs += r
        read.append(n)
    if select is not None and (select.include or select.exclude):
        check_listed(select, recs, read)
    return recs


def minutes(recs: list[Record]) -> dict:
    """Minutes kept by type and set (train and held out apart), and minutes out by reason."""
    train = defaultdict(lambda: defaultdict(float))
    held = defaultdict(float)
    out = defaultdict(float)
    count = defaultdict(int)
    for r in recs:
        kind = r.type or "untyped"
        if r.kept:
            if r.held:
                held[kind] += r.used
            else:
                train[kind][r.set] += r.used
            count[kind] += 1
        else:
            out[r.why.split(":", 1)[0]] += r.used or r.frames
    m = lambda f: round(f / FPS / 60, 1)  # noqa: E731
    return {"train": {k: {s: m(f) for s, f in v.items()} for k, v in train.items()},
            "held": {k: m(f) for k, f in held.items()}, "out": {k: m(f) for k, f in out.items()},
            "takes": dict(count)}


def table(recs: list[Record], select: Selection | None = None) -> str:
    """The minutes that survive, by type (rows) and set (columns), training's and held out."""
    d = minutes(recs)
    sets = sorted({s for v in d["train"].values() for s in v})
    kinds = sorted(set(d["train"]) | set(d["held"]))
    head = f"{'type':12}{'takes':>7}{'train min':>11}" + "".join(f"{s[:14]:>16}" for s in sets) + f"{'held out':>10}"
    lines = [f"takes read: {select.describe() if select else 'every take the review allows'}", head]
    tot = defaultdict(float)
    for k in kinds:
        row = d["train"].get(k, {})
        total = sum(row.values())
        tot["train"] += total
        tot["held"] += d["held"].get(k, 0.0)
        for s in sets:
            tot[s] += row.get(s, 0.0)
        lines.append(f"{k:12}{d['takes'].get(k, 0):>7}{total:>11.1f}" + "".join(f"{row.get(s, 0.0):>16.1f}" for s in sets)
                     + f"{d['held'].get(k, 0.0):>10.1f}")
    lines.append(f"{'all':12}{sum(d['takes'].values()):>7}{tot['train']:>11.1f}" + "".join(f"{tot[s]:>16.1f}" for s in sets)
                 + f"{tot['held']:>10.1f}")
    if d["out"]:
        lines.append("left out (minutes): " + ", ".join(f"{k} {v:.1f}" for k, v in sorted(d["out"].items(), key=lambda kv: -kv[1])))
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="The minutes that survive a take filter, by type and set (nothing is trained)")
    ap.add_argument("sets", nargs="*", help="sets (default: every data/rig/*.npz)")
    ap.add_argument("--decisions", default=str(review.DECISIONS))
    ap.add_argument("--write-takes", default=None, metavar="JSON",
                    help="write the kept takes as a take list {include: [...], exclude: []} for the owner to edit")
    add_arguments(ap)
    args = ap.parse_args(argv)
    select = from_args(args)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    names = args.sets or sorted(p.stem for p in RIG.glob("*.npz"))
    recs = plan_sets(names, decisions, select, progress=lambda s: print(s, flush=True))
    print(table(recs, select))
    if args.write_takes:
        path = pathlib.Path(args.write_takes)
        kept = [r.id for r in recs if r.kept]
        path.write_text(json.dumps({
            "note": f"roadmap 153000: the takes kept by {select.describe()}; edit include and exclude from the candidate videos "
                    "(target/shots/2026-10-30/natural-candidates/), then train with --takes <this file>",
            "include": kept, "exclude": []}, indent=1) + "\n", encoding="utf-8")
        print(f"written {path}: {len(kept)} takes included, none excluded")


if __name__ == "__main__":
    main()
