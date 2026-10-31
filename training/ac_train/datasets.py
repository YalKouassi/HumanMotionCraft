"""Motion datasets for the in-betweener: where they come from, their licences, and the readers.

The repo supplies free-licence sets only: every set here allows commercial use. Each carries
``commercial`` all the same, so a set you add yourself under a non-commercial licence is marked:
``python -m ac_train.train`` reads the ``commercial=True`` sets alone (``--noncommercial`` adds
yours, for a model of your own), and each trained model records the sets (and licences) it saw in
its card.

``build(name)`` reads a set from ``training/data/raw`` (download it first, see
``training/README.md``), retargets every take onto the app's rig and writes
``training/data/rig/<name>.npz``: rotations (frames, 104, 4) float16, root offsets (frames, 3),
``starts`` (take boundaries), ``fps``, the take names and ``labels`` (each take's own words: CMU's
description, a 100STYLE style and its description; otherwise the take name), and its build stamp
(:mod:`ac_train.stamp`: the rig and code it was built from).

Take names and labels drive the style labels (:mod:`ac_train.labels`) and the library's clip types
(:func:`ac_train.library.classify`). CMU's files are only numbered, so its take names are built
from the description in the set's index: ``<type>_cmu_<subject>_<trial>``. 100STYLE's are
``<type>_100style_<gait>_<style number>``: the gait gives the type, and the style name stays out of
the take name (a style called "Kick" or "Crouched" is still a walk) and goes into ``labels``.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
from dataclasses import dataclass, field

import numpy as np

from . import ASSETS, DATA, OUT, stamp
from .bvh import load as load_bvh
from .quality import level
from .retarget import retarget, scale_to
from .rig import Skeleton

RAW = DATA / "raw"
RIG = OUT / "rig"
FPS = 30.0


@dataclass
class Dataset:
    name: str
    title: str
    licence: str
    commercial: bool
    url: str
    files: str  # glob under data/raw
    joints: dict[str, str] = field(default_factory=dict)  # landmark -> source joint
    up: str = "y"  # source up axis
    units: float = 0.01  # source units -> metres (scale_to rescales to the rig anyway)
    skip_frames: int = 0  # leading frames that are not motion (an added T-pose)
    note: str = ""
    # may the retargeted set be passed on (``python -m ac_train.publish``)? "yes", or "never" (a set
    # you add whose licence forbids derived copies)
    share: str = "never"
    # a folder of ``.clip.json`` files the app's FBX import wrote (``clip.import_batch``): read as
    # they are, already on the rig (roadmap 152000). ``files`` is then unused; ``folder`` is the
    # folder (absolute, or under data/raw). In-place takes are left out: they teach sliding feet.
    folder: str = ""


_CMU = {"hips": "Hips", "spine": "Spine", "neck": "Neck", "head": "Head",
        "l_shoulder": "LeftArm", "l_elbow": "LeftForeArm", "l_wrist": "LeftHand",
        "l_hip": "LeftUpLeg", "l_knee": "LeftLeg", "l_ankle": "LeftFoot", "l_toe": "LeftToeBase",
        "r_shoulder": "RightArm", "r_elbow": "RightForeArm", "r_wrist": "RightHand",
        "r_hip": "RightUpLeg", "r_knee": "RightLeg", "r_ankle": "RightFoot", "r_toe": "RightToeBase"}
_STYLE100 = {"hips": "Hips", "spine": "Chest", "neck": "Neck", "head": "Head",
         "l_shoulder": "LeftShoulder", "l_elbow": "LeftElbow", "l_wrist": "LeftWrist",
         "l_hip": "LeftHip", "l_knee": "LeftKnee", "l_ankle": "LeftAnkle", "l_toe": "LeftToe",
         "r_shoulder": "RightShoulder", "r_elbow": "RightElbow", "r_wrist": "RightWrist",
         "r_hip": "RightHip", "r_knee": "RightKnee", "r_ankle": "RightAnkle", "r_toe": "RightToe"}
# NVIDIA's SOMA skeleton (Kimodo's BVH): "Leg" is the thigh and "Shin" the knee
_SOMA = {"hips": "Hips", "spine": "Spine1", "neck": "Neck1", "head": "Head",
         "l_shoulder": "LeftArm", "l_elbow": "LeftForeArm", "l_wrist": "LeftHand",
         "l_hip": "LeftLeg", "l_knee": "LeftShin", "l_ankle": "LeftFoot", "l_toe": "LeftToeBase",
         "r_shoulder": "RightArm", "r_elbow": "RightForeArm", "r_wrist": "RightHand",
         "r_hip": "RightLeg", "r_knee": "RightShin", "r_ankle": "RightFoot", "r_toe": "RightToeBase"}
# ACCAD's five character-figure BVHs (walkdog, handspring, quickwalk, sprint, swagger); the 299
# Female1/Male1/Male2 files use _CMU's joint names
_ACCAD_FIG = {"hips": "root", "spine": "lowerback", "neck": "neck", "head": "head",
              "l_shoulder": "lhumerus", "l_elbow": "lradius", "l_wrist": "lhand",
              "l_hip": "lfemur", "l_knee": "ltibia", "l_ankle": "lfoot", "l_toe": "ltoes",
              "r_shoulder": "rhumerus", "r_elbow": "rradius", "r_wrist": "rhand",
              "r_hip": "rfemur", "r_knee": "rtibia", "r_ankle": "rfoot", "r_toe": "rtoes"}

REGISTRY = {
    "builtin_loops": Dataset(
        "builtin_loops", "the built-in CC0 loops (assets/library, origin osb)", "CC0 1.0", True, "assets/library", "",
        note="already on the rig", share="yes"),
    "cmu": Dataset(
        "cmu", "CMU Graphics Lab Motion Capture Database (BVH conversion, 2,548 takes)",
        "free for research and commercial use (mocap.cs.cmu.edu)", True,
        "https://github.com/una-dinosauria/cmu-mocap", "cmu-mocap/data/**/*.bvh", _CMU,
        units=0.0254 / 0.45, skip_frames=1, note="frame 0 of every file is an added T-pose; 120 fps", share="yes"),
    "100style": Dataset(
        "100style", "100STYLE (Ian Mason et al., 100 locomotion styles, 810 takes)", "CC BY 4.0", True,
        "https://zenodo.org/record/8127870", "100STYLE/*/*.bvh", _STYLE100,
        note="60 fps; only the frames inside Frame_Cuts.csv are read (each file starts with setup frames)", share="yes"),
    "accad": Dataset(
        "accad", "ACCAD Open Motion Project (The Ohio State University, 304 takes)", "CC BY 3.0", True,
        "https://accad.osu.edu/research/motion-lab/mocap-system-and-data", "accad/**/*.bvh", _CMU,
        note="30 and 120 fps; Female1, Male1, Male2 and the five character-figure BVHs", share="yes"),
    "addbiomechanics": Dataset(
        "addbiomechanics", "AddBiomechanics Dataset 1.0 (overground With_Arm studies, .b3d)", "CC BY 4.0", True,
        "https://addbiomechanics.org/download_data.html", "addbiomechanics/**/*.b3d",
        units=1.0, note="Rajagopal 2015 OpenSim joint centers and ground-reaction forces (ac_train.b3d)", share="yes"),
    "kimodo": Dataset(
        "kimodo", "Kimodo-generated motion (NVIDIA Kimodo-SOMA-RP-v1.2, one clip per prompt)",
        "generated by a model under the NVIDIA Open Model License, which claims no ownership of outputs", True,
        "https://github.com/nv-tlabs/kimodo", "kimodo/*.bvh", _SOMA,
        note="30 fps SOMA BVH from kimodo_gen --bvh; kimodo/prompts.tsv (name, type, seconds, prompt) "
             "gives each clip its type word and its prompt as the label", share="yes"),
}

# 100STYLE gait codes -> type word
STYLE100_GAITS = {"FW": "walk", "BW": "walk", "FR": "run", "BR": "run", "SW": "sidestep", "SR": "run",
                  "ID": "standing", "TR1": "any", "TR2": "any", "TR3": "any"}
STYLE100_GAIT_WORDS = {"FW": "forwards walk", "BW": "backwards walk", "FR": "forwards run", "BR": "backwards run",
                       "SW": "sideways walk", "SR": "sideways run", "ID": "idle", "TR1": "transitions",
                       "TR2": "transitions", "TR3": "transitions"}

# CMU descriptions -> one type word the style labels and library types understand. First match
# wins, so the specific comes before the general; "skip" drops the take (it needs scenery the rig
# does not have: the body stands on or hangs from something, so the root leaves the floor).
CMU_TYPES = [
    (r"playground|climb|ladder|stair|step ?stool|uneven terrain|monkey ?bar|motorcycle|platform|\bhang|"
     r"onto|on top|balance beam|pushing (a )?swing|swing on", "skip"),
    (r"cartwheel|flip|\bflp\b|handspring|somersault|handstand|break ?dance|breaksequence", "flip"),
    (r"\bfall|faint|collapse", "fallandgetup"),
    (r"boxing|punch|kick|karate|martial|kung ?fu|fight|swordplay|sword|attack|strike", "fight"),
    (r"danc|salsa|charleston|lambada|waltz|ballet|arabesque|pirouette|jete|twist|lindy|foxtrot|tango|cha ?cha|"
     r"chicken|indian", "dance"),
    (r"basketball|dribble|soccer|football|golf|baseball|tennis|bowl|frisbee|throw|catch|volleyball|"
     r"\bswing|putt|\btee\b|ball", "sports"),
    (r"stretch|jumping jacks|squat|exercise|yoga|tai ?chi|calisthenic|pushup|situp", "stretch"),
    (r"jump|hop|leap|skip", "jump"),
    (r"sprint", "sprint"),
    (r"\brun|run\b|jog", "run"),
    (r"crawl|lie down|lying|laying|on the floor|on the ground|from (the )?(floor|ground)|roll", "ground"),
    (r"\bsit|stool|chair|bench", "sit"),
    (r"side ?step|sideways", "sidestep"),
    (r"carry|suitcase|pick up|lift|box\b|moving .*object|push|pull", "carry"),
    (r"walk|march|stroll|stride|navigate|veer|step|sneak|limp", "walk"),
    (r"signal|gesture|talk|convers|pantomime|wave|point|story|rhyme|mime|charade|expression", "gesture"),
    (r"stand|idle|wait|look(ing)? around|pose|balance", "standing"),
]

# ACCAD file stems (Female1_B18_WalkToLeapToWalk, Male2_G4_SpinningBackKick, flip, ...) -> one type word
ACCAD_TYPES = [
    (r"cartwheel|handspring|\bflip\b", "flip"),
    (r"kick|jab|hook|uppercut|backfist|cross(left|right)|block|dodge|duck|taunt|victory|bow|stance|"
     r"ready|advance|retreat|capoeira|roundhouse|crescent|extended|form_\d|_e\d+_bounce|_e\d+_sidestep|"
     r"_e\d+_turnaround", "fight"),
    (r"pickupbox|liftbox|boxlift|withbox|putdownbox", "carry"),
    (r"crawl|lietocrouch|crouchtolie|_lie", "ground"),
    (r"crouch", "crouch"),
    (r"hop|leap|jump", "jump"),
    (r"sidestep", "sidestep"),
    (r"sprint|ericrun", "sprint"),
    (r"run", "run"),
    (r"walk|swagger|eric1|ericdog|skip", "walk"),
    (r"gesture|urban", "gesture"),
    (r"stand|sway|swing|look|wait", "standing"),
]

# AddBiomechanics trial names -> one type word ("skip" drops stairs, treadmill in-place trials, and quadruped)
ADDBIOMECHANICS_TYPES = [
    (r"stair|tmf|treadmill|(?<![a-z])dog(?![a-z])|^$|^_segment_", "skip"),
    (r"crouch", "crouch"),
    (r"jumpingjack|sidestretch|worrier|warrior|stretch|squat|tree", "stretch"),
    (r"(?<![a-z])sts|chair|sit", "sit"),
    (r"(?<![a-z])dj|smalljump|jump|hop", "jump"),
    (r"ballet|lambada|dance|whirl", "dance"),
    (r"taichi", "fight"),
    (r"soccer|tennis|kick", "sports"),
    (r"static|idling|idle", "standing"),
    (r"run|cutting", "run"),
    (r"gait|walk|step|turn|bouncy|medialthrust|smooth|trunksway|^t\d+", "walk"),
]


def cmu_index() -> dict[str, str]:
    """Take stem (``02_01``) -> its description, from the set's index file."""
    p = RAW / "cmu-mocap" / "cmu-mocap-index-text.txt"
    out = {}
    for line in p.read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r"^(\d+_\d+)\t(.*)$", line)
        if m:
            out[m.group(1)] = m.group(2).strip()
    return out


def cmu_take(stem: str, description: str) -> str | None:
    """``<type>_cmu_<stem>`` for a CMU take, or None when it is skipped."""
    d = description.lower()
    for pattern, word in CMU_TYPES:
        if re.search(pattern, d):
            return None if word == "skip" else f"{word}_cmu_{stem}"
    return f"any_cmu_{stem}"


def style100_index() -> dict[str, dict]:
    """Style name -> {"number", "description", "cuts": {gait: (start, stop)}} from the set's CSVs."""
    import csv

    root = RAW / "100STYLE"
    with open(root / "Dataset_List.csv", encoding="utf-8-sig", newline="") as fh:
        styles = [r for r in csv.DictReader(fh)]
    with open(root / "Frame_Cuts.csv", encoding="utf-8-sig", newline="") as fh:
        cuts = {r["STYLE_NAME"]: r for r in csv.DictReader(fh)}
    out = {}
    for i, r in enumerate(styles, 1):
        name = r["Style Name"]
        c = cuts.get(name, {})
        out[name] = {"number": i, "description": r.get("Description", "").strip(),
                     "cuts": {g: (int(c[f"{g}_START"]), int(c[f"{g}_STOP"])) for g in STYLE100_GAITS
                              if c.get(f"{g}_START", "N/A") != "N/A"}}
    return out


def style100_take(stem: str, index: dict[str, dict]) -> tuple[str, str, tuple[int, int]] | None:
    """(take name, label, frame cut) for a 100STYLE file stem (``Aeroplane_FW``); None without a cut."""
    style, _, gait = stem.rpartition("_")
    entry = index.get(style)
    if entry is None or gait not in entry["cuts"]:
        return None
    take = f"{STYLE100_GAITS[gait]}_100style_{gait}_{entry['number']:03d}"
    label = f"{style}: {entry['description']} ({STYLE100_GAIT_WORDS[gait]})"
    return take, label, entry["cuts"][gait]


def accad_take(stem: str) -> tuple[str, str]:
    """(take name, label) for an ACCAD file stem (``Female1_B18_WalkToLeapToWalk``)."""
    label = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", stem).replace("_", " ").strip()
    s = stem.lower()
    for pattern, word in ACCAD_TYPES:
        if re.search(pattern, s):
            return f"{word}_accad_{s}", label
    return f"any_accad_{s}", label


def addbiomechanics_take(study: str, subject: str, trial_name: str) -> tuple[str, str] | None:
    """(take name, label) for a trial in an AddBiomechanics ``.b3d`` file, or None when skipped."""
    t = trial_name.strip().lower()
    if not t or t.startswith("_segment_"):
        return None
    study_short = study.lower().split("_")[0]
    sub = re.sub(r"[^a-z0-9]+", "_", subject.lower()).strip("_")
    slug = re.sub(r"[^a-z0-9]+", "_", t).strip("_")
    label = f"{study_short} {subject}: {trial_name.replace('_', ' ').strip()}"
    for pattern, word in ADDBIOMECHANICS_TYPES:
        if re.search(pattern, t):
            return None if word == "skip" else (f"{word}_addbiomechanics_{study_short}_{sub}_{slug}", label)
    return f"any_addbiomechanics_{study_short}_{sub}_{slug}", label


def kimodo_index() -> dict[str, dict]:
    """Clip name -> {"type", "prompt"} from ``kimodo/prompts.tsv`` (written beside the BVHs)."""
    import csv

    with open(RAW / "kimodo" / "prompts.tsv", encoding="utf-8-sig", newline="") as fh:
        return {r["name"]: {"type": r["type"].strip(), "prompt": r["prompt"].strip()}
                for r in csv.DictReader(fh, delimiter="\t")}


def kimodo_take(stem: str, index: dict[str, dict]) -> tuple[str, str] | None:
    """(take name, label) for a Kimodo clip: ``<type>_kimodo_<name>`` and its prompt; None when the
    clip is not in the index. The type is given, not guessed: "picks up a box from the floor" is a
    carry, though a pattern on the words would call it a floor take."""
    entry = index.get(stem)
    if entry is None or not entry["type"]:
        return None
    return f"{entry['type']}_kimodo_{stem}", entry["prompt"]


def _to_rig_axes(p: np.ndarray, up: str) -> np.ndarray:
    """Source axes -> the rig's (Z up). A proper rotation, so left stays left."""
    if up == "y":
        return np.stack([p[..., 0], -p[..., 2], p[..., 1]], -1)
    return p


def landmarks(ds: Dataset, path, cut: tuple[int, int] | None = None) -> tuple[dict[str, np.ndarray], float]:
    """Landmark positions in the rig's axes; ``cut`` keeps the file's frames start..stop (inclusive)."""
    if str(path).lower().endswith(".glb"):
        from .gltf_motion import load as load_glb

        m = load_glb(path, FPS)
        joints = ds.joints
    else:
        m = load_bvh(path)
        joints = _ACCAD_FIG if ds.name == "accad" and "root" in m.names else ds.joints
        if cut is None and ds.name == "accad" and len(m.positions) > 1:
            # ACCAD's five character-figure BVHs are padded to 7,200 frames (60 s at 120 fps) with the
            # last pose held still after the motion ends: trim a trailing still run of >= 1 s
            active = np.flatnonzero(np.abs(np.diff(m.positions, axis=0)).max((1, 2)) > 1e-6)
            if len(active) and len(m.positions) - 1 - int(active[-1]) >= int(m.fps):
                cut = (0, int(active[-1]) + 1)
    missing = [j for j in joints.values() if j not in m.names]
    if missing:
        raise ValueError(f"{path}: joints not found {missing} (have {m.names[:30]})")
    pos = m.positions[ds.skip_frames:]
    if cut is not None:
        if not 0 <= cut[0] < cut[1] < len(m.positions):
            raise ValueError(f"{path}: frame cut {cut} outside its {len(m.positions)} frames")
        pos = m.positions[cut[0] : cut[1] + 1]
    src = {k: _to_rig_axes(pos[:, m.names.index(j)] * ds.units, ds.up) for k, j in joints.items()}
    lead = leading_junk(src["hips"])
    return {k: v[lead:] for k, v in src.items()}, m.fps


JUMP = 0.25  # m: hips that move this far in one frame at a take's start are a setup frame, not motion


def leading_junk(hips: np.ndarray, most: int = 5) -> int:
    """How many frames at the start are setup, not motion: frames the hips leave by a jump.

    Some mocap files start with one T-pose frame at the origin, metres from where the take is;
    others in the same set don't (2026-10-28), so a fixed skip would cut real motion.
    """
    n = 0
    while n < min(most, len(hips) - 3) and np.linalg.norm(hips[n + 1] - hips[n]) > JUMP:
        n += 1
    return n


def resample(x: np.ndarray, fps: float, to: float = FPS) -> np.ndarray:
    if abs(fps - to) < 1e-3:
        return x
    t = np.arange(0, (len(x) - 1) / fps + 1e-9, 1 / to) * fps
    i = np.minimum(np.floor(t).astype(int), len(x) - 2)
    a = (t - i).reshape((-1,) + (1,) * (x.ndim - 1))
    return x[i] * (1 - a) + x[i + 1] * a


def in_place(skel: Skeleton, rots: np.ndarray, roots: np.ndarray, fps: float = FPS) -> bool:
    """Whether a take walks on the spot: the feet swing past the hips as in a walk, but the hips
    stay. In-place animations (game loops) teach the model that planted feet slide, so training
    leaves them out. Standing still is not in place: the feet hardly move."""
    from .quality import feet

    pts, hips = feet(skel, rots, roots)
    if len(hips) < 2:
        return False
    seconds = (len(hips) - 1) / fps
    hips_speed = np.linalg.norm(np.diff(hips[:, :2], axis=0), axis=-1).sum() / seconds
    ankles = pts[:, 0:2, :2] - hips[:, None, :2]  # the ankles' horizontal path relative to the hips
    foot_speed = np.linalg.norm(np.diff(ankles, axis=0), axis=-1).sum() / seconds
    return foot_speed > IN_PLACE_FOOT_MS and hips_speed < IN_PLACE_SHARE * foot_speed


IN_PLACE_FOOT_MS = 0.3  # the feet move past the hips at least this fast on average: stepping
IN_PLACE_SHARE = 0.15  # ...while the hips travel less than this share of it: on the spot


def clip_folder(ds: Dataset) -> pathlib.Path:
    f = pathlib.Path(ds.folder)
    return f if f.is_absolute() else RAW / f


def clips_set(name: str, folder, commercial: bool = False, licence: str = "your own clips") -> Dataset:
    """A set from a folder of the app's ``.clip.json`` files, not in the registry."""
    return Dataset(name, f"clips from {folder}", licence, commercial, str(folder), "", folder=str(folder))


def _clip_takes(ds: Dataset, skel: Skeleton, limit: int | None):
    """(take, rotations, roots, label) of every clip file in the set's folder that is not in place."""
    from .clip import Clip

    folder = clip_folder(ds)
    files = sorted(folder.rglob("*.clip.json"))
    if not files:
        raise FileNotFoundError(f"{ds.name}: no .clip.json files under {folder} (write them with clip.import_batch)")
    n = 0
    for f in files:
        if limit is not None and n >= limit:
            break
        try:
            clip = Clip.load(f)
        except (OSError, ValueError, KeyError) as e:
            print(f"  {ds.name}: {f.name} skipped (not a clip file: {e})", flush=True)
            continue
        if list(clip.bones) != list(skel.names):
            print(f"  {ds.name}: {f.name} skipped (made for another rig: {len(clip.bones)} bones)", flush=True)
            continue
        if not clip.base.keys:
            print(f"  {ds.name}: {f.name} skipped (no keys)", flush=True)
            continue
        r, o = clip.sample_all()
        if in_place(skel, r, o, clip.fps):
            print(f"  {ds.name}: {f.name} skipped (in place: the feet step but the hips stay)", flush=True)
            continue
        r, o = resample(r, clip.fps), resample(o, clip.fps)
        if len(r) < 3:
            print(f"  {ds.name}: {f.name} skipped (too short)", flush=True)
            continue
        take = re.sub(r"[^a-z0-9]+", "_", f.name[: -len(".clip.json")].lower()).strip("_") or "clip"
        n += 1
        yield take, r, o, clip.name or take


def _builtin_loops():
    """(take, rotations, roots) of every built-in clip that is not mocap (the CC0 loops)."""
    from .clip import Clip

    for p in sorted((ASSETS / "library" / "animations").glob("*/*.clip.json")):
        if json.loads(p.read_text(encoding="utf-8")).get("origin") == "mocap":
            continue
        clip = Clip.load(p)
        r, o = clip.sample_all()
        yield clip.name, r, o


def build(name: str | Dataset, skel: Skeleton | None = None, limit: int | None = None) -> pathlib.Path:
    """Retarget every take of a set onto the rig; writes and returns ``data/rig/<name>.npz``.
    ``name`` is a registry name, or a :class:`Dataset` of your own (see :func:`clips_set`)."""
    ds = name if isinstance(name, Dataset) else REGISTRY[name]
    name = ds.name
    out = RIG / f"{name}.npz"
    out.parent.mkdir(parents=True, exist_ok=True)
    skel = skel or Skeleton.load()
    rots, roots, starts, takes, labels = [], [], [], [], []
    n = 0

    def add(take, r, o, label=None):
        nonlocal n
        rots.append(r)
        roots.append(o)
        starts.append(n)
        takes.append(take)
        labels.append(label or take)
        n += len(r)

    if name == "builtin_loops":
        for take, r, o in _builtin_loops():
            add(take, r, level(skel, r, o))
    elif ds.folder:
        for take, r, o, label in _clip_takes(ds, skel, limit):
            add(take, r, level(skel, r, o), label)
            print(f"  {name}: {take}: {len(r)} frames", flush=True)
    else:
        files = sorted((RAW).glob(ds.files))
        if not files:
            raise FileNotFoundError(f"{name}: no files under {RAW / ds.files} (download {ds.url})")
        index = (cmu_index() if name == "cmu" else style100_index() if name == "100style"
                 else kimodo_index() if name == "kimodo" else {})
        skipped = 0
        for f in files:
            if limit is not None and len(takes) >= limit:
                break
            if name == "addbiomechanics":
                from . import b3d

                study = f.parent.name if f.parent.name != "addbiomechanics" else "addbiomechanics"
                for tr in b3d.load(f):
                    if limit is not None and len(takes) >= limit:
                        break
                    found = addbiomechanics_take(study, f.stem, tr.name)
                    if found is None:
                        skipped += 1
                        continue
                    take, label = found
                    src, fps = b3d.landmarks(tr)
                    lead = leading_junk(src["hips"])
                    src = {k: v[lead:] for k, v in src.items()}
                    if len(src["hips"]) < 3:
                        continue
                    src = {k: resample(v, fps) for k, v in scale_to(skel, src).items()}
                    r, o = retarget(skel, src)
                    o = level(skel, r, o)
                    add(take, r, o, label)
                    print(f"  {name}: {f.name}/{tr.name} -> {take}: {len(r)} frames", flush=True)
                continue
            take, label, cut = f.stem, None, None
            if name == "cmu":
                label = index.get(f.stem, "unknown")
                take = cmu_take(f.stem, label)
                if take is None:
                    skipped += 1
                    continue
            elif name == "100style":
                found = style100_take(f.stem, index)
                if found is None:
                    print(f"  {name}: {f.name} skipped (no frame cut in Frame_Cuts.csv)", flush=True)
                    continue
                take, label, cut = found
            elif name == "accad":
                take, label = accad_take(f.stem)
            elif name == "kimodo":
                found = kimodo_take(f.stem, index)
                if found is None:
                    print(f"  {name}: {f.name} skipped (not in prompts.tsv)", flush=True)
                    continue
                take, label = found
            src, fps = landmarks(ds, f, cut)
            if len(src["hips"]) < 3:
                print(f"  {name}: {f.name} skipped (too short)", flush=True)
                continue
            src = {k: resample(v, fps) for k, v in scale_to(skel, src).items()}
            r, o = retarget(skel, src)
            o = level(skel, r, o)
            add(take, r, o, label)
            print(f"  {name}: {f.name} -> {take}: {len(r)} frames", flush=True)
        if skipped:
            print(f"  {name}: {skipped} takes skipped (scenery the rig does not have)", flush=True)
    if not takes:
        raise ValueError(f"{name}: no usable takes (every file was skipped)")
    np.savez_compressed(
        out, rotations=np.concatenate(rots).astype(np.float16), roots=np.concatenate(roots).astype(np.float32),
        starts=np.array(starts), takes=np.array(takes), labels=np.array(labels), fps=FPS, licence=ds.licence,
        commercial=ds.commercial, **{k: np.array(v) for k, v in stamp.build_stamp().items()})
    print(f"{name}: {len(takes)} takes, {n} frames ({n / FPS / 3600:.2f} h) -> {out}")
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description="Retarget motion datasets onto the rig (training/data/rig/*.npz)")
    ap.add_argument("names", nargs="*", default=list(REGISTRY), help=f"sets ({', '.join(REGISTRY)})")
    ap.add_argument("--limit", type=int, help="first N takes only (a quick check)")
    ap.add_argument("--clips", metavar="FOLDER", help="build one set from a folder of the app's .clip.json files "
                    "(clip.import_batch) instead of the registry")
    ap.add_argument("--name", default="myclips", help="the set's name with --clips (default myclips)")
    ap.add_argument("--commercial", action="store_true", help="with --clips: the clips are yours to train the shipped "
                    "model on (default: a non-commercial set, read with --noncommercial)")
    args = ap.parse_args(argv)
    if args.clips:
        build(clips_set(args.name, pathlib.Path(args.clips).resolve(), commercial=args.commercial), limit=args.limit)
        return
    for n in args.names:
        try:
            build(n, limit=args.limit)
        except FileNotFoundError as e:
            print(f"skip {n}: {e}")


if __name__ == "__main__":
    main()
