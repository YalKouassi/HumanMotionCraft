"""A small glTF binary writer: the test character of the app's export onto a character.

The app reads characters in Rust (``crates/ac-io/src/character/read.rs``, roadmap 146000); this
module only writes, so there is one reader. It writes what that reader and the retarget have
to cope with: a skeleton with other bone names than the rig's (``Pelvis``, ``L_Thigh``,
``L_Calf``, ``L_Toe0``, ...), different proportions (1.9 m tall, long legs), an A-pose with the
arms 45 degrees down, a bone roll on every joint (each joint's local Y axis runs along its
bone, so no rest rotation is the identity), an ``Armature`` root scaled by 0.01 with the joint
translations in centimetres, a forearm twist joint between the forearm and the hand, fingers,
end ``Nub`` joints and a prop, none of which the rig drives, and a skinned box mesh so an
outside renderer shows it. Written from the glTF 2.0 specification, not from any other
product's code.

``python -m ac_train.glbwrite`` writes ``fixtures/data/tall-character.glb`` and prints the
joint positions the fixture's numbers are worked out from.
"""

from __future__ import annotations

import json
import pathlib
import struct

import numpy as np

from . import REPO

OUT = REPO / "fixtures" / "data" / "tall-character.glb"
ARMATURE_SCALE = 0.01  # the root node's scale; joint translations are in centimetres

# glTF axes: Y up, the character faces +Z, its left side is +X. Metres.
A45 = np.array([np.sin(np.pi / 4), -np.cos(np.pi / 4), 0.0])  # the A-pose arm direction


def _side(s: float) -> dict[str, np.ndarray]:
    """One side's joints, `s` = +1 left, -1 right."""
    upper = np.array([0.20 * s, 1.55, 0.0])
    fore = upper + 0.32 * A45 * np.array([s, 1.0, 1.0])
    hand = fore + 0.28 * A45 * np.array([s, 1.0, 1.0])
    d = A45 * np.array([s, 1.0, 1.0])
    out = {
        "Clavicle": np.array([0.03 * s, 1.56, 0.02]),
        "UpperArm": upper,
        "Forearm": fore,
        "ForearmTwist": fore + 0.14 * d,
        "Hand": hand,
        "Finger0": hand + np.array([0.03 * s, -0.03, 0.04]),
        "Finger01": hand + np.array([0.05 * s, -0.05, 0.07]),
        "Finger1": hand + 0.09 * d,
        "Finger11": hand + 0.12 * d,
        "Thigh": np.array([0.11 * s, 1.02, 0.0]),
        "Calf": np.array([0.12 * s, 0.60, 0.0]),
        "Foot": np.array([0.12 * s, 0.10, -0.02]),
        "Toe0": np.array([0.12 * s, 0.03, 0.16]),
        "Toe0Nub": np.array([0.12 * s, 0.02, 0.24]),
    }
    return out


def joints() -> list[tuple[str, str | None, np.ndarray]]:
    """(name, parent name, world position) of every joint, parents first."""
    out: list[tuple[str, str | None, np.ndarray]] = [
        ("Pelvis", None, np.array([0.0, 1.08, 0.0])),
        ("Spine", "Pelvis", np.array([0.0, 1.16, 0.01])),
        ("Spine1", "Spine", np.array([0.0, 1.30, 0.0])),
        ("Spine2", "Spine1", np.array([0.0, 1.44, -0.01])),
        ("Neck", "Spine2", np.array([0.0, 1.60, 0.0])),
        ("Head", "Neck", np.array([0.0, 1.70, 0.01])),
        ("HeadNub", "Head", np.array([0.0, 1.92, 0.02])),
    ]
    for side, s in (("L", 1.0), ("R", -1.0)):
        j = _side(s)
        p = lambda n: f"{side}_{n}"  # noqa: E731
        out += [
            (p("Clavicle"), "Spine2", j["Clavicle"]),
            (p("UpperArm"), p("Clavicle"), j["UpperArm"]),
            (p("Forearm"), p("UpperArm"), j["Forearm"]),
            (p("ForearmTwist"), p("Forearm"), j["ForearmTwist"]),
            (p("Hand"), p("ForearmTwist"), j["Hand"]),
            (p("Finger0"), p("Hand"), j["Finger0"]),
            (p("Finger01"), p("Finger0"), j["Finger01"]),
            (p("Finger1"), p("Hand"), j["Finger1"]),
            (p("Finger11"), p("Finger1"), j["Finger11"]),
            (p("Thigh"), "Pelvis", j["Thigh"]),
            (p("Calf"), p("Thigh"), j["Calf"]),
            (p("Foot"), p("Calf"), j["Foot"]),
            (p("Toe0"), p("Foot"), j["Toe0"]),
            (p("Toe0Nub"), p("Toe0"), j["Toe0Nub"]),
        ]
    out.append(("Weapon_Prop", "R_Hand", _side(-1.0)["Hand"] + np.array([0.0, -0.10, 0.10])))
    return out


# the segment each joint's box covers and its half width (m); joints not listed have no box
BOXES = {
    "Pelvis": ("Spine", 0.14),
    "Spine": ("Spine1", 0.13),
    "Spine1": ("Spine2", 0.13),
    "Spine2": ("Neck", 0.14),
    "Neck": ("Head", 0.05),
    "Head": ("HeadNub", 0.09),
}
for _s in ("L", "R"):
    BOXES[f"{_s}_Clavicle"] = (f"{_s}_UpperArm", 0.04)
    BOXES[f"{_s}_UpperArm"] = (f"{_s}_Forearm", 0.05)
    BOXES[f"{_s}_Forearm"] = (f"{_s}_Hand", 0.04)
    BOXES[f"{_s}_Hand"] = (f"{_s}_Finger11", 0.035)
    BOXES[f"{_s}_Thigh"] = (f"{_s}_Calf", 0.08)
    BOXES[f"{_s}_Calf"] = (f"{_s}_Foot", 0.06)


def _unit(v: np.ndarray) -> np.ndarray:
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else v


def frame(direction: np.ndarray) -> np.ndarray:
    """A rotation whose Y axis is `direction` (columns X, Y, Z), the X axis across +Z when it can."""
    y = _unit(direction)
    hint = np.array([0.0, 0.0, 1.0]) if abs(y[2]) < 0.9 else np.array([1.0, 0.0, 0.0])
    x = _unit(np.cross(hint, y))
    z = np.cross(x, y)
    return np.stack([x, y, z], axis=1)


def quat_of(m: np.ndarray) -> list[float]:
    """(x, y, z, w) of a rotation matrix."""
    t = np.trace(m)
    if t > 0:
        s = np.sqrt(t + 1.0) * 2
        w, x, y, z = 0.25 * s, (m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = np.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        w, x, y, z = (m[2, 1] - m[1, 2]) / s, 0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s
    elif m[1, 1] > m[2, 2]:
        s = np.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        w, x, y, z = (m[0, 2] - m[2, 0]) / s, (m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s
    else:
        s = np.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        w, x, y, z = (m[1, 0] - m[0, 1]) / s, (m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s
    q = np.array([x, y, z, w])
    q /= np.linalg.norm(q)
    return [float(v) for v in q]


def build() -> tuple[dict, bytes, dict[str, np.ndarray]]:
    js = joints()
    names = [n for n, _, _ in js]
    pos = {n: p for n, _, p in js}
    parent = {n: p for n, p, _ in js}
    children: dict[str, list[str]] = {n: [] for n in names}
    for n, p, _ in js:
        if p is not None:
            children[p].append(n)
    # world rotations: Y along the bone to the first child (a leaf keeps its parent's)
    world_r: dict[str, np.ndarray] = {}
    for n in names:
        if children[n]:
            world_r[n] = frame(pos[children[n][0]] - pos[n])
        else:
            world_r[n] = world_r[parent[n]]
    # world matrices: the armature's scale, then the joint in centimetres (so every local
    # translation under the armature is in centimetres, as the scale propagates to all of them)
    armature4 = np.diag([ARMATURE_SCALE, ARMATURE_SCALE, ARMATURE_SCALE, 1.0])

    def world4(n: str) -> np.ndarray:
        m = np.eye(4)
        m[:3, :3] = world_r[n]
        m[:3, 3] = pos[n] / ARMATURE_SCALE
        return armature4 @ m

    nodes = [{"name": "Armature", "scale": [ARMATURE_SCALE] * 3, "children": [1]}]
    index = {"Armature": 0}
    for n in names:
        index[n] = len(nodes)
        nodes.append({"name": n})
    for n in names:
        p = parent[n]
        p4 = armature4 if p is None else world4(p)
        local = np.linalg.inv(p4) @ world4(n)
        t = local[:3, 3]
        r = local[:3, :3]
        # the parent's scale is in the local matrix: take it out of the rotation
        s = np.linalg.norm(r[:, 0])
        node = nodes[index[n]]
        node["translation"] = [float(v) for v in t]
        node["rotation"] = quat_of(r / s)
        if p is not None:
            nodes[index[p]].setdefault("children", []).append(index[n])

    # the mesh: a box per segment, every vertex on one joint
    verts: list[np.ndarray] = []
    norms: list[np.ndarray] = []
    jidx: list[int] = []
    tris: list[tuple[int, int, int]] = []
    skin_joints = [index[n] for n in names]
    skin_of = {n: k for k, n in enumerate(names)}

    def box(corners: np.ndarray, joint: str) -> None:
        base = len(verts)
        centre = corners.mean(axis=0)
        for c in corners:
            verts.append(c)
            norms.append(_unit(c - centre))
            jidx.append(skin_of[joint])
        # corners ordered (t, x, z) = 000 001 010 011 100 101 110 111 over (near/far, -x/+x, -z/+z)
        faces = [
            (0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3),
        ]
        for a, b, c, d in faces:
            tris.append((base + a, base + b, base + c))
            tris.append((base + a, base + c, base + d))

    for n, (end, w) in BOXES.items():
        p, q = pos[n], pos[end]
        f = frame(q - p)
        x, y, z = f[:, 0], f[:, 1], f[:, 2]
        length = float(np.linalg.norm(q - p))
        corners = np.array(
            [p + y * (t * length) + x * (sx * w) + z * (sz * w) for t in (0.0, 1.0) for sx in (-1.0, 1.0) for sz in (-1.0, 1.0)]
        )
        box(corners, n)
    for side in ("L", "R"):
        a = pos[f"{side}_Foot"]
        corners = np.array(
            [np.array([a[0] + sx * 0.05, y, a[2] + z]) for y in (0.0, 0.12) for sx in (-1.0, 1.0) for z in (-0.06, 0.22)]
        )
        box(corners, f"{side}_Foot")

    v = np.array(verts, dtype=np.float32)
    nrm = np.array(norms, dtype=np.float32)
    jn = np.zeros((len(verts), 4), dtype=np.uint16)
    jn[:, 0] = np.array(jidx, dtype=np.uint16)
    wt = np.zeros((len(verts), 4), dtype=np.float32)
    wt[:, 0] = 1.0
    idx = np.array(tris, dtype=np.uint32).reshape(-1)
    ibm = np.array([np.linalg.inv(world4(n)).T for n in names], dtype=np.float32)  # column-major

    bin_parts: list[bytes] = []
    views: list[dict] = []
    accessors: list[dict] = []

    def add(array: np.ndarray, kind: str, comp: int, extra: dict | None = None, target: int | None = None) -> int:
        data = array.tobytes()
        off = sum(len(b) for b in bin_parts)
        bin_parts.append(data)
        pad = (-len(data)) % 4
        if pad:
            bin_parts.append(b"\0" * pad)
        view = {"buffer": 0, "byteOffset": off, "byteLength": len(data)}
        if target is not None:
            view["target"] = target
        views.append(view)
        count = array.shape[0] if array.ndim > 1 or kind == "SCALAR" else array.shape[0]
        acc = {"bufferView": len(views) - 1, "componentType": comp, "count": int(count), "type": kind}
        if extra:
            acc.update(extra)
        accessors.append(acc)
        return len(accessors) - 1

    a_ibm = add(ibm.reshape(len(names), 16), "MAT4", 5126)
    a_pos = add(v, "VEC3", 5126, {"min": [float(x) for x in v.min(axis=0)], "max": [float(x) for x in v.max(axis=0)]}, 34962)
    a_nrm = add(nrm, "VEC3", 5126, None, 34962)
    a_jn = add(jn, "VEC4", 5123, None, 34962)
    a_wt = add(wt, "VEC4", 5126, None, 34962)
    a_idx = add(idx, "SCALAR", 5125, None, 34963)

    mesh_node = len(nodes)
    nodes.append({"name": "Body", "mesh": 0, "skin": 0})
    gltf = {
        "asset": {"version": "2.0", "generator": "ac_train.glbwrite"},
        "scene": 0,
        "scenes": [{"nodes": [0, mesh_node]}],
        "nodes": nodes,
        "skins": [{"name": "Armature", "joints": skin_joints, "skeleton": index["Pelvis"], "inverseBindMatrices": a_ibm}],
        "meshes": [{"name": "Body", "primitives": [{"attributes": {"POSITION": a_pos, "NORMAL": a_nrm, "JOINTS_0": a_jn, "WEIGHTS_0": a_wt}, "indices": a_idx, "material": 0}]}],
        "materials": [{"name": "Grey", "pbrMetallicRoughness": {"baseColorFactor": [0.55, 0.6, 0.7, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.8}}],
        "bufferViews": views,
        "accessors": accessors,
        "buffers": [{"byteLength": sum(len(b) for b in bin_parts)}],
    }
    return gltf, b"".join(bin_parts), pos


def write_glb(path: pathlib.Path, gltf: dict, binary: bytes) -> None:
    text = json.dumps(gltf, separators=(",", ":")).encode("utf-8")
    text += b" " * ((-len(text)) % 4)
    binary += b"\0" * ((-len(binary)) % 4)
    total = 12 + 8 + len(text) + 8 + len(binary)
    with open(path, "wb") as f:
        f.write(struct.pack("<III", 0x46546C67, 2, total))
        f.write(struct.pack("<II", len(text), 0x4E4F534A))
        f.write(text)
        f.write(struct.pack("<II", len(binary), 0x004E4942))
        f.write(binary)


def main() -> None:
    gltf, binary, pos = build()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    write_glb(OUT, gltf, binary)
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes, {len(gltf['nodes'])} nodes)")
    for n in ("Pelvis", "L_Thigh", "L_Calf", "L_Foot", "L_Toe0", "L_UpperArm", "L_Forearm", "L_Hand", "Head"):
        print(f"  {n:12s} {pos[n].round(4).tolist()}")
    thigh = float(np.linalg.norm(pos["L_Calf"] - pos["L_Thigh"]))
    shin = float(np.linalg.norm(pos["L_Foot"] - pos["L_Calf"]))
    print(f"  leg (thigh + shin) = {thigh:.6f} + {shin:.6f} = {thigh + shin:.6f} m")


if __name__ == "__main__":
    main()
