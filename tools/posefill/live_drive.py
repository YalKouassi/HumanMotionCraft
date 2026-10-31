"""The target test for live motion control (roadmap 158000, "measure first"): a scripted stick path
driven through the installed helper with our own target set, the loop the app will run written
here in plain Python so the owner can judge the targets by eye before any window is built.

    uv run python tools/posefill/live_drive.py --out target/shots/2026-10-31/live-motion

The loop, each frame at 30 fps: the stick (heading in degrees, speed in m/s) is smoothed into a
desired velocity the way a character runtime does (critically damped: half-lives of 0.6 s speeding
up, 0.4 s slowing down, 0.2 s for the direction); the helper is asked for a new stretch when there
is none, when playback reaches the stretch's transition point, or when the input changed enough
(started moving, the speed moved by over 30% of what was left of the range or 10% of the top speed,
the direction turned past cos 0.92); the frames shown are recorded. The same rules go into the Rust
`Live` state machine (crates/ac-posefill/src/live.rs). Writes `<name>.clip.json` (every frame a key
at 30 fps), `summary.json` (every call's timings) and `steps.jsonl`.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import pathlib
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
RIG = REPO / "assets" / "rig" / "default_rig.json"
TARGETS = REPO / "assets" / "posefill-targets" / "locomotion.json"
FPS = 30.0
HALF_UP, HALF_DOWN, HALF_TURN = 0.6, 0.4, 0.2
SPEED_FRACTION, MIN_SPEED_FRACTION, TURN_DOT = 0.3, 0.1, 0.92
#: while moving: a target a second ahead reached in 26 to 34 frames, so the character keeps up with the stick
MOVING_TOKENS = "8,9,10"

#: the stick path: (seconds, heading degrees (0 forward, +left), speed m/s); each row holds until the next
PATH = [
    (0.0, 0.0, 0.0),
    (1.0, 0.0, 1.3),  # walk
    (4.0, 0.0, 3.6),  # speed up to a run
    (7.0, 90.0, 3.6),  # turn left
    (9.0, 0.0, 3.6),  # and back right
    (11.0, 0.0, 1.3),  # slow to a walk
    (13.0, 0.0, 0.0),  # stop
    (15.0, 0.0, 0.0),
]


def install() -> dict:
    d = os.environ.get("AC_POSEFILL_DIR") or str(pathlib.Path(os.environ["LOCALAPPDATA"]) / "HumanMotionCraft" / "posefill")
    return json.loads((pathlib.Path(d) / "install.json").read_text(encoding="utf-8"))


class Helper:
    def __init__(self, log: pathlib.Path):
        inst = install()
        self.p = subprocess.Popen([inst["python"], "-I", inst["helper"]], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=open(log, "w", encoding="utf-8"), text=True, encoding="utf-8",
                                  env={**os.environ, **inst["env"]}, cwd=pathlib.Path(inst["helper"]).parent)
        self.n = 0

    def ask(self, op: str, **req) -> dict:
        self.n += 1
        rid = self.n
        self.p.stdin.write(json.dumps({"id": rid, "op": op, **req}) + "\n")
        self.p.stdin.flush()
        t0 = time.perf_counter()
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise SystemExit("the helper ended")
            msg = json.loads(line)
            if msg.get("id") != rid:
                continue
            if "progress" in msg:
                print(f"  [{rid}] {msg['progress']}", flush=True)
                continue
            msg["wall_ms"] = round((time.perf_counter() - t0) * 1000, 2)
            if "error" in msg:
                raise SystemExit(f"{op}: {msg['error']}")
            return msg

    def close(self) -> None:
        self.p.stdin.close()
        self.p.wait(timeout=60)


def decay(x: float, goal: float, half: float, dt: float) -> float:
    return goal + (x - goal) * 0.5 ** (dt / half)


def angle_to(a: float, goal: float, half: float, dt: float) -> float:
    d = (goal - a + math.pi) % (2 * math.pi) - math.pi
    return a + d * (1 - 0.5 ** (dt / half))


def stick_at(t: float) -> tuple[float, float]:
    heading, speed = PATH[0][1], PATH[0][2]
    for s, h, v in PATH:
        if t >= s:
            heading, speed = h, v
    return heading, speed


def write_clip(path: pathlib.Path, name: str, bones: list[str], frames: list[tuple[list, list]]) -> None:
    keys = []
    for i, (rot, root) in enumerate(frames):
        k = {"frame": i, "rotations": rot, "root": root, "interp": "smooth"}
        if i > 0:
            k["source"], k["model"] = "ai", "posefill"
        keys.append(k)
    doc = {"format": "humanmotioncraft.clip", "version": 1, "name": name, "group": "Live motion", "origin": "posefill",
           "fps": FPS, "loop": False, "bones": bones, "keys": keys}
    path.write_text(json.dumps(doc), encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--targets", type=pathlib.Path, default=TARGETS)
    ap.add_argument("--name", default="stick-path")
    ap.add_argument("--seconds", type=float, default=PATH[-1][0] + 1.0)
    ap.add_argument("--tokens", default=MOVING_TOKENS, help="token counts the model may choose from while moving; empty for the helper's")
    args = ap.parse_args()
    tokens = [int(t) for t in args.tokens.split(",") if t.strip()] or None
    args.out.mkdir(parents=True, exist_ok=True)
    targets = json.loads(args.targets.read_text(encoding="utf-8"))
    bones = json.loads(RIG.read_text(encoding="utf-8"))["names"]
    by_name = {t["name"]: t for t in targets["targets"]}
    max_speed = max(t["speed"] for t in targets["targets"])
    start = by_name["stand"]["poses"][0] if "stand" in by_name else targets["targets"][0]["poses"][0]

    h = Helper(args.out / f"{args.name}.helper-stderr.txt")
    info = h.ask("info")
    load = h.ask("load")
    print(f"helper: {info['result'].get('gpu')} plan {info['result'].get('plan')}; load {load['wall_ms']} ms", flush=True)
    s = h.ask("live_start", rig=str(RIG), targets=targets, start=start)
    session = s["result"]["session"]

    dt = 1.0 / FPS
    desired_speed, desired_angle = 0.0, math.radians(0.0) + math.atan2(-1.0, 0.0)  # the rig faces -Y
    facing_angle = desired_angle
    segment, played, shown_since = None, 0, 0
    seg_speed, seg_dir = 0.0, (0.0, -1.0)
    recorded: list[tuple[list, list]] = [(start["rotations"], start["root"])]
    steps: list[dict] = []
    n_frames = int(args.seconds * FPS)
    t_take = time.perf_counter()
    for i in range(n_frames):
        t = i * dt
        heading_deg, speed_in = stick_at(t)
        goal_angle = math.radians(heading_deg) + math.atan2(-1.0, 0.0)
        desired_angle = angle_to(desired_angle, goal_angle, HALF_TURN, dt)
        speed_in = min(speed_in, max_speed)
        desired_speed = decay(desired_speed, speed_in, HALF_UP if speed_in > desired_speed else HALF_DOWN, dt)
        if desired_speed < 1e-3:
            desired_speed = 0.0
        direction = (math.cos(desired_angle), math.sin(desired_angle))
        if desired_speed > 0.0:
            facing_angle = desired_angle
        facing = (math.cos(facing_angle), math.sin(facing_angle))
        velocity = (direction[0] * desired_speed, direction[1] * desired_speed)

        why = ""
        if segment is None:
            why = "start"
        elif played > segment["transition"]:
            why = "transition"
        else:
            started = desired_speed > 0.0 and seg_speed <= 1e-6
            delta = desired_speed - seg_speed
            remaining = (max_speed - seg_speed) if delta > 0 else seg_speed
            threshold = max(remaining * SPEED_FRACTION, max_speed * MIN_SPEED_FRACTION)
            if started or abs(delta) > threshold:
                why = "speed"
            elif seg_speed > 1e-6 and (direction[0] * seg_dir[0] + direction[1] * seg_dir[1]) < TURN_DOT:
                why = "turn"
        if why:
            r = h.ask("live_step", session=session, shown=shown_since, velocity=list(velocity), facing=list(facing),
                      tokens=tokens if desired_speed > 0.0 else None)
            res = r["result"]
            segment = res
            played, shown_since = 1, 0
            seg_speed, seg_dir = desired_speed, direction
            steps.append({"frame": i, "t": round(t, 3), "why": why, "speed": round(desired_speed, 3), "heading_deg": round(heading_deg, 1),
                          "frames": res["frames"], "transition": res["transition"], "target": res["target"],
                          "wall_ms": r["wall_ms"], "helper_wall_ms": res["wall_ms"], "gpu_ms": round(res["seconds"] * 1000, 2),
                          "stages_ms": res["stages_ms"], "convert_ms": res["convert_ms"], "position": res["position"]})
            print(f"  {t:5.2f} s {why:10s} speed {desired_speed:4.2f} -> {res['target']:14s} {res['frames']:2d} frames; "
                  f"round trip {r['wall_ms']:6.1f} ms (gpu {res['seconds'] * 1000:5.1f}, convert in {res['convert_ms']['in']:5.1f} "
                  f"out {res['convert_ms']['out']:5.1f})", flush=True)
        recorded.append((segment["rotations"][played], segment["roots"][played]))
        played += 1
        shown_since += 1
    take_seconds = time.perf_counter() - t_take
    h.close()

    write_clip(args.out / f"{args.name}.clip.json", args.name, bones, recorded)
    walls = [s["wall_ms"] for s in steps]
    gpus = [s["gpu_ms"] for s in steps]
    conv = [s["convert_ms"]["out"] for s in steps]
    frames_per = [s["frames"] for s in steps]
    summary = {
        "gpu": info["result"].get("gpu"), "load_ms": load["wall_ms"], "frames": len(recorded), "calls": len(steps),
        "take_wall_s": round(take_seconds, 2),
        "round_trip_ms": {"mean": round(sum(walls) / len(walls), 1), "max": round(max(walls), 1), "min": round(min(walls), 1),
                          "first": walls[0], "over_100": sum(w > 100 for w in walls)},
        "gpu_ms": {"mean": round(sum(gpus) / len(gpus), 2), "max": round(max(gpus), 2)},
        "convert_out_ms": {"mean": round(sum(conv) / len(conv), 1), "max": round(max(conv), 1),
                           "per_frame": round(sum(conv) / sum(frames_per), 2)},
        "frames_per_call": {"mean": round(sum(frames_per) / len(frames_per), 1), "min": min(frames_per), "max": max(frames_per)},
        "steps": steps,
    }
    (args.out / f"{args.name}.summary.json").write_text(json.dumps(summary, indent=1), encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k != "steps"}, indent=1), flush=True)


if __name__ == "__main__":
    sys.exit(main())
