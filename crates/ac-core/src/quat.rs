//! Quaternions as `[x, y, z, w]` in `f64`, and the 3x3 rotation maths FK needs.
//!
//! The formulas follow cf-animate's `quat.py` and `rig.py` operation for operation, so the parity
//! goldens match to round-off.

pub type Quat = [f64; 4];
pub type Vec3 = [f64; 3];
/// Row-major: `m[row][col]`.
pub type Mat3 = [[f64; 3]; 3];

pub const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];
pub const MAT3_IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn norm(q: Quat) -> f64 {
    (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt()
}

/// `q / |q|`; the identity for a zero or non-finite quaternion (hostile input never divides by 0).
pub fn normalize(q: Quat) -> Quat {
    let n = norm(q);
    if !(n.is_finite() && n > 0.0) {
        return IDENTITY;
    }
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

pub fn conj(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

pub fn mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// `q` flipped into the hemisphere of `reference` (q and -q are the same rotation).
pub fn align(q: Quat, reference: Quat) -> Quat {
    let d = q[0] * reference[0] + q[1] * reference[1] + q[2] * reference[2] + q[3] * reference[3];
    if d < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q }
}

/// Unit quaternion -> rotation vector / 2.
pub fn log(q: Quat) -> Vec3 {
    let q = normalize(q);
    let v = [q[0], q[1], q[2]];
    let w = q[3].clamp(-1.0, 1.0);
    let n = len3(v);
    let angle = n.atan2(w);
    let scale = if n > 1e-12 { angle / n.max(1e-12) } else { 1.0 };
    scale3(v, scale)
}

/// Inverse of [`log`].
pub fn exp(v: Vec3) -> Quat {
    let n = len3(v);
    let s = if n > 1e-12 { n.sin() / n.max(1e-12) } else { 1.0 };
    [v[0] * s, v[1] * s, v[2] * s, n.cos()]
}

pub fn slerp(a: Quat, b: Quat, t: f64) -> Quat {
    let b = align(b, a);
    normalize(mul(a, exp(scale3(log(mul(conj(a), b)), t))))
}

/// Shoemake's spherical cubic between `q0` and `q1` with inner controls `s0`, `s1`.
pub fn squad(q0: Quat, q1: Quat, s0: Quat, s1: Quat, t: f64) -> Quat {
    slerp(slerp(q0, q1, t), slerp(s0, s1, t), 2.0 * t * (1.0 - t))
}

/// [`squad`] whose inner arc runs from `s0` to `s1` as they lie, without flipping `s1` into
/// `s0`'s hemisphere: clamped tangents put the controls far out on a long turn, and a flip would
/// send the curve round the other side.
pub fn squad_arc(q0: Quat, q1: Quat, s0: Quat, s1: Quat, t: f64) -> Quat {
    let inner = normalize(mul(s0, exp(scale3(log(mul(conj(s0), s1)), t))));
    slerp(slerp(q0, q1, t), inner, 2.0 * t * (1.0 - t))
}

/// Inner control point of `cur` for a squad through `prev`, `cur`, `next`.
pub fn squad_control(prev: Quat, cur: Quat, next: Quat) -> Quat {
    let prev = align(prev, cur);
    let next = align(next, cur);
    let ic = conj(cur);
    let a = log(mul(ic, next));
    let b = log(mul(ic, prev));
    normalize(mul(cur, exp(scale3(add3(a, b), -0.25))))
}

/// Rotation angles in degrees about X, then Y, then Z (`q = Rz * Ry * Rx`).
pub fn to_euler(q: Quat) -> Vec3 {
    let [x, y, z, w] = q;
    let rx = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let ry = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
    let rz = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
    [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()]
}

/// The rotation of [`to_euler`] angles (degrees).
pub fn from_euler(e: Vec3) -> Quat {
    let (rx, ry, rz) = (e[0].to_radians(), e[1].to_radians(), e[2].to_radians());
    let (cx, sx) = ((rx / 2.0).cos(), (rx / 2.0).sin());
    let (cy, sy) = ((ry / 2.0).cos(), (ry / 2.0).sin());
    let (cz, sz) = ((rz / 2.0).cos(), (rz / 2.0).sin());
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

pub fn to_mat3(q: Quat) -> Mat3 {
    let [x, y, z, w] = normalize(q);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

pub fn mat3_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
        }
    }
    out
}

pub fn mat3_vec(m: &Mat3, v: Vec3) -> Vec3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

pub fn len3(v: Vec3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

pub fn add3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale3(v: Vec3, s: f64) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}

/// Rotation matrix -> unit quaternion with `w >= 0` (Shepperd's method, as cf-animate's
/// `rig.mat_to_quat`).
pub fn from_mat3(m: &Mat3) -> Quat {
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            0.25 * s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[2][1] - m[1][2]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
            (m[0][2] - m[2][0]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
            (m[1][0] - m[0][1]) / s,
        ]
    };
    let q = normalize(q);
    if q[3] >= 0.0 {
        q
    } else {
        [-q[0], -q[1], -q[2], -q[3]]
    }
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
