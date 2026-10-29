"""Shared by helper.py (the app's helper) and generate.py (training clips): load Kimodo once,
generate from a prompt (optionally held to the app's key poses), and bring the motion onto the
app's rig. Runs inside the Kimodo install's venv (tools/textmotion/setup_textmotion.py), beside the
`ac_rig` package the setup copies from training/ac_train (rig, quat, retarget).

Axes: Kimodo is Y up, metres, a body facing +Z at heading 0; the rig is Z up and faces -Y, so a
Kimodo point (x, y, z) is the rig's (x, -z, y), and heading 0 is the rig's rest facing.
"""

from __future__ import annotations

import json
import math
import os
import pathlib
import sys

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))
# the install's own settings (model cache, local text encoder), unless the caller set them
if (HERE / "install.json").exists():
    for _k, _v in json.loads((HERE / "install.json").read_text(encoding="utf-8")).get("env", {}).items():
        os.environ.setdefault(_k, str(_v))

from ac_rig import quat as Q  # noqa: E402
from ac_rig.retarget import retarget, scale_to  # noqa: E402
from ac_rig.rig import Skeleton  # noqa: E402

MODEL = "Kimodo-SOMA-RP-v1.2"
FPS = 30.0
MAX_FRAMES = 300  # the model's limit: 10 s a prompt
STEPS = 100  # denoising steps (the model's default)

# landmark -> SOMA joint (the same map as training's `kimodo` set: "Leg" is the thigh, "Shin" the knee)
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
REST_YAW = -math.pi / 2  # the rig's rest facing (-Y) as an angle in the XY plane


def soma_to_rig(p: np.ndarray) -> np.ndarray:
    return np.stack([p[..., 0], -p[..., 2], p[..., 1]], -1)


def rig_to_soma(p: np.ndarray) -> np.ndarray:
    return np.stack([p[..., 0], p[..., 2], -p[..., 1]], -1)


def facing(lm: dict[str, np.ndarray]) -> np.ndarray:
    """Facing angle(s) in the rig's XY plane: forward is cross(left - right, Z) over the hip and
    shoulder lines; the rig's rest gives -pi/2."""
    across = (lm["l_hip"] - lm["r_hip"]) + (lm["l_shoulder"] - lm["r_shoulder"])
    return np.arctan2(-across[..., 0], across[..., 1])


def turn(lm: dict[str, np.ndarray], angle: float, about: np.ndarray, to: np.ndarray) -> dict[str, np.ndarray]:
    """Landmarks turned by ``angle`` about Z around ``about`` (XY), then moved so ``about`` lands on ``to``."""
    c, s = math.cos(angle), math.sin(angle)
    out = {}
    for k, v in lm.items():
        x, y = v[..., 0] - about[0], v[..., 1] - about[1]
        out[k] = np.stack([c * x - s * y + to[0], s * x + c * y + to[1], v[..., 2]], -1)
    return out


def rig_landmarks(skel: Skeleton, rotations: np.ndarray, root: np.ndarray) -> dict[str, np.ndarray]:
    """One rig pose's landmark positions (rig world, metres)."""
    _, P = skel.fk(np.asarray(rotations, dtype=np.float64), np.asarray(root, dtype=np.float64))
    return {k: P[skel.index[b]] for k, b in RIG.items()}


def resample(x: np.ndarray, fps: float, to: float) -> np.ndarray:
    """Linear resampling along the first axis; the last frame is kept."""
    if abs(fps - to) < 1e-6 or len(x) < 2:
        return x
    n = int(round((len(x) - 1) * to / fps)) + 1
    t = np.linspace(0.0, len(x) - 1, n)
    i = np.minimum(np.floor(t).astype(int), len(x) - 2)
    a = (t - i).reshape((-1,) + (1,) * (x.ndim - 1))
    return x[i] * (1 - a) + x[i + 1] * a


def _smooth(t: np.ndarray) -> np.ndarray:
    return t * t * (3.0 - 2.0 * t)


def blend_ends(rots: np.ndarray, roots: np.ndarray, start=None, end=None):
    """Bend the motion so its first and last frames are exactly the given poses: the difference at
    each end fades out smoothly over the clip (Kimodo comes close to its constraints, not onto them)."""
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


class Generator:
    """Kimodo, loaded once (the text encoder alone takes about a minute on the CPU)."""

    def __init__(self, say=print):
        import torch

        from kimodo import load_model

        self.torch = torch
        self.device = "cuda:0" if torch.cuda.is_available() else "cpu"
        say(f"loading {MODEL} on {self.device} (the text encoder loads first)")
        self.model = load_model(MODEL, device=self.device, default_family="Kimodo")
        self.skel30 = self.model.skeleton
        self.skel77 = self.skel30.somaskel77.to(self.device)
        self.idx77 = {k: self.skel77.bone_index[j] for k, j in SOMA.items()}
        self.idx30 = {k: self.skel30.bone_index[j] for k, j in SOMA.items()}
        rest = self.skel30.neutral_joints.detach().cpu().numpy()
        i = self.skel30.bone_index
        self.soma_leg = float(np.linalg.norm(rest[i["LeftLeg"]] - rest[i["LeftShin"]])
                              + np.linalg.norm(rest[i["LeftShin"]] - rest[i["LeftFoot"]]))
        self.rigs: dict[str, Skeleton] = {}
        say("ready")

    def rig(self, path: str) -> Skeleton:
        if path not in self.rigs:
            self.rigs[path] = Skeleton.load(path)
        return self.rigs[path]

    # -- SOMA poses for constraints ---------------------------------------------------------

    def _fit(self, targets: np.ndarray, iters: int = 400):
        """SOMA-30 local rotations whose joints land on ``targets`` (K, 17, 3), Kimodo axes,
        metres: Adam on axis-angles through the model's own forward kinematics."""
        torch = self.torch
        dev = self.device
        names = list(SOMA)
        tgt = torch.tensor(targets, dtype=torch.float32, device=dev)
        k = len(targets)
        aa = torch.zeros((k, self.skel30.nbjoints, 3), device=dev, requires_grad=True)
        root = tgt[:, names.index("hips")]
        sel = torch.tensor([self.idx30[n] for n in names], device=dev)
        opt = torch.optim.Adam([aa], lr=0.05)
        for _ in range(iters):
            opt.zero_grad()
            _, pos, _ = self.skel30.fk(_axis_angle_to_matrix(torch, aa), root)
            loss = ((pos[:, sel] - tgt) ** 2).sum() + 1e-4 * (aa ** 2).sum()
            loss.backward()
            opt.step()
        with torch.no_grad():
            grot, pos, _ = self.skel30.fk(_axis_angle_to_matrix(torch, aa), root)
            err = (pos[:, sel] - tgt).norm(dim=-1).max().item()
        return pos, grot, err

    # -- generation ----------------------------------------------------------------------------

    def soma_motion(self, prompt: str, frames: int, seed=None, constraints=None, heading: float = 0.0) -> np.ndarray:
        """Kimodo joint positions (frames, 77, 3), canonical: the root starts at XZ 0."""
        torch = self.torch
        from kimodo.tools import seed_everything

        if seed is not None:
            seed_everything(int(seed))
        out = self.model([prompt], [int(frames)], num_denoising_steps=STEPS, constraint_lst=constraints or [],
                         num_samples=1, multi_prompt=True, post_processing=True, return_numpy=True,
                         first_heading_angle=torch.tensor([heading], device=self.device), progress_bar=_quiet)
        return np.asarray(out["posed_joints"][0], dtype=np.float64)

    def to_rig(self, joints: np.ndarray, skel: Skeleton) -> dict[str, np.ndarray]:
        return {k: soma_to_rig(joints[:, j]) for k, j in self.idx77.items()}

    def generate(self, prompt: str, frames: int, fps: float, rig: str, seed=None, start=None, end=None, say=print):
        """Rig rotations (frames, B, 4) and root offsets (frames, 3) at ``fps``.

        ``start`` / ``end``: (rotations, root) poses the motion begins / ends on: full-body
        constraints on its first / last frame, then each end is blended exactly onto its pose. The
        motion is placed and turned to the start pose (without one: at the origin, rest facing)."""
        torch = self.torch
        skel = self.rig(rig)
        n = int(round((frames - 1) * FPS / fps)) + 1
        if not 2 <= n <= MAX_FRAMES:
            raise ValueError(f"{frames} frames at {fps:g} fps is {n} model frames; the model makes 2 to {MAX_FRAMES}")
        notes = []
        placed = start is not None
        if placed:
            lm0 = rig_landmarks(skel, *start)
            yaw0, at0 = float(facing(lm0)), lm0["hips"][:2].copy()
        else:
            yaw0, at0 = REST_YAW, np.zeros(2)
        constraints = []
        held = [(0, start)] if start is not None else []
        if end is not None:
            held.append((n - 1, end))
        if held:
            from kimodo.constraints import FullBodyConstraintSet

            canon = [turn(rig_landmarks(skel, *pose), REST_YAW - yaw0, at0, np.zeros(2)) for _, pose in held]
            scale = self.soma_leg / _leg(skel)
            names = list(SOMA)
            targets = np.stack([np.stack([rig_to_soma(c[k] * scale) for k in names]) for c in canon])
            say("fitting the key poses")
            pos, grot, err = self._fit(targets)
            if err > 0.05:
                notes.append(f"the key poses fit the model's body to within {err * 100:.0f} cm")
            frames_held = torch.tensor([f for f, _ in held], device=self.device)
            constraints = [FullBodyConstraintSet(self.skel30, frames_held, pos, grot)]
        say("generating")
        joints = self.soma_motion(prompt, n, seed, constraints, heading=0.0)
        src = self.to_rig(joints, skel)
        src = {k: resample(v, FPS, fps) for k, v in src.items()}
        if len(src["hips"]) != frames:  # rounding: keep the frame count the caller asked for
            src = {k: _fit_length(v, frames) for k, v in src.items()}
        src = scale_to(skel, src)
        src = turn(src, yaw0 - REST_YAW, np.zeros(2), at0)
        rots, roots = retarget(skel, src)
        rots, roots = np.asarray(rots, dtype=np.float64), np.asarray(roots, dtype=np.float64)
        rots, roots = blend_ends(rots, roots, start, end)
        return rots, roots, notes


def _leg(skel: Skeleton) -> float:
    H, i = skel.heads, skel.index
    return float(np.linalg.norm(H[i["upperleg01.L"]] - H[i["lowerleg01.L"]])
                 + np.linalg.norm(H[i["lowerleg01.L"]] - H[i["foot.L"]]))


def _fit_length(x: np.ndarray, n: int) -> np.ndarray:
    t = np.linspace(0.0, len(x) - 1, n)
    i = np.minimum(np.floor(t).astype(int), len(x) - 2)
    a = (t - i).reshape((-1,) + (1,) * (x.ndim - 1))
    return x[i] * (1 - a) + x[i + 1] * a


def _axis_angle_to_matrix(torch, aa):
    """Rodrigues' formula, batched; smooth at zero."""
    theta = aa.norm(dim=-1, keepdim=True).clamp_min(1e-8)
    k = aa / theta
    kx, ky, kz = k.unbind(-1)
    zero = torch.zeros_like(kx)
    K = torch.stack([zero, -kz, ky, kz, zero, -kx, -ky, kx, zero], -1).reshape(*aa.shape[:-1], 3, 3)
    eye = torch.eye(3, device=aa.device, dtype=aa.dtype).expand(K.shape)
    s, c = torch.sin(theta)[..., None], torch.cos(theta)[..., None]
    return eye + s * K + (1 - c) * (K @ K)


def _quiet(it, *args, **kwargs):
    return it
