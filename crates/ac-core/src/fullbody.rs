//! Full-body IK: move one joint to a target while every pinned joint stays where it is, the rest of
//! the body following as little as it can, inside the joint limits.
//!
//! Written fresh (not a port: cf-animate's L-BFGS solver had known bugs). Damped least squares
//! over small world-axis turns:
//!
//! - unknowns: a turn `w_j` (3 numbers, radians about world X, Y, Z) at every movable bone's head,
//!   and a root shift `t` (metres);
//! - goals: the dragged joint at its target, every pin at its pin (3 rows each);
//! - a turn at joint `j` moves a goal joint `e` below it by `w_j x (p_e - p_j)`; the root shift
//!   moves every joint by `t` (an analytic Jacobian, no finite differences);
//! - each step solves `dx = W Jt (J W Jt + l^2 I)^-1 e` (W: 1 / stiffness, so the spine and pelvis
//!   move less than the limbs, l: damping), caps the largest turn, applies it, then clamps every
//!   bone's local rotation vector into its limits;
//! - fingers, toes and eyes are frozen: dragging a body does not curl them.
//!
//! Deterministic: fixed iteration order and count, no randomness.

use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

pub const FROZEN_PREFIXES: &[&str] = &["finger", "metacarpal", "toe", "eye"];
/// Relative stiffness (higher moves less), by name prefix; everything else is 1.
pub const STIFF: &[(&str, f64)] = &[
    ("root", 4.0),
    ("pelvis", 6.0),
    ("spine", 2.0),
    ("neck", 2.0),
    ("head", 1.5),
    ("clavicle", 3.0),
    ("shoulder01", 3.0),
];
/// Stiffness of the root shift (per metre, against radians of turn).
pub const ROOT_SHIFT_STIFF: f64 = 3.0;
pub const ITERATIONS: usize = 60;
/// A goal is met when it is this close (m).
pub const TOLERANCE: f64 = 1e-5;
/// Largest turn (radians) and root shift (m) one step may make.
const MAX_STEP: f64 = 0.25;
const DAMPING: f64 = 0.02;

/// Per-bone ranges of the local rotation vector (radians), from `assets/rig/limits.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub lo: Vec<Vec3>,
    pub hi: Vec<Vec3>,
}

impl Limits {
    /// Parse the limits file and line it up with the rig (by bone name).
    pub fn from_json(text: &str, sk: &Skeleton) -> R<Limits> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("limits: {e}"))?;
        if v["space"] != "rotation_vector_radians" {
            return Err("limits: expected space rotation_vector_radians".into());
        }
        let names: Vec<&str> = v["names"]
            .as_array()
            .ok_or("limits: no names")?
            .iter()
            .filter_map(|x| x.as_str())
            .collect();
        let vec3 = |key: &str, i: usize| -> R<Vec3> {
            let a = v[key][i]
                .as_array()
                .ok_or_else(|| format!("limits: {key}[{i}] missing"))?;
            let f = |k: usize| {
                a.get(k)
                    .and_then(|x| x.as_f64())
                    .ok_or_else(|| format!("limits: {key}[{i}] is not 3 numbers"))
            };
            Ok([f(0)?, f(1)?, f(2)?])
        };
        let mut lo = Vec::with_capacity(sk.len());
        let mut hi = Vec::with_capacity(sk.len());
        for n in &sk.names {
            let i = names
                .iter()
                .position(|x| x == n)
                .ok_or_else(|| format!("limits: no entry for {n}"))?;
            let (l, h) = (vec3("lo", i)?, vec3("hi", i)?);
            if (0..3).any(|k| l[k] > h[k]) {
                return Err(format!("limits: {n} has lo above hi"));
            }
            lo.push(l);
            hi.push(h);
        }
        Ok(Limits { lo, hi })
    }

    /// `q` with its rotation vector clamped into bone `i`'s range.
    pub fn clamp(&self, i: usize, q: Quat) -> Quat {
        let (Some(lo), Some(hi)) = (self.lo.get(i), self.hi.get(i)) else {
            return q;
        };
        let rv = quat::scale3(quat::log(quat::align(q, quat::IDENTITY)), 2.0);
        let c = [
            rv[0].clamp(lo[0], hi[0]),
            rv[1].clamp(lo[1], hi[1]),
            rv[2].clamp(lo[2], hi[2]),
        ];
        if c == rv {
            q
        } else {
            quat::exp(quat::scale3(c, 0.5))
        }
    }

    /// These limits grown just enough to take in `rots` (so a solve starting there does not snap
    /// a keyed pose that was already outside its range).
    pub fn widened(&self, rots: &[Quat]) -> Limits {
        let mut w = self.clone();
        for (i, q) in rots.iter().enumerate().take(w.lo.len()) {
            let rv = quat::scale3(quat::log(quat::align(*q, quat::IDENTITY)), 2.0);
            for (k, v) in rv.iter().enumerate() {
                w.lo[i][k] = w.lo[i][k].min(*v);
                w.hi[i][k] = w.hi[i][k].max(*v);
            }
        }
        w
    }

    /// Whether bone `i`'s rotation is inside its range (within `eps` radians).
    pub fn contains(&self, i: usize, q: Quat, eps: f64) -> bool {
        let rv = quat::scale3(quat::log(quat::align(q, quat::IDENTITY)), 2.0);
        match (self.lo.get(i), self.hi.get(i)) {
            (Some(lo), Some(hi)) => (0..3).all(|k| rv[k] >= lo[k] - eps && rv[k] <= hi[k] + eps),
            _ => true,
        }
    }
}

/// A joint to put somewhere (metres, rig world).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Goal {
    pub bone: usize,
    pub target: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    pub rotations: Vec<Quat>,
    /// Root offset, hip heights.
    pub root: Vec3,
    /// Largest distance (m) of a goal from its target.
    pub error: f64,
    pub iterations: usize,
}

fn stiffness(name: &str) -> f64 {
    STIFF
        .iter()
        .find(|(p, _)| name.starts_with(p))
        .map_or(1.0, |(_, s)| *s)
}

fn frozen(name: &str) -> bool {
    FROZEN_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Solve a small dense system `a x = b` (Gaussian elimination with partial pivoting).
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> R<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n)
            .max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))
            .ok_or("empty system")?;
        if a[p][c].abs() < 1e-14 {
            return Err("the goals pull against each other (singular system)".into());
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            let (top, rest) = a.split_at_mut(r);
            for (x, y) in rest[0][c..n].iter_mut().zip(&top[c][c..n]) {
                *x -= f * y;
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Ok(x)
}

/// Move the goals' joints to their targets from `start` (rotations, root offset in hip heights).
pub fn solve(sk: &Skeleton, start: (&[Quat], Vec3), goals: &[Goal], limits: Option<&Limits>) -> R<Solved> {
    let n = sk.len();
    if start.0.len() != n {
        return Err(format!("pose has {} rotations for {n} bones", start.0.len()));
    }
    if goals.is_empty() {
        return Err("nothing to solve for".into());
    }
    if let Some(g) = goals
        .iter()
        .find(|g| g.bone >= n || g.target.iter().any(|x| !x.is_finite()))
    {
        return Err(format!("bad goal for bone {}", g.bone));
    }
    // which bones move, and which goals each one moves (a turn at j moves the joints below j)
    let movable: Vec<usize> = (0..n).filter(|&j| !frozen(&sk.names[j])).collect();
    let below = |e: usize, j: usize| -> bool {
        let mut k = sk.parents[e];
        while let Some(p) = k {
            if p == j {
                return true;
            }
            k = sk.parents[p];
        }
        false
    };
    let weight: Vec<f64> = movable.iter().map(|&j| 1.0 / stiffness(&sk.names[j])).collect();
    let mut rots: Vec<Quat> = start.0.to_vec();
    let mut root = start.1;
    let h = sk.hip_height;
    let rows = goals.len() * 3;
    let mut iterations = 0;
    let mut error = f64::INFINITY;
    for it in 0..=ITERATIONS {
        let (world, pos) = sk.fk(&rots, root).map_err(|e| e.to_string())?;
        let err: Vec<f64> = goals
            .iter()
            .flat_map(|g| quat::sub3(g.target, pos[g.bone]))
            .collect();
        error = goals
            .iter()
            .map(|g| quat::len3(quat::sub3(g.target, pos[g.bone])))
            .fold(0.0, f64::max);
        iterations = it;
        if error < TOLERANCE || it == ITERATIONS {
            break;
        }
        // columns: 3 per movable bone (turns about world X, Y, Z), then 3 for the root shift
        let cols = movable.len() * 3 + 3;
        let mut jac = vec![vec![0.0; cols]; rows];
        let mut wcol = vec![0.0; cols];
        for (m, &j) in movable.iter().enumerate() {
            for k in 0..3 {
                wcol[m * 3 + k] = weight[m];
            }
            for (gi, g) in goals.iter().enumerate() {
                if !below(g.bone, j) {
                    continue;
                }
                let r = quat::sub3(pos[g.bone], pos[j]);
                for (k, axis) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
                    .iter()
                    .enumerate()
                {
                    let d = quat::cross(*axis, r);
                    for c in 0..3 {
                        jac[gi * 3 + c][m * 3 + k] = d[c];
                    }
                }
            }
        }
        for k in 0..3 {
            wcol[movable.len() * 3 + k] = 1.0 / ROOT_SHIFT_STIFF;
            for gi in 0..goals.len() {
                jac[gi * 3 + k][movable.len() * 3 + k] = 1.0;
            }
        }
        // (J W Jt + l^2 I) y = e ; dx = W Jt y
        let mut a = vec![vec![0.0; rows]; rows];
        for r1 in 0..rows {
            for r2 in 0..rows {
                a[r1][r2] = (0..cols).map(|c| jac[r1][c] * wcol[c] * jac[r2][c]).sum();
            }
            a[r1][r1] += DAMPING * DAMPING;
        }
        let y = solve_linear(a, err)?;
        let mut dx: Vec<f64> = (0..cols)
            .map(|c| wcol[c] * (0..rows).map(|r| jac[r][c] * y[r]).sum::<f64>())
            .collect();
        let biggest = dx
            .chunks(3)
            .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
            .fold(0.0, f64::max);
        if biggest > MAX_STEP {
            dx.iter_mut().for_each(|x| *x *= MAX_STEP / biggest);
        }
        // apply: a world turn w at joint j is, locally, qp^-1 exp(w/2) qp q
        for (m, &j) in movable.iter().enumerate() {
            let w = [dx[m * 3], dx[m * 3 + 1], dx[m * 3 + 2]];
            if w == [0.0; 3] {
                continue;
            }
            let qp = sk.parents[j].map_or(quat::IDENTITY, |p| quat::from_mat3(&world[p]));
            let turn = quat::mul(quat::conj(qp), quat::mul(quat::exp(quat::scale3(w, 0.5)), qp));
            let mut q = quat::normalize(quat::mul(turn, rots[j]));
            if let Some(l) = limits {
                q = l.clamp(j, q);
            }
            rots[j] = q;
        }
        let t = [dx[cols - 3], dx[cols - 2], dx[cols - 1]];
        root = quat::add3(root, quat::scale3(t, 1.0 / h));
    }
    Ok(Solved {
        rotations: rots,
        root,
        error,
        iterations,
    })
}
