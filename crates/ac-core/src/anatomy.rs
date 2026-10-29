//! Anatomical joint angles: how far a body segment has turned from its anatomical zero, measured
//! against the segment it hangs from (the thigh against the pelvis, the upper arm against the
//! chest), in degrees.
//!
//! A bone's own rotation is not an anatomical angle: the rest pose is an A-pose with bent elbows
//! and slightly bent knees, twist bones split one joint over two bones, and a bone's local X, Y and
//! Z are world axes, not the limb's. So for each joint:
//!
//! - `T = R_P^-1 R_D Z`: the distal bone's world rotation relative to the proximal bone's
//!   (`R_*`, both identity at rest), times `Z`, the shortest turn from the joint's anatomical zero
//!   direction to the segment's rest direction (identity where the rest pose is the zero);
//! - `T = S W`, a swing `S` (axis across the segment) after a twist `W` about the segment's zero
//!   direction `u`;
//! - **flexion** and **abduction** are the swing's rotation vector along `u x d` for the direction
//!   `d` the segment moves toward (hip flexion: forward; hip abduction: outward), so a pure motion
//!   reads its plain angle and a mixed one splits like a goniometer reading;
//! - **rotation** is the twist angle about `u`, signed so inward (internal) rotation is positive.
//!
//! Left and right read the same: a right-side joint mirrors every direction in X. The body at rest
//! faces -Y with Z up, its left at +X. `training/ac_train/anatomy.py` computes the same numbers for
//! the mocap envelope; `assets/rig/ranges.json` carries sample poses that hold the two together.

use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

pub const JOINTS: &[&str] = &["hip", "knee", "ankle", "shoulder", "trunk", "neck"];
pub const COMPONENTS: &[&str] = &["flexion", "abduction", "rotation"];

const FWD: Vec3 = [0.0, -1.0, 0.0];
const BACK: Vec3 = [0.0, 1.0, 0.0];
const UP: Vec3 = [0.0, 0.0, 1.0];
const DOWN: Vec3 = [0.0, 0.0, -1.0];
/// Outward for the left side (its mirror for the right).
const OUT: Vec3 = [1.0, 0.0, 0.0];
const IN: Vec3 = [-1.0, 0.0, 0.0];

/// A joint's anatomical zero direction for the distal segment.
#[derive(Clone, Copy)]
enum Dir {
    /// A body direction (mirrored on the right).
    Body(Vec3),
    /// From one rest head to another (the rig's own, so already mirrored).
    Bones(&'static str, &'static str),
}

struct Def {
    joint: &'static str,
    sided: bool,
    proximal: &'static str,
    distal: &'static str,
    zero: Dir,
    /// The segment's rest direction, when the rest pose is not the anatomical zero.
    rest: Option<(&'static str, &'static str)>,
    flex_to: Vec3,
    abd_to: Vec3,
    /// Positive rotation turns `twist.0` toward `twist.1`.
    twist: (Vec3, Vec3),
    /// The bones [`Joint::set_angles`] turns, top first: the turn is shared between them.
    edit: &'static [&'static str],
}

const DEFS: &[Def] = &[
    Def {
        joint: "hip",
        sided: true,
        proximal: "root",
        distal: "upperleg02",
        zero: Dir::Body(DOWN),
        rest: Some(("upperleg01", "lowerleg01")),
        flex_to: FWD,
        abd_to: OUT,
        twist: (FWD, IN),
        edit: &["upperleg01"],
    },
    Def {
        joint: "knee",
        sided: true,
        proximal: "upperleg02",
        distal: "lowerleg02",
        zero: Dir::Bones("upperleg01", "lowerleg01"),
        rest: Some(("lowerleg01", "foot")),
        flex_to: BACK,
        abd_to: OUT,
        twist: (FWD, IN),
        edit: &["lowerleg01"],
    },
    Def {
        joint: "ankle",
        sided: true,
        proximal: "lowerleg02",
        distal: "foot",
        zero: Dir::Bones("foot", "toe3-1"),
        rest: None,
        flex_to: UP,
        abd_to: OUT,
        twist: (DOWN, IN),
        edit: &["foot"],
    },
    Def {
        joint: "shoulder",
        sided: true,
        proximal: "spine01",
        distal: "upperarm02",
        zero: Dir::Body(DOWN),
        rest: Some(("upperarm01", "lowerarm01")),
        flex_to: FWD,
        abd_to: OUT,
        twist: (FWD, IN),
        edit: &["upperarm01"],
    },
    Def {
        joint: "trunk",
        sided: false,
        proximal: "root",
        distal: "spine01",
        zero: Dir::Body(UP),
        rest: None,
        flex_to: FWD,
        abd_to: OUT,
        twist: (FWD, OUT),
        edit: &["spine05", "spine04", "spine03", "spine02", "spine01"],
    },
    Def {
        joint: "neck",
        sided: false,
        proximal: "spine01",
        distal: "head",
        zero: Dir::Body(UP),
        rest: None,
        flex_to: FWD,
        abd_to: OUT,
        twist: (FWD, OUT),
        edit: &["neck01", "neck02", "neck03", "head"],
    },
];

/// Whether `joint` has a left and a right (the trunk and neck have one).
pub fn sided(joint: &str) -> bool {
    DEFS.iter().any(|d| d.joint == joint && d.sided)
}

/// What each joint's three numbers mean, positive first.
pub fn meaning(joint: &str) -> [&'static str; 3] {
    match joint {
        "knee" => [
            "flexion (+) / hyperextension (-)",
            "varus (+) / valgus (-)",
            "tibia inward (+) / outward (-)",
        ],
        "ankle" => [
            "dorsiflexion (+) / plantarflexion (-)",
            "toes out (+) / in (-)",
            "inversion (+) / eversion (-)",
        ],
        "trunk" | "neck" => [
            "flexion (+) / extension (-)",
            "bend left (+) / right (-)",
            "turn left (+) / right (-)",
        ],
        _ => [
            "flexion (+) / extension (-)",
            "abduction (+) / adduction (-)",
            "internal (+) / external (-) rotation",
        ],
    }
}

/// Most bones a joint's turn is shared between ([`Def::edit`]).
const MAX_EDIT: usize = 5;

/// A joint's bones and directions, resolved on a rig for one side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    proximal: usize,
    distal: usize,
    edit: [usize; MAX_EDIT],
    edits: usize,
    zero_turn: Quat,
    u: Vec3,
    flex_axis: Vec3,
    abd_axis: Vec3,
    twist_sign: f64,
}

fn unit(v: Vec3) -> Option<Vec3> {
    let n = quat::len3(v);
    (n.is_finite() && n > 1e-9).then(|| quat::scale3(v, 1.0 / n))
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `d` with its part along unit `u` removed.
fn across(d: Vec3, u: Vec3) -> Vec3 {
    quat::sub3(d, quat::scale3(u, dot(d, u)))
}

/// The shortest turn taking unit `a` to unit `b`.
fn arc(a: Vec3, b: Vec3) -> Quat {
    let c = quat::cross(a, b);
    let w = 1.0 + dot(a, b);
    if w < 1e-9 {
        return quat::IDENTITY; // opposite directions: never so in a rig's rest pose
    }
    quat::normalize([c[0], c[1], c[2], w])
}

fn mirror(v: Vec3, side: &str) -> Vec3 {
    if side == "R" { [-v[0], v[1], v[2]] } else { v }
}

fn bone(sk: &Skeleton, stem: &str, side: &str) -> R<usize> {
    sk.bone(stem)
        .or_else(|| sk.bone(&format!("{stem}.{side}")))
        .ok_or_else(|| format!("the rig has no bone {stem} (or {stem}.{side})"))
}

fn rest_dir(sk: &Skeleton, from: &str, to: &str, side: &str) -> R<Vec3> {
    let (a, b) = (bone(sk, from, side)?, bone(sk, to, side)?);
    let (Some(ha), Some(hb)) = (sk.heads.get(a), sk.heads.get(b)) else {
        return Err(format!("no rest heads for {from} and {to}"));
    };
    unit(quat::sub3(*hb, *ha)).ok_or_else(|| format!("{from} and {to} share a rest head"))
}

impl Joint {
    /// `joint` on `sk`, `side` "L" or "R" (ignored for the trunk and neck).
    pub fn new(sk: &Skeleton, joint: &str, side: &str) -> R<Joint> {
        let def = DEFS
            .iter()
            .find(|d| d.joint == joint)
            .ok_or_else(|| format!("no joint {joint:?} (joints: {})", JOINTS.join(", ")))?;
        let side = if def.sided {
            match side {
                "L" | "R" => side,
                other => return Err(format!("side must be \"L\" or \"R\", got {other:?}")),
            }
        } else {
            "L"
        };
        let u = match def.zero {
            Dir::Body(v) => mirror(v, side),
            Dir::Bones(a, b) => rest_dir(sk, a, b, side)?,
        };
        let zero_turn = match def.rest {
            Some((a, b)) => arc(u, rest_dir(sk, a, b, side)?),
            None => quat::IDENTITY,
        };
        let axis = |d: Vec3| {
            unit(quat::cross(u, across(mirror(d, side), u)))
                .ok_or_else(|| format!("{joint}: a direction lies along the segment"))
        };
        if def.edit.is_empty() || def.edit.len() > MAX_EDIT {
            return Err(format!("{joint}: 1 to {MAX_EDIT} bones to turn"));
        }
        let mut edit = [0; MAX_EDIT];
        for (slot, stem) in edit.iter_mut().zip(def.edit) {
            *slot = bone(sk, stem, side)?;
        }
        let (t0, t1) = (mirror(def.twist.0, side), mirror(def.twist.1, side));
        let twist_axis = quat::cross(across(t0, u), across(t1, u));
        Ok(Joint {
            proximal: bone(sk, def.proximal, side)?,
            distal: bone(sk, def.distal, side)?,
            edit,
            edits: def.edit.len(),
            zero_turn,
            u,
            flex_axis: axis(def.flex_to)?,
            abd_axis: axis(def.abd_to)?,
            twist_sign: if dot(twist_axis, u) < 0.0 { -1.0 } else { 1.0 },
        })
    }

    /// [flexion, abduction, rotation] in degrees for a pose's local rotations.
    pub fn angles(&self, sk: &Skeleton, rotations: &[Quat]) -> R<[f64; 3]> {
        let rp = world(sk, rotations, self.proximal)?;
        let rd = world(sk, rotations, self.distal)?;
        Ok(self.read(quat::mul(quat::mul(quat::conj(rp), rd), self.zero_turn)))
    }

    /// The turn `T` (see the module notes) that reads `want` ([flexion, abduction, rotation],
    /// degrees): the twist about `u`, then the swing whose rotation vector has those flexion and
    /// abduction parts (both axes lie across `u`; they need not be square to each other).
    fn turn(&self, want: [f64; 3]) -> Quat {
        let [f, a, r] = want.map(f64::to_radians);
        let c = dot(self.flex_axis, self.abd_axis);
        let det = (1.0 - c * c).max(1e-12);
        let (p, q) = ((f - c * a) / det, (a - c * f) / det);
        let s = quat::add3(quat::scale3(self.flex_axis, p), quat::scale3(self.abd_axis, q));
        let swing = quat::exp(quat::scale3(s, 0.5));
        let half = 0.5 * r * self.twist_sign;
        let w = [
            self.u[0] * half.sin(),
            self.u[1] * half.sin(),
            self.u[2] * half.sin(),
            half.cos(),
        ];
        quat::normalize(quat::mul(swing, w))
    }

    /// Turn the joint's bones so it reads `want` (degrees), leaving every other joint's reading
    /// as it was. The turn is shared between the joint's bones (the five spine bones for the
    /// trunk), so a bend stays spread along the spine.
    pub fn set_angles(&self, sk: &Skeleton, rotations: &mut [Quat], want: [f64; 3]) -> R<()> {
        if want.iter().any(|v| !v.is_finite()) {
            return Err("the angles to set are not finite".into());
        }
        let rp = world(sk, rotations, self.proximal)?;
        let goal = quat::mul(quat::mul(rp, self.turn(want)), quat::conj(self.zero_turn));
        let bones = self.edit.get(..self.edits).unwrap_or(&[]);
        for (k, &e) in bones.iter().enumerate() {
            let rd = world(sk, rotations, self.distal)?;
            let delta = quat::mul(goal, quat::conj(rd));
            let share = quat::slerp(quat::IDENTITY, delta, 1.0 / (bones.len() - k) as f64);
            let we = world(sk, rotations, e)?;
            let local = rotations
                .get_mut(e)
                .ok_or_else(|| format!("the pose has no rotation for bone {e}"))?;
            // world' = share * world, so local' = local * (world^-1 share world)
            let change = quat::mul(quat::mul(quat::conj(we), share), we);
            *local = quat::normalize(quat::mul(*local, change));
        }
        Ok(())
    }

    /// [flexion, abduction, rotation] in degrees of a turn `T`.
    fn read(&self, t: Quat) -> [f64; 3] {
        let along = dot([t[0], t[1], t[2]], self.u);
        let w = quat::normalize([self.u[0] * along, self.u[1] * along, self.u[2] * along, t[3]]);
        let w = if w.iter().all(|x| x.is_finite()) {
            w
        } else {
            quat::IDENTITY
        };
        let swing = quat::mul(t, quat::conj(w));
        let s = quat::scale3(quat::log(quat::align(swing, quat::IDENTITY)), 2.0);
        let twist_along = dot([w[0], w[1], w[2]], self.u);
        let mut twist = 2.0 * twist_along.atan2(w[3]);
        if twist > std::f64::consts::PI {
            twist -= std::f64::consts::TAU;
        } else if twist <= -std::f64::consts::PI {
            twist += std::f64::consts::TAU;
        }
        [
            dot(s, self.flex_axis).to_degrees(),
            dot(s, self.abd_axis).to_degrees(),
            (twist * self.twist_sign).to_degrees(),
        ]
    }
}

/// Bone `i`'s world rotation: its parents' local rotations, root first.
fn world(sk: &Skeleton, rotations: &[Quat], i: usize) -> R<Quat> {
    let mut chain = Vec::new();
    let mut at = Some(i);
    while let Some(b) = at {
        chain.push(b);
        at = sk.parents.get(b).copied().flatten();
        if chain.len() > sk.len() {
            return Err("the rig's parents loop".into());
        }
    }
    let mut r = quat::IDENTITY;
    for b in chain.into_iter().rev() {
        let q = rotations
            .get(b)
            .copied()
            .ok_or_else(|| format!("the pose has no rotation for bone {b}"))?;
        r = quat::mul(r, quat::normalize(q));
    }
    Ok(r)
}

/// Component index of `name` (flexion 0, abduction 1, rotation 2).
pub fn component(name: &str) -> R<usize> {
    COMPONENTS
        .iter()
        .position(|c| *c == name)
        .ok_or_else(|| format!("no component {name:?} (components: {})", COMPONENTS.join(", ")))
}
