"""World joint positions of a glTF/glb animation (the first one), sampled on a fixed frame rate."""

from __future__ import annotations

import numpy as np

from .bvh import Motion
from .glb import read_accessor, read_glb


def _quat_mats(q):
    x, y, z, w = (q / np.linalg.norm(q, axis=-1, keepdims=True)).T
    return np.stack([1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w),
                     2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w),
                     2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)], -1).reshape(-1, 3, 3)


def _sample(times, values, at, interp):
    """Values of a channel at times ``at`` (STEP or LINEAR; quaternions are re-normalised later)."""
    i = np.clip(np.searchsorted(times, at, side="right") - 1, 0, len(times) - 1)
    if interp == "STEP" or len(times) == 1:
        return values[i]
    j = np.minimum(i + 1, len(times) - 1)
    span = np.maximum(times[j] - times[i], 1e-9)
    a = np.clip((at - times[i]) / span, 0, 1)[:, None]
    v0, v1 = values[i], values[j]
    if values.shape[1] == 4:  # shortest path for rotations
        v1 = np.where((np.sum(v0 * v1, -1) < 0)[:, None], -v1, v1)
    return v0 * (1 - a) + v1 * a


def load(path, fps: float = 30.0) -> Motion:
    gltf, binary = read_glb(path)
    nodes = gltf["nodes"]
    N = len(nodes)
    parents = [-1] * N
    for i, n in enumerate(nodes):
        for c in n.get("children", []):
            parents[c] = i
    anim = gltf["animations"][0]
    t_end = max(float(read_accessor(gltf, binary, s["input"]).max()) for s in anim["samplers"])
    at = np.arange(0, t_end + 1e-9, 1.0 / fps)
    F = len(at)
    T = np.array([n.get("translation", [0, 0, 0]) for n in nodes], dtype=np.float64)[None].repeat(F, 0)
    Q = np.array([n.get("rotation", [0, 0, 0, 1]) for n in nodes], dtype=np.float64)[None].repeat(F, 0)
    S = np.array([n.get("scale", [1, 1, 1]) for n in nodes], dtype=np.float64)[None].repeat(F, 0)
    for ch in anim["channels"]:
        s = anim["samplers"][ch["sampler"]]
        times = read_accessor(gltf, binary, s["input"])[:, 0].astype(np.float64)
        vals = read_accessor(gltf, binary, s["output"]).astype(np.float64)
        v = _sample(times, vals, at, s.get("interpolation", "LINEAR"))
        node, path_ = ch["target"]["node"], ch["target"]["path"]
        {"translation": T, "rotation": Q, "scale": S}[path_][:, node] = v
    # world matrices, parents first (glTF does not order nodes, so walk from the roots)
    order, stack = [], [i for i in range(N) if parents[i] < 0]
    while stack:
        i = stack.pop()
        order.append(i)
        stack.extend(nodes[i].get("children", []))
    W = np.zeros((F, N, 4, 4))
    for i in order:
        M = np.zeros((F, 4, 4))
        M[:, :3, :3] = _quat_mats(Q[:, i]) * S[:, i, None, :]
        M[:, :3, 3] = T[:, i]
        M[:, 3, 3] = 1
        W[:, i] = M if parents[i] < 0 else W[:, parents[i]] @ M
    names = [n.get("name") or f"node{i}" for i, n in enumerate(nodes)]
    return Motion(names, parents, W[:, :, :3, 3], fps)
