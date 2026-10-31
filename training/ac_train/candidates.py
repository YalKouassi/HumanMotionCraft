"""One long MP4 per motion type of the takes a filter keeps, for the owner to say which to train on.

    python -m ac_train.candidates --natural --out ../target/shots/2026-10-30/natural-candidates \\
        --ac ../target/shots/2026-10-30/natural-candidates/ac-copy.exe --assets ../assets

For each type (walk first, then idle, run, jump, then the rest as ``others-<type>``) every kept take
plays once, at its own length up to ``--cap`` seconds (the overlay says when a take is cut short),
from one fixed three-quarter camera with physics off, with a one-second title card before it and
a burnt-in line saying the running index, the ``set/take`` id, its length, its style word, whether
validation holds it out, and the frames the review cuts. A type with more than ``--fast-over``
takes plays at twice the speed (said on every frame). Beside ``<type>.mp4``:

- ``<type>.chapters.txt``: ffmpeg metadata chapters, one a take, muxed into the MP4 so a player
  shows the markers;
- ``<type>.index.csv``: index, take id, start second, length, type, style, held out.

Each take is written as a ``.clip.json`` (:mod:`ac_train.clip`, one linear key a frame), rendered
by the app's CLI (``ac video --fixed``) and overlaid and joined by ffmpeg (``AC_FFMPEG``, the PATH
or ``C:/tools/ffmpeg/bin/ffmpeg.exe``). The ids are the ones ``training/review/natural-takes.json``
lists, so what the owner says about a take goes straight into that file.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

import numpy as np

from . import review, selection
from .clip import Clip, Key
from .quality import FPS, RIG

FIRST = ("walk", "idle", "run", "jump")
FONT = "C:/Windows/Fonts/consola.ttf"
TITLE_SECONDS = 1.0


def ffmpeg() -> str:
    for c in (os.environ.get("AC_FFMPEG"), shutil.which("ffmpeg"), "C:/tools/ffmpeg/bin/ffmpeg.exe"):
        if c and pathlib.Path(c).exists():
            return c
    raise SystemExit("ffmpeg not found: set AC_FFMPEG, put it on the PATH, or install it at C:/tools/ffmpeg/bin")


def esc(text: str) -> str:
    """A drawtext text: the characters the filter graph and drawtext read specially, escaped."""
    out = text.replace("\\", "\\\\")
    for c in "':,;[]%":
        out = out.replace(c, "\\" + c)
    return out


def drawtext(text: str, x: str, y: str, size: int, box: bool = True) -> str:
    font = FONT.replace(":", "\\:")
    return (f"drawtext=fontfile='{font}':text='{esc(text)}':x={x}:y={y}:fontsize={size}:fontcolor=white"
            + (":box=1:boxcolor=black@0.6:boxborderw=6" if box else ""))


def run(cmd: list[str], what: str) -> None:
    r = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        raise RuntimeError(f"{what} failed ({r.returncode}):\n{r.stderr.strip()[-2000:]}")


def frames_of(path: pathlib.Path, ffprobe: str) -> int:
    r = subprocess.run([ffprobe, "-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries",
                        "stream=nb_read_frames", "-of", "csv=p=0", str(path)], capture_output=True, text=True)
    if r.returncode != 0 or not r.stdout.strip().isdigit():
        raise RuntimeError(f"ffprobe on {path} failed: {r.stderr.strip()}")
    return int(r.stdout.strip())


def take_slices(recs: list[selection.Record], cap_frames: int) -> dict[str, tuple[np.ndarray, np.ndarray]]:
    """``set/take`` -> (rotations, roots) of the first ``cap_frames`` frames of every kept take, one
    set loaded at a time (a set's rotations are gigabytes)."""
    out = {}
    for n in sorted({r.set for r in recs if r.kept}):
        d = np.load(RIG / f"{n}.npz")
        starts, rot, roots = d["starts"], d["rotations"], d["roots"]
        ends = [*starts[1:], len(roots)]
        by = {str(t): (int(s), int(e)) for t, s, e in zip(d["takes"], starts, ends)}
        for r in recs:
            if r.kept and r.set == n:
                s, e = by[r.take]
                e = min(e, s + cap_frames)
                out[r.id] = (rot[s:e].astype(np.float64), roots[s:e].astype(np.float64))
        del rot
    return out


def write_clip(path: pathlib.Path, bones, rot: np.ndarray, roots: np.ndarray, name: str) -> None:
    clip = Clip(bones=bones, fps=float(FPS), name=name, group="candidates", origin="mocap")
    clip.physics = False
    for f in range(len(rot)):
        clip.set_key(Key(f, rot[f], roots[f], "linear"))
    clip.save(path)


def segment(args, work: pathlib.Path, bones, i: int, n: int, rec: selection.Record, rot, roots, cuts: int,
            speed: int, kind: str) -> pathlib.Path:
    """Render one take and give it its title card and overlay; the segment file, 30 fps."""
    stem = work / f"{i:05d}"
    clip_path, script, raw, seg = stem.with_suffix(".clip.json"), stem.with_suffix(".json"), stem.with_name(f"{stem.name}.raw.mp4"), stem.with_name(f"{stem.name}.seg.mp4")
    if seg.exists():
        return seg
    write_clip(clip_path, bones, rot, roots, rec.id)
    script.write_text(json.dumps([{"command": "clip.import", "params": {"path": str(clip_path.resolve()).replace("\\", "/")}},
                                  {"command": "physics.set", "params": {"on": False}}]), encoding="utf-8")
    run([args.ac, "--assets", args.assets, "video", "--script", str(script), "--view", "three-quarter", "--size", args.size,
         "--fixed", "--fps", str(FPS), "--out", str(raw)], f"ac video of {rec.id}")
    full = rec.frames / FPS
    shown = len(rot) / FPS
    style = ", ".join(rec.styles) if rec.styles else "no style word"
    line1 = f"{i}/{n}  {rec.id}"
    bits = [kind, f"{shown:.1f} s" + (f" (first {shown:.0f} s of {full:.1f} s)" if len(rot) < rec.frames else ""), style]
    if rec.held:
        bits.append("held out")
    if cuts:
        bits.append(f"{cuts} frames cut")
    if speed > 1:
        bits.append(f"{speed}x speed")
    line2 = "  ".join(bits)
    w, h = args.size.split("x")
    fit = lambda text, size: min(size, int((int(w) - 24) / (0.56 * max(len(text), 1))))  # noqa: E731  the whole line on the frame
    title = ";".join([
        f"[0:v]{drawtext(f'{i} / {n}', '(w-text_w)/2', 'h/2-70', 44, box=False)},"
        f"{drawtext(rec.id, '(w-text_w)/2', 'h/2-10', fit(rec.id, 24), box=False)},"
        f"{drawtext(line2, '(w-text_w)/2', 'h/2+30', fit(line2, 18), box=False)}[t]",
        f"[1:v]{'setpts=PTS/' + str(speed) + ',' if speed > 1 else ''}{drawtext(line1, '10', '10', fit(line1, 20))},"
        f"{drawtext(line2, '10', '42', fit(line2, 17))}[b]",
        "[t][b]concat=n=2:v=1:a=0,format=yuv420p[v]"])
    run([args.ffmpeg, "-y", "-loglevel", "error", "-f", "lavfi", "-i", f"color=c=black:s={w}x{h}:r={FPS}:d={TITLE_SECONDS}",
         "-i", str(raw), "-filter_complex", title, "-map", "[v]", "-r", str(FPS), "-c:v", "libx264", "-preset", "veryfast",
         "-crf", "23", str(seg)], f"ffmpeg overlay of {rec.id}")
    for p in (clip_path, script, raw):
        p.unlink()
    return seg


def join(args, work: pathlib.Path, out: pathlib.Path, kind: str, rows: list[dict]) -> float:
    """Concatenate the segments with chapters; writes ``<type>.chapters.txt`` and ``.index.csv``.
    Returns the video's length in seconds."""
    ffprobe = str(pathlib.Path(args.ffmpeg).with_name("ffprobe" + pathlib.Path(args.ffmpeg).suffix))
    chapters = [";FFMETADATA1", f"title={kind} candidates (roadmap 153000)"]
    at = 0.0
    for row in rows:
        n = frames_of(row["segment"], ffprobe)
        length = n / FPS
        row["start_s"] = round(at, 3)
        row["length_s"] = round(length - TITLE_SECONDS, 3)
        chapters += ["[CHAPTER]", "TIMEBASE=1/1000", f"START={int(at * 1000)}", f"END={int((at + length) * 1000) - 1}",
                     f"title={row['index']} {row['take']}"]
        at += length
    chap = out / f"{kind}.chapters.txt"
    chap.write_text("\n".join(chapters) + "\n", encoding="utf-8")
    listing = work / "list.txt"
    listing.write_text("".join(f"file '{str(r['segment'].resolve()).replace(chr(92), '/')}'\n" for r in rows), encoding="utf-8")
    run([args.ffmpeg, "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing), "-i", str(chap),
         "-map", "0:v", "-map_metadata", "1", "-map_chapters", "1", "-c", "copy", "-movflags", "+faststart",
         str(out / f"{kind}.mp4")], f"ffmpeg concat of {kind}")
    with open(out / f"{kind}.index.csv", "w", encoding="utf-8", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["index", "take", "start_s", "length_s", "type", "style", "held_out", "frames", "shown_frames", "speed"])
        for r in rows:
            w.writerow([r["index"], r["take"], r["start_s"], r["length_s"], kind, r["style"], r["held_out"], r["frames"],
                        r["shown_frames"], r["speed"]])
    return at


def main(argv=None):
    from .rig import Skeleton

    ap = argparse.ArgumentParser(description="Candidate videos: one long MP4 per motion type of the takes a filter keeps")
    ap.add_argument("sets", nargs="*", help="sets (default: every data/rig/*.npz)")
    ap.add_argument("--out", required=True, help="where the videos go (under target/shots/)")
    ap.add_argument("--ac", required=True, help="the app's CLI (a copy of ac.exe)")
    ap.add_argument("--assets", required=True, help="the assets folder the CLI loads")
    ap.add_argument("--decisions", default=str(review.DECISIONS))
    ap.add_argument("--only", default=None, help="comma list of types to render (default: every kept type)")
    ap.add_argument("--cap", type=float, default=10.0, help="seconds a take plays at most")
    ap.add_argument("--fast-over", type=int, default=300, help="a type with more takes than this plays at 2x")
    ap.add_argument("--size", default="640x480")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--limit", type=int, default=None, help="only the first N takes of each type (a check)")
    ap.add_argument("--keep-work", action="store_true", help="keep the per-take segments")
    selection.add_arguments(ap)
    args = ap.parse_args(argv)
    args.ffmpeg = ffmpeg()
    select = selection.from_args(args)
    decisions = review.load_decisions(pathlib.Path(args.decisions))
    names = args.sets or sorted(p.stem for p in RIG.glob("*.npz"))
    recs = selection.plan_sets(names, decisions, select, progress=lambda s: print(s, flush=True))
    print(selection.table(recs, select), flush=True)
    kinds = sorted({r.type or "untyped" for r in recs if r.kept}, key=lambda k: (k not in FIRST, FIRST.index(k) if k in FIRST else 0, k))
    if args.only:
        kinds = [k for k in kinds if k in args.only.split(",")]
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    bones = list(Skeleton.load().names)
    cap_frames = int(args.cap * FPS)
    plans = {}
    for n in names:
        if (RIG / f"{n}.npz").exists():
            d = np.load(RIG / f"{n}.npz")
            plans[n] = {t.name: t for t in review.plan_set(n, len(d["roots"]), d["starts"], d["takes"], decisions, npz=d).takes}
    summary = {}
    for kind in kinds:
        takes = sorted((r for r in recs if r.kept and (r.type or "untyped") == kind), key=lambda r: (r.set, r.take))
        if args.limit:
            takes = takes[: args.limit]
        speed = 2 if len(takes) > args.fast_over else 1
        name = kind if kind in FIRST else f"others-{kind}"
        work = out / "work" / name
        work.mkdir(parents=True, exist_ok=True)
        print(f"{name}: {len(takes)} takes at {speed}x, up to {args.cap:g} s each", flush=True)
        data = take_slices(takes, cap_frames)
        rows = []
        with ThreadPoolExecutor(args.jobs) as pool:
            futures = []
            for i, rec in enumerate(takes, 1):
                rot, roots = data[rec.id]
                cuts = int(plans[rec.set][rec.take].bad().sum())
                futures.append((i, rec, len(rot), pool.submit(segment, args, work, bones, i, len(takes), rec, rot, roots, cuts, speed, kind)))
            for i, rec, shown, fut in futures:
                seg = fut.result()
                rows.append({"index": i, "take": rec.id, "segment": seg, "style": ", ".join(rec.styles), "held_out": rec.held,
                             "frames": rec.frames, "shown_frames": shown, "speed": speed})
                if i % 100 == 0:
                    print(f"  {i}/{len(takes)}", flush=True)
        seconds = join(args, work, out, name, rows)
        size = (out / f"{name}.mp4").stat().st_size
        summary[name] = {"takes": len(takes), "seconds": round(seconds, 1), "speed": speed, "bytes": size}
        print(f"  {out / (name + '.mp4')}: {len(takes)} takes, {seconds / 60:.1f} min, {size / 1e6:.1f} MB", flush=True)
        if not args.keep_work:
            shutil.rmtree(work)
    (out / "summary.json").write_text(json.dumps({"selection": select.describe(), "videos": summary}, indent=1), encoding="utf-8")
    print(json.dumps(summary))


if __name__ == "__main__":
    try:
        main()
    except RuntimeError as e:
        print(e, file=sys.stderr)
        sys.exit(1)
