"""Target pose sets for live motion control (roadmap 158000): poses chosen by speed from the free
sets on the rig, written in our own format for the pose-to-pose helper to aim at.

    python -m ac_train.posetargets                       # assets/posefill-targets/locomotion.json
    python -m ac_train.posetargets --sets 100style,cmu   # from those sets only, in that order of preference
    python -m ac_train.posetargets --list                 # the candidates of every band, best first

The pose-to-pose model moves towards target poses, and those decide what the walk looks like. The
engine's own demo targets point at evaluation-only clips, so we make our own: for each gait (stand,
walk, jog, run) four consecutive frames at 30 fps of one take of a registered free set (CMU,
100STYLE's Neutral style, our Kimodo-generated clips), picked where the hips move at a steady speed
inside the gait's band, straight ahead and facing the way they go. Every pose records where it came
from (set, take, frames), so ``ATTRIBUTION.md`` can name the sets.

The four poses are turned so the motion goes along the rig's forward (-Y) and moved so the first
pose's hips stand over the origin: the live loop places them ahead of the character and turns them
to its heading by moving the root alone. Deterministic: the same sets give the same file (ties go
by set order, then take name, then frame).

Takes the review rejected and frames it cut (``training/review/decisions.json``, the quality scan)
are never used. No style targets yet: 100STYLE's styles are not natural walks, and the owner
judges the plain gaits first.
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import re
import sys
from dataclasses import dataclass

import numpy as np

from . import ASSETS, OUT, labels, review, selection
from .rig import Skeleton

FORMAT = "humanmotioncraft.posetargets"
VERSION = 1
FPS = 30.0
POSES = 4  # one target token: four poses a frame apart
STEADY = 15  # frames either side of the window over which the speed and heading must hold
DEFAULT_OUT = ASSETS / "posefill-targets" / "locomotion.json"
DEFAULT_SETS = ("100style", "cmu", "kimodo")

#: (name, the speed aimed at in m/s, the band of mean speeds that qualify, the take types it comes from)
GAITS = (
    ("stand", 0.0, (0.0, 0.08), ("standing", "idle")),
    ("walk", 1.3, (1.0, 1.7), ("walk",)),
    ("jog", 2.4, (2.0, 2.9), ("run", "walk")),
    ("run", 3.6, (3.2, 5.0), ("run", "sprint")),
)
FACING_DOT = 0.9  # the hips face the way they move at least this well (no backwards walks)
TURN_DEG = 10.0  # the heading changes at most this much over the steady window
SPEED_SPREAD = 0.2  # the speed's standard deviation over the steady window, as a share of its mean


@dataclass
class Candidate:
    gait: str
    set: str
    take: str
    label: str
    frame: int  # the first of the four poses
    speed: float  # mean hips speed over the steady window, m/s
    score: float  # distance from the gait's speed (lower is better)
    rotations: np.ndarray  # (4, B, 4)
    roots: np.ndarray  # (4, 3)


def hips_path(skel: Skeleton, roots: np.ndarray) -> np.ndarray:
    """The hips' world position (frames, 3) from root offsets (frames, 3)."""
    return skel.heads[skel.index["root"]][None] + np.asarray(roots, dtype=np.float64) * skel.hip_height


def facing(rotations: np.ndarray) -> np.ndarray:
    """The ground direction (frames, 2) the hips face: the root bone turns the rig's forward (-Y)."""
    from .rig import quat_to_mat

    R = quat_to_mat(np.asarray(rotations[:, 0], dtype=np.float64))  # (frames, 3, 3): the root is bone 0
    f = R @ np.array([0.0, -1.0, 0.0])
    f = f[:, :2]
    n = np.linalg.norm(f, axis=-1, keepdims=True)
    return f / np.maximum(n, 1e-9)


def yaw_quat(angle: float) -> np.ndarray:
    return np.array([0.0, 0.0, math.sin(angle / 2), math.cos(angle / 2)])


def turned(skel: Skeleton, rotations: np.ndarray, roots: np.ndarray, angle: float, about: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The poses turned by ``angle`` about the vertical through ``about`` (x, y), the root alone moved:
    rotations (F, B, 4), roots (F, 3) -> the same shapes."""
    from . import quat as Q

    q = yaw_quat(angle)
    rots = np.array(rotations, dtype=np.float64)
    rots[:, 0] = Q.normalize(Q.mul(np.broadcast_to(q, rots[:, 0].shape), rots[:, 0]))
    c, s = math.cos(angle), math.sin(angle)
    P = hips_path(skel, roots)
    rel = P[:, :2] - about[None]
    P2 = P.copy()
    P2[:, 0] = c * rel[:, 0] - s * rel[:, 1] + about[0]
    P2[:, 1] = s * rel[:, 0] + c * rel[:, 1] + about[1]
    out = (P2 - skel.heads[skel.index["root"]][None]) / skel.hip_height
    return rots, out


def normalised(skel: Skeleton, rotations: np.ndarray, roots: np.ndarray, heading: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The four poses turned so ``heading`` (x, y) becomes -Y and moved so the first pose's hips
    stand over the origin."""
    angle = math.atan2(-1.0, 0.0) - math.atan2(float(heading[1]), float(heading[0]))
    P0 = hips_path(skel, roots)[0, :2]
    rots, out = turned(skel, rotations, roots, angle, P0)
    P = hips_path(skel, out)
    P[:, :2] -= P0[None]
    out = (P - skel.heads[skel.index["root"]][None]) / skel.hip_height
    return rots, out


#: CMU's takes carry only a description: these are the plain gaits (a march, a stride or a circle is not)
PLAIN = re.compile(r"^(walk|walking|walk forward|run|run/jog|jog|jogging|running|stand|standing|idle)(,? ?(forward|straight))?$")


def take_type(set_name: str, take: str, kind: str) -> str:
    """A take's motion type: the review's word, else the type word its name starts with
    (``<type>_<set>_<stem>``, as ``ac_train.datasets`` names them)."""
    return labels.word(kind) or take.split(f"_{set_name}_")[0]


def natural(set_name: str, label: str, tags) -> bool:
    """A plain gait: no style word but "natural", and for CMU a description that says only walk, run or stand."""
    words = selection.style_words(set_name, label, tags)
    if not all(w in selection.NATURAL_STYLES for w in words):
        return False
    return set_name != "cmu" or PLAIN.match(label.strip().lower()) is not None


def candidates(set_name: str, skel: Skeleton, decisions: dict) -> list[Candidate]:
    """Every window of a set that qualifies for some gait."""
    path = OUT / "rig" / f"{set_name}.npz"
    if not path.exists():
        print(f"  {set_name}: no {path} (build it first: python -m ac_train.datasets {set_name}); skipped", flush=True)
        return []
    z = np.load(path)
    rots_all, roots_all, starts = z["rotations"], z["roots"], z["starts"]
    takes, take_labels = [str(t) for t in z["takes"]], [str(x) for x in z["labels"]]
    fps = float(z["fps"])
    if abs(fps - FPS) > 1e-6:
        raise SystemExit(f"{path}: {fps} fps; the targets are read at {FPS}")
    plan, _ = selection.records(set_name, z, decisions)
    out: list[Candidate] = []
    for i, (take, label, info) in enumerate(zip(takes, take_labels, plan.takes)):
        a, b = int(starts[i]), int(starts[i + 1]) if i + 1 < len(starts) else len(roots_all)
        if info.drop:
            continue
        kind = take_type(set_name, take, info.kind)
        if not natural(set_name, label, info.tags):
            continue
        rots = rots_all[a:b].astype(np.float64)
        roots = roots_all[a:b].astype(np.float64)
        n = len(roots)
        if n < POSES + 2 * STEADY + 1 or not (np.isfinite(rots).all() and np.isfinite(roots).all()):
            continue
        bad = info.bad()
        P = hips_path(skel, roots)
        v = np.diff(P[:, :2], axis=0) * FPS  # (n-1, 2)
        speed = np.linalg.norm(v, axis=-1)
        face = facing(rots)
        for name, aim, (lo, hi), types in GAITS:
            if kind not in types:
                continue
            for f in range(STEADY, n - POSES - STEADY):
                w0, w1 = f - STEADY, f + POSES + STEADY  # the steady window's frames
                if bad[w0:w1].any():
                    continue
                s = speed[w0 : w1 - 1]
                mean = float(s.mean())
                if not lo <= mean <= hi:
                    continue
                if name == "stand":
                    if float(s.max()) > hi:
                        continue
                    heading = face[f]
                else:
                    if float(s.std()) > SPEED_SPREAD * mean:
                        continue
                    heading = v[f : f + POSES - 1].mean(0)
                    heading = heading / max(float(np.linalg.norm(heading)), 1e-9)
                    first, last = v[w0] / max(float(speed[w0]), 1e-9), v[w1 - 2] / max(float(speed[w1 - 2]), 1e-9)
                    if math.degrees(math.acos(min(1.0, max(-1.0, float(first @ last))))) > TURN_DEG:
                        continue
                    if float(face[f : f + POSES].mean(0) @ heading) < FACING_DOT:
                        continue
                r4, o4 = normalised(skel, rots[f : f + POSES], roots[f : f + POSES], heading)
                out.append(Candidate(name, set_name, take, label, f, mean, abs(mean - aim), r4, o4))
    return out


def best(cands: list[Candidate], sets: tuple[str, ...]) -> dict[str, Candidate]:
    """The best candidate of each gait: the speed nearest the gait's (to 0.1 m/s), ties by set order, take, frame."""
    order = {s: i for i, s in enumerate(sets)}
    chosen: dict[str, Candidate] = {}
    for name, *_ in GAITS:
        of = [c for c in cands if c.gait == name]
        if of:
            chosen[name] = min(of, key=lambda c: (round(c.score, 1), order[c.set], c.take, c.frame))
    return chosen


def document(skel: Skeleton, chosen: dict[str, Candidate], sets: tuple[str, ...]) -> dict:
    targets = []
    for name, *_ in GAITS:
        c = chosen.get(name)
        if c is None:
            continue
        targets.append({
            "name": name,
            "speed": round(c.speed, 4),
            "poses": [{"rotations": [[round(float(x), 6) for x in q] for q in r], "root": [round(float(x), 6) for x in o]}
                      for r, o in zip(c.rotations, c.roots)],
            "from": {"set": c.set, "take": c.take, "frames": [c.frame, c.frame + POSES - 1], "label": c.label},
        })
    return {
        "format": FORMAT,
        "version": VERSION,
        "note": "target poses for live motion control (python -m ac_train.posetargets); four poses a frame apart at 30 fps per gait, "
                "the motion along -Y from the first pose's hips over the origin; root offsets in hip heights",
        "fps": FPS,
        "bones": len(skel.names),
        "sets": list(sets),
        "targets": targets,
    }


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", type=pathlib.Path, default=DEFAULT_OUT)
    ap.add_argument("--sets", default=",".join(DEFAULT_SETS), help="the sets, in order of preference for ties")
    ap.add_argument("--list", action="store_true", help="print the candidates of every gait, best first, and write nothing")
    args = ap.parse_args(argv)
    sets = tuple(s.strip() for s in args.sets.split(",") if s.strip())
    skel = Skeleton.load()
    decisions = review.load_decisions()
    cands: list[Candidate] = []
    for s in sets:
        found = candidates(s, skel, decisions)
        print(f"  {s}: {len(found)} windows qualify", flush=True)
        cands += found
    chosen = best(cands, sets)
    for name, aim, band, _ in GAITS:
        of = sorted((c for c in cands if c.gait == name), key=lambda c: (round(c.score, 1), sets.index(c.set), c.take, c.frame))
        print(f"{name} (aim {aim} m/s, band {band[0]}-{band[1]}): {len(of)} candidates", flush=True)
        shown = of if args.list else of[:5]
        seen: set[str] = set()
        for c in shown:
            key = f"{c.set}/{c.take}"
            if not args.list and key in seen:
                continue
            seen.add(key)
            mark = " <- chosen" if chosen.get(name) is c else ""
            print(f"    {c.speed:5.2f} m/s  {key} frames {c.frame}-{c.frame + POSES - 1}  ({c.label[:50]}){mark}", flush=True)
    missing = [name for name, *_ in GAITS if name not in chosen]
    if missing:
        print(f"no candidate for: {', '.join(missing)}", file=sys.stderr)
    if args.list:
        return
    if not chosen:
        raise SystemExit("nothing to write")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(document(skel, chosen, sets), separators=(",", ":")) + "\n", encoding="utf-8")
    print(f"wrote {args.out} ({args.out.stat().st_size // 1024} KB, {len(chosen)} targets)", flush=True)


if __name__ == "__main__":
    main()
