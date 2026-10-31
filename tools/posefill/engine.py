"""The app's pose-to-pose engine: load the upstream motion model once (a TensorRT plan, built on
the first run), fill between the app's key poses, and bring the motion back onto the app's rig.
Runs inside the install's venv (tools/posefill/setup_posefill.py), beside the `ac_rig` package
the setup copies from training/ac_train (rig, quat, retarget) and the upstream package in
`upstream/`.

The model takes 4 recent poses (the context, ending on key A) and 1 to 4 target poses (starting
on key B) on its own 30-joint body, and makes 18 to 58 frames at 30 fps that start on the last
context pose and end on the first target. Poses cross over in both directions:

- rig -> model: each model joint takes the world rotation of its rig bone, carried over as the
  bone's turn from its rest (both rests are a standing pose, so a constant per-joint frame change
  K lines the two rests up, lateral axis to lateral axis); the model body keeps its own bone
  lengths, and its hips are placed so its two hip joints' midpoint lands on the rig's.
- model -> rig: the model body's joint positions become landmarks and `ac_rig.retarget` puts the
  rig on them (the same path as motion from text), then each end is blended exactly onto its key.

Axes: the model is Y up, metres; the rig is Z up and faces -Y, so a model point (x, y, z) is the
rig's (x, -z, y). The functions marked "shared with tools/textmotion/engine.py" are copies for
now; the roadmap lists them for factoring out. Live motion control (`live_start`, `live_step`,
roadmap 158000) keeps its session in `live.py` and uses `generate` and the conversions here.
"""

from __future__ import annotations

import math
import pathlib
import subprocess
import sys
import threading
import time

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))
UPSTREAM = HERE / "upstream"
if UPSTREAM.exists() and str(UPSTREAM) not in sys.path:
    sys.path.insert(0, str(UPSTREAM))

from ac_rig import quat as Q  # noqa: E402
from ac_rig.retarget import retarget, scale_to  # noqa: E402
from ac_rig.rig import Skeleton, mat_to_quat, quat_to_mat  # noqa: E402

MODEL = "mmm_v6_soma_fp32"
FPS = 30.0
CONTEXT = 4  # poses before and on key A
MAX_TARGETS = 4  # poses on and after key B
POSES_PER_TOKEN = 4
MIN_TOKENS, MAX_TOKENS = 6, 16
BOUNDARY = 3  # frames the engine trims at each end
MIN_FRAMES, MAX_FRAMES = MIN_TOKENS * 4 - 2 * BOUNDARY, MAX_TOKENS * 4 - 2 * BOUNDARY  # 18, 58
CACHE = HERE / "trt-cache"
WARP_CACHE = HERE / "warp-cache"

# -- shared with tools/textmotion/engine.py (copied; to factor out) -------------------------------

# landmark -> model joint ("Leg" is the thigh, "Shin" the knee)
SOMA = {"hips": "Hips", "neck": "Neck1", "head": "Head",
        "l_shoulder": "LeftArm", "l_elbow": "LeftForeArm", "l_wrist": "LeftHand",
        "l_hip": "LeftLeg", "l_knee": "LeftShin", "l_ankle": "LeftFoot", "l_toe": "LeftToeBase",
        "r_shoulder": "RightArm", "r_elbow": "RightForeArm", "r_wrist": "RightHand",
        "r_hip": "RightLeg", "r_knee": "RightShin", "r_ankle": "RightFoot", "r_toe": "RightToeBase"}
# landmark -> the rig bone whose head is that point
RIG = {"hips": "root", "neck": "neck01", "head": "head",
       "l_shoulder": "upperarm01.L", "l_elbow": "lowerarm01.L", "l_wrist": "wrist.L",
       "l_hip": "upperleg01.L", "l_knee": "lowerleg01.L", "l_ankle": "foot.L", "l_toe": "toe3-1.L",
       "r_shoulder": "upperarm01.R", "r_elbow": "lowerarm01.R", "r_wrist": "wrist.R",
       "r_hip": "upperleg01.R", "r_knee": "lowerleg01.R", "r_ankle": "foot.R", "r_toe": "toe3-1.R"}


def soma_to_rig(p: np.ndarray) -> np.ndarray:
    return np.stack([p[..., 0], -p[..., 2], p[..., 1]], -1)


def rig_to_soma(p: np.ndarray) -> np.ndarray:
    return np.stack([p[..., 0], p[..., 2], -p[..., 1]], -1)


def rig_landmarks(skel: Skeleton, rotations: np.ndarray, root: np.ndarray) -> dict[str, np.ndarray]:
    """One rig pose's landmark positions (rig world, metres)."""
    _, P = skel.fk(np.asarray(rotations, dtype=np.float64), np.asarray(root, dtype=np.float64))
    return {k: P[skel.index[b]] for k, b in RIG.items()}


def resample(x: np.ndarray, fps: float, to: float) -> np.ndarray:
    """Linear resampling along the first axis; the last frame is kept."""
    if abs(fps - to) < 1e-6 or len(x) < 2:
        return x
    n = int(round((len(x) - 1) * to / fps)) + 1
    return _fit_length(x, n)


def _smooth(t: np.ndarray) -> np.ndarray:
    return t * t * (3.0 - 2.0 * t)


def blend_ends(rots: np.ndarray, roots: np.ndarray, start=None, end=None):
    """Bend the motion so its first and last frames are exactly the given poses: the difference at
    each end fades out smoothly over the clip."""
    rots, roots = rots.copy(), roots.copy()
    n = len(rots)
    t = _smooth(np.linspace(0.0, 1.0, n)) if n > 1 else np.zeros(1)
    for pose, w in ((start, 1.0 - t), (end, t)):
        if pose is None:
            continue
        q, o = np.asarray(pose[0], dtype=np.float64), np.asarray(pose[1], dtype=np.float64)
        ref = rots[0] if pose is start else rots[-1]
        delta = Q.normalize(Q.mul(Q.align(q, ref), Q.conj(ref)))  # (B, 4): ref -> pose
        for f in range(n):
            d = Q.slerp(np.broadcast_to(Q.IDENTITY, delta.shape), delta, np.full(len(delta), w[f]))
            rots[f] = Q.normalize(Q.mul(d, rots[f]))
        roots += w[:, None] * (o - (roots[0] if pose is start else roots[-1]))[None]
    return rots, roots


def _leg(skel: Skeleton) -> float:
    H, i = skel.heads, skel.index
    return float(np.linalg.norm(H[i["upperleg01.L"]] - H[i["lowerleg01.L"]])
                 + np.linalg.norm(H[i["lowerleg01.L"]] - H[i["foot.L"]]))


def _fit_length(x: np.ndarray, n: int) -> np.ndarray:
    if n == len(x):
        return x
    t = np.linspace(0.0, len(x) - 1, n)
    i = np.minimum(np.floor(t).astype(int), len(x) - 2)
    a = (t - i).reshape((-1,) + (1,) * (x.ndim - 1))
    return x[i] * (1 - a) + x[i + 1] * a


# -- rig <-> model body ---------------------------------------------------------------------------

# rig point -> model point, as a matrix: (x, y, z) -> (x, z, -y)
C = np.array([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]])
# (model joint, rig bone, model child giving the bone's direction, rig child giving the bone's
# direction); None: the direction coming in from the parent
TRANSFER = [("Hips", "root", "Spine1", "spine04"),
            ("Spine1", "spine05", "Spine2", "spine04"),
            ("Spine2", "spine03", "Chest", "spine02"),
            ("Chest", "spine01", "Neck1", "neck01"),
            ("Neck1", "neck01", "Neck2", "neck02"),
            ("Neck2", "neck03", "Head", "head"),
            ("Head", "head", None, None)]
for _S, _s in (("Left", "L"), ("Right", "R")):
    TRANSFER += [(f"{_S}Shoulder", f"clavicle.{_s}", f"{_S}Arm", f"upperarm01.{_s}"),
                 (f"{_S}Arm", f"upperarm01.{_s}", f"{_S}ForeArm", f"lowerarm01.{_s}"),
                 (f"{_S}ForeArm", f"lowerarm01.{_s}", f"{_S}Hand", f"wrist.{_s}"),
                 (f"{_S}Hand", f"wrist.{_s}", f"{_S}HandMiddleEnd", f"metacarpal3.{_s}"),
                 (f"{_S}Leg", f"upperleg01.{_s}", f"{_S}Shin", f"lowerleg01.{_s}"),
                 (f"{_S}Shin", f"lowerleg01.{_s}", f"{_S}Foot", f"foot.{_s}"),
                 (f"{_S}Foot", f"foot.{_s}", f"{_S}ToeBase", f"toe3-1.{_s}"),
                 (f"{_S}ToeBase", f"toe3-1.{_s}", None, None)]


def _unit(v: np.ndarray) -> np.ndarray:
    return v / max(float(np.linalg.norm(v)), 1e-12)


def _frame(primary: np.ndarray, secondary: np.ndarray) -> np.ndarray:
    """A right-handed frame (columns) with x along ``primary`` and y in its plane with ``secondary``."""
    x = _unit(primary)
    z = _unit(np.cross(x, secondary))
    return np.stack([x, np.cross(z, x), z], -1)


def _rotate(q: np.ndarray, v: np.ndarray) -> np.ndarray:
    """Vectors (..., 3) turned by quaternions (..., 4)."""
    return np.einsum("...ij,...j->...i", quat_to_mat(q), v)


class Body:
    """The model's skeleton (joint names, parents, reference pose) and the maps to the rig."""

    def __init__(self, names: list[str], parents: np.ndarray, ref_local: np.ndarray):
        self.names, self.parents = list(names), np.asarray(parents, dtype=int)
        self.index = {n: i for i, n in enumerate(self.names)}
        self.J = len(names)
        self.ref_local = np.asarray(ref_local, dtype=np.float64)  # (J, 7): position, quaternion x y z w
        # parents first
        order, seen = [], set()
        pending = list(range(self.J))
        while pending:
            rest = []
            for j in pending:
                p = int(self.parents[j])
                if p < 0 or p in seen:
                    order.append(j)
                    seen.add(j)
                else:
                    rest.append(j)
            if len(rest) == len(pending):
                raise ValueError("the model skeleton's parents form a loop")
            pending = rest
        self.order = order
        self.ref_pos, self.ref_rot = self.fk(self.ref_local[None])
        self.ref_pos, self.ref_rot = self.ref_pos[0], quat_to_mat(self.ref_rot[0])  # (J, 3), (J, 3, 3)
        for k, j in SOMA.items():
            if j not in self.index:
                raise ValueError(f"the model skeleton has no joint {j} (for {k})")
        i = self.index
        self.leg = float(np.linalg.norm(self.ref_pos[i["LeftLeg"]] - self.ref_pos[i["LeftShin"]])
                         + np.linalg.norm(self.ref_pos[i["LeftShin"]] - self.ref_pos[i["LeftFoot"]]))
        self.lateral = _unit(self.ref_pos[i["LeftLeg"]] - self.ref_pos[i["RightLeg"]])
        mid = (self.ref_pos[i["LeftLeg"]] + self.ref_pos[i["RightLeg"]]) / 2
        self.hips_to_mid = self.ref_rot[0].T @ (mid - self.ref_pos[0])
        self.rigs: dict[str, tuple[Skeleton, list]] = {}

    def fk(self, local: np.ndarray):
        """World positions (F, J, 3) and rotations (F, J, 4) of local transforms (F, J, 7)."""
        F = local.shape[0]
        P, R = np.zeros((F, self.J, 3)), np.zeros((F, self.J, 4))
        for j in self.order:
            p = int(self.parents[j])
            if p < 0:
                P[:, j], R[:, j] = local[:, j, :3], local[:, j, 3:]
            else:
                R[:, j] = Q.normalize(Q.mul(R[:, p], local[:, j, 3:]))
                P[:, j] = P[:, p] + _rotate(R[:, p], local[:, j, :3])
        return P, R

    def direction(self, j: int, child: str | None) -> np.ndarray:
        if child is not None and child in self.index:
            return self.ref_pos[self.index[child]] - self.ref_pos[j]
        return self.ref_pos[j] - self.ref_pos[int(self.parents[j])]

    def rig(self, path: str, skel: Skeleton | None = None):
        """The rig at ``path`` and its transfer table [(model joint, rig bone, K)]."""
        if path in self.rigs:
            return self.rigs[path]
        skel = skel or Skeleton.load(path)
        H, idx = skel.heads, skel.index
        lateral_rig = C @ _unit(H[idx["upperleg01.L"]] - H[idx["upperleg01.R"]])
        up = np.array([0.0, 1.0, 0.0])
        table = []
        for joint, bone, child, rig_child in TRANSFER:
            if joint not in self.index or bone not in idx:
                continue
            j, b = self.index[joint], idx[bone]
            d_model = self.direction(j, child)
            if rig_child is not None and rig_child in idx:
                d_rig = C @ (H[idx[rig_child]] - H[b])
            else:
                d_rig = C @ (H[b] - H[int(skel.parents[b])])
            # the secondary axis: the body's lateral line, or up for a bone that lies along it
            along = abs(float(np.dot(_unit(d_model), self.lateral))) > 0.7
            sec_model, sec_rig = (up, up) if along else (self.lateral, lateral_rig)
            K = _frame(d_rig, sec_rig) @ _frame(d_model, sec_model).T
            table.append((j, b, K))
        self.rigs[path] = (skel, table)
        return self.rigs[path]

    def from_rig(self, skel: Skeleton, table: list, rotations: np.ndarray, root: np.ndarray, scale: float) -> np.ndarray:
        """One rig pose as the model body's local transforms (J, 7), model axes and metres."""
        R, P = skel.fk(np.asarray(rotations, dtype=np.float64), np.asarray(root, dtype=np.float64))
        W = np.zeros((self.J, 3, 3))
        mapped = {}
        for j, b, K in table:
            mapped[j] = C @ R[b] @ C.T @ K @ self.ref_rot[j]
        local_rot = quat_to_mat(self.ref_local[:, 3:])
        for j in self.order:
            p = int(self.parents[j])
            if j in mapped:
                W[j] = mapped[j]
            elif p < 0:
                W[j] = self.ref_rot[j]
            else:
                W[j] = W[p] @ local_rot[j]
        out = self.ref_local.copy()
        for j in self.order:
            p = int(self.parents[j])
            out[j, 3:] = mat_to_quat(W[j] if p < 0 else W[p].T @ W[j])
        i = skel.index
        mid = C @ ((P[i["upperleg01.L"]] + P[i["upperleg01.R"]]) / 2) * scale
        out[0, :3] = mid - W[0] @ self.hips_to_mid
        return out

    def landmarks(self, local: np.ndarray) -> dict[str, np.ndarray]:
        """Model poses (F, J, 7) -> landmark positions (F, 3) in the rig's axes (model metres)."""
        P, _ = self.fk(local)
        return {k: soma_to_rig(P[:, self.index[j]]) for k, j in SOMA.items()}


# -- the engine -----------------------------------------------------------------------------------

def gpu_name() -> str | None:
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=name", "--format=csv,noheader"],
                             capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return out.strip().splitlines()[0].strip() if out.strip() else None


def plan_built() -> bool:
    return CACHE.is_dir() and any(CACHE.glob("*.plan"))


def describe() -> dict:
    """What `info` says without loading the model."""
    versions = {}
    for mod in ("tensorrt", "warp", "numpy"):
        try:
            versions[mod] = __import__(mod).__version__
        except Exception as e:  # noqa: BLE001 - reported, not hidden
            versions[mod] = f"not importable: {type(e).__name__}: {e}"
    return {"model": MODEL, "fps": FPS, "min_frames": MIN_FRAMES, "max_frames": MAX_FRAMES, "context": CONTEXT,
            "max_targets": MAX_TARGETS, "plan": "built" if plan_built() else "not built (the first fill builds it)",
            "gpu": gpu_name(), "versions": versions}


class Engine:
    """The model, loaded once: the first load builds the TensorRT plan (minutes), later ones read it."""

    def __init__(self, say=print):
        import warp as wp

        self.wp = wp
        wp.config.quiet = True
        WARP_CACHE.mkdir(parents=True, exist_ok=True)
        wp.config.kernel_cache_dir = str(WARP_CACHE)
        wp.init()
        device = wp.get_device("cuda:0")
        if not device.is_cuda:
            raise RuntimeError("no CUDA device: the engine runs on an NVIDIA GPU only")
        wp.set_device("cuda:0")
        self.device = device
        self.gpu = device.name

        from motionbricks.inference.progress import TensorRTBuildProgressMonitor
        from motionbricks.io.usd import load_skeleton_from_usd
        from motionbricks.io.utils import SOMA_SKELETON_PRIM_PATH, get_asset_file
        from motionbricks.model.soma.contract import SOMA_N1_FP32_ONNX_PATH
        from motionbricks.model.soma.engine import MotionBricksSOMAEngine

        skeleton = load_skeleton_from_usd(get_asset_file("human", "soma_base_skel_minimal_no_root.usd"), SOMA_SKELETON_PRIM_PATH)
        ref = skeleton.reference_local_transforms.numpy().astype(np.float64)
        self.body = Body(skeleton.joint_names, skeleton._np_parent_indices, ref)
        self.skeleton = skeleton

        had_plan = plan_built()
        say("loading the TensorRT plan" if had_plan else "building the TensorRT plan for this GPU (a few minutes, once)")
        monitor = TensorRTBuildProgressMonitor()
        building = threading.Event()
        last = [""]

        def watch() -> None:
            while not building.wait(2.0):
                s = monitor.snapshot()
                if s.phase_name:
                    msg = f"building the TensorRT plan: {s.phase_name} {s.fraction * 100:.0f}%"
                    if msg != last[0]:
                        last[0] = msg
                        say(msg)

        t0 = time.perf_counter()
        threading.Thread(target=watch, daemon=True).start()
        try:
            self.engine = MotionBricksSOMAEngine(SOMA_N1_FP32_ONNX_PATH, cache_dir=CACHE, build_progress_monitor=monitor)
        finally:
            building.set()
        self.load_seconds = time.perf_counter() - t0
        self.plan_was_built = not had_plan
        self.context = self.engine.create_input(skeleton)
        self.target = self.engine.create_input(skeleton)
        self.output = self.engine.create_output(skeleton)
        say(f"ready ({'plan built' if self.plan_was_built else 'plan loaded'} in {self.load_seconds:.0f} s on {self.gpu})")

    def rig(self, path: str) -> Skeleton:
        return self.body.rig(path)[0]

    @staticmethod
    def leg_of(skel: Skeleton) -> float:
        return _leg(skel)

    def generate(self, ctx: np.ndarray, tgt: np.ndarray, tokens: tuple, say=print):
        """One generation: context poses (4, J, 7) and target poses (1-4, J, 7) in the model's layout
        and metres -> (poses (F, J, 7), foot contacts (F, 4), GPU seconds, stage milliseconds). The
        model picks its length from ``tokens`` (frames = 4 x tokens - 6); the first pose is the last
        context pose and the last the first target."""
        wp = self.wp
        from motionbricks.model.config import MotionBricksInferenceConfig

        t0 = time.perf_counter()
        with wp.ScopedStream(self.engine.stream):
            ctx_wp = wp.array(np.asarray(ctx, dtype=np.float32), dtype=wp.transform, device=self.device)
            tgt_wp = wp.array(np.asarray(tgt, dtype=np.float32), dtype=wp.transform, device=self.device)
            self.context.update_from_poses(ctx_wp, stream=self.engine.stream)
            self.target.update_from_poses(tgt_wp, stream=self.engine.stream)
            self.engine.generate_animation(self.context, self.target, self.output,
                                           config=MotionBricksInferenceConfig(allowed_token_counts=tuple(tokens)))
            self.engine.synchronize()
            m = int(self.output.num_frames)
            poses = self.output.poses[:m].numpy().astype(np.float64)
            contacts = self.output.foot_contacts[:m].numpy().astype(np.float64)
        seconds = time.perf_counter() - t0
        stages = {k: round(float(v), 3) for k, v in self.engine.timer.milliseconds.items()}
        if poses.ndim != 3 or poses.shape[1:] != (self.body.J, 7) or m < 2:
            raise ValueError(f"the engine returned {poses.shape} transforms; (frames, {self.body.J}, 7) were expected")
        if not np.isfinite(poses).all():
            raise ValueError("the engine returned non-finite values")
        return poses, contacts, seconds, stages

    # -- live motion control (roadmap 158000) --------------------------------------------------

    def live_start(self, rig: str, targets: dict, start, say=print) -> dict:
        """A new live session from the rig pose ``start`` (rotations, root) aiming at ``targets``
        (a target pose set document). One session at a time: starting again ends the last."""
        from live import Session

        self.live = Session(self, rig, targets, start)
        self.live_id = getattr(self, "live_id", 0) + 1
        say(f"live session {self.live_id}: targets {', '.join(self.live.targets.names)}")
        return {"session": self.live_id, "targets": self.live.targets.names, "max_speed": self.live.targets.max_speed,
                "position": [round(float(x), 4) for x in self.live.position()]}

    def live_rig(self) -> str:
        """The rig the live session was started with (for the helper's check of a step's frames)."""
        live = getattr(self, "live", None)
        if live is None:
            raise ValueError("no live session (start one with live_start)")
        return live.rig

    def live_step(self, session: int, shown: int, velocity, facing, style, tokens, say=print) -> dict:
        """The next stretch of the live session: frames 1..``shown`` of the last stretch join the
        history, the target set is blended for ``velocity`` (x, y m/s on the ground), placed ahead
        and turned to ``facing``, and the model's answer comes back on the rig."""
        from live import TOKENS

        live = getattr(self, "live", None)
        if live is None or session != getattr(self, "live_id", 0):
            raise ValueError(f"no live session {session} (start one with live_start)")
        return live.next(int(shown), np.asarray(velocity, dtype=np.float64), np.asarray(facing, dtype=np.float64), style,
                         tokens or TOKENS, say=say)

    def info(self) -> dict:
        return {"gpu": self.gpu, "load_seconds": round(self.load_seconds, 2), "plan_was_built": self.plan_was_built,
                "joints": self.body.J}

    def fill(self, context, targets, frames: int, fps: float, rig: str, say=print):
        """Rig rotations (frames, B, 4) and root offsets (frames, 3) at ``fps`` from the last context
        pose to the first target: ``context`` 1 to 4 (rotations, root) poses ending on key A (fewer
        than 4: the first is repeated, and said), ``targets`` 1 to 4 starting on key B."""
        skel, table = self.body.rig(rig)
        frames = int(frames)
        n = int(round((frames - 1) * FPS / fps)) + 1
        if not MIN_FRAMES <= n <= MAX_FRAMES:
            raise ValueError(f"{frames} frames at {fps:g} fps is {n} model frames; the model makes {MIN_FRAMES} to "
                             f"{MAX_FRAMES} ({MIN_FRAMES / FPS:.1f} to {MAX_FRAMES / FPS:.2f} s): a longer gap is filled "
                             "through a key inside it")
        if not 1 <= len(context) <= CONTEXT:
            raise ValueError(f"the context is 1 to {CONTEXT} poses, the last on key A")
        if not 1 <= len(targets) <= MAX_TARGETS:
            raise ValueError(f"the targets are 1 to {MAX_TARGETS} poses, the first on key B")
        notes = []
        if len(context) < CONTEXT:
            notes.append(f"{len(context)} context pose(s) given; the first was repeated to make {CONTEXT}")
            context = [context[0]] * (CONTEXT - len(context)) + list(context)
        tokens = min(MAX_TOKENS, max(MIN_TOKENS, int(round((n + 2 * BOUNDARY) / POSES_PER_TOKEN))))
        valid = tokens * POSES_PER_TOKEN - 2 * BOUNDARY
        scale = self.body.leg / _leg(skel)
        ctx = np.stack([self.body.from_rig(skel, table, r, o, scale) for r, o in context]).astype(np.float32)
        tgt = np.stack([self.body.from_rig(skel, table, r, o, scale) for r, o in targets]).astype(np.float32)

        say(f"generating {valid} model frames ({tokens} tokens)")
        poses, contacts, seconds, stages = self.generate(ctx, tgt, (tokens,), say=say)
        if poses.shape != (valid, self.body.J, 7):
            raise ValueError(f"the engine returned {poses.shape} transforms; {(valid, self.body.J, 7)} were expected")

        src = self.body.landmarks(poses)
        src = scale_to(skel, src)
        rots, roots = retarget(skel, src)
        rots, roots = np.asarray(rots, dtype=np.float64), np.asarray(roots, dtype=np.float64)
        if len(rots) != n:
            notes.append(f"{valid} model frames stretched to {n} ({tokens} tokens of 4 frames, less 6 of boundary)")
            rots, roots = _fit_length(rots, n), _fit_length(roots, n)
        rots, roots = resample(rots, FPS, fps), resample(roots, FPS, fps)
        if len(rots) != frames:
            rots, roots = _fit_length(rots, frames), _fit_length(roots, frames)
        rots = Q.normalize(rots)
        start = (np.asarray(context[-1][0], dtype=np.float64), np.asarray(context[-1][1], dtype=np.float64))
        end = (np.asarray(targets[0][0], dtype=np.float64), np.asarray(targets[0][1], dtype=np.float64))
        rots, roots = blend_ends(rots, roots, start, end)
        if not (np.isfinite(rots).all() and np.isfinite(roots).all()):
            raise ValueError("the retarget returned non-finite values")
        return {"rotations": rots, "roots": roots, "notes": notes, "model_frames": valid, "tokens": tokens,
                "seconds": round(seconds, 4), "stages_ms": stages, "gpu": self.gpu,
                "contacts": _fit_length(contacts, frames).round(3)}


def is_finite_pose(rotations, root) -> bool:
    return all(math.isfinite(x) for q in rotations for x in q) and all(math.isfinite(x) for x in root)
