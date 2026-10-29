"""Quaternion maths on numpy arrays, (..., 4) as x, y, z, w (the order of the rig's pose library).

Everything is float64 and branch-stable so that the same keys give byte-identical samples.
"""

from __future__ import annotations

import numpy as np

IDENTITY = np.array([0.0, 0.0, 0.0, 1.0])


def normalize(q: np.ndarray) -> np.ndarray:
    q = np.asarray(q, dtype=np.float64)
    return q / np.linalg.norm(q, axis=-1, keepdims=True)


def conj(q: np.ndarray) -> np.ndarray:
    return q * np.array([-1.0, -1.0, -1.0, 1.0])


def mul(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    ax, ay, az, aw = np.moveaxis(a, -1, 0)
    bx, by, bz, bw = np.moveaxis(b, -1, 0)
    return np.stack(
        [
            aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz,
        ],
        axis=-1,
    )


def align(q: np.ndarray, ref: np.ndarray) -> np.ndarray:
    """Flip q to the hemisphere of ref (q and -q are the same rotation)."""
    s = np.sign(np.sum(q * ref, axis=-1, keepdims=True))
    return q * np.where(s == 0, 1.0, s)


def log(q: np.ndarray) -> np.ndarray:
    """Unit quaternion -> rotation vector / 2 (pure imaginary part, (..., 3))."""
    q = normalize(q)
    v = q[..., :3]
    w = np.clip(q[..., 3:], -1.0, 1.0)
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    angle = np.arctan2(n, w)
    scale = np.where(n > 1e-12, angle / np.maximum(n, 1e-12), 1.0)
    return v * scale


def exp(v: np.ndarray) -> np.ndarray:
    """(..., 3) -> unit quaternion; inverse of :func:`log`."""
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    s = np.where(n > 1e-12, np.sin(n) / np.maximum(n, 1e-12), 1.0)
    return np.concatenate([v * s, np.cos(n)], axis=-1)


def slerp(a: np.ndarray, b: np.ndarray, t) -> np.ndarray:
    b = align(b, a)
    t = np.asarray(t, dtype=np.float64)[..., None]
    return normalize(mul(a, exp(log(mul(conj(a), b)) * t)))


def squad(q0, q1, s0, s1, t) -> np.ndarray:
    """Shoemake's spherical cubic between q0 and q1 with inner controls s0, s1."""
    t = np.asarray(t, dtype=np.float64)
    return slerp(slerp(q0, q1, t), slerp(s0, s1, t), 2.0 * t * (1.0 - t))


def squad_arc(q0, q1, s0, s1, t) -> np.ndarray:
    """``squad`` whose inner arc runs from s0 to s1 as they lie, without flipping s1 into s0's
    hemisphere: clamped tangents put the controls far out on a long turn, and a flip would send the
    curve round the other side."""
    t = np.asarray(t, dtype=np.float64)
    inner = normalize(mul(s0, exp(log(mul(conj(s0), s1)) * t[..., None])))
    return slerp(slerp(q0, q1, t), inner, 2.0 * t * (1.0 - t))


def squad_control(prev, cur, nxt) -> np.ndarray:
    """Inner control point of `cur` for a squad through prev, cur, nxt."""
    prev = align(prev, cur)
    nxt = align(nxt, cur)
    ic = conj(cur)
    return normalize(mul(cur, exp(-0.25 * (log(mul(ic, nxt)) + log(mul(ic, prev))))))


def to_euler(q: np.ndarray) -> np.ndarray:
    """Rotation angles (..., 3) in degrees about X, then Y, then Z (q = Rz * Ry * Rx)."""
    x, y, z, w = np.moveaxis(np.asarray(q, dtype=np.float64), -1, 0)
    rx = np.arctan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y))
    ry = np.arcsin(np.clip(2 * (w * y - z * x), -1.0, 1.0))
    rz = np.arctan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z))
    return np.degrees(np.stack([rx, ry, rz], -1))


def from_euler(e: np.ndarray) -> np.ndarray:
    """The rotation of :func:`to_euler` angles (degrees)."""
    rx, ry, rz = np.moveaxis(np.radians(np.asarray(e, dtype=np.float64)), -1, 0)
    cx, sx, cy, sy, cz, sz = np.cos(rx / 2), np.sin(rx / 2), np.cos(ry / 2), np.sin(ry / 2), np.cos(rz / 2), np.sin(rz / 2)
    return np.stack([sx * cy * cz - cx * sy * sz, cx * sy * cz + sx * cy * sz, cx * cy * sz - sx * sy * cz, cx * cy * cz + sx * sy * sz], -1)
