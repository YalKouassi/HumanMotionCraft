"""How good a fill is, measured the way it looks: the evaluation suite behind every model card.

    python -m ac_train.evaluate --tag parts            # writes "evaluation" into the card
    python -m ac_train.evaluate --tag parts --sheets   # and contact sheets of fixed hard fills

Windows come from the held-out subjects (:mod:`ac_train.split`) of every set the registry allows,
the same windows for every model (seeded per gap), and each is filled by the model and by the
plain interpolation it starts from. Per window, over the gap frames:

- **pose**: ``joint_m`` mean joint position error, ``end_m`` the wrists and feet alone,
  ``root_mid_m`` the root's error at mid-gap, ``rot_deg`` mean rotation error per bone
  (``bones_deg``: each bone's);
- **feet** (:mod:`ac_train.kinematics`, the quality scan's contact test): ``slide_ms`` the fill's
  foot speed on frames where the recorded foot is planted, ``skate_share`` the share of those
  faster than the scan's skate limit, ``floor_m`` how far the lowest foot point sinks under the
  floor, ``sink_share`` frames sinking past the scan's limit;
- **continuity**: ``seam_a_ms`` and ``seam_b_ms`` the velocity error leaving key A and arriving
  at key B, ``jerk_ms3`` the mean third difference over the gap and both keys;
- **possible**: ``impossible_per_1000`` gap frames with a joint past the possible limits
  (``assets/rig/ranges.json``), per 1,000.

``truth`` gives the recording's own jerk, floor and impossible numbers beside them.

**Posed keys** (``posed``): keys A and B are two of the built-in hand-made poses
(``assets/library/poses``, each stood on the floor by its lowest foot point), key A held still
before the gap and key B a random step away, turned
up to :data:`POSED_TURN_DEG`, as a person keying by hand gives them. Nothing was recorded between
them, so only what needs no recording is measured: ``impossible_per_1000``, ``floor_m``,
``sink_share`` and ``jerk_ms3``, the model's beside the interpolation's. This is where impossible
fills come from in the app (136000), and where the sparse-key context (129000) is meant to help.

**Root path** (``trajectory``, 141000): on the clean windows, the root put on a path across the gap
(the recording's, smoothed over :data:`TRAJECTORY_SMOOTH` frames: rounder, as the keys' spline
is), as the app's ``trajectory`` 1 does after the network. ``blend``: the fill with the root moved
onto the path afterwards, beside the interpolation moved the same way. ``given`` (a model trained
with the path): the network also given the path, beside ``blend``, so its ``slide_ms`` against
``blend``'s is 141000's acceptance.

They are reported by gap bin (:data:`BINS`), for every context the app can give
(:data:`ac_train.context.MODES`: clean mocap, a spline through sparse keys (smooth, linear, step),
key A held still), by motion type and style, on a hard slice (a foot plants or lifts inside the
gap, or the path turns), and for part layers: each part group's complement given, the error on the
free bones alone. One composite number (:func:`score`) picks training's best checkpoint.

**Scenarios** (126500, ``scenarios``): fixed slices of the clean windows, read from the recording's
own motion, not its take name (:func:`scenarios`): ``start`` and ``stop`` (the hips going from still
to moving, or back), ``crouch``, ``flight`` (both feet off the floor inside the gap) and ``turn_step``
(a turn with a foot planting or lifting), each with its own measures (:func:`scene_measure`) beside
plain interpolation's, its window count, and ``too_small`` under :data:`MIN_WINDOWS`. Carrying has no
slice of its own: the rig has no load model, so it is the ``carry`` row of ``by_type``.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
from collections import defaultdict

import numpy as np
import torch

from . import MODELS, REPO, context, review, selection
from .kinematics import EXTREMITIES, FOOT_POINTS, POINTS, Body, foot_speed, geodesic
from .model import BONES, CONTEXT, PART_GROUPS, given_features, layout, smooth_path, unfeatures
from .quality import FPS, THRESHOLDS

#: gap bins (frames between the keys, both ends included)
BINS = ((1, 4), (5, 10), (11, 20), (21, 30), (31, 45))
#: what the composite score weighs: (input, metric); each as the model's share of the
#: interpolation's
SCORE = (("clean", "joint_m"), ("clean", "seam_a_ms"), ("clean", "seam_b_ms"), ("clean", "slide_ms"),
         ("smooth", "joint_m"), ("still", "joint_m"))
#: posed keys: windows per gap bin, the turn and the step (hip heights per frame) up to which
#: key B is moved from key A
POSED_WINDOWS = 64
POSED_TURN_DEG = 30.0
POSED_STEP = 0.03
#: the root-path case: smoothing of the recorded path (frames)
TRAJECTORY_SMOOTH = 9
#: a turn this sharp (degrees) between the path leaving key A and arriving at key B is hard...
TURN_DEG = 45.0
#: ...when the root moves at least this fast at both ends (m/s)
TURN_SPEED = 0.5
#: scenarios: the hips are still under this speed (m/s), and going over the second
STILL_MS = 0.2
MOVING_MS = 0.7
#: a crouch: hips between these shares of their standing height (lower is lying down or sitting)
CROUCH_SHARE = (0.4, 0.75)
#: a flight: this many frames in a row with both feet off the floor, inside the gap
FLIGHT_FRAMES = 2
#: a slice with fewer windows than this says so: its numbers aren't to be trusted
MIN_WINDOWS = 30
SCENARIOS = ("start", "stop", "crouch", "flight", "turn_step")


def bin_name(lo: int, hi: int) -> str:
    return f"{lo}-{hi}"


def bin_of(gap: int) -> str:
    for lo, hi in BINS:
        if lo <= gap <= hi:
            return bin_name(lo, hi)
    raise ValueError(f"gap {gap} is outside every bin")


# ---- per-window measurements -------------------------------------------------------------------

def free_points(gmask, body: Body):
    """(N, J) given bones -> (N, P) points that move with a free bone (an end point goes with the
    bone it hangs off)."""
    free = 1 - gmask
    return torch.cat([free, free[:, body.end_parent]], 1)


class Pose:
    """One filled window's rotations, points and world rotations (from features)."""

    def __init__(self, f, body: Body):
        self.local, self.root = unfeatures(f, body.hip_height)
        self.world = body.world(self.local)
        self.points = body.points(self.local, self.root, self.world)


def measure(pose: Pose, truth: Pose, body: Body, a: int, b: int, free=None, gfree=None) -> dict:
    """Per-window metrics of a fill against the recording: name -> (value (N,), weight (N,)).
    ``free`` (N, P): points to score (default all); ``gfree`` (N, J): bones to score."""
    gap = slice(a + 1, b)
    N, g = pose.points.shape[0], b - a - 1
    one = torch.ones(N, device=pose.points.device)
    err = (pose.points - truth.points).norm(dim=-1)  # (N, T, P)
    J = len(BONES)
    pm = torch.ones(N, len(POINTS), device=err.device) if free is None else free
    pj = pm[:, :J]
    out = {}
    out["joint_m"] = ((err[:, gap, :J] * pj[:, None]).sum((1, 2)) / (pj.sum(1) * g).clamp(min=1), one)
    em = pm[:, list(EXTREMITIES)]
    out["end_m"] = ((err[:, gap][..., list(EXTREMITIES)] * em[:, None]).sum((1, 2)) / (em.sum(1) * g).clamp(min=1), em.sum(1).clamp(max=1))
    mid = a + (g + 1) // 2
    out["root_mid_m"] = ((pose.root[:, mid] - truth.root[:, mid]).norm(dim=-1), one)
    rot = geodesic(pose.local[:, gap], truth.local[:, gap]) * (180 / np.pi)  # (N, g, J)
    bj = torch.ones(N, J, device=err.device) if gfree is None else gfree
    out["rot_deg"] = ((rot * bj[:, None]).sum((1, 2)) / (bj.sum(1) * g).clamp(min=1), one)
    # feet: planted on the recording, how fast the fill's foot moves there
    planted = body.planted(truth.points)[:, gap]  # (N, g, 2)
    speed = foot_speed(pose.points)[:, gap]
    n_pl = planted.sum((1, 2)).float()
    out["slide_ms"] = ((speed * planted).sum((1, 2)) / n_pl.clamp(min=1), n_pl)
    out["skate_share"] = (((speed > THRESHOLDS["skate_ms"]) & planted).sum((1, 2)) / n_pl.clamp(min=1), n_pl)
    low = body.foot_heights(pose.points[:, gap]).min(-1).values  # (N, g)
    out["floor_m"] = (torch.relu(-low).mean(1), one * g)
    out["sink_share"] = ((low < -THRESHOLDS["sink_m"]).float().mean(1), one * g)
    # continuity: velocity leaving A and arriving at B, against the recording's
    v = lambda p, i: p[:, i + 1] - p[:, i]  # noqa: E731
    seam = lambda i: ((v(pose.points, i) - v(truth.points, i)).norm(dim=-1) * pm).sum(-1) / pm.sum(-1).clamp(min=1) * FPS  # noqa: E731
    out["seam_a_ms"] = (seam(a), one)
    out["seam_b_ms"] = (seam(b - 1), one)
    out["jerk_ms3"] = (jerk(pose.points, a, b), one)
    bad = body.impossible(pose.world[:, gap])  # (N, g)
    out["impossible_per_1000"] = (bad.float().mean(1) * 1000, one * g)
    out["bones_deg"] = (rot.mean(1), one)  # (N, J): kept apart (a vector per window)
    return out


def jerk(points, a: int, b: int):
    """(N,) mean third difference of the points over frames a - 1 .. b (m/s^3)."""
    p = points[:, max(a - 1, 0) : b + 1]
    if p.shape[1] < 4:
        return torch.zeros(p.shape[0], device=p.device)
    d3 = p[:, 3:] - 3 * p[:, 2:-1] + 3 * p[:, 1:-2] - p[:, :-3]
    return d3.norm(dim=-1).mean((1, 2)) * FPS**3


def measure_posed(pose: Pose, body: Body, a: int, b: int) -> dict:
    """What a fill between hand-made poses can be measured by, with no recording to compare with."""
    gap = slice(a + 1, b)
    N, g = pose.points.shape[0], b - a - 1
    one = torch.ones(N, device=pose.points.device)
    low = body.foot_heights(pose.points[:, gap]).min(-1).values
    return {"impossible_per_1000": (body.impossible(pose.world[:, gap]).float().mean(1) * 1000, one * g),
            "floor_m": (torch.relu(-low).mean(1), one * g),
            "sink_share": ((low < -THRESHOLDS["sink_m"]).float().mean(1), one * g),
            "jerk_ms3": (jerk(pose.points, a, b), one)}


def library_poses(root: pathlib.Path | None = None) -> tuple[np.ndarray, np.ndarray]:
    """The built-in poses as (P, J, 4) rotations (x, y, z, w; the model's bones) and (P, 3) root
    offsets (hip heights)."""
    root = root or REPO / "assets" / "library" / "poses"
    rots, roots = [], []
    for path in sorted(root.rglob("*.pose.json")):
        p = json.loads(path.read_text(encoding="utf-8"))
        at = {n: i for i, n in enumerate(p["bones"])}
        r = np.tile([0.0, 0.0, 0.0, 1.0], (len(BONES), 1))
        for j, n in enumerate(BONES):
            if n in at:
                r[j] = p["rotations"][at[n]]
        rots.append(r)
        roots.append(p["root"])
    return np.asarray(rots, np.float64), np.asarray(roots, np.float64)


def ground(poses, body: Body, device) -> tuple[np.ndarray, np.ndarray]:
    """The poses with each one's root raised or lowered until its lowest foot point is on the floor:
    the pose files keep the root where it was posed (a floor pose's near 0, its feet underground)."""
    from .train import prepare

    rots, roots = poses
    q = torch.tensor(rots, dtype=torch.float32, device=device)[:, None].repeat(1, CONTEXT + 2, 1, 1)
    r = torch.tensor(roots, dtype=torch.float32, device=device)[:, None].repeat(1, CONTEXT + 2, 1)
    f, _, _, _, _, _ = prepare(q, r, 1, body, device)
    low = body.foot_heights(Pose(f, body).points[:, CONTEXT - 1]).min(-1).values
    out = roots.copy()
    out[:, 2] -= low.double().cpu().numpy() / body.hip_height
    return rots, out


def posed_windows(poses, n: int, gap: int, rng: np.random.Generator, after: int = 0):
    """``n`` windows between two hand-made poses: (n, T, J, 4) rotations and (n, T, 3) root
    offsets; key A held from the first frame, key B from the frame after the gap to the last
    (``after`` context frames after B hold it too: the clip's last key)."""
    from . import quat

    rots, roots = poses
    a, _, T = layout(gap, after)
    ia, ib = rng.integers(len(rots), size=n), rng.integers(len(rots), size=n)
    yaw = np.radians(rng.uniform(-POSED_TURN_DEG, POSED_TURN_DEG, n))
    turn = np.stack([np.zeros(n), np.zeros(n), np.sin(yaw / 2), np.cos(yaw / 2)], -1)
    q = np.repeat(rots[ia][:, None], T, 1)
    r = np.repeat(roots[ia][:, None], T, 1)
    qb = rots[ib].copy()
    qb[:, 0] = quat.mul(turn, qb[:, 0])  # the root bone turns about Z
    step = rng.uniform(0, POSED_STEP * gap, n)
    heading = rng.uniform(0, 2 * np.pi, n)
    rb = roots[ib].copy()
    rb[:, 0] = roots[ia][:, 0] + step * np.cos(heading)
    rb[:, 1] = roots[ia][:, 1] + step * np.sin(heading)
    q[:, a + 1 :] = qb[:, None]
    r[:, a + 1 :] = rb[:, None]
    return torch.tensor(q, dtype=torch.float32), torch.tensor(r, dtype=torch.float32)


def truth_numbers(truth: Pose, body: Body, a: int, b: int) -> dict:
    """The recording's own jerk, floor and impossible frames, to read the fills' against."""
    gap = slice(a + 1, b)
    N, g = truth.points.shape[0], b - a - 1
    one = torch.ones(N, device=truth.points.device)
    low = body.foot_heights(truth.points[:, gap]).min(-1).values
    return {"jerk_ms3": (jerk(truth.points, a, b), one), "floor_m": (torch.relu(-low).mean(1), one * g),
            "impossible_per_1000": (body.impossible(truth.world[:, gap]).float().mean(1) * 1000, one * g)}


def hard(truth: Pose, body: Body, a: int, b: int) -> dict:
    """(N,) bool per hard kind: a foot plants or lifts inside the gap; the path turns."""
    pl = body.planted(truth.points)[:, a : b + 1]
    plant = (pl[:, 1:] != pl[:, :-1]).any(-1).any(-1)
    r = truth.root[..., :2]
    v0, v1 = (r[:, a + 1] - r[:, a]) * FPS, (r[:, b] - r[:, b - 1]) * FPS
    fast = (v0.norm(dim=-1) > TURN_SPEED) & (v1.norm(dim=-1) > TURN_SPEED)
    cos = (v0 * v1).sum(-1) / (v0.norm(dim=-1) * v1.norm(dim=-1)).clamp(min=1e-9)
    turn = fast & (cos < np.cos(np.radians(TURN_DEG)))
    return {"plant_change": plant, "turn": turn, "either": plant | turn}


def hip_speed(pose: Pose):
    """(N, T - 1) the root's speed across the floor between frames (m/s)."""
    return (pose.root[:, 1:, :2] - pose.root[:, :-1, :2]).norm(dim=-1) * FPS


def airborne(truth: Pose, body: Body):
    """(N, T) bool: every foot point is off the floor."""
    return body.foot_heights(truth.points).min(-1).values > THRESHOLDS["contact_m"]


def scenarios(truth: Pose, body: Body, a: int, b: int) -> dict:
    """(N,) bool per scenario, from the recording's own motion."""
    v = hip_speed(truth)
    start = (v[:, a] < STILL_MS) & (v[:, b - 1] > MOVING_MS)
    stop = (v[:, a] > MOVING_MS) & (v[:, b - 1] < STILL_MS)
    gap = slice(a + 1, b)
    share = truth.points[:, gap, 0, 2].mean(1) / body.hip_height
    in_air = airborne(truth, body)[:, gap]
    crouch = (share > CROUCH_SHARE[0]) & (share < CROUCH_SHARE[1]) & (~in_air).all(1)
    air = in_air.int()
    run_len = torch.zeros_like(air)
    run_len[:, 0] = air[:, 0]
    for t in range(1, air.shape[1]):
        run_len[:, t] = (run_len[:, t - 1] + 1) * air[:, t]
    h = hard(truth, body, a, b)
    return {"start": start, "stop": stop, "crouch": crouch, "flight": run_len.max(1).values >= FLIGHT_FRAMES,
            "turn_step": h["turn"] & h["plant_change"]}


def scene_measure(pose: Pose, truth: Pose, body: Body, a: int, b: int) -> dict:
    """The scenarios' own measures of a fill against the recording: name -> (value (N,), weight (N,)).

    ``hip_speed_ms``: how far the hips' speed is from the recording's, from key to key.
    ``flight_root_m``: how far the root is on the frames the recording is in the air (the flight's shape).
    ``landing_m``: how far the root is on the first frame down after them, when that is in the gap.
    ``foot_place_m``: how far across the floor a foot is from where the recording plants it."""
    gap = slice(a + 1, b)
    N = pose.points.shape[0]
    one = torch.ones(N, device=pose.points.device)
    out = {"hip_speed_ms": ((hip_speed(pose) - hip_speed(truth))[:, a:b].abs().mean(1), one)}
    root = (pose.root - truth.root).norm(dim=-1)  # (N, T)
    air = airborne(truth, body)
    n_air = air[:, gap].sum(1).float()
    out["flight_root_m"] = ((root[:, gap] * air[:, gap]).sum(1) / n_air.clamp(min=1), n_air)
    landed = air[:, a + 1 : b - 1] & ~air[:, a + 2 : b]  # in the air on a frame, down on the next, both in the gap
    if landed.shape[1]:
        first = landed.int().argmax(1) + a + 2
        out["landing_m"] = (root[torch.arange(N, device=root.device), first], landed.any(1).float())
    else:  # a gap of one frame has no frame to land on after one in the air
        out["landing_m"] = (one * 0, one * 0)
    planted = body.planted(truth.points)[:, gap]  # (N, g, 2)
    err = (pose.points - truth.points)[:, gap][..., list(FOOT_POINTS), :2].norm(dim=-1)
    foot = (err[..., 0:2] + err[..., 2:4]) / 2
    n_pl = planted.sum((1, 2)).float()
    out["foot_place_m"] = ((foot * planted).sum((1, 2)) / n_pl.clamp(min=1), n_pl)
    return out


# ---- aggregation --------------------------------------------------------------------------------

class Acc:
    """Weighted sums per metric, for one row of the report."""

    def __init__(self):
        self.s, self.w, self.windows = defaultdict(float), defaultdict(float), 0

    def add(self, m: dict, rows=None):
        for k, (v, w) in m.items():
            if rows is not None:
                v, w = v[rows], w[rows]
            if v.dim() > 1:  # bones_deg: a vector each
                self.s[k] = self.s[k] + (v * w[:, None]).sum(0).double().cpu().numpy()
            else:
                self.s[k] += float((v * w).sum())
            self.w[k] += float(w.sum())

    def result(self) -> dict:
        out = {}
        for k in self.s:
            if not self.w[k]:
                continue
            v = self.s[k] / self.w[k]
            out[k] = {b: _r(x) for b, x in zip(BONES, v)} if isinstance(v, np.ndarray) else _r(v)
        return out


def _r(x: float) -> float:
    return float(f"{float(x):.5g}")


class Row:
    """A report row: the model's, the interpolation's and the recording's numbers."""

    def __init__(self):
        self.model, self.interp, self.truth = Acc(), Acc(), Acc()
        self.windows = 0

    def add(self, mm, mi, mt=None, rows=None):
        self.model.add(mm, rows)
        self.interp.add(mi, rows)
        if mt is not None:
            self.truth.add(mt, rows)
        self.windows += int(rows.sum()) if rows is not None else next(iter(mm.values()))[0].shape[0]

    def result(self) -> dict:
        out = {"windows": self.windows, "model": self.model.result(), "interp": self.interp.result()}
        t = self.truth.result()
        if t:
            out["truth"] = t
        return out


# ---- the run ------------------------------------------------------------------------------------

def window_of(model) -> dict:
    """How ``model`` lays out its windows, for :func:`ac_train.train.prepare`: ``after`` context
    frames after key B (140000) and ``multi_key`` (142000); both off for a model without them."""
    return {"after": int(getattr(model, "context_after", 0)), "multi_key": bool(getattr(model, "multi_key", False))}


def _forward(model, truth_f, interp, mask, ttg, style, gmask=None, path=None):
    """The fill as the app keeps it: the network's gap frames, the known frames as they are.
    ``path``: the root path on the gap frames (a model that takes one)."""
    extra = {"path": path, "pmask": 1 - mask} if path is not None and getattr(model, "path", False) else {}
    out = model(truth_f, interp, mask, ttg, style, gmask=gmask, **extra) if getattr(model, "given", False) else model(truth_f, interp, mask, ttg, style)
    out = out[0] if isinstance(out, tuple) else out
    return out * (1 - mask) + truth_f * mask


@torch.no_grad()
def run(model, sets, body: Body, device, windows: int = 512, inputs=context.MODES, parts: bool = True,
        breakdown: bool = True, seed: int = 1, posed: int = POSED_WINDOWS, trajectory: bool = True) -> dict:
    """The evaluation of ``model`` on ``sets`` (a :class:`ac_train.train.Sets` with styles learnt).
    ``windows`` per gap bin, spread over its gaps; ``posed`` windows per bin between hand-made
    poses (0: none); ``trajectory``: the root-path case."""
    from .train import prepare

    was = model.training
    model.eval()
    bins = {bin_name(*b): Row() for b in BINS}
    total = Row()
    by_input = {m: {"all": Row(), "bins": {bin_name(*b): Row() for b in BINS}} for m in inputs}
    by_type, by_style, by_hard = defaultdict(Row), defaultdict(Row), defaultdict(Row)
    by_scene = {k: Row() for k in SCENARIOS}
    by_part = {p: Row() for p in PART_GROUPS} if parts else {}
    traj = {"blend": Row(), **({"given": Row()} if getattr(model, "path", False) else {})} if trajectory else {}
    win = window_of(model)
    a = CONTEXT - win["after"] - 1
    for lo, hi in BINS:
        name = bin_name(lo, hi)
        gaps = range(lo, hi + 1)
        per = max(1, windows // len(gaps))
        for g in gaps:
            b = a + g + 1
            gen = torch.Generator(device=device).manual_seed(seed * 1000 + g)
            q, root, style, take = sets.batch(per, g, gen)
            for mode in inputs:
                cgen = torch.Generator(device=device).manual_seed(seed * 1000 + g + 100 * (1 + context.MODES.index(mode)))
                mq, mr = context.rebuild(q, root, a, mode, cgen, b=b)
                truth_f, interp, mask, ttg, _, _ = prepare(mq, mr, g, body, device, **win)
                pred = _forward(model, truth_f, interp, mask, ttg, style)
                tp, pp, ip = Pose(truth_f, body), Pose(pred, body), Pose(interp, body)
                mm, mi = measure(pp, tp, body, a, b), measure(ip, tp, body, a, b)
                by_input[mode]["all"].add(mm, mi)
                by_input[mode]["bins"][name].add(mm, mi)
                if mode != "clean":
                    continue
                mt = truth_numbers(tp, body, a, b)
                bins[name].add(mm, mi, mt)
                total.add(mm, mi, mt)
                if traj:
                    path = smooth_path(truth_f[..., -3:], TRAJECTORY_SMOOTH)
                    gap_f = 1 - mask
                    on_path = lambda f: torch.cat([f[..., :-3], f[..., -3:] * mask + path * gap_f], -1)  # noqa: E731
                    blend = measure(Pose(on_path(pred), body), tp, body, a, b)
                    traj["blend"].add(blend, measure(Pose(on_path(interp), body), tp, body, a, b))
                    if "given" in traj:
                        pg = _forward(model, truth_f, interp, mask, ttg, style, path=path)
                        traj["given"].add(measure(Pose(on_path(pg), body), tp, body, a, b), blend)
                if breakdown:
                    takes = take.tolist()
                    kinds = [sets.types[t] or "untyped" for t in takes]
                    for k in sorted(set(kinds)):
                        by_type[k].add(mm, mi, mt, torch.tensor([x == k for x in kinds], device=device))
                    words = [set(sets.labels[t][1:]) for t in takes]
                    for w in sorted(set().union(*words)):
                        by_style[w].add(mm, mi, mt, torch.tensor([w in x for x in words], device=device))
                    for kind, rows in hard(tp, body, a, b).items():
                        if rows.any():
                            by_hard[kind].add(mm, mi, mt, rows)
                    scenes = scenarios(tp, body, a, b)
                    if any(rows.any() for rows in scenes.values()):
                        sm, si = {**mm, **scene_measure(pp, tp, body, a, b)}, {**mi, **scene_measure(ip, tp, body, a, b)}
                        for kind, rows in scenes.items():
                            if rows.any():
                                by_scene[kind].add(sm, si, mt, rows)
                for part, grp in by_part.items():
                    gm = torch.ones(q.shape[0], len(BONES), device=device)
                    gm[:, list(PART_GROUPS[part])] = 0  # every bone given but the part's
                    pred_p = _forward(model, truth_f, interp, mask, ttg, style, gmask=gm)
                    fp = free_points(gm, body)
                    fm = given_features(gm)  # the plain fill keeps the given bones too, as the app does
                    base = Pose(interp * (1 - fm) + truth_f * fm, body)
                    grp.add(measure(Pose(pred_p, body), tp, body, a, b, fp, 1 - gm),
                            measure(base, tp, body, a, b, fp, 1 - gm))
    posed_rows = {"all": Row(), "bins": {bin_name(*b): Row() for b in BINS}}
    poses = ground(library_poses(), body, device) if posed else None
    for lo, hi in BINS if posed else ():
        gaps = range(lo, hi + 1)
        for g in gaps:
            b = a + g + 1
            q, root = posed_windows(poses, max(1, posed // len(gaps)), g, np.random.default_rng(seed * 1000 + g), win["after"])
            q, root = q.to(device), root.to(device)
            style = torch.zeros(q.shape[0], sets.style.shape[1], device=device)
            style[:, 0] = 1.0  # "any"
            truth_f, interp, mask, ttg, _, _ = prepare(q, root, g, body, device, **win)
            pred = _forward(model, truth_f, interp, mask, ttg, style)
            mm, mi = measure_posed(Pose(pred, body), body, a, b), measure_posed(Pose(interp, body), body, a, b)
            posed_rows["all"].add(mm, mi)
            posed_rows["bins"][bin_name(lo, hi)].add(mm, mi)
    model.train(was)
    out = {
        "windows_per_bin": windows,
        "seed": seed,
        "bins": {k: r.result() for k, r in bins.items()},
        "all": total.result(),
        "inputs": {m: {"all": v["all"].result(), "bins": {k: r.result() for k, r in v["bins"].items()}}
                   for m, v in by_input.items()},
    }
    if breakdown:
        out["by_type"] = {k: by_type[k].result() for k in sorted(by_type)}
        out["by_style"] = {k: by_style[k].result() for k in sorted(by_style)}
        out["hard"] = {k: by_hard[k].result() for k in sorted(by_hard)}
        out["scenarios"] = {k: {**r.result(), "too_small": r.windows < MIN_WINDOWS} for k, r in by_scene.items()}
    if parts:
        out["parts"] = {k: r.result() for k, r in by_part.items()}
    if traj:
        out["trajectory"] = {k: r.result() for k, r in traj.items()}
    if posed:
        out["posed"] = {"all": posed_rows["all"].result(),
                        "bins": {k: r.result() for k, r in posed_rows["bins"].items()}}
    out["score"] = score(out)
    return out


def score(ev: dict) -> float | None:
    """One number to pick a checkpoint by, lower is better: the mean of the model's share of the
    interpolation's error over :data:`SCORE` (inputs the evaluation didn't run are skipped)."""
    parts = []
    for mode, metric in SCORE:
        row = ev.get("inputs", {}).get(mode, {}).get("all", {})
        m, i = row.get("model", {}).get(metric), row.get("interp", {}).get(metric)
        if m is not None and i:
            parts.append(m / i)
    return _r(sum(parts) / len(parts)) if parts else None


# ---- contact sheets -----------------------------------------------------------------------------

SHEET_GAP = 30
SHEET_FILLS = 6


def ac_binary() -> pathlib.Path | None:
    """The headless app (``ac``) to draw sheets with: a release build, else a debug one, else PATH."""
    for p in (REPO / "target" / "release" / "ac.exe", REPO / "target" / "release" / "ac",
              REPO / "target" / "debug" / "ac.exe", REPO / "target" / "debug" / "ac"):
        if p.is_file():
            return p
    found = shutil.which("ac")
    return pathlib.Path(found) if found else None


@torch.no_grad()
def sheets(model, sets, body: Body, device, out: pathlib.Path, ac: pathlib.Path | None = None, seed: int = 7) -> list[str]:
    """Contact sheets (physics off) of the same hard fills for every model: the recording, the
    model's fill and the plain interpolation, one PNG each, under ``out``. Returns the files."""
    from .clip import Clip, Key
    from .rig import Skeleton
    from .train import prepare

    ac = ac or ac_binary()
    if ac is None:
        raise SystemExit("no ac binary to draw the sheets with: cargo build --release -p ac-cli")
    skel = Skeleton.load()
    idx = [skel.index[n] for n in BONES]
    win = window_of(model)
    g = SHEET_GAP
    a, b, _ = layout(g, win["after"])
    gen = torch.Generator(device=device).manual_seed(seed)
    q, root, style, _ = sets.batch(64, g, gen)
    truth_f, interp, mask, ttg, _, _ = prepare(q, root, g, body, device, **win)
    tp = Pose(truth_f, body)
    order = hard(tp, body, a, b)["either"].float().argsort(descending=True, stable=True)[:SHEET_FILLS]
    pred = _forward(model, truth_f, interp, mask, ttg, style)
    out.mkdir(parents=True, exist_ok=True)
    frames = [int(x) for x in np.linspace(a, b, 8).round()]
    files = []
    from .model import mat_to_quat

    for k, w in enumerate(order.tolist()):
        for what, f in (("truth", truth_f), ("model", pred), ("interp", interp)):
            R, P = unfeatures(f[w : w + 1], body.hip_height)
            rots = np.tile([0.0, 0.0, 0.0, 1.0], (R.shape[1], len(skel.names), 1))
            rots[:, idx] = mat_to_quat(R[0]).double().cpu().numpy()
            offs = (P[0] / body.hip_height).double().cpu().numpy()
            clip = Clip(bones=list(skel.names), name=f"eval {k} {what}",
                        keys=[Key(i, rots[i], offs[i], interp="linear") for i in range(len(rots))])
            clip.physics = False
            path = out / f"{k}-{what}.clip.json"
            clip.save(path)
            png = out / f"{k}-{what}.png"
            script = out / f"{k}-{what}.script.json"
            script.write_text(json.dumps([
                {"command": "clip.import", "params": {"path": str(path)}},
                {"command": "physics.set", "params": {"on": False}},
                {"command": "clip.sheet", "params": {"path": str(png), "frames": frames, "view": "side", "width": 200, "height": 250}},
            ]), encoding="utf-8")
            r = subprocess.run([str(ac), "script", str(script)], capture_output=True, text=True)
            if r.returncode != 0:
                raise SystemExit(f"ac could not draw {png.name}: {r.stderr.strip() or r.stdout.strip()}")
            files.append(str(png))
    return files


# ---- the command --------------------------------------------------------------------------------

def report(ev: dict) -> str:
    """A short table: per bin and per input, the model's numbers beside the interpolation's."""
    lines = [f"{'':14}{'joint mm':>18}{'seam A m/s':>16}{'seam B m/s':>16}{'slide m/s':>16}{'impossible/1000':>20}"]

    def row(name, r):
        m, i = r.get("model", {}), r.get("interp", {})
        cell = lambda k, s=1.0: f"{m.get(k, float('nan')) * s:7.3g} / {i.get(k, float('nan')) * s:<7.3g}"  # noqa: E731
        return f"{name:14}{cell('joint_m', 1000):>18}{cell('seam_a_ms'):>16}{cell('seam_b_ms'):>16}{cell('slide_ms'):>16}{cell('impossible_per_1000'):>20}"

    for k, r in ev["bins"].items():
        lines.append(row(f"gap {k}", r))
    lines.append(row("all", ev["all"]))
    for k, r in ev["inputs"].items():
        lines.append(row(f"input {k}", r["all"]))
    for k, r in ev.get("hard", {}).items():
        lines.append(row(f"hard {k}", r))
    for k, r in ev.get("parts", {}).items():
        lines.append(row(f"part {k}", r))
    if "scenarios" in ev:
        lines.append(f"{'scenarios':14}{'windows':>9}{'hip speed m/s':>18}{'flight mm':>18}{'landing mm':>18}{'foot place mm':>18}")

        def scene_row(name, r):
            m, i = r.get("model", {}), r.get("interp", {})
            cell = lambda k, s=1.0: f"{m.get(k, float('nan')) * s:7.3g} / {i.get(k, float('nan')) * s:<7.3g}"  # noqa: E731
            small = "  too few windows to trust" if r["too_small"] else ""
            return (f"{name:14}{r['windows']:>9}{cell('hip_speed_ms'):>18}{cell('flight_root_m', 1000):>18}"
                    f"{cell('landing_m', 1000):>18}{cell('foot_place_m', 1000):>18}{small}")

        for k, r in ev["scenarios"].items():
            lines.append(scene_row(f"  {k}", r))
    for k, r in ev.get("trajectory", {}).items():
        lines.append(row(f"path {k}", r))
    if "posed" in ev:
        lines.append(f"{'posed keys':14}{'impossible/1000':>20}{'floor mm':>18}{'jerk m/s3':>18}")

        def posed_row(name, r):
            m, i = r.get("model", {}), r.get("interp", {})
            cell = lambda k, s=1.0: f"{m.get(k, float('nan')) * s:7.3g} / {i.get(k, float('nan')) * s:<7.3g}"  # noqa: E731
            return f"{name:14}{cell('impossible_per_1000'):>20}{cell('floor_m', 1000):>18}{cell('jerk_ms3'):>18}"

        for k, r in ev["posed"]["bins"].items():
            lines.append(posed_row(f"  gap {k}", r))
        lines.append(posed_row("  all", ev["posed"]["all"]))
    lines.append(f"score {ev.get('score')} (model / interpolation, lower is better)")
    return "\n".join(lines)


def main(argv=None):
    from .datasets import REGISTRY
    from .rig import Skeleton
    from .train import Sets

    ap = argparse.ArgumentParser(description="Evaluate a trained in-betweener and write the numbers into its card")
    ap.add_argument("--tag", default="parts", help="training/data/models/<tag>")
    ap.add_argument("--checkpoint", default="model", choices=("model", "best"), help="model.pt (the last) or best.pt")
    ap.add_argument("--sets", default=None, help="comma list of sets (default: every set the registry allows)")
    ap.add_argument("--windows", type=int, default=512, help="windows per gap bin")
    ap.add_argument("--sheets", action="store_true", help="also draw contact sheets of fixed hard fills (needs ac)")
    ap.add_argument("--decisions", default=str(review.DECISIONS), help="the review's decisions file")
    ap.add_argument("--no-write", action="store_true", help="print only; leave the card as it is")
    ap.add_argument("--subset", default=None, metavar="NAME",
                    help="with a take filter: write the numbers under evaluation[_best]_<NAME> in the card, "
                         "so the full evaluation stays (e.g. natural, roadmap 153000)")
    selection.add_arguments(ap)
    args = ap.parse_args(argv)

    from . import infer

    select = selection.from_args(args)
    if select.active() and not args.subset:
        ap.error("a take filter (--types, --styles, --natural, --takes) needs --subset <name>: the numbers of a restricted "
                 "evaluation go under their own key in the card, never over the full one")
    if args.subset and not select.active():
        ap.error("--subset names a restricted evaluation: give the filter too (--natural, --types, --styles, --takes)")
    net, card = infer.load(args.tag, args.checkpoint)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    net = net.to(device)
    names = [n for n in args.sets.split(",") if n] if args.sets else [n for n in REGISTRY if REGISTRY[n].commercial]
    skel = Skeleton.load()
    body = Body(skel, device)
    print(f"takes: {select.describe()}", flush=True)
    val = Sets(names, device, val=True, decisions=review.load_decisions(pathlib.Path(args.decisions)), select=select)
    print(selection.table(val.records, select), flush=True)
    styles = card.get("styles") or ["any"]
    val.learn(styles, val.label_frames)
    ev = run(net, val, body, device, windows=args.windows)
    ev["checkpoint"] = args.checkpoint
    ev["holdout"] = val.held
    ev["sets"] = val.used
    ev["selection"] = select.describe()
    ev["takes_kept"] = sum(r.kept and r.held for r in val.records)
    seen = [n for n in val.used if n in {s["name"] for s in card.get("sets", [])}]
    if card.get("split") != "subjects" and seen:
        ev["note"] = (f"this model was trained with a per-take split: it may have seen takes of these held-out subjects "
                      f"in {', '.join(seen)}, which flatters it there")
    print(report(ev))
    if args.sheets:
        ev["sheets"] = sheets(net, val, body, device, MODELS / args.tag / f"sheets-{args.checkpoint}")
        print(f"sheets: {len(ev['sheets'])} in {MODELS / args.tag / f'sheets-{args.checkpoint}'}")
    if not args.no_write:
        path = MODELS / args.tag / "model.json"
        card = json.loads(path.read_text(encoding="utf-8"))
        key = ("evaluation" if args.checkpoint == "model" else "evaluation_best") + (f"_{args.subset}" if args.subset else "")
        card[key] = ev
        path.write_text(json.dumps(card, indent=1), encoding="utf-8")
        print(f"written into {path} as {key}")


if __name__ == "__main__":
    main()
