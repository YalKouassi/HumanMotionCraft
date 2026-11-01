"""A named training set (roadmap 177000): what ``ac_train.train --set <name>`` reads.

    python -m ac_train.train --set base-natural --dry-run     # its takes and minutes; nothing trained
    python -m ac_train.train --set my-set --library <folder> --minutes 180
    python -m ac_train.trainset <tag>                         # what is in that model

``<folder>/<name>.set.json`` is a list of clips the review app keeps (roadmap 174000):

    {"format": "<product id>.training-set", "version": 1, "name": "base-natural", "set_version": 2, "public": false,
     "clips": [{"id": "cmu/walk_cmu_02_01", "status": "keep", "labels": ["walk"], "cuts": [[10, 20]]}]}

- An id is ``<set>/<take>`` (a set in ``data/rig``) or ``library:<group>/<name>`` (the clip
  ``<library>/animations/<group>/<name>.clip.json``). A library clip is private: a set file holding
  one is refused inside a git checkout.
- ``keep`` clips are exactly the takes read (:class:`ac_train.selection.Selection`'s ``include``);
  a library clip the set doesn't keep is never opened. The scan and the review still apply, as for
  ``--takes``: a rejected take stays out, a take's cuts stay cut.
- ``labels``, when given, replace the review's for this run: the first that is a motion type is the
  type, and all of them are the take's labels. ``cuts`` (first and last frame included, as in the
  review's decisions) are added to the take's cuts.

``usage.json`` beside the set file says what may leave the machine, for a pack or one clip:
``{"packs": {"<pack>": label}, "clips": {"<id>": label}}``, a label being ``share-freely`` (the clip
and models of it), ``train-only`` (models of it, never the clip) or ``just-me`` (neither). A clip's
own label wins. A library clip's pack starts its group (``<pack> - <action>``, the port's folders);
a rig take's is its set. With no label a clip is ``just-me``, except a set of the registry, whose
licence is known: a free-licence one is ``share-freely``, a non-commercial one always
``noncommercial``. A public set may not keep ``just-me`` or ``noncommercial`` clips, and a model of
a private one can't be exported into the repo (:func:`repo_refusal`).

The model card gets ``training_set`` (the name, version, hash and totals, no clip or pack name, as a
card may be exported); ``clips.json`` beside it lists every clip and is never exported.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import pathlib
from collections import Counter
from dataclasses import dataclass, field

import numpy as np

from . import MODELS, REPO, labels, selection, stamp
from .clip import Clip
from .datasets import REGISTRY, in_place, resample
from .quality import FPS, level, scan_take
from .selection import LIBRARY, Overlay

SETS = REPO / "training" / "review" / "sets"
SUFFIX = ".set.json"
USAGE = "usage.json"
VERSION = 1
STATUSES = ("keep", "reject")
MARKS = ("share-freely", "train-only", "just-me")
#: what no public model may hold
PRIVATE = ("just-me", "noncommercial")
PREFIX = LIBRARY + ":"
#: the file beside a model's card that names its clips
CLIPS_FILE = "clips.json"


def inside_checkout(path: pathlib.Path) -> bool:
    """Whether ``path`` lies in a git checkout (``.git`` is a folder, or a file in a worktree)."""
    p = pathlib.Path(path).resolve()
    return any((d / ".git").exists() for d in p.parents)


def is_library(clip_id: str) -> bool:
    return clip_id.startswith(PREFIX)


def pack_of(clip_id: str, packs: dict[str, str] | None = None) -> str:
    """A clip's pack: a rig take's set; a library clip's group, or its start before `` - `` (a mark
    in ``packs`` for the group or the longest start of it wins, as a pack's name may hold `` - ``)."""
    if not is_library(clip_id):
        return clip_id.split("/", 1)[0]
    group = clip_id[len(PREFIX):].split("/", 1)[0]
    marked = [k for k in (packs or {}) if group == k or group.startswith(k + " - ")]
    return max(marked, key=len) if marked else group.split(" - ", 1)[0]


def source_of(clip_id: str, usage: dict) -> str:
    """A clip's label, or ``noncommercial`` (see the module doc)."""
    packs, clips = usage.get("packs", {}), usage.get("clips", {})
    pack = pack_of(clip_id, packs)
    known = REGISTRY.get(pack) if not is_library(clip_id) else None
    if known is not None and not known.commercial:
        return "noncommercial"
    if clip_id in clips:
        return clips[clip_id]
    if pack in packs:
        return packs[pack]
    return "share-freely" if known is not None else "just-me"


@dataclass(frozen=True)
class SetClip:
    id: str
    status: str
    labels: tuple[str, ...] = ()
    cuts: tuple[tuple[int, int], ...] = ()


@dataclass
class TrainingSet:
    name: str
    version: int
    public: bool
    clips: list[SetClip]
    #: sha-256 of the file's JSON with sorted keys: line endings and key order don't change it
    hash: str
    usage: dict = field(default_factory=dict)

    def kept(self) -> list[SetClip]:
        return [c for c in self.clips if c.status == "keep"]


def _clip(path, raw) -> SetClip:
    if not isinstance(raw, dict) or not isinstance(raw.get("id"), str):
        raise SystemExit(f"{path}: every clip needs an id, got {raw!r}")
    cid = raw["id"]
    rest = cid[len(PREFIX):] if is_library(cid) else cid
    parts = rest.split("/")
    if len(parts) != 2 or not all(parts) or any(p in (".", "..") or "\\" in p for p in parts):
        raise SystemExit(f"{path}: {cid!r}: an id is <set>/<take> or {PREFIX}<group>/<name>")
    if raw.get("status") not in STATUSES:
        raise SystemExit(f"{path}: {cid}: status must be one of {', '.join(STATUSES)}")
    words = raw.get("labels", [])
    if not isinstance(words, list) or not all(isinstance(w, str) for w in words):
        raise SystemExit(f"{path}: {cid}: labels must be a list of words")
    cuts = raw.get("cuts", [])
    good = isinstance(cuts, list) and all(
        isinstance(c, list) and len(c) == 2 and all(type(x) is int and x >= 0 for x in c) and c[0] <= c[1] for c in cuts)
    if not good:
        raise SystemExit(f"{path}: {cid}: cuts must be [[first, last], ...], whole frames, first <= last")
    return SetClip(cid, raw["status"], tuple(words), tuple((a, b) for a, b in cuts))


def load_usage(path: pathlib.Path) -> dict:
    """The labels beside a set file: none when there is no file. A pack's name is private, so a file
    naming a pack that isn't a set of the registry, or a library clip, is refused inside a checkout."""
    if not path.exists():
        return {}
    try:
        d = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise SystemExit(f"{path}: {e}") from e
    good = isinstance(d, dict) and set(d) <= {"packs", "clips"} and all(
        isinstance(m, dict) and all(isinstance(k, str) and v in MARKS for k, v in m.items()) for m in d.values())
    if not good:
        raise SystemExit(f"{path}: usage is {{packs: {{pack: label}}, clips: {{id: label}}}}, a label one of {' | '.join(MARKS)}")
    if inside_checkout(path) and (any(k not in REGISTRY for k in d.get("packs", {})) or any(is_library(k) for k in d.get("clips", {}))):
        raise SystemExit(f"{path}: names packs and lies inside a git checkout: move it out, with its set")
    return d


def load(path: pathlib.Path) -> TrainingSet:
    """A set file and the labels beside it; anything wrong with it stops here."""
    try:
        d = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise SystemExit(f"{path}: {e}") from e
    if not isinstance(d, dict) or not str(d.get("format", "")).endswith(".training-set"):
        raise SystemExit(f"{path}: not a training set file")
    if d.get("version") != VERSION:
        raise SystemExit(f"{path}: version {d.get('version')} (this pipeline reads {VERSION})")
    name = d.get("name")
    if not isinstance(name, str) or path.name != f"{name}{SUFFIX}":
        raise SystemExit(f"{path}: its name is {name!r}: a set is <name>{SUFFIX}")
    if type(d.get("set_version")) is not int or type(d.get("public")) is not bool or not isinstance(d.get("clips"), list):
        raise SystemExit(f"{path}: set_version (a whole number), public (true or false) and clips (a list) are needed")
    clips = [_clip(path, c) for c in d["clips"]]
    ids = Counter(c.id for c in clips)
    twice = sorted(i for i, n in ids.items() if n > 1)
    if twice:
        raise SystemExit(f"{path}: {len(twice)} clips are listed twice, e.g. {twice[0]}")
    if any(is_library(c.id) for c in clips) and inside_checkout(path):
        raise SystemExit(f"{path}: holds private library clips and lies inside a git checkout: move it out")
    canon = json.dumps(d, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return TrainingSet(name, d["set_version"], d["public"], clips, hashlib.sha256(canon.encode()).hexdigest(),
                       load_usage(path.parent / USAGE))


def find(name: str, folder: pathlib.Path = SETS) -> pathlib.Path:
    """``name`` in ``folder``, or ``name`` itself when it is a path to a set file."""
    if name.endswith(SUFFIX) or "/" in name or "\\" in name:
        return pathlib.Path(name)
    path = folder / f"{name}{SUFFIX}"
    if not path.exists():
        have = sorted(p.name[: -len(SUFFIX)] for p in folder.glob(f"*{SUFFIX}"))
        raise SystemExit(f"no set {name} in {folder}" + (f": it has {', '.join(have)}" if have else "; give --set-folder"))
    return path


def check_public(ts: TrainingSet) -> None:
    """A public set keeps only clips anyone may have a model of."""
    if not ts.public:
        return
    bad = [(c.id, source_of(c.id, ts.usage)) for c in ts.kept()]
    bad = [(i, s) for i, s in bad if s in PRIVATE]
    if bad:
        raise SystemExit(f"{ts.name} is public and keeps {len(bad)} clips marked Just me, not labelled or non-commercial, "
                         f"e.g. {bad[0][0]} ({bad[0][1]}): reject them, label them in {USAGE}, or make the set private")


class Arrays(dict):
    """The arrays of a set made in memory, read as an opened npz is; ``scan`` is its scan record."""
    scan: dict | None = None

    @property
    def files(self):
        return list(self)


def read_library(ts: TrainingSet, library: pathlib.Path | None, scanned: bool = True) -> tuple[Arrays | None, dict[str, Overlay]]:
    """The set's kept library clips as a set, at 30 fps and levelled, as ``datasets`` builds one, and
    what each must overlay: its cuts at 30 fps, and why it is out. Only these files are opened."""
    from .rig import Skeleton

    named = [c for c in ts.kept() if is_library(c.id)]
    if not named:
        return None, {}
    if library is None:
        raise SystemExit(f"{ts.name} keeps {len(named)} library clips: give --library <folder>")
    skel = Skeleton.load()
    root = (pathlib.Path(library) / "animations").resolve()
    rots, roots, starts, takes, own, scans, over = [], [], [], [], [], [], {}
    at = 0
    for c in named:
        take = c.id[len(PREFIX):]
        group, name = take.split("/", 1)
        path = root / group / f"{name}.clip.json"
        try:
            clip = Clip.load(path)
        except (OSError, ValueError, KeyError) as e:
            raise SystemExit(f"{c.id}: can't read it: {e}") from e
        if clip.evaluation_only:
            raise SystemExit(f"{c.id}: evaluation-only: its licence doesn't allow training on it")
        if list(clip.bones) != list(skel.names):
            raise SystemExit(f"{c.id}: made for another rig ({len(clip.bones)} bones)")
        if not clip.base.keys:
            raise SystemExit(f"{c.id}: has no keys")
        if any(lay.source == "link" for lay in clip.layers):
            raise SystemExit(f"{c.id}: links another clip, which the set doesn't name")
        r, o = clip.sample_all()
        if not (np.isfinite(r).all() and np.isfinite(o).all()):
            raise SystemExit(f"{c.id}: has values that aren't finite")
        drop = "in place: the feet step but the hips stay" if in_place(skel, r, o, clip.fps) else ""
        r, o = resample(r, clip.fps), resample(o, clip.fps)
        o = level(skel, r, o)
        over[c.id] = Overlay(c.labels, cut_frames(c.cuts, clip.fps), drop)
        rots.append(r)
        roots.append(o)
        starts.append(at)
        takes.append(take)
        own.append(clip.name or name)
        if scanned:
            scans.append({"take": take, "label": own[-1], **scan_take(skel, r, o)})
        at += len(r)
    out = Arrays(rotations=np.concatenate(rots).astype(np.float32), roots=np.concatenate(roots).astype(np.float32),
                 starts=np.array(starts), takes=np.array(takes), labels=np.array(own),
                 **{k: np.array(v) for k, v in stamp.build_stamp().items()})
    out.scan = {"set": LIBRARY, "fps": FPS, "takes": scans} if scanned else None
    return out, over


def cut_frames(cuts, fps: float) -> tuple[tuple[int, int], ...]:
    """A clip's cuts on the 30 fps frames it is read at, rounded outwards so a cut frame stays cut."""
    k = FPS / fps
    return tuple((math.floor(a * k), math.ceil(b * k)) for a, b in cuts)


@dataclass
class Named:
    """A set made ready for a run."""
    set: TrainingSet
    select: selection.Selection
    #: the rig sets its kept clips name, and ``library`` when it keeps library clips
    names: list[str]
    #: the sets made in memory, by name
    extra: dict[str, Arrays]

    def check(self, recs) -> None:
        """Every clip the set keeps was found: a set not built is an error, not fewer takes."""
        seen = {r.id for r in recs}
        lost = [c.id for c in self.set.kept() if c.id not in seen]
        if lost:
            raise SystemExit(f"{self.set.name}: {len(lost)} kept clips aren't in any read set, e.g. {lost[0]}: "
                             "build that set (python -m ac_train.datasets) or fix the id")

    def listing(self, recs) -> str:
        """Each clip of the set: minutes, source, labels, kept or why not."""
        by_id = {r.id: r for r in recs}
        lines = [f"set {self.set.name} v{self.set.version} ({'public' if self.set.public else 'private'}), hash {self.set.hash}"]
        for c in self.set.clips:
            src = source_of(c.id, self.set.usage)
            r = by_id.get(c.id)
            if r is None:
                lines.append(f"  {c.id}  {src}  out: rejected by the set")
            else:
                words = labels.of(r.set, r.take, r.label, r.kind, r.tags)
                lines.append(f"  {c.id}  {r.used / FPS / 60:.2f} min  {src}  [{', '.join(words)}]  {'kept' if r.kept else 'out: ' + r.why}")
        kept = [r for r in recs if r.kept]
        lines.append(f"{len(kept)} of {len(self.set.clips)} clips read, {sum(r.used for r in kept) / FPS / 60:.2f} min")
        return "\n".join(lines)

    def card(self, recs) -> dict:
        """The card's ``training_set``: nothing in it names a clip or a pack."""
        kept = [r for r in recs if r.kept]
        return {"name": self.set.name, "set_version": self.set.version, "hash": self.set.hash, "public": self.set.public,
                "clips": len(self.set.clips), "clips_read": len(kept),
                "minutes": round(sum(r.used for r in kept) / FPS / 60, 2),
                "sources": dict(Counter(source_of(r.id, self.set.usage) for r in kept)),
                "library_clips": sum(r.set == LIBRARY for r in kept)}

    def clips(self, recs) -> dict:
        """What ``clips.json`` holds: every clip of the set, under the set's hash."""
        by_id = {r.id: r for r in recs}
        rows = []
        for c in self.set.clips:
            r = by_id.get(c.id)
            rows.append({"id": c.id, "status": c.status, "source": source_of(c.id, self.set.usage),
                         "labels": labels.of(r.set, r.take, r.label, r.kind, r.tags) if r else list(c.labels),
                         "minutes": round(r.used / FPS / 60, 3) if r and r.kept else 0.0, "held_out": bool(r and r.held),
                         "out": (r.why if r else "rejected by the set")})
        return {"name": self.set.name, "set_version": self.set.version, "hash": self.set.hash, "clips": rows}

    def card_sets(self, used, recs) -> list[dict]:
        """The card's ``sets``: a registry set's own row; anything else is free of terms only when
        the set is public and everything read from it is the owner's."""
        rows = []
        for n in used:
            if n in REGISTRY:
                d = REGISTRY[n]
                rows.append({"name": n, "title": d.title, "licence": d.licence, "commercial": d.commercial})
                continue
            mine = all(source_of(r.id, self.set.usage) not in PRIVATE for r in recs if r.set == n and r.kept)
            rows.append({"name": n if n == LIBRARY else "own clips", "title": "clips the training set names",
                         "licence": "see training_set", "commercial": self.set.public and mine})
        return rows


def prepare(name: str, folder: pathlib.Path = SETS, library: pathlib.Path | None = None, noncommercial: bool = False,
            scanned: bool = True) -> Named:
    """Read and check a set, and read the library clips it keeps. Refusals stop here, before a run
    or a dry run reads anything else."""
    ts = load(find(name, pathlib.Path(folder)))
    check_public(ts)
    kept = ts.kept()
    if not kept:
        raise SystemExit(f"{ts.name} keeps no clips")  # an empty include would mean every take
    nc = sorted({pack_of(c.id) for c in kept if source_of(c.id, ts.usage) == "noncommercial"})
    if nc and not noncommercial:
        raise SystemExit(f"{ts.name} keeps clips of {', '.join(nc)}, which only allow non-commercial use: add --noncommercial")
    arrays, over = read_library(ts, library, scanned)
    for c in kept:
        if not is_library(c.id) and (c.labels or c.cuts):
            over[c.id] = Overlay(c.labels, c.cuts)
    names = list(dict.fromkeys(LIBRARY if is_library(c.id) else c.id.split("/", 1)[0] for c in kept))
    select = selection.Selection(
        include=frozenset(c.id for c in kept),
        # a rejected library clip is left unopened, so it is no id of a read set
        exclude=frozenset(c.id for c in ts.clips if c.status == "reject" and not is_library(c.id)),
        takes_file=f"set {ts.name} v{ts.version}", clips=over)
    return Named(ts, select, names, {LIBRARY: arrays} if arrays is not None else {})


def public_held(held: dict) -> dict:
    """The groups held out, as a card may say them: a library's are pack names, so they go."""
    return {k: v for k, v in held.items() if k != LIBRARY}


def repo_refusal(card: dict) -> str:
    """Why this model can't be exported into the repo ("" when it may be)."""
    ts = card.get("training_set")
    if not ts:
        return ""
    if not ts.get("public"):
        return f"it was trained on the private set {ts.get('name')}"
    bad = sum(ts.get("sources", {}).get(s, 0) for s in PRIVATE)
    return f"its set {ts.get('name')} read {bad} Just me or non-commercial clips" if bad else ""


def main(argv=None):
    ap = argparse.ArgumentParser(description="What is in a model trained on a named set")
    ap.add_argument("tag", help="training/data/models/<tag>")
    args = ap.parse_args(argv)
    folder = MODELS / args.tag
    try:
        card = json.loads((folder / "model.json").read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise SystemExit(f"{folder}: {e}") from e
    ts = card.get("training_set")
    if not ts:
        raise SystemExit(f"{args.tag} was not trained on a named set")
    print(f"{ts['name']} v{ts['set_version']} ({'public' if ts['public'] else 'private'}), hash {ts['hash']}")
    print(f"{ts['clips_read']} of {ts['clips']} clips read, {ts['minutes']} min; sources {ts['sources']}")
    path = folder / CLIPS_FILE
    if not path.exists():
        raise SystemExit(f"{path} is missing: the totals above are all there is")
    for c in json.loads(path.read_text(encoding="utf-8"))["clips"]:
        print(f"  {c['id']}  {c['minutes']} min  {c['source']}  {c['labels']}" + (f"  out: {c['out']}" if c["out"] else ""))


if __name__ == "__main__":
    main()
