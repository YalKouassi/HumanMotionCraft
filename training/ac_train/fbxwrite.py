"""A small binary FBX writer: the test files and the retarget golden for the app's FBX import.

The app reads FBX in Rust (``crates/ac-io/src/fbx``, roadmap 152000); this module only writes, so
there is one importer and no second reader in Python. It writes what that reader has to cope
with: a skeleton of ``LimbNode`` models with static or animated ``Lcl Translation`` and
``Lcl Rotation``, a ``PreRotation`` and a ``RotationOrder`` on some joints, end sites, a finger
and a prop with no landmark, one animation stack with a curve per channel, and ``GlobalSettings``
with the axis system, the unit scale and the time mode. The layout is the public binary one:
node records with 32-bit offsets (7.4) or 64-bit ones (7.5 and later), typed properties, and
zlib-compressed arrays. Written from that layout, not from any other product's code.

``python -m ac_train.fbxwrite`` writes the fixture files (``fixtures/data/walk-*.fbx``) and the
golden (``crates/ac-core/tests/golden/retarget-walk.json``) from the built-in CC0 walk: the
landmarks the files hold, and what the Python retarget (:mod:`ac_train.retarget`) makes of them,
which the Rust port is pinned to.
"""

from __future__ import annotations

import json
import pathlib
import struct
import zlib

import numpy as np

from . import ASSETS, REPO
from .clip import Clip
from .quality import floor_of, lowest
from .retarget import LANDMARKS, retarget, scale_to
from .rig import Skeleton

MAGIC = b"Kaydara FBX Binary  \x00\x1a\x00"
TICKS_PER_SECOND = 46186158000  # the FBX time unit
# the rig's axes (Z up, facing -Y) -> a Y-up file's (Y up, facing +Z): a proper rotation
RIG_TO_YUP = np.array([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]])
# RotationOrder enum: the axis applied first comes first; the matrix is the product in reverse
ORDERS = {"XYZ": 0, "XZY": 1, "YZX": 2, "YXZ": 3, "ZXY": 4, "ZYX": 5}
# FBX joint -> (rig bone, FBX parent). Twist, roll, end, finger and prop joints have no landmark.
JOINTS = [
    ("Hips", "root", None),
    ("LHipJoint", "pelvis.L", "Hips"),
    ("LeftUpLeg", "upperleg01.L", "LHipJoint"),
    ("LeftUpLegRoll", "upperleg02.L", "LeftUpLeg"),
    ("LeftLeg", "lowerleg01.L", "LeftUpLegRoll"),
    ("LeftLegRoll", "lowerleg02.L", "LeftLeg"),
    ("LeftFoot", "foot.L", "LeftLegRoll"),
    ("LeftToeBase", "toe3-1.L", "LeftFoot"),
    ("RHipJoint", "pelvis.R", "Hips"),
    ("RightUpLeg", "upperleg01.R", "RHipJoint"),
    ("RightUpLegRoll", "upperleg02.R", "RightUpLeg"),
    ("RightLeg", "lowerleg01.R", "RightUpLegRoll"),
    ("RightLegRoll", "lowerleg02.R", "RightLeg"),
    ("RightFoot", "foot.R", "RightLegRoll"),
    ("RightToeBase", "toe3-1.R", "RightFoot"),
    ("Spine", "spine05", "Hips"),
    ("Spine1", "spine04", "Spine"),
    ("Spine2", "spine03", "Spine1"),
    ("Spine3", "spine02", "Spine2"),
    ("Spine4", "spine01", "Spine3"),
    ("Neck", "neck01", "Spine4"),
    ("Neck1", "neck02", "Neck"),
    ("Neck2", "neck03", "Neck1"),
    ("Head", "head", "Neck2"),
    ("LeftShoulder", "clavicle.L", "Spine4"),
    ("LeftShoulderTwist", "shoulder01.L", "LeftShoulder"),
    ("LeftArm", "upperarm01.L", "LeftShoulderTwist"),
    ("LeftArmRoll", "upperarm02.L", "LeftArm"),
    ("LeftForeArm", "lowerarm01.L", "LeftArmRoll"),
    ("LeftForeArmRoll", "lowerarm02.L", "LeftForeArm"),
    ("LeftHand", "wrist.L", "LeftForeArmRoll"),
    ("LeftHandThumb1", "finger1-1.L", "LeftHand"),
    ("RightShoulder", "clavicle.R", "Spine4"),
    ("RightShoulderTwist", "shoulder01.R", "RightShoulder"),
    ("RightArm", "upperarm01.R", "RightShoulderTwist"),
    ("RightArmRoll", "upperarm02.R", "RightArm"),
    ("RightForeArm", "lowerarm01.R", "RightArmRoll"),
    ("RightForeArmRoll", "lowerarm02.R", "RightForeArm"),
    ("RightHand", "wrist.R", "RightForeArmRoll"),
]
# static end sites and a prop: (name, parent, offset in the parent's rest frame, metres)
ENDS = [
    ("LeftToe_End", "LeftToeBase", "toe3-2.L"),
    ("RightToe_End", "RightToeBase", "toe3-2.R"),
    ("HeadTop_End", "Head", (0.0, 0.0, 0.15)),
    ("Prop_Sword", "RightHand", (0.0, -0.3, 0.0)),
]
PRE_ROTATION = {"Spine": (90.0, 0.0, 0.0), "LeftArm": (0.0, 0.0, -30.0)}
ROTATION_ORDER = {"LeftLeg": "ZXY", "RightArm": "ZYX", "Hips": "YXZ", "Spine1": "XZY"}
# the landmark each FBX joint stands for (the golden's input)
LANDMARK_OF = {"hips": "Hips", "spine": "Spine", "neck": "Neck", "head": "Head",
               "l_shoulder": "LeftArm", "l_elbow": "LeftForeArm", "l_wrist": "LeftHand",
               "l_hip": "LeftUpLeg", "l_knee": "LeftLeg", "l_ankle": "LeftFoot", "l_toe": "LeftToeBase",
               "r_shoulder": "RightArm", "r_elbow": "RightForeArm", "r_wrist": "RightHand",
               "r_hip": "RightUpLeg", "r_knee": "RightLeg", "r_ankle": "RightFoot", "r_toe": "RightToeBase"}
# the built-in walk that travels (-Y) and the side step to the rig's left (+X), both 100STYLE
# (CC BY 4.0) clips of the library, cut to the first frames
WALK = ASSETS / "library" / "animations" / "Locomotion" / "Walk 12.clip.json"
WALK_FRAMES = 40
SIDESTEP = ASSETS / "library" / "animations" / "Locomotion" / "Side step 9.clip.json"
SIDESTEP_FRAMES = 30
FIXTURES = REPO / "fixtures" / "data"
GOLDEN = REPO / "crates" / "ac-core" / "tests" / "golden" / "retarget-walk.json"


# ---- the binary layout ----------------------------------------------------------------------


class Node:
    """One node record: a name, typed properties and nested nodes."""

    def __init__(self, name: str, *props, children=None):
        self.name = name
        self.props = list(props)
        self.children = list(children or [])

    def add(self, child: "Node") -> "Node":
        self.children.append(child)
        return child


def S(text) -> tuple:
    return ("S", text)


def P(name: str, kind: str, label: str, flags: str, *values) -> Node:
    """A ``Properties70`` entry: ``P: name, type, label, flags, values...``."""
    return Node("P", S(name), S(kind), S(label), S(flags), *values)


def D(*xs) -> list:
    return [("D", float(x)) for x in xs]


_SCALAR = {"Y": "<h", "C": "<B", "I": "<i", "L": "<q", "F": "<f", "D": "<d"}
_ARRAY = {"f": "<f4", "d": "<f8", "l": "<i8", "i": "<i4", "b": "<u1"}


def _prop(p: tuple, compress: bool) -> bytes:
    code, v = p
    if code == "S":
        b = v.encode("utf-8") if isinstance(v, str) else bytes(v)
        return b"S" + struct.pack("<I", len(b)) + b
    if code == "R":
        return b"R" + struct.pack("<I", len(v)) + bytes(v)
    if code in _SCALAR:
        return code.encode() + struct.pack(_SCALAR[code], v)
    if code in _ARRAY:
        raw = np.ascontiguousarray(np.asarray(v, dtype=_ARRAY[code])).tobytes()
        data = zlib.compress(raw) if compress and raw else raw
        return code.encode() + struct.pack("<III", len(np.asarray(v)), 1 if compress and raw else 0, len(data)) + data
    raise ValueError(f"unknown property code {code!r}")


def _record(node: Node, offset: int, wide: bool, compress: bool) -> bytes:
    """A node record starting at ``offset`` (its end offset is absolute, so the position matters)."""
    fmt = "<QQQB" if wide else "<IIIB"
    null = 25 if wide else 13
    name = node.name.encode("utf-8")
    props = b"".join(_prop(p, compress) for p in node.props)
    start = offset + struct.calcsize(fmt) + len(name) + len(props)
    nested = b""
    if node.children or not node.props:  # an empty node also ends with a null record
        pos = start
        for c in node.children:
            b = _record(c, pos, wide, compress)
            nested += b
            pos += len(b)
        nested += bytes(null)
    return struct.pack(fmt, start + len(nested), len(node.props), len(props), len(name)) + name + props + nested


def write_fbx(path, nodes: list[Node], version: int = 7400, compress: bool = True) -> bytes:
    """Write ``nodes`` as a binary FBX of ``version`` (7500 and later use 64-bit offsets)."""
    wide = version >= 7500
    out = bytearray(MAGIC + struct.pack("<I", version))
    for n in nodes:
        out += _record(n, len(out), wide, compress)
    out += bytes(25 if wide else 13)  # the top-level null record: the document ends here
    # the footer: an id, padding to 16 bytes, the version again, and the closing bytes
    out += bytes([0xFA, 0xBC, 0xAB, 0x09, 0xD0, 0xC8, 0xD4, 0x66, 0xB1, 0x76, 0xFB, 0x83, 0x1C, 0xF7, 0x26, 0x7E])
    out += bytes(4)
    out += bytes((16 - len(out) % 16) % 16)
    out += bytes(4) + struct.pack("<I", version) + bytes(120)
    out += bytes([0xF8, 0x5A, 0x8C, 0x6A, 0xDE, 0xF5, 0xD9, 0x7E, 0xEC, 0xE9, 0x0C, 0xE3, 0x75, 0x8F, 0x29, 0x0B])
    pathlib.Path(path).write_bytes(out)
    return bytes(out)


# ---- rotations ------------------------------------------------------------------------------


def _axis(i: int, a: float) -> np.ndarray:
    c, s = np.cos(a), np.sin(a)
    if i == 0:
        return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])
    if i == 1:
        return np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def euler_to_mat(deg, order: str = "XYZ") -> np.ndarray:
    """The rotation of FBX Euler angles ``(x, y, z)`` in degrees: the angles are always about X, Y
    and Z in that storage order; ``order`` says which axis turns first (checked against a
    reference importer, 2026-10-30)."""
    i, j, k = ("XYZ".index(ch) for ch in order)
    r = np.radians(np.asarray(deg, dtype=np.float64))
    return _axis(k, r[k]) @ _axis(j, r[j]) @ _axis(i, r[i])


def mat_to_euler(R: np.ndarray, order: str = "XYZ") -> np.ndarray:
    """FBX Euler angles ``(x, y, z)`` (degrees) of a rotation matrix in ``order``; inverse of
    :func:`euler_to_mat`."""
    i, j, k = ("XYZ".index(ch) for ch in order)
    e = 1.0 if order in ("XYZ", "YZX", "ZXY") else -1.0
    second = np.arcsin(np.clip(-e * R[k, i], -1.0, 1.0))
    if abs(R[k, i]) > 1.0 - 1e-10:  # gimbal lock: the first and third axes line up, so the third turns 0
        third = 0.0
        Q = _axis(j, second).T @ R  # what is left is a turn about the first axis alone
        first = np.arctan2(Q[(i + 2) % 3, (i + 1) % 3], Q[(i + 1) % 3, (i + 1) % 3])
    else:
        first = np.arctan2(e * R[k, j], R[k, k])
        third = np.arctan2(e * R[j, i], R[i, i])
    out = np.zeros(3)
    out[i], out[j], out[k] = first, second, third
    return np.degrees(out)


# ---- the scene -------------------------------------------------------------------------------


def _curve(nid: int, times: np.ndarray, values: np.ndarray) -> Node:
    n = len(times)
    return Node("AnimationCurve", ("L", nid), S(b"AnimCurve::\x00\x01AnimCurve"), S(""), children=[
        Node("Default", ("D", 0.0)),
        Node("KeyVer", ("I", 4009)),
        Node("KeyTime", ("l", times)),
        Node("KeyValueFloat", ("f", values)),
        Node("KeyAttrFlags", ("i", [4])),  # linear between keys
        Node("KeyAttrDataFloat", ("f", [0.0, 0.0, 0.0, 0.0])),
        Node("KeyAttrRefCount", ("i", [n])),
    ])


def _model(nid: int, name: str, kind: str, t, r, s, pre=None, order="XYZ") -> Node:
    props = [P("Lcl Translation", "Lcl Translation", "", "A", *D(*t)),
             P("Lcl Rotation", "Lcl Rotation", "", "A", *D(*r)),
             P("Lcl Scaling", "Lcl Scaling", "", "A", *D(*s)),
             P("RotationActive", "bool", "", "", ("I", 1)),
             P("RotationOrder", "enum", "", "", ("I", ORDERS[order]))]
    if pre is not None:
        props.append(P("PreRotation", "Vector3D", "Vector", "", *D(*pre)))
    return Node("Model", ("L", nid), S(f"{name}\x00\x01Model".encode()), S(kind), children=[
        Node("Version", ("I", 232)),
        Node("Properties70", children=props),
        Node("Shading", ("C", 1)),
        Node("Culling", S("CullingOff")),
    ])


class Scene:
    """A humanoid skeleton with one animation, as nodes for :func:`write_fbx`.

    ``up`` is ``"y"`` (the file's axes are Y up, front +Z) or ``"z"`` (Z up, front -Y: the rig's
    own); ``units`` is the file's unit in centimetres (1: centimetres, 100: metres). ``reference``
    puts a plain node (not a joint) above the hips, as many tools do.
    """

    def __init__(self, fps: float = 30.0, up: str = "y", units: float = 1.0, reference: bool = False,
                 creator: str = "ac_train.fbxwrite"):
        self.fps, self.up, self.units, self.reference, self.creator = fps, up, units, reference, creator
        self.joints: list[tuple[str, str, str | None]] = []  # (name, kind, parent)
        self.static: dict[str, tuple] = {}  # name -> (t, r, s, pre, order), the rest transform
        self.curves: dict[str, dict[str, np.ndarray]] = {}  # name -> {"T"/"R": (F, 3)}
        self.frames = 0

    def joint(self, name: str, parent: str | None, t, r, kind: str = "LimbNode", pre=None, order="XYZ"):
        self.joints.append((name, kind, parent))
        self.static[name] = (np.asarray(t, float), np.asarray(r, float), np.ones(3), pre, order)

    def animate(self, name: str, channel: str, values: np.ndarray):
        self.curves.setdefault(name, {})[channel] = np.asarray(values, dtype=np.float64)
        self.frames = max(self.frames, len(values))

    def nodes(self) -> list[Node]:
        ticks = int(round(TICKS_PER_SECOND / self.fps))
        stop = (max(self.frames, 1) - 1) * ticks
        up_axis, front_axis, front_sign, scale = (1, 2, 1, self.units) if self.up == "y" else (2, 1, -1, self.units)
        header = Node("FBXHeaderExtension", children=[
            Node("FBXHeaderVersion", ("I", 1003)), Node("FBXVersion", ("I", 7400)), Node("Creator", S(self.creator))])
        settings = Node("GlobalSettings", children=[Node("Version", ("I", 1000)), Node("Properties70", children=[
            P("UpAxis", "int", "Integer", "", ("I", up_axis)), P("UpAxisSign", "int", "Integer", "", ("I", 1)),
            P("FrontAxis", "int", "Integer", "", ("I", front_axis)), P("FrontAxisSign", "int", "Integer", "", ("I", front_sign)),
            P("CoordAxis", "int", "Integer", "", ("I", 0)), P("CoordAxisSign", "int", "Integer", "", ("I", 1)),
            P("OriginalUpAxis", "int", "Integer", "", ("I", -1)), P("OriginalUpAxisSign", "int", "Integer", "", ("I", 1)),
            P("UnitScaleFactor", "double", "Number", "", ("D", float(scale))),
            P("OriginalUnitScaleFactor", "double", "Number", "", ("D", float(scale))),
            P("TimeMode", "enum", "", "", ("I", 6 if abs(self.fps - 30) < 1e-9 else 14)),
            P("CustomFrameRate", "double", "Number", "", ("D", float(self.fps) if abs(self.fps - 30) >= 1e-9 else -1.0)),
            P("TimeSpanStart", "KTime", "Time", "", ("L", 0)), P("TimeSpanStop", "KTime", "Time", "", ("L", stop)),
        ])])
        objects, connections = Node("Objects"), Node("Connections")
        ids = {}
        nid = 1000
        for name, kind, parent in self.joints:
            nid += 1
            ids[name] = nid
            t, r, s, pre, order = self.static[name]
            objects.add(_model(nid, name, kind, t, r, s, pre, order))
            nid += 1
            objects.add(Node("NodeAttribute", ("L", nid), S("\x00\x01NodeAttribute".encode()), S(kind),
                             children=[Node("TypeFlags", S("Skeleton" if kind == "LimbNode" else "Null"))]))
            connections.add(Node("C", S("OO"), ("L", nid), ("L", ids[name])))
            connections.add(Node("C", S("OO"), ("L", ids[name]), ("L", ids[parent] if parent else 0)))
        nid += 1
        stack = nid
        objects.add(Node("AnimationStack", ("L", stack), S(b"Take 001\x00\x01AnimStack"), S(""), children=[
            Node("Properties70", children=[
                P("LocalStart", "KTime", "Time", "", ("L", 0)), P("LocalStop", "KTime", "Time", "", ("L", stop)),
                P("ReferenceStart", "KTime", "Time", "", ("L", 0)), P("ReferenceStop", "KTime", "Time", "", ("L", stop))])]))
        nid += 1
        layer = nid
        objects.add(Node("AnimationLayer", ("L", layer), S(b"BaseLayer\x00\x01AnimLayer"), S("")))
        connections.add(Node("C", S("OO"), ("L", layer), ("L", stack)))
        times = np.arange(self.frames, dtype=np.int64) * ticks
        for name, channels in self.curves.items():
            for ch, values in channels.items():
                nid += 1
                cn = nid
                objects.add(Node("AnimationCurveNode", ("L", cn), S(f"{ch}\x00\x01AnimCurveNode".encode()), S(""), children=[
                    Node("Properties70", children=[P(f"d|{ax}", "Number", "", "A", ("D", float(values[0, k])))
                                                   for k, ax in enumerate("XYZ")])]))
                connections.add(Node("C", S("OO"), ("L", cn), ("L", layer)))
                connections.add(Node("C", S("OP"), ("L", cn), ("L", ids[name]),
                                     S("Lcl Translation" if ch == "T" else "Lcl Rotation")))
                for k, ax in enumerate("XYZ"):
                    nid += 1
                    objects.add(_curve(nid, times, values[:, k]))
                    connections.add(Node("C", S("OP"), ("L", nid), ("L", cn), S(f"d|{ax}")))
        takes = Node("Takes", children=[Node("Current", S("Take 001")), Node("Take", S("Take 001"), children=[
            Node("FileName", S("Take_001.tak")), Node("LocalTime", ("L", 0), ("L", stop)),
            Node("ReferenceTime", ("L", 0), ("L", stop))])])
        definitions = Node("Definitions", children=[Node("Version", ("I", 100)), Node("Count", ("I", len(objects.children)))])
        return [header, settings, definitions, objects, connections, takes]


# ---- the walk as an FBX --------------------------------------------------------------------


def walk_motion(skel: Skeleton, clip_path=WALK, frames: int = WALK_FRAMES):
    """World rotations (F, B, 3, 3) and joint positions (F, B, 3) of a built-in clip's first
    ``frames`` frames, rig axes."""
    clip = Clip.load(clip_path)
    rots, roots = clip.sample_all()
    rots, roots = rots[:frames], roots[:frames]
    R, P_ = zip(*(skel.fk(r, o) for r, o in zip(rots, roots)))
    return np.stack(R), np.stack(P_), clip.fps


def landmarks_of(skel: Skeleton, positions: np.ndarray) -> dict[str, np.ndarray]:
    rig = {name: bone for name, bone, _ in JOINTS}
    return {lm: positions[:, skel.index[rig[joint]]] for lm, joint in LANDMARK_OF.items()}


def walk_scene(skel: Skeleton, up: str = "y", units: float = 1.0, reference: bool = False, fps: float = 30.0,
               clip_path=WALK, frames: int = WALK_FRAMES) -> Scene:
    """A built-in clip as a humanoid FBX scene (see :class:`Scene` for ``up``, ``units``, ``reference``)."""
    R, P_, _ = walk_motion(skel, clip_path, frames)
    F = len(R)
    A = RIG_TO_YUP if up == "y" else np.eye(3)
    to_units = 100.0 / units  # metres -> the file's unit
    sc = Scene(fps=fps, up=up, units=units, reference=reference)
    world_r = {}
    world_p = {}
    if reference:
        sc.joint("Reference", None, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), kind="Null")
    for name, bone, parent in JOINTS:
        b = skel.index[bone]
        wr, wp = R[:, b], P_[:, b]
        # the motion expressed in the file's axes: joints rest along those axes
        wr, wp = A[None] @ wr @ A.T, wp @ A.T
        if parent is None:
            pr, pp = np.broadcast_to(np.eye(3), (F, 3, 3)), np.zeros((F, 3))
        else:
            pr, pp = world_r[parent], world_p[parent]
        world_r[name], world_p[name] = wr, wp
        local_r = np.swapaxes(pr, -1, -2) @ wr
        local_t = np.einsum("fji,fj->fi", pr, wp - pp) * to_units
        pre = PRE_ROTATION.get(name)
        order = ROTATION_ORDER.get(name, "XYZ")
        if pre is not None:
            local_r = euler_to_mat(pre).T[None] @ local_r
        eul = np.stack([mat_to_euler(m, order) for m in local_r])
        if np.abs(local_t - local_t[0]).max() > 1e-9:
            sc.joint(name, parent, local_t[0], eul[0], pre=pre, order=order)
            sc.animate(name, "T", local_t)
        else:
            sc.joint(name, parent, local_t[0], eul[0], pre=pre, order=order)
        sc.animate(name, "R", eul)
    for name, parent, off in ENDS:
        pb = skel.index[dict((n, b) for n, b, _ in JOINTS)[parent]]
        rest = skel.heads[skel.index[off]] - skel.heads[pb] if isinstance(off, str) else np.asarray(off, float)
        sc.joint(name, parent, A @ rest * to_units, (0.0, 0.0, 0.0), kind="LimbNode" if "End" in name else "Null")
    return sc


def golden(skel: Skeleton) -> dict:
    """The Python retarget's result on the walk's landmarks: what the Rust port must reproduce."""
    _, P_, fps = walk_motion(skel)
    src = landmarks_of(skel, P_)
    scaled = scale_to(skel, src)
    rots, roots = retarget(skel, scaled)
    off = floor_of(lowest(skel, rots, roots))
    r7 = lambda a: np.round(np.asarray(a, dtype=np.float64), 10).tolist()  # noqa: E731
    return {
        "what": "ac_train.retarget on the landmarks of the built-in Walk 12's first frames (metres, rig axes); "
                "python -m ac_train.fbxwrite",
        "fps": fps,
        "frames": int(len(P_)),
        "bones": list(skel.names),
        "landmarks": {k: r7(v) for k, v in src.items()},
        "scale": float(np.linalg.norm(scaled["hips"][0]) / max(np.linalg.norm(src["hips"][0]), 1e-12)),
        "rotations": r7(rots),
        "roots": r7(roots),
        "floor": round(float(off), 10),
        "landmark_order": list(LANDMARKS),
    }


def not_humanoid_scene() -> Scene:
    """Three joints with no humanoid names: the import must refuse it and say what is missing."""
    sc = Scene(up="y", units=1.0)
    sc.joint("Root", None, (0.0, 100.0, 0.0), (0.0, 0.0, 0.0))
    sc.joint("Bone1", "Root", (0.0, 40.0, 0.0), (0.0, 0.0, 0.0))
    sc.joint("Bone2", "Bone1", (0.0, 40.0, 0.0), (0.0, 0.0, 0.0))
    sc.animate("Root", "R", np.array([[0.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 20.0, 0.0]]))
    return sc


def write_fixtures(fixtures=FIXTURES, golden_path=GOLDEN) -> list[pathlib.Path]:
    skel = Skeleton.load()
    fixtures.mkdir(parents=True, exist_ok=True)
    out = []
    # the side step to the rig's left, Y up in centimetres, 7.4
    sc = walk_scene(skel, up="y", units=1.0, clip_path=SIDESTEP, frames=SIDESTEP_FRAMES)
    out.append(fixtures / "sidestep-left.fbx")
    write_fbx(out[-1], sc.nodes(), version=7400)
    out.append(fixtures / "not-humanoid.fbx")
    write_fbx(out[-1], not_humanoid_scene().nodes(), version=7400)
    # Y up in centimetres under a plain parent node (as many tools write), 7.4
    sc = walk_scene(skel, up="y", units=1.0, reference=True)
    out.append(fixtures / "walk-yup.fbx")
    write_fbx(out[-1], sc.nodes(), version=7400)
    # Z up in metres, the rig's own axes, 7.4, arrays left uncompressed
    sc = walk_scene(skel, up="z", units=100.0)
    out.append(fixtures / "walk-zup.fbx")
    write_fbx(out[-1], sc.nodes(), version=7400, compress=False)
    # Y up in centimetres, the hips at the top, 7.5 (64-bit records)
    sc = walk_scene(skel, up="y", units=1.0)
    out.append(fixtures / "walk-yup-75.fbx")
    write_fbx(out[-1], sc.nodes(), version=7500)
    golden_path.parent.mkdir(parents=True, exist_ok=True)
    golden_path.write_text(json.dumps(golden(skel), separators=(",", ":")) + "\n", encoding="utf-8")
    out.append(golden_path)
    return out


def main(argv=None):
    for p in write_fixtures():
        print(f"wrote {p} ({p.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
