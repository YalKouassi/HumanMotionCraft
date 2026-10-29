"""Minimal BVH reader: joint names, parents and world joint positions for every frame (vectorised)."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np


@dataclass
class Motion:
    names: list[str]
    parents: list[int]
    positions: np.ndarray  # (frames, joints, 3) in the file's units and axes
    fps: float


def _axis_mats(axis: str, deg: np.ndarray) -> np.ndarray:
    a = np.radians(deg)
    c, s = np.cos(a), np.sin(a)
    z, o = np.zeros_like(a), np.ones_like(a)
    if axis == "X":
        m = [o, z, z, z, c, -s, z, s, c]
    elif axis == "Y":
        m = [c, z, s, z, o, z, -s, z, c]
    else:
        m = [c, -s, z, s, c, z, z, z, o]
    return np.stack(m, -1).reshape(a.shape + (3, 3))


def load(path) -> Motion:
    tok = open(path, encoding="utf-8", errors="replace").read().split()
    names, parents, offsets, channels = [], [], [], []
    stack, last, i = [], -1, 0
    while i < len(tok):
        t = tok[i]
        if t in ("ROOT", "JOINT"):
            names.append(tok[i + 1])
            parents.append(stack[-1] if stack else -1)
            offsets.append(np.zeros(3))
            channels.append([])
            last = len(names) - 1
            i += 2
        elif t == "End":  # end sites carry no channels
            depth, i = 0, i + 2
            while True:
                if tok[i] == "{":
                    depth += 1
                elif tok[i] == "}":
                    depth -= 1
                    if depth == 0:
                        i += 1
                        break
                i += 1
        elif t == "{":
            stack.append(last)
            i += 1
        elif t == "}":
            stack.pop()
            i += 1
        elif t == "OFFSET":
            offsets[last] = np.array([float(x) for x in tok[i + 1 : i + 4]])
            i += 4
        elif t == "CHANNELS":
            n = int(tok[i + 1])
            channels[last] = tok[i + 2 : i + 2 + n]
            i += 2 + n
        elif t == "MOTION":
            i += 1
            break
        else:
            i += 1
    nf = int(tok[i + 1])
    ft = float(tok[i + 4])
    width = sum(len(c) for c in channels)
    data = np.array(tok[i + 5 : i + 5 + nf * width], dtype=np.float64).reshape(nf, width)

    J = len(names)
    R = np.empty((nf, J, 3, 3))
    P = np.empty((nf, J, 3))
    col = 0
    for j in range(J):
        local = np.broadcast_to(np.eye(3), (nf, 3, 3)).copy()
        trans = np.broadcast_to(offsets[j], (nf, 3)).copy()
        for ch in channels[j]:
            v = data[:, col]
            col += 1
            if ch.endswith("rotation"):
                local = local @ _axis_mats(ch[0].upper(), v)  # listed order, intrinsic
            elif ch.endswith("position"):
                trans[:, "XYZ".index(ch[0].upper())] = v
        p = parents[j]
        if p < 0:
            R[:, j], P[:, j] = local, trans
        else:
            R[:, j] = R[:, p] @ local
            P[:, j] = P[:, p] + np.einsum("fij,fj->fi", R[:, p], trans)
    return Motion(names, parents, P, 1.0 / ft if ft > 0 else 30.0)
