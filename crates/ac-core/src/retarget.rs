//! Joint positions of any humanoid skeleton -> the rig's rotations and root offsets.
//!
//! A port of `training/ac_train/retarget.py`, operation for operation, pinned to its results by
//! `crates/ac-core/tests/golden/retarget-walk.json` (written by `python -m ac_train.fbxwrite`).
//!
//! The source gives world positions of a few landmarks per frame (hips, neck, head, and per side
//! the shoulder, elbow, wrist, hip, knee, ankle, toe; a spine joint is optional), in the rig's axes
//! (metres, Z up, facing -Y). The retarget walks the rig's bones parents first and turns each solved
//! bone so that its child joint points the way the source's segment points (the smallest turn).
//! The root and the chest also take their facing from the hip and shoulder lines, the spine bends
//! evenly over its five bones, and each arm and leg is solved as a two-bone hinge in one bend
//! plane. Bones without a source (fingers, eyes, twist bones) keep the rest rotation.
//!
//! The rig's root sits about 11.5 cm above the hip joints, so the pelvis is placed so that its hip
//! joints land on the source's, not the root on the source's "hips" joint.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::error::CoreError;
use crate::quat::{self, MAT3_IDENTITY, Mat3, Quat, Vec3};
use crate::rig::Skeleton;

/// The landmarks a source must give.
pub const LANDMARKS: [&str; 17] = [
    "hips",
    "neck",
    "head",
    "l_shoulder",
    "l_elbow",
    "l_wrist",
    "l_hip",
    "l_knee",
    "l_ankle",
    "l_toe",
    "r_shoulder",
    "r_elbow",
    "r_wrist",
    "r_hip",
    "r_knee",
    "r_ankle",
    "r_toe",
];
/// The optional landmark: the first spine joint above the hips (the pelvis's own tilt).
pub const SPINE: &str = "spine";
/// Most frames one retarget takes (the solve keeps every frame of every bone in memory).
pub const MAX_FRAMES: usize = 20_000;

const SPINE_BONES: [&str; 5] = ["spine05", "spine04", "spine03", "spine02", "spine01"];
/// Without a source pelvis: how far it follows the trunk's tilt (the spine bends the rest).
const PELVIS_SHARE: f64 = 0.5;
/// Degrees: below this bend the source plane is ignored (the roll is interpolated in time).
const STRAIGHT: f64 = 20.0;
/// Degrees: from this bend on, the source plane sets the roll outright.
const TRUST: f64 = 35.0;
/// Arms reach in proportion: the rig's arm length over the actor's scales the way to the wrist.
const ARM_REACH: bool = true;
const EPS: f64 = 1e-12;

/// Landmark positions per frame, in the rig's axes and metres.
#[derive(Debug, Clone)]
pub struct Landmarks {
    frames: usize,
    points: BTreeMap<String, Vec<Vec3>>,
}

impl Landmarks {
    /// Every name of [`LANDMARKS`] must be there with the same number of finite positions; other
    /// names ([`SPINE`]) are kept too.
    pub fn new(points: BTreeMap<String, Vec<Vec3>>) -> Result<Landmarks, CoreError> {
        for name in LANDMARKS {
            if !points.contains_key(name) {
                return Err(CoreError::Rig(format!("landmarks: no {name}")));
            }
        }
        let frames = points.get("hips").map_or(0, Vec::len);
        if frames == 0 {
            return Err(CoreError::Rig("landmarks: no frames".into()));
        }
        if frames > MAX_FRAMES {
            return Err(CoreError::Rig(format!(
                "landmarks: {frames} frames is more than {MAX_FRAMES}; cut the take into shorter ones"
            )));
        }
        for (name, p) in &points {
            if p.len() != frames {
                return Err(CoreError::Rig(format!(
                    "landmarks: {name} has {} frames, hips {frames}",
                    p.len()
                )));
            }
            if p.iter().any(|v| v.iter().any(|x| !x.is_finite())) {
                return Err(CoreError::Rig(format!(
                    "landmarks: {name} has a non-finite position"
                )));
            }
        }
        Ok(Landmarks { frames, points })
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The positions of a landmark (empty when it is not there: only [`SPINE`] can be).
    pub fn get(&self, name: &str) -> &[Vec3] {
        self.points.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.points.keys().map(String::as_str)
    }

    /// Every position scaled by `s`.
    pub fn scaled(&self, s: f64) -> Landmarks {
        Landmarks {
            frames: self.frames,
            points: self
                .points
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().map(|&p| quat::scale3(p, s)).collect()))
                .collect(),
        }
    }
}

fn at(v: &[Vec3], f: usize) -> Vec3 {
    v.get(f).copied().unwrap_or_default()
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `v / max(|v|, 1e-12)` (numpy's `_unit`).
fn unit(v: Vec3) -> Vec3 {
    quat::scale3(v, 1.0 / quat::len3(v).max(EPS))
}

fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

fn skew(v: Vec3) -> Mat3 {
    [[0.0, -v[2], v[1]], [v[2], 0.0, -v[0]], [-v[1], v[0], 0.0]]
}

fn mat_add(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = a[r][c] + b[r][c];
        }
    }
    out
}

fn mat_scale(a: &Mat3, s: f64) -> Mat3 {
    a.map(|row| row.map(|x| x * s))
}

fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    quat::mat3_mul(a, b)
}

fn apply(m: &Mat3, v: Vec3) -> Vec3 {
    quat::mat3_vec(m, v)
}

/// The rotation by `angle` about the unit `axis` (Rodrigues).
fn rodrigues(axis: Vec3, angle: f64) -> Mat3 {
    let k = skew(axis);
    let kk = mul(&k, &k);
    mat_add(
        &mat_add(&MAT3_IDENTITY, &mat_scale(&k, angle.sin())),
        &mat_scale(&kk, 1.0 - angle.cos()),
    )
}

/// The smallest rotation taking direction `a` to direction `b`.
fn between(a: Vec3, b: Vec3) -> Mat3 {
    let (a, b) = (unit(a), unit(b));
    let v = quat::cross(a, b);
    let c = dot(a, b);
    if c < -0.999999 {
        // opposite directions: half turn about any perpendicular
        let p = quat::cross(a, [1.0, 0.0, 0.0]);
        let perp = unit([p[0] + 1e-9, p[1] + 1e-9, p[2] + 1e-9]);
        let mut out = [[0.0; 3]; 3];
        for (r, row) in out.iter_mut().enumerate() {
            for (col, cell) in row.iter_mut().enumerate() {
                *cell = 2.0 * perp[r] * perp[col] - if r == col { 1.0 } else { 0.0 };
            }
        }
        return out;
    }
    let k = skew(v);
    let kk = mul(&k, &k);
    mat_add(
        &mat_add(&MAT3_IDENTITY, &k),
        &mat_scale(&kk, 1.0 / (1.0 + c).max(1e-9)),
    )
}

/// An orthonormal frame with columns (primary, secondary', primary x secondary').
fn frame(primary: Vec3, secondary: Vec3) -> Mat3 {
    let x = unit(primary);
    let y = unit(quat::sub3(secondary, quat::scale3(x, dot(secondary, x))));
    let z = quat::cross(x, y);
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

/// `R^t` (axis-angle scaling).
fn power(r: &Mat3, t: f64) -> Mat3 {
    let trace = r[0][0] + r[1][1] + r[2][2];
    let ang = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
    let axis = unit([
        r[2][1] - r[1][2] + EPS,
        r[0][2] - r[2][0] + EPS,
        r[1][0] - r[0][1] + EPS,
    ]);
    rodrigues(axis, ang * t)
}

/// Rotation matrix -> unit quaternion (x, y, z, w) with w >= 0, as the Python retarget does it
/// (not Shepperd's method: the goldens pin the bits).
fn mat_to_quat(r: &Mat3) -> Quat {
    let w = (1.0 + r[0][0] + r[1][1] + r[2][2]).max(0.0).sqrt() / 2.0;
    let x = (1.0 + r[0][0] - r[1][1] - r[2][2]).max(0.0).sqrt() / 2.0;
    let y = (1.0 - r[0][0] + r[1][1] - r[2][2]).max(0.0).sqrt() / 2.0;
    let z = (1.0 - r[0][0] - r[1][1] + r[2][2]).max(0.0).sqrt() / 2.0;
    let q = [
        x.copysign(r[2][1] - r[1][2]),
        y.copysign(r[0][2] - r[2][0]),
        z.copysign(r[1][0] - r[0][1]),
        w,
    ];
    let n = quat::norm(q);
    if !(n.is_finite() && n > 0.0) {
        return quat::IDENTITY;
    }
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

/// numpy's median: the middle value, or the mean of the two middle ones.
pub fn median(values: &[f64]) -> f64 {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let mid = v.get(n / 2).copied().unwrap_or(0.0);
    if n % 2 == 1 {
        mid
    } else {
        (v.get(n / 2 - 1).copied().unwrap_or(0.0) + mid) / 2.0
    }
}

/// numpy's percentile (linear between the two nearest ranks).
pub fn percentile(values: &[f64], q: f64) -> f64 {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let pos = q / 100.0 * (v.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let a = v.get(lo).copied().unwrap_or(0.0);
    let b = v.get(hi).copied().unwrap_or(a);
    a + (b - a) * (pos - lo as f64)
}

/// numpy's unwrap: jumps of more than pi between neighbours are taken back by 2 pi.
fn unwrap(p: &[f64]) -> Vec<f64> {
    let mut out = p.to_vec();
    let mut correction = 0.0;
    for i in 1..p.len() {
        let (Some(&cur), Some(&prev)) = (p.get(i), p.get(i - 1)) else {
            break;
        };
        let dd = cur - prev;
        let mut ddmod = (dd + PI).rem_euclid(2.0 * PI) - PI;
        if ddmod == -PI && dd > 0.0 {
            ddmod = PI;
        }
        let ph = if dd.abs() < PI { 0.0 } else { ddmod - dd };
        correction += ph;
        if let Some(o) = out.get_mut(i) {
            *o = cur + correction;
        }
    }
    out
}

/// numpy's interp: linear between the points `(xp[i], fp[i])` (xp rising), held flat outside.
fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let n = xp.len().min(fp.len());
    if n == 0 {
        return 0.0;
    }
    let (Some(&x0), Some(&f0), Some(&xn), Some(&fn_)) =
        (xp.first(), fp.first(), xp.get(n - 1), fp.get(n - 1))
    else {
        return 0.0;
    };
    if x <= x0 {
        return f0;
    }
    if x >= xn {
        return fn_;
    }
    // the last i with xp[i] <= x
    let i = xp
        .get(..n)
        .map_or(0, |s| s.partition_point(|&v| v <= x))
        .saturating_sub(1);
    let (Some(&xa), Some(&xb), Some(&fa), Some(&fb)) = (xp.get(i), xp.get(i + 1), fp.get(i), fp.get(i + 1))
    else {
        return fn_;
    };
    if xb - xa <= 0.0 {
        return fa;
    }
    fa + (fb - fa) * (x - xa) / (xb - xa)
}

/// The rig's bones the retarget drives, looked up once.
struct Bones {
    neck01: usize,
    upperleg_l: usize,
    upperleg_r: usize,
    upperarm_l: usize,
    upperarm_r: usize,
    spine: [usize; 5],
}

fn bone(skel: &Skeleton, name: &str) -> Result<usize, CoreError> {
    skel.bone(name).ok_or_else(|| {
        CoreError::Rig(format!(
            "the retarget needs the default rig's bone {name:?}, which this rig lacks"
        ))
    })
}

fn head(skel: &Skeleton, i: usize) -> Vec3 {
    skel.heads.get(i).copied().unwrap_or_default()
}

/// (rig bone, its child joint whose direction is matched, source segment from -> to; from `None`
/// = the bone's own head, i.e. aim at the source point).
fn aims() -> Vec<(String, String, Option<&'static str>, &'static str)> {
    let mut out = vec![("neck01".to_string(), "head".to_string(), Some("neck"), "head")];
    for (s, c) in [("L", "l"), ("R", "r")] {
        let lm = |k: &str| -> &'static str {
            match (c, k) {
                ("l", "shoulder") => "l_shoulder",
                ("l", "elbow") => "l_elbow",
                ("l", "wrist") => "l_wrist",
                ("l", "hip") => "l_hip",
                ("l", "knee") => "l_knee",
                ("l", "ankle") => "l_ankle",
                ("l", "toe") => "l_toe",
                ("r", "shoulder") => "r_shoulder",
                ("r", "elbow") => "r_elbow",
                ("r", "wrist") => "r_wrist",
                ("r", "hip") => "r_hip",
                ("r", "knee") => "r_knee",
                ("r", "ankle") => "r_ankle",
                _ => "r_toe",
            }
        };
        // a clavicle starts beside the neck, not at it: it aims at the shoulder point itself
        out.push((
            format!("clavicle.{s}"),
            format!("upperarm01.{s}"),
            None,
            lm("shoulder"),
        ));
        out.push((
            format!("upperarm01.{s}"),
            format!("lowerarm01.{s}"),
            Some(lm("shoulder")),
            lm("elbow"),
        ));
        out.push((
            format!("lowerarm01.{s}"),
            format!("wrist.{s}"),
            Some(lm("elbow")),
            lm("wrist"),
        ));
        out.push((
            format!("upperleg01.{s}"),
            format!("lowerleg01.{s}"),
            Some(lm("hip")),
            lm("knee"),
        ));
        out.push((
            format!("lowerleg01.{s}"),
            format!("foot.{s}"),
            Some(lm("knee")),
            lm("ankle"),
        ));
        out.push((
            format!("foot.{s}"),
            format!("toe3-1.{s}"),
            Some(lm("ankle")),
            lm("toe"),
        ));
    }
    out
}

/// Hinge limbs: (upper bone, its lower bone, the lower's child, source root -> mid -> end).
fn hinges() -> Vec<(String, String, String, &'static str, &'static str, &'static str)> {
    let mut out = Vec::new();
    for (s, c) in [("L", "l"), ("R", "r")] {
        let (hip, knee, ankle, sh, el, wr) = if c == "l" {
            ("l_hip", "l_knee", "l_ankle", "l_shoulder", "l_elbow", "l_wrist")
        } else {
            ("r_hip", "r_knee", "r_ankle", "r_shoulder", "r_elbow", "r_wrist")
        };
        out.push((
            format!("upperleg01.{s}"),
            format!("lowerleg01.{s}"),
            format!("foot.{s}"),
            hip,
            knee,
            ankle,
        ));
        out.push((
            format!("upperarm01.{s}"),
            format!("lowerarm01.{s}"),
            format!("wrist.{s}"),
            sh,
            el,
            wr,
        ));
    }
    out
}

/// The pelvis's up direction on every frame (see `_pelvis_up` in the Python).
fn pelvis_up(src: &Landmarks) -> Vec<Vec3> {
    let f = src.frames();
    let (hips, neck) = (src.get("hips"), src.get("neck"));
    let trunk: Vec<Vec3> = (0..f)
        .map(|i| unit(quat::sub3(at(neck, i), at(hips, i))))
        .collect();
    let spine = src.get(SPINE);
    let seg_len: Vec<f64> = (0..f)
        .map(|i| quat::len3(quat::sub3(at(spine, i), at(hips, i))))
        .collect();
    if spine.len() != f || median(&seg_len) < 0.01 {
        // absent, or no length to aim with: a fixed share of the trunk and the thighs (knees -> hips)
        let (lh, rh, lk, rk) = (
            src.get("l_hip"),
            src.get("r_hip"),
            src.get("l_knee"),
            src.get("r_knee"),
        );
        return (0..f)
            .map(|i| {
                let thighs = unit(quat::sub3(
                    quat::add3(at(lh, i), at(rh, i)),
                    quat::add3(at(lk, i), at(rk, i)),
                ));
                unit(quat::add3(
                    quat::scale3(at(&trunk, i), PELVIS_SHARE),
                    quat::scale3(thighs, 1.0 - PELVIS_SHARE),
                ))
            })
            .collect();
    }
    let up: Vec<Vec3> = (0..f)
        .map(|i| unit(quat::sub3(at(spine, i), at(hips, i))))
        .collect();
    let (lh, rh) = (src.get("l_hip"), src.get("r_hip"));
    let side: Vec<Vec3> = (0..f).map(|i| unit(quat::sub3(at(lh, i), at(rh, i)))).collect();
    // the pelvis's lean against the trunk about the hip line, on the upright frames
    let ang: Vec<f64> = (0..f)
        .map(|i| {
            let (t, u, s) = (at(&trunk, i), at(&up, i), at(&side, i));
            dot(s, quat::cross(t, u)).atan2(dot(t, u))
        })
        .collect();
    let upright: Vec<f64> = (0..f)
        .filter(|&i| at(&trunk, i)[2].clamp(-1.0, 1.0).acos().to_degrees() < 15.0)
        .filter_map(|i| ang.get(i).copied())
        .collect();
    let off = if upright.len() >= 10 {
        median(&upright)
    } else {
        median(&ang)
    };
    (0..f)
        .map(|i| unit(apply(&rodrigues(at(&side, i), -off), at(&up, i))))
        .collect()
}

/// Two-bone solve of a hinge limb over every frame: (the upper bone's world rotation, where its
/// end goes). See `_limb` in the Python for the reasoning (one bend plane for the knee/elbow and
/// the upper bone's roll; a near-straight limb's roll interpolated in time).
#[allow(clippy::too_many_arguments)]
fn limb(
    rp: &[Mat3],
    a_pos: &[Vec3],
    rest_n: Vec3,
    la: f64,
    lb: f64,
    rest_lower: Vec3,
    src: &Landmarks,
    hinge: &(String, String, String, &'static str, &'static str, &'static str),
    reach: f64,
) -> (Vec<Mat3>, Vec<Vec3>) {
    let f = src.frames();
    let (sa, sm, sz) = (src.get(hinge.3), src.get(hinge.4), src.get(hinge.5));
    let mut to = Vec::with_capacity(f);
    let mut u = Vec::with_capacity(f);
    let mut bend = Vec::with_capacity(f);
    let mut reference = Vec::with_capacity(f);
    let mut want = Vec::with_capacity(f);
    let mut theta = Vec::with_capacity(f);
    for i in 0..f {
        let t = quat::scale3(quat::sub3(at(sz, i), at(a_pos, i)), reach);
        let ui = unit(t);
        let flat = |v: Vec3| unit(quat::sub3(v, quat::scale3(ui, dot(v, ui))));
        let seg1 = quat::sub3(at(sm, i), at(sa, i));
        let seg2 = quat::sub3(at(sz, i), at(sm, i));
        let w = quat::cross(seg1, seg2);
        bend.push(quat::len3(w).atan2(dot(seg1, seg2)).to_degrees());
        let r = flat(apply(&rp.get(i).copied().unwrap_or(MAT3_IDENTITY), rest_n));
        let w = flat(w);
        theta.push(dot(ui, quat::cross(r, w)).atan2(dot(r, w)));
        reference.push(r);
        want.push(w);
        to.push(t);
        u.push(ui);
    }
    let strong: Vec<usize> = (0..f).filter(|&i| at_f(&bend, i) >= TRUST).collect();
    let angle: Vec<f64> = if strong.is_empty() {
        vec![0.0; f] // never clearly bent: the parent's plane
    } else {
        let xp: Vec<f64> = strong.iter().map(|&i| i as f64).collect();
        let fp = unwrap(&strong.iter().map(|&i| at_f(&theta, i)).collect::<Vec<_>>());
        (0..f)
            .map(|i| {
                let held = interp(i as f64, &xp, &fp);
                let tw = ((at_f(&bend, i) - STRAIGHT) / (TRUST - STRAIGHT)).clamp(0.0, 1.0);
                // to want or -want, whichever is nearer
                let near = (at_f(&theta, i) - held + PI / 2.0).rem_euclid(PI) - PI / 2.0;
                held + tw * near
            })
            .collect()
    };
    let mut out_r = Vec::with_capacity(f);
    let mut out_end = Vec::with_capacity(f);
    for i in 0..f {
        let (ui, ti, a) = (at(&u, i), at(&to, i), at(a_pos, i));
        let n = apply(&rodrigues(ui, at_f(&angle, i)), at(&reference, i));
        // the knee/elbow in that plane
        let d = quat::len3(ti).clamp((la - lb).abs() + 1e-6, la + lb - 1e-6);
        let x = (la * la - lb * lb + d * d) / (2.0 * d);
        let h = (la * la - x * x).max(0.0).sqrt();
        let knee = quat::add3(
            quat::add3(a, quat::scale3(ui, x)),
            quat::scale3(unit(quat::cross(ui, n)), h),
        );
        // the upper bone: aim its child at the knee, then roll it about itself until its rest
        // normal is n
        let rp_i = rp.get(i).copied().unwrap_or(MAT3_IDENTITY);
        let ri = mul(&between(apply(&rp_i, rest_lower), quat::sub3(knee, a)), &rp_i);
        let axis = unit(quat::sub3(knee, a));
        let cur = apply(&ri, rest_n);
        let cur = unit(quat::sub3(cur, quat::scale3(axis, dot(cur, axis))));
        let roll = dot(axis, quat::cross(cur, n)).atan2(dot(cur, n));
        out_r.push(mul(&rodrigues(axis, roll), &ri));
        out_end.push(quat::add3(a, ti));
    }
    (out_r, out_end)
}

fn at_f(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(0.0)
}

/// Rotations `[frame][bone]` and root offsets `[frame]` of the rig following the source
/// landmarks. `src` is in the rig's axes (Z up) and metres, its legs scaled to the rig's (see
/// [`scale_to`]).
pub fn retarget(skel: &Skeleton, src: &Landmarks) -> Result<(Vec<Vec<Quat>>, Vec<Vec3>), CoreError> {
    let f = src.frames();
    let b = skel.len();
    let bones = Bones {
        neck01: bone(skel, "neck01")?,
        upperleg_l: bone(skel, "upperleg01.L")?,
        upperleg_r: bone(skel, "upperleg01.R")?,
        upperarm_l: bone(skel, "upperarm01.L")?,
        upperarm_r: bone(skel, "upperarm01.R")?,
        spine: [
            bone(skel, "spine05")?,
            bone(skel, "spine04")?,
            bone(skel, "spine03")?,
            bone(skel, "spine02")?,
            bone(skel, "spine01")?,
        ],
    };
    let aim_list = aims();
    let hinge_list = hinges();
    let mut aim_of: BTreeMap<usize, (usize, Option<&str>, &str)> = BTreeMap::new();
    for (bn, child, a, z) in &aim_list {
        aim_of.insert(bone(skel, bn)?, (bone(skel, child)?, *a, z));
    }
    let mut hinge_of: BTreeMap<usize, (usize, usize, usize)> = BTreeMap::new();
    for (i, (upper, lower, end, _, _, _)) in hinge_list.iter().enumerate() {
        hinge_of.insert(bone(skel, upper)?, (i, bone(skel, lower)?, bone(skel, end)?));
    }
    let h0 = head(skel, 0);

    // the root: hips position, facing from the hip line, tilted as the source's own pelvis
    let hip_line = quat::sub3(head(skel, bones.upperleg_l), head(skel, bones.upperleg_r));
    let rest = frame(quat::sub3(head(skel, bones.neck01), h0), hip_line);
    let rest_t = transpose(&rest);
    let pelvis = pelvis_up(src);
    let (lh, rh) = (src.get("l_hip"), src.get("r_hip"));
    let mid_rest = quat::scale3(
        quat::add3(head(skel, bones.upperleg_l), head(skel, bones.upperleg_r)),
        0.5,
    );
    let mut r: Vec<Vec<Mat3>> = vec![vec![MAT3_IDENTITY; b]; f];
    let mut p: Vec<Vec<Vec3>> = vec![vec![[0.0; 3]; b]; f];
    let mut local: Vec<Vec<Mat3>> = vec![vec![MAT3_IDENTITY; b]; f];
    let mut root = Vec::with_capacity(f);
    for i in 0..f {
        let want = frame(at(&pelvis, i), quat::sub3(at(lh, i), at(rh, i)));
        let l0 = mul(&want, &rest_t);
        let mid_src = quat::scale3(quat::add3(at(lh, i), at(rh, i)), 0.5);
        // place the pelvis so its hip joints land on the source's
        let ro = quat::scale3(
            quat::sub3(quat::sub3(mid_src, apply(&l0, quat::sub3(mid_rest, h0))), h0),
            1.0 / skel.hip_height,
        );
        root.push(ro);
        if let (Some(lf), Some(rf), Some(pf)) = (local.get_mut(i), r.get_mut(i), p.get_mut(i)) {
            if let Some(x) = lf.get_mut(0) {
                *x = l0;
            }
            if let Some(x) = rf.get_mut(0) {
                *x = l0;
            }
            if let Some(x) = pf.get_mut(0) {
                *x = quat::add3(h0, quat::scale3(ro, skel.hip_height));
            }
        }
    }

    let mut spine_turn: Option<Vec<Mat3>> = None;
    let mut ends: BTreeMap<usize, Vec<Vec3>> = BTreeMap::new();
    for bi in 1..b {
        let Some(parent) = skel.parents.get(bi).copied().flatten() else {
            continue;
        };
        let hi = head(skel, bi);
        let hp = head(skel, parent);
        let off = quat::sub3(hi, hp);
        let rp: Vec<Mat3> = (0..f)
            .map(|i| {
                r.get(i)
                    .and_then(|row| row.get(parent))
                    .copied()
                    .unwrap_or(MAT3_IDENTITY)
            })
            .collect();
        let pos: Vec<Vec3> = (0..f)
            .map(|i| {
                let pp = p
                    .get(i)
                    .and_then(|row| row.get(parent))
                    .copied()
                    .unwrap_or_default();
                quat::add3(pp, apply(&at_m(&rp, i), off))
            })
            .collect();
        let mut ri: Vec<Mat3> = rp.clone(); // R[parent] @ local[i], local being the identity so far
        if bones.spine.contains(&bi) {
            if spine_turn.is_none() {
                spine_turn = Some(spine_turns(skel, &bones, &ri, &pos, src));
            }
            if let Some(st) = &spine_turn {
                for i in 0..f {
                    if let Some(x) = ri.get_mut(i) {
                        *x = mul(&at_m(st, i), x);
                    }
                }
            }
        } else if let Some(&(child, a, z)) = aim_of.get(&bi) {
            let child_off = quat::sub3(head(skel, child), hi);
            if let Some(&(hidx, lower, end)) = hinge_of.get(&bi) {
                let Some(hinge) = hinge_list.get(hidx) else {
                    continue;
                };
                let rest_lower = quat::sub3(head(skel, lower), hi);
                let rest_end = quat::sub3(head(skel, end), head(skel, lower));
                let rest_n = unit(quat::cross(rest_lower, rest_end));
                let la = quat::len3(rest_lower);
                let lb = quat::len3(rest_end);
                let mut reach = 1.0;
                if ARM_REACH && skel.names.get(bi).is_some_and(|n| n.starts_with("upperarm")) {
                    let own = la + lb;
                    let (sa, sm, sz) = (src.get(hinge.3), src.get(hinge.4), src.get(hinge.5));
                    let theirs: Vec<f64> = (0..f)
                        .map(|i| {
                            quat::len3(quat::sub3(at(sm, i), at(sa, i)))
                                + quat::len3(quat::sub3(at(sz, i), at(sm, i)))
                        })
                        .collect();
                    reach = own / median(&theirs).max(1e-9);
                }
                let (solved, end_at) = limb(&rp, &pos, rest_n, la, lb, rest_lower, src, hinge, reach);
                ri = solved;
                ends.insert(lower, end_at);
            } else {
                // the shin/forearm points at its hinge solve's end, every other bone along its segment
                let end_at = ends.get(&bi);
                let (sz, sa) = (src.get(z), a.map(|n| src.get(n)));
                for i in 0..f {
                    let cur = apply(&at_m(&ri, i), child_off);
                    let aim = match (end_at, sa) {
                        (Some(e), _) => quat::sub3(at(e, i), at(&pos, i)),
                        (None, Some(sa)) => quat::sub3(at(sz, i), at(sa, i)),
                        (None, None) => quat::sub3(at(sz, i), at(&pos, i)),
                    };
                    if let Some(x) = ri.get_mut(i) {
                        *x = mul(&between(cur, aim), x);
                    }
                }
            }
        }
        for i in 0..f {
            let rpi = at_m(&rp, i);
            let rii = at_m(&ri, i);
            if let Some(row) = local.get_mut(i)
                && let Some(x) = row.get_mut(bi)
            {
                *x = mul(&transpose(&rpi), &rii);
            }
            if let Some(row) = r.get_mut(i)
                && let Some(x) = row.get_mut(bi)
            {
                *x = rii;
            }
            if let Some(row) = p.get_mut(i)
                && let Some(x) = row.get_mut(bi)
            {
                *x = at(&pos, i);
            }
        }
    }
    let rots = local
        .iter()
        .map(|row| row.iter().map(mat_to_quat).collect())
        .collect();
    Ok((rots, root))
}

fn at_m(v: &[Mat3], i: usize) -> Mat3 {
    v.get(i).copied().unwrap_or(MAT3_IDENTITY)
}

/// The chest's turn against the root, spread evenly over the five spine bones, re-aimed so the
/// chain still reaches the neck.
fn spine_turns(skel: &Skeleton, bones: &Bones, ri: &[Mat3], pos: &[Vec3], src: &Landmarks) -> Vec<Mat3> {
    let f = src.frames();
    let i0 = bones.spine.first().copied().unwrap_or(0);
    let neck_off = quat::sub3(head(skel, bones.neck01), head(skel, i0));
    let shoulder_line = quat::sub3(head(skel, bones.upperarm_l), head(skel, bones.upperarm_r));
    let chain: Vec<usize> = bones.spine.iter().copied().chain([bones.neck01]).collect();
    let segs: Vec<Vec3> = chain
        .windows(2)
        .map(|w| {
            quat::sub3(
                head(skel, w.get(1).copied().unwrap_or(0)),
                head(skel, w.first().copied().unwrap_or(0)),
            )
        })
        .collect();
    let (neck, ls, rs) = (src.get("neck"), src.get("l_shoulder"), src.get("r_shoulder"));
    (0..f)
        .map(|i| {
            let rii = at_m(ri, i);
            let cur = frame(apply(&rii, neck_off), apply(&rii, shoulder_line));
            let want_dir = quat::sub3(at(neck, i), at(pos, i));
            let mut tgt = frame(want_dir, quat::sub3(at(ls, i), at(rs, i)));
            let mut turn = MAT3_IDENTITY;
            for _ in 0..4 {
                turn = power(&mul(&tgt, &transpose(&cur)), 1.0 / SPINE_BONES.len() as f64);
                let mut m = rii;
                let mut end = [0.0; 3];
                for s in &segs {
                    m = mul(&turn, &m);
                    end = quat::add3(end, apply(&m, *s));
                }
                tgt = mul(&between(end, want_dir), &tgt);
            }
            turn
        })
        .collect()
}

/// The scale that makes the source's leg length (hip -> ankle, the median over the frames) the
/// rig's.
pub fn scale_factor(skel: &Skeleton, src: &Landmarks) -> Result<f64, CoreError> {
    let f = src.frames();
    let (lh, lk, la) = (src.get("l_hip"), src.get("l_knee"), src.get("l_ankle"));
    let legs: Vec<f64> = (0..f)
        .map(|i| quat::len3(quat::sub3(at(lh, i), at(lk, i))) + quat::len3(quat::sub3(at(lk, i), at(la, i))))
        .collect();
    let leg_src = median(&legs);
    let (u, l, ft) = (
        bone(skel, "upperleg01.L")?,
        bone(skel, "lowerleg01.L")?,
        bone(skel, "foot.L")?,
    );
    let leg_rig = quat::len3(quat::sub3(head(skel, u), head(skel, l)))
        + quat::len3(quat::sub3(head(skel, l), head(skel, ft)));
    Ok(leg_rig / leg_src.max(1e-9))
}

/// The source scaled so its leg length matches the rig's; positions keep their axes.
pub fn scale_to(skel: &Skeleton, src: &Landmarks) -> Result<(Landmarks, f64), CoreError> {
    let s = scale_factor(skel, src)?;
    Ok((src.scaled(s), s))
}

/// The feet points the floor is found from: the ankles and the balls of the feet.
const FEET: [&str; 4] = ["foot.L", "foot.R", "toe2-1.L", "toe2-1.R"];
/// A floor further off than this is not levelled: the take is odd, and the result says so.
pub const FLOOR_MOST_M: f64 = 0.2;

/// How far the lowest foot point is above where it would be standing flat, on every frame.
fn lowest(skel: &Skeleton, rots: &[Vec<Quat>], roots: &[Vec3]) -> Result<Vec<f64>, CoreError> {
    let feet: Vec<usize> = FEET.iter().map(|b| bone(skel, b)).collect::<Result<_, _>>()?;
    // how far the rest pose rises to stand on the floor
    let lift = skel.hip_height - head(skel, 0)[2];
    let flat: Vec<f64> = feet.iter().map(|&b| head(skel, b)[2] + lift).collect();
    let mut out = Vec::with_capacity(rots.len());
    for (q, &ro) in rots.iter().zip(roots) {
        let (_, pos) = skel.fk(q, ro)?;
        let low = feet
            .iter()
            .zip(&flat)
            .map(|(&b, &fl)| pos.get(b).map_or(0.0, |p| p[2]) - fl)
            .fold(f64::INFINITY, f64::min);
        out.push(low);
    }
    Ok(out)
}

/// Where a take's floor is: a low percentile of the lowest foot point (the feet are down on most
/// frames; a few frames below it are noise or a foot through the floor).
pub fn floor_offset(skel: &Skeleton, rots: &[Vec<Quat>], roots: &[Vec3]) -> Result<f64, CoreError> {
    Ok(percentile(&lowest(skel, rots, roots)?, 5.0))
}

/// Root offsets with the take's floor moved to z = 0, and the offset taken off (0 when the floor
/// is more than [`FLOOR_MOST_M`] off: the take is left as it is).
pub fn level(skel: &Skeleton, rots: &[Vec<Quat>], roots: &[Vec3]) -> Result<(Vec<Vec3>, f64), CoreError> {
    let off = floor_offset(skel, rots, roots)?;
    if off.abs() > FLOOR_MOST_M || !off.is_finite() {
        return Ok((roots.to_vec(), 0.0));
    }
    let moved = roots
        .iter()
        .map(|r| [r[0], r[1], r[2] - off / skel.hip_height])
        .collect();
    Ok((moved, off))
}

/// How far the rig's hip joints land from the source's on the worst frame (metres): the midpoint
/// of the two, and each side on its own.
pub fn hip_error(
    skel: &Skeleton,
    rots: &[Vec<Quat>],
    roots: &[Vec3],
    src: &Landmarks,
) -> Result<[f64; 3], CoreError> {
    let (ul, ur) = (bone(skel, "upperleg01.L")?, bone(skel, "upperleg01.R")?);
    let (lh, rh) = (src.get("l_hip"), src.get("r_hip"));
    let mut worst = [0.0f64; 3];
    for (i, (q, &ro)) in rots.iter().zip(roots).enumerate() {
        let (_, pos) = skel.fk(q, ro)?;
        let (pl, pr) = (at(&pos, ul), at(&pos, ur));
        let (sl, sr) = (at(lh, i), at(rh, i));
        let mid = quat::len3(quat::sub3(
            quat::scale3(quat::add3(pl, pr), 0.5),
            quat::scale3(quat::add3(sl, sr), 0.5),
        ));
        worst = [
            worst[0].max(mid),
            worst[1].max(quat::len3(quat::sub3(pl, sl))),
            worst[2].max(quat::len3(quat::sub3(pr, sr))),
        ];
    }
    Ok(worst)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;

    #[test]
    fn numpy_helpers() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        assert!((percentile(&[0.0, 10.0, 20.0], 5.0) - 1.0).abs() < 1e-12);
        assert_eq!(interp(0.5, &[0.0, 1.0], &[0.0, 2.0]), 1.0);
        assert_eq!(interp(-1.0, &[0.0, 1.0], &[0.0, 2.0]), 0.0);
        assert_eq!(interp(5.0, &[0.0, 1.0], &[0.0, 2.0]), 2.0);
        let u = unwrap(&[0.0, 3.0, -3.0, 0.2]);
        assert!((u[2] - (2.0 * PI - 3.0)).abs() < 1e-12, "{u:?}");
    }

    #[test]
    fn between_and_frame() {
        let m = between([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let v = apply(&m, [1.0, 0.0, 0.0]);
        assert!((v[1] - 1.0).abs() < 1e-12 && v[0].abs() < 1e-12);
        // opposite directions: a half turn about a perpendicular (as the Python, which takes the
        // perpendicular from the cross with X: only an X direction itself gets a lopsided one)
        let flip = between([0.0, 1.0, 0.0], [0.0, -1.0, 0.0]);
        let v = apply(&flip, [0.0, 1.0, 0.0]);
        assert!((v[1] + 1.0).abs() < 1e-6, "{v:?}");
        let fr = frame([0.0, 0.0, 2.0], [1.0, 1.0, 0.0]);
        assert!((fr[2][0] - 1.0).abs() < 1e-12);
        let q = mat_to_quat(&rodrigues([0.0, 0.0, 1.0], PI / 2.0));
        assert!((q[2] - (PI / 4.0).sin()).abs() < 1e-12 && q[3] > 0.0);
    }
}
