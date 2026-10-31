"""The walks cut down to what a person can review (roadmap 153000): one clip a group, not 1,687 takes.

    python -m ac_train.walkgroups plan
    python -m ac_train.walkgroups render --out ../target/shots/2026-10-31/walk-review --ac <ac copy> --assets ../assets
    python -m ac_train.walkgroups apply --reject G003,G017 --reject-take cmu/walk_cmu_35_01

Reads ``data/walk-check.json`` (:mod:`ac_train.gaitcheck`) and sorts the natural walks:

- **too short**: under ``train.WINDOW`` usable frames, so training never reads them;
- **flagged**: the gait check's words or motion stand out; out without a review (walk has minutes to
  spare, and every other type is far smaller);
- **groups**: the rest, by performer or session, description and :func:`kind`. A take with a start or
  a stop stays in: those are the fills between standing and walking. The clip shown is the group's
  most unusual take (:func:`pick`), so a group that passes on it passes on the rest.

``plan`` writes ``review/walk-groups.json``; ``render`` writes one MP4 with a chapter a group;
``apply`` writes the take list with the too short, the flagged and the rejected excluded. Other
types stay as ``review/natural-takes.json`` has them.

``--type idle``, ``run`` or ``jump`` does the same for that type, without the gait check, whose
measures read a walk. These types have few minutes, so only a description in :data:`STYLED` is
flagged; everything else is reviewed. A group shows its longest take.
"""

from __future__ import annotations

import argparse
import csv
import json
import pathlib
import re
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor

import numpy as np

from . import OUT, candidates, gaitcheck, review, selection
from .clip import Clip, Key
from .posetargets import hips_path
from .quality import FPS, RIG
from .rig import Skeleton
from .train import WINDOW

STAND = 0.35  # m/s, over half a second: slower is standing (a stride's sway stays under it)
#: a stretch shorter than this is no start or stop
SETTLED = 0.7
STEADY_SECONDS, CHANGE_SECONDS = 3.0, 4.0
FORWARDS, CHANGES, OTHER_WAY = "steady walk", "starts or stops", "backwards or sideways"
STEADY_RUN, IDLE, JUMP = "steady run", "idle", "jump"
ORDER = (FORWARDS, STEADY_RUN, IDLE, JUMP, CHANGES, OTHER_WAY)
#: kinds shown for longer than a steady stretch
LONGER = (CHANGES, IDLE)
#: Descriptions of idle, run and jump takes that name a character or a stunt, not plain motion
STYLED = re.compile(r"\b(cat|dinosaur|drunk|gangly|graceful|sexy|strong|old|lady|flashlight|forehead|defensive|guard"
                    r"|dives?|roll|scramble|flip|handspring|trunksway\d*|chopping|onions|singing)\b", re.I)


def groups_file(type_name: str) -> pathlib.Path:
    return review.DECISIONS.parent / f"{type_name}-groups.json"


def speeds(skel: Skeleton, roots: np.ndarray) -> np.ndarray:
    """Hip speed along the ground, over half a second."""
    v = np.linalg.norm(np.diff(hips_path(skel, roots.astype(np.float64))[:, :2], axis=0), axis=-1) * FPS
    width = max(1, int(FPS / 2))
    return np.convolve(v, np.ones(width) / width, mode="same")


def first_change(speed: np.ndarray) -> int | None:
    """The frame where standing first turns to walking or back, each side lasting ``SETTLED`` s."""
    still = speed < STAND
    need = int(SETTLED * FPS)
    edges = np.flatnonzero(np.diff(still.astype(np.int8))) + 1
    bounds = [0, *edges.tolist(), len(still)]
    long = [(a, b) for a, b in zip(bounds, bounds[1:]) if b - a >= need]
    for (_, end), (start, _) in zip(long, long[1:]):
        if still[end - 1] != still[start]:
            return (end + start) // 2
    return None


def kind(group: str, change: int | None) -> str:
    """How a take reads on the review: the gait check's backwards or sideways, else by a start or stop."""
    if group == OTHER_WAY:
        return OTHER_WAY
    return CHANGES if change is not None else FORWARDS


def group_key(take_id: str, label: str, how: str) -> tuple:
    """Takes that would look the same on a review: the same performer or session, description
    without its numbers, and kind. An ACCAD performer stays apart from the others on the same
    script; an AddBiomechanics study is one group, its labels being file names."""
    set_name, take = take_id.split("/", 1)
    words = re.sub(r"[\d_]+", " ", label.lower())
    words = " ".join(re.sub(r"\b(t|take|trial|segment|og)\b", " ", words).split())
    if set_name == "addbiomechanics" and how not in (FORWARDS, CHANGES, OTHER_WAY):
        found = re.match(r"[a-z]+_addbiomechanics_([a-z]+\d*)", take)
        return (set_name, found.group(1) if found else take, words.split(":")[-1].strip(), how)
    if set_name == "cmu":
        found = re.search(r"_cmu_(\d+)", take)
        return (set_name, found.group(1) if found else take, words, how)
    if set_name == "accad":
        found = re.search(r"_accad_((?:fe)?male\d+)", take)
        return (set_name, found.group(1) if found else "", re.sub(r"^(fe)?male [a-z]? ?", "", words), how)
    if set_name == "addbiomechanics":
        found = re.match(r"[a-z]+_addbiomechanics_([a-z]+\d*)", take)
        return (set_name, found.group(1) if found else take, how)
    return (set_name, take)


def oddness(measures: dict, centre: dict) -> float:
    """How far a take's largest measure is from the walks' median, in robust deviations."""
    if not measures:
        return -1.0
    return max(abs(measures[m] - centre[m][0]) / max(centre[m][1], 1e-9) for m in gaitcheck.MEASURES)


def pick(takes: list[dict], centre: dict) -> dict:
    """The group's most unusual take; ties (nothing measured) the longest."""
    return max(takes, key=lambda t: (oddness(t["measures"], centre), t["minutes"]))


def plan(check: pathlib.Path = OUT / "walk-check.json") -> dict:
    d = json.loads(check.read_text(encoding="utf-8"))
    centre = {k: tuple(v) for k, v in d["centre"].items()}
    by_set = defaultdict(dict)
    for t in d["takes"]:
        s, name = t["take"].split("/", 1)
        by_set[s][name] = t
    skel = Skeleton.load()
    decisions = review.load_decisions()
    short, flagged, kept = [], [], []
    for s, wanted in sorted(by_set.items()):
        z = np.load(RIG / f"{s}.npz")
        starts, takes, roots = z["starts"], z["takes"], z["roots"]
        for at, t in zip(starts, review.plan_set(s, len(roots), starts, takes, decisions, True, npz=z).takes):
            row = wanted.get(t.name)
            if row is None:
                continue
            if not len(review.starts(t, WINDOW)):
                short.append(row["take"])
            elif row["group"] in ("words", "motion"):
                flagged.append({"take": row["take"], "why": row["word"] or "; ".join(row["why"])})
            else:
                change = first_change(speeds(skel, roots[int(at) : int(at) + t.frames]))
                kept.append({**row, "kind": kind(row["group"], change), "change": change})
    groups = defaultdict(list)
    for t in kept:
        groups[group_key(t["take"], t["label"], t["kind"])].append(t)
    rows = []
    ordered = sorted(groups.items(), key=lambda kv: (ORDER.index(kv[1][0]["kind"]), kv[1][0]["take"]))
    for i, (_, members) in enumerate(ordered, 1):
        shown = pick(members, centre)
        rows.append({"group": f"G{i:03d}", "kind": shown["kind"], "label": shown["label"], "shown": shown["take"],
                     "change": shown["change"], "minutes": round(sum(t["minutes"] for t in members), 2),
                     "takes": sorted(t["take"] for t in members)})
    return {"note": "roadmap 153000, from ac_train.walkgroups: reject a group by its G number on the review video",
            "too_short": sorted(short), "flagged": sorted(flagged, key=lambda x: x["take"]), "groups": rows}


def plan_type(type_name: str) -> dict:
    """:func:`plan` for idle, run or jump."""
    select = selection.Selection(frozenset(selection.NATURAL_TYPES), frozenset(selection.NATURAL_STYLES))
    skel = Skeleton.load()
    decisions = review.load_decisions()
    short, flagged, kept = [], [], []
    for path in sorted(RIG.glob("*.npz")):
        z = np.load(path)
        starts, roots = z["starts"], z["roots"]
        built, recs = selection.records(path.stem, z, decisions, select)
        for at, t, r in zip(starts, built.takes, recs):
            if not r.kept or r.type != type_name:
                continue
            found = STYLED.search(re.sub(r"(?<=[a-z])(?=[A-Z0-9])", " ", r.label))
            if not len(review.starts(t, WINDOW)):
                short.append(r.id)
            elif found:
                flagged.append({"take": r.id, "why": found.group(0)})
            else:
                part = roots[int(at) : int(at) + t.frames]
                centre = first_change(speeds(skel, part)) if type_name == "run" else None
                how = IDLE if type_name == "idle" else CHANGES if centre is not None else STEADY_RUN
                if type_name == "jump":
                    how, centre = JUMP, int(np.argmax(hips_path(skel, part.astype(np.float64))[:, 2]))
                kept.append({"take": r.id, "label": r.label, "minutes": round(r.used / FPS / 60, 2), "kind": how, "change": centre})
    groups = defaultdict(list)
    for t in kept:
        groups[group_key(t["take"], t["label"], t["kind"])].append(t)
    rows = []
    ordered = sorted(groups.values(), key=lambda m: (ORDER.index(m[0]["kind"]), m[0]["take"]))
    for i, members in enumerate(ordered, 1):
        shown = max(members, key=lambda t: t["minutes"])
        rows.append({"group": f"{type_name[0].upper()}{i:03d}", "kind": shown["kind"], "label": shown["label"], "shown": shown["take"],
                     "change": shown["change"], "minutes": round(sum(t["minutes"] for t in members), 2),
                     "takes": sorted(t["take"] for t in members)})
    return {"note": f"roadmap 153000, from ac_train.walkgroups --type {type_name}: reject a group by its number on the review video",
            "too_short": sorted(short), "flagged": sorted(flagged, key=lambda x: x["take"]), "groups": rows}


def summary(p: dict) -> str:
    minutes = sum(g["minutes"] for g in p["groups"])
    lines = [f"too short to train: {len(p['too_short'])} takes", f"flagged: {len(p['flagged'])} takes",
             f"left: {sum(len(g['takes']) for g in p['groups'])} takes, {minutes:.1f} min, in {len(p['groups'])} groups"]
    for k in ORDER:
        of = [g for g in p["groups"] if g["kind"] == k]
        lines.append(f"  {k:24s}{len(of):4d} groups {sum(len(g['takes']) for g in of):4d} takes {sum(g['minutes'] for g in of):6.1f} min")
    return "\n".join(lines)


def span(frames: int, change: int | None, how: str, allowed: np.ndarray) -> tuple[int, int] | None:
    """(start, length) of the frames to show: around the first start or stop, else the middle, on
    frames the review keeps. ``allowed``: the window starts of the shortest length training reads."""
    want = min(frames, int((CHANGE_SECONDS if how in LONGER else STEADY_SECONDS) * FPS))
    if not len(allowed):
        return None
    centre = change if change is not None else frames // 2
    start = int(allowed[np.argmin(np.abs(allowed - (centre - want // 2)))])
    return start, min(want, frames - start)


def render_group(args, work: pathlib.Path, bones, i: int, n: int, g: dict, rot, roots) -> pathlib.Path:
    stem = work / g["group"]
    seg = stem.with_name(f"{stem.name}.seg.mp4")
    if seg.exists():
        return seg
    clip_path, script, raw = stem.with_suffix(".clip.json"), stem.with_suffix(".json"), stem.with_name(f"{stem.name}.raw.mp4")
    clip = Clip(bones=bones, fps=float(FPS), name=g["shown"], group="candidates", origin="mocap")
    clip.physics = False
    for f in range(len(rot)):
        clip.set_key(Key(f, rot[f].astype(np.float64), roots[f].astype(np.float64), "linear"))
    clip.save(clip_path)
    script.write_text(json.dumps([{"command": "clip.import", "params": {"path": str(clip_path.resolve()).replace("\\", "/")}},
                                  {"command": "physics.set", "params": {"on": False}}]), encoding="utf-8")
    candidates.run([args.ac, "--assets", args.assets, "video", "--script", str(script), "--view", "three-quarter", "--size", args.size,
                    "--fixed", "--fps", str(FPS), "--out", str(raw)], f"ac video of {g['shown']}")
    width = int(args.size.split("x")[0])
    fit = lambda text, size: min(size, int((width - 24) / (0.56 * max(len(text), 1))))  # noqa: E731  the whole line on the frame
    head = f"{g['group']}  ({i}/{n})  {g['kind']}"
    many = len(g["takes"])
    line = f"{g['shown']}  " + (f"{'oddest' if args.type == 'walk' else 'longest'} of {many} takes" if many > 1 else "1 take") + f"  {g['label'][:60]}"
    candidates.run([args.ffmpeg, "-y", "-loglevel", "error", "-i", str(raw), "-vf",
                    f"{candidates.drawtext(head, '10', '10', fit(head, 30))},{candidates.drawtext(line, '10', '56', fit(line, 18))},format=yuv420p",
                    "-r", str(FPS), "-c:v", "libx264", "-preset", "veryfast", "-crf", "21", str(seg)], f"ffmpeg overlay of {g['shown']}")
    for p in (clip_path, script, raw):
        p.unlink()
    return seg


def render(args) -> None:
    p = json.loads(groups_file(args.type).read_text(encoding="utf-8"))
    args.ffmpeg = candidates.ffmpeg()
    ffprobe = str(pathlib.Path(args.ffmpeg).with_name("ffprobe" + pathlib.Path(args.ffmpeg).suffix))
    out = pathlib.Path(args.out)
    work = out / "work"
    work.mkdir(parents=True, exist_ok=True)
    bones = list(Skeleton.load().names)
    groups = p["groups"][: args.limit] if args.limit else p["groups"]
    decisions = review.load_decisions()
    data = {}
    for s in sorted({g["shown"].split("/")[0] for g in groups}):
        z = np.load(RIG / f"{s}.npz")
        starts, takes, roots, rot = z["starts"], z["takes"], z["roots"], z["rotations"]
        at = dict(zip((str(t) for t in takes), starts))
        by = {t.name: t for t in review.plan_set(s, len(roots), starts, takes, decisions, True, npz=z).takes}
        for g in groups:
            if g["shown"].split("/")[0] != s:
                continue
            t = by[g["shown"].split("/", 1)[1]]
            first, length = span(t.frames, g["change"], g["kind"], review.starts(t, WINDOW))
            a = int(at[t.name]) + first
            data[g["group"]] = (rot[a : a + length].copy(), roots[a : a + length].copy())
        del rot
    with ThreadPoolExecutor(args.jobs) as pool:
        futures = [(g, pool.submit(render_group, args, work, bones, i, len(groups), g, *data[g["group"]])) for i, g in enumerate(groups, 1)]
        segments = [(g, f.result()) for g, f in futures]
    chapters = [";FFMETADATA1", f"title={args.type} review (roadmap 153000)"]
    rows, seconds = [], 0.0
    for g, seg in segments:
        length = candidates.frames_of(seg, ffprobe) / FPS
        chapters += ["[CHAPTER]", "TIMEBASE=1/1000", f"START={int(seconds * 1000)}", f"END={int((seconds + length) * 1000) - 1}",
                     f"title={g['group']} {g['kind']}"]
        rows.append([g["group"], round(seconds, 2), g["kind"], g["shown"], len(g["takes"]), g["minutes"], g["label"]])
        seconds += length
    chap = out / f"{args.type}-review.chapters.txt"
    chap.write_text("\n".join(chapters) + "\n", encoding="utf-8")
    listing = work / "list.txt"
    listing.write_text("".join(f"file '{str(s.resolve()).replace(chr(92), '/')}'\n" for _, s in segments), encoding="utf-8")
    candidates.run([args.ffmpeg, "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing), "-i", str(chap),
                    "-map", "0:v", "-map_metadata", "1", "-map_chapters", "1", "-c", "copy", "-movflags", "+faststart",
                    str(out / f"{args.type}-review.mp4")], "ffmpeg concat")
    with open(out / f"{args.type}-review.index.csv", "w", encoding="utf-8", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["group", "start_s", "kind", "shown", "takes", "minutes", "label"])
        w.writerows(rows)
    print(f"{out / (args.type + '-review.mp4')}: {len(rows)} groups, {seconds / 60:.1f} min")


def apply(type_name: str, reject_groups: set[str], reject_takes: set[str], base: pathlib.Path, to: pathlib.Path) -> dict:
    """The take list: ``base`` without the too short, the flagged and the rejected; what it
    already excludes stays excluded."""
    p = json.loads(groups_file(type_name).read_text(encoding="utf-8"))
    known = {g["group"] for g in p["groups"]}
    if reject_groups - known:
        raise SystemExit(f"no such group: {', '.join(sorted(reject_groups - known))}")
    d = json.loads(base.read_text(encoding="utf-8"))
    if reject_takes - set(d["include"]):
        raise SystemExit(f"not in {base.name}: {', '.join(sorted(reject_takes - set(d['include'])))}")
    out = set(d.get("exclude", [])) | set(p["too_short"]) | {t["take"] for t in p["flagged"]} | reject_takes
    out |= {t for g in p["groups"] if g["group"] in reject_groups for t in g["takes"]}
    result = {"note": f"roadmap 153000, from ac_train.walkgroups: {len(reject_groups)} groups and {len(reject_takes)} takes rejected by the owner",
              "include": [t for t in d["include"] if t not in out], "exclude": sorted(out)}
    to.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    return result


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--type", default="walk", choices=("walk", "idle", "run", "jump"))
    sub = ap.add_subparsers(dest="what", required=True)
    sub.add_parser("plan")
    r = sub.add_parser("render")
    r.add_argument("--out", required=True)
    r.add_argument("--ac", required=True)
    r.add_argument("--assets", required=True)
    r.add_argument("--size", default="800x600")
    r.add_argument("--jobs", type=int, default=4)
    r.add_argument("--limit", type=int, default=None)
    a = sub.add_parser("apply")
    a.add_argument("--reject", default="", help="comma list of group numbers")
    a.add_argument("--reject-take", default="", help="comma list of set/take ids")
    a.add_argument("--base", type=pathlib.Path, default=review.DECISIONS.parent / "natural-takes.json")
    a.add_argument("--to", type=pathlib.Path, default=review.DECISIONS.parent / "natural-takes.json")
    args = ap.parse_args(argv)
    if args.what == "plan":
        p = plan() if args.type == "walk" else plan_type(args.type)
        groups_file(args.type).write_text(json.dumps(p, indent=1) + "\n", encoding="utf-8")
        print(summary(p))
    elif args.what == "render":
        render(args)
    else:
        words = lambda s: {x.strip() for x in s.split(",") if x.strip()}  # noqa: E731
        d = apply(args.type, words(args.reject), words(args.reject_take), args.base, args.to)
        print(f"wrote {args.to}: {len(d['include'])} takes kept, {len(d['exclude'])} excluded")


if __name__ == "__main__":
    try:
        main()
    except RuntimeError as e:
        print(e, file=sys.stderr)
        sys.exit(1)
