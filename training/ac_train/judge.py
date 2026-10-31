"""Two models filling the same gaps, beside the real motion, for the owner to judge (roadmap 153000).

    python -m ac_train.judge --a parts-lowweights --a-name SHIPPED --b parts-natural --b-name NATURAL \\
        --out ../target/shots/2026-10-31/natural-judge --ac <ac copy> --assets ../assets

Every scene is a take validation holds out, so neither model trained on it. A gap is cut around a
moment found in the motion (:func:`moment`), and each model fills it from its ``best`` checkpoint
with no style. One MP4 a scene shows the real motion and the two fills side by side, physics off,
playing once at full speed and once at half, with "FILL" on the frames a model made and "GAP" on the same frames of the real motion. ``ALL.mp4``
joins them with a chapter a scene.
"""

from __future__ import annotations

import argparse
import pathlib
from typing import NamedTuple

import numpy as np

from . import candidates, infer, review, selection
from .clip import Clip, Key
from .gaitcheck import facing, hips_path
from .quality import FPS, RIG
from .rig import Skeleton
from .walkgroups import first_change, speeds

#: real motion shown before and after the gap
AROUND = 30
REAL = "REAL MOTION"


class Scene(NamedTuple):
    name: str
    take: str
    moment: str
    gap: int


SCENES = (
    Scene("01-walk-1s", "cmu/walk_cmu_113_25", "middle", 30),
    Scene("02-walk-long-gap", "cmu/walk_cmu_113_06", "middle", 45),
    Scene("03-walk-turn-around", "cmu/walk_cmu_36_02", "turn", 30),
    Scene("04-walk-stop-or-start", "cmu/walk_cmu_36_09", "change", 30),
    Scene("05-walk-backwards", "cmu/walk_cmu_113_01", "middle", 30),
    Scene("06-walk-sideways", "cmu/sidestep_cmu_113_18", "middle", 30),
    Scene("07-idle-sway", "accad/standing_accad_male2_a2_sway", "middle", 45),
    Scene("08-idle-look-around", "accad/standing_accad_male2_a4_lookaround", "middle", 45),
    Scene("09-run-forward", "cmu/run_cmu_113_07", "middle", 30),
    Scene("10-run-sideways", "100style/run_100style_SR_052", "middle", 30),
    Scene("11-jump-sideways", "kimodo/jump_kimodo_jump_side_s1", "peak", 30),
    Scene("12-jump-drop", "addbiomechanics/jump_addbiomechanics_uhlrich2023_subject9_dj2_segment_0", "peak", 30),
    Scene("13-cost-sit-down", "cmu/sit_cmu_113_15", "dip", 30),
    Scene("14-cost-crouch-to-lie", "accad/ground_accad_male2_a8_crouchtolie", "dip", 30),
)


def moment(kind: str, skel: Skeleton, rots: np.ndarray, roots: np.ndarray) -> int:
    """The frame a gap centres on: ``middle``; ``change``, the first start or stop; ``peak``, the
    hips' highest; ``dip``, where the hips drop fastest; ``turn``, where the facing turns most in a second."""
    n = len(roots)
    if kind == "change":
        found = first_change(speeds(skel, roots))
        return n // 2 if found is None else found
    if kind == "peak":
        return int(np.argmax(hips_path(skel, roots.astype(np.float64))[:, 2]))
    if kind == "dip":
        height = hips_path(skel, roots.astype(np.float64))[:, 2]
        return int(np.argmin(height[int(FPS) :] - height[: -int(FPS)])) + int(FPS) // 2
    if kind == "turn":
        ahead = facing(rots.astype(np.float64))
        angle = np.unwrap(np.arctan2(ahead[:, 1], ahead[:, 0]))
        return int(np.argmax(np.abs(angle[int(FPS) :] - angle[: -int(FPS)]))) + int(FPS) // 2
    return n // 2


def span(centre: int, gap: int, frames: int) -> tuple[int, int]:
    """(key A, key B) around ``centre``, inside the take with the model's context before A."""
    a = int(np.clip(centre - gap // 2 - 1, 10, max(10, frames - gap - 2)))
    return a, a + gap + 1


def clip_of(bones, rot, roots, frames, name: str) -> Clip:
    clip = Clip(bones=bones, fps=float(FPS), name=name, group="judge", origin="mocap")
    clip.physics = False
    for f in frames:
        clip.set_key(Key(int(f), rot[f].astype(np.float64), roots[f].astype(np.float64), "linear"))
    return clip


def panel(args, stem: pathlib.Path, clip: Clip, label: str, text_from: float, text_to: float) -> pathlib.Path:
    path, script, raw, out = stem.with_suffix(".clip.json"), stem.with_suffix(".json"), stem.with_name(stem.name + ".raw.mp4"), stem.with_name(stem.name + ".mp4")
    clip.save(path)
    script.write_text(candidates.json.dumps([{"command": "clip.import", "params": {"path": str(path.resolve()).replace("\\", "/")}},
                                             {"command": "physics.set", "params": {"on": False}}]), encoding="utf-8")
    candidates.run([args.ac, "--assets", args.assets, "video", "--script", str(script), "--view", "three-quarter", "--size", args.size,
                    "--fixed", "--fps", str(FPS), "--out", str(raw)], f"ac video of {stem.name}")
    word = "GAP (real frames)" if label == REAL else "FILL"
    fill = candidates.drawtext(word, "10", "h-85", 26) + f":enable='between(t,{text_from:.3f},{text_to:.3f})'"
    candidates.run([args.ffmpeg, "-y", "-loglevel", "error", "-i", str(raw), "-vf", f"{candidates.drawtext(label, '10', '10', 28)},{fill},format=yuv420p",
                    "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", str(out)], f"ffmpeg label of {stem.name}")
    for p in (path, script, raw):
        p.unlink()
    return out


def scene_video(args, work: pathlib.Path, out: pathlib.Path, skel: Skeleton, bones, s: Scene, a_rot, a_root) -> pathlib.Path:
    centre = moment(s.moment, skel, a_rot, a_root)
    a, b = span(centre, s.gap, len(a_root))
    first, last = max(0, a - AROUND), min(len(a_root) - 1, b + AROUND)
    keys = [f for f in range(first, last + 1) if f <= a or f >= b]
    around = clip_of(bones, a_rot, a_root, keys, s.name)
    # the frames from 0 keep the fill's context; only ``first`` on are played
    fills = {}
    for tag in (args.a, args.b):
        r, o = infer.fill(around, a, b, skel, tag, checkpoint="best")
        rot, root = a_rot.astype(np.float64).copy(), a_root.astype(np.float64).copy()
        rot[a + 1 : b], root[a + 1 : b] = r, o
        fills[tag] = (rot, root)
    played = np.arange(first, last + 1)
    shift = lambda rot, root, name: clip_of(bones, rot[played], root[played], np.arange(len(played)), name)  # noqa: E731
    t0, t1 = (a + 1 - first) / FPS, (b - first) / FPS
    panels = [panel(args, work / f"{s.name}-real", shift(a_rot, a_root, "real"), REAL, t0, t1),
              panel(args, work / f"{s.name}-a", shift(*fills[args.a], args.a), args.a_name, t0, t1),
              panel(args, work / f"{s.name}-b", shift(*fills[args.b], args.b), args.b_name, t0, t1)]
    title = f"{s.name}  {s.take}  gap {s.gap} frames"
    video = out / f"{s.name}.mp4"
    candidates.run([args.ffmpeg, "-y", "-loglevel", "error", *sum((["-i", str(p)] for p in panels), []), "-filter_complex",
                    f"[0][1][2]hstack=3,{candidates.drawtext(title, '10', 'h-30', 18)},split[x][y];[y]setpts=2*PTS[z];[x][z]concat=n=2:v=1:a=0,format=yuv420p[v]",
                    "-map", "[v]", "-r", str(FPS), "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", str(video)], f"ffmpeg stack of {s.name}")
    for p in panels:
        p.unlink()
    print(f"{s.name}: {s.take} frames {a}..{b}", flush=True)
    return video


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--a", required=True)
    ap.add_argument("--a-name", required=True)
    ap.add_argument("--b", required=True)
    ap.add_argument("--b-name", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--ac", required=True)
    ap.add_argument("--assets", required=True)
    ap.add_argument("--size", default="640x480")
    args = ap.parse_args(argv)
    args.ffmpeg = candidates.ffmpeg()
    ffprobe = str(pathlib.Path(args.ffmpeg).with_name("ffprobe" + pathlib.Path(args.ffmpeg).suffix))
    out = pathlib.Path(args.out)
    work = out / "work"
    work.mkdir(parents=True, exist_ok=True)
    skel = Skeleton.load()
    bones = list(skel.names)
    decisions = review.load_decisions()
    videos = []
    for s in SCENES:
        set_name, take = s.take.split("/", 1)
        z = np.load(RIG / f"{set_name}.npz")
        _, recs = selection.records(set_name, z, decisions)
        rec = next(r for r in recs if r.take == take)
        if not rec.held or not rec.kept:
            raise SystemExit(f"{s.take}: a scene must be a held-out take the review keeps")
        takes = [str(t) for t in z["takes"]]
        i = takes.index(take)
        begin = int(z["starts"][i])
        end = begin + rec.frames
        videos.append((s, scene_video(args, work, out, skel, bones, s, z["rotations"][begin:end], z["roots"][begin:end])))
    chapters, at = [";FFMETADATA1", "title=model comparison"], 0.0
    for s, v in videos:
        length = candidates.frames_of(v, ffprobe) / FPS
        chapters += ["[CHAPTER]", "TIMEBASE=1/1000", f"START={int(at * 1000)}", f"END={int((at + length) * 1000) - 1}", f"title={s.name}"]
        at += length
    chap = work / "chapters.txt"
    chap.write_text("\n".join(chapters) + "\n", encoding="utf-8")
    listing = work / "list.txt"
    listing.write_text("".join(f"file '{str(v.resolve()).replace(chr(92), '/')}'\n" for _, v in videos), encoding="utf-8")
    candidates.run([args.ffmpeg, "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing), "-i", str(chap), "-map", "0:v",
                    "-map_metadata", "1", "-map_chapters", "1", "-c", "copy", "-movflags", "+faststart", str(out / "ALL.mp4")], "ffmpeg concat")
    print(f"{out / 'ALL.mp4'}: {len(videos)} scenes, {at / 60:.1f} min")


if __name__ == "__main__":
    main()
