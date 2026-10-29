//! `pose.query`: measurements of one pose an agent needs before (and after) it edits: where bones
//! are and how they are turned, how far apart two bones are or how high one is above the ground,
//! how far each limb is stretched toward its reach, and which joints are past their limits.
//!
//! - bones: world head position (m), world rotation, local rotation, height above the ground
//!   under it (m), and how far (degrees) the local rotation is past the bone's limits;
//! - pairs: `[a, b]` bone names, or `[a, "floor"]`: the distance (m) and `b - a` as a vector
//!   (to the floor: straight down to the ground under `a`);
//! - limbs: for each arm and leg, the shoulder/hip-to-hand/foot distance, the limb's length (upper
//!   plus lower bone), their ratio `reach` (1: straight, it can reach no further) and the bend at
//!   the elbow or knee (degrees; 0 straight);
//! - outside_limits: every bone past its limits by more than
//!   [`crate::metrics::LIMIT_SLACK_DEG`], worst first (null when no limits are loaded).

use serde_json::{Value, json};

use crate::document::{Document, Pose};
use crate::quat::{self, Vec3};

type R<T> = Result<T, String>;

/// Most bones or pairs in one query.
pub const MAX_ASKED: usize = 1024;
/// A limb this close to its full length is straight.
pub const STRAIGHT: f64 = 0.999;

/// The pose a query looks at: the one shown (`None`), or the motion shown at a frame.
pub fn pose_at(doc: &Document, frame: Option<f64>) -> R<Pose> {
    match frame {
        None => Ok(doc.pose.clone()),
        Some(f) => {
            if doc.clip.base().keys.is_empty() {
                return Err(crate::error::hinted(
                    "the animation has no keys to sample",
                    "leave `frame` out to ask about the pose shown, or key it first (key.set)",
                ));
            }
            doc.shown_poses(&[f])?
                .into_iter()
                .next()
                .ok_or_else(|| format!("no pose on frame {f}"))
        }
    }
}

/// The `[a, b]` pairs, checked: lists of two names, `b` may be "floor".
pub fn pairs(doc: &Document, v: Option<&Value>) -> R<Vec<(usize, Option<usize>)>> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let list = v
        .as_array()
        .ok_or("`pairs` must be a list of [bone, bone] or [bone, \"floor\"]")?;
    if list.len() > MAX_ASKED {
        return Err(format!("more than {MAX_ASKED} pairs"));
    }
    list.iter()
        .enumerate()
        .map(|(i, p)| {
            let names: Vec<&str> = p
                .as_array()
                .filter(|a| a.len() == 2)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .filter(|n: &Vec<&str>| n.len() == 2)
                .ok_or_else(|| format!("pair {i}: expected [bone, bone] or [bone, \"floor\"], got {p}"))?;
            let (a, b) = (
                names.first().copied().unwrap_or_default(),
                names.get(1).copied().unwrap_or_default(),
            );
            let b = if b == "floor" { None } else { Some(doc.bone(b)?) };
            Ok((doc.bone(a)?, b))
        })
        .collect()
}

/// The query's result.
pub fn query(
    doc: &Document,
    frame: Option<f64>,
    bones: &[String],
    pairs: &[(usize, Option<usize>)],
) -> R<Value> {
    if bones.len() > MAX_ASKED {
        return Err(format!("more than {MAX_ASKED} bones"));
    }
    let idx = bones.iter().map(|b| doc.bone(b)).collect::<R<Vec<usize>>>()?;
    let pose = pose_at(doc, frame)?;
    let sk = &doc.skeleton;
    let (world, pos) = sk.fk(&pose.rotations, pose.root).map_err(|e| e.to_string())?;
    let at = |i: usize| -> R<Vec3> { pos.get(i).copied().ok_or_else(|| format!("no joint {i}")) };
    let name = |i: usize| sk.names.get(i).cloned().unwrap_or_default();
    let height = |p: Vec3| p[2] - crate::physics::ground(&doc.clip.surfaces, p[0], p[1]);
    let limits = doc.limits.as_deref();
    let bones_out = idx
        .iter()
        .map(|&i| {
            let p = at(i)?;
            let m = world.get(i).ok_or_else(|| format!("no joint {i}"))?;
            let local = pose.rotations.get(i).copied().unwrap_or(quat::IDENTITY);
            Ok(json!({
                "bone": name(i),
                "position": p,
                "rotation": quat::from_mat3(m),
                "local": local,
                "height": height(p),
                "past_limits_deg": limits.map(|l| crate::metrics::limit_excess(l, i, local)),
            }))
        })
        .collect::<R<Vec<Value>>>()?;
    let pairs_out = pairs
        .iter()
        .map(|&(a, b)| {
            let pa = at(a)?;
            let (pb, bname) = match b {
                Some(b) => (at(b)?, name(b)),
                None => ([pa[0], pa[1], pa[2] - height(pa)], "floor".to_string()),
            };
            let d = quat::sub3(pb, pa);
            Ok(json!({ "a": name(a), "b": bname, "distance": quat::len3(d), "delta": d }))
        })
        .collect::<R<Vec<Value>>>()?;
    let limbs = crate::ik::LIMBS
        .iter()
        .filter_map(|(limb, b)| {
            let [u, l, e] = b.map(|n| sk.bone(n));
            let (pu, pl, pe) = (at(u?).ok()?, at(l?).ok()?, at(e?).ok()?);
            let (l1, l2) = (quat::len3(quat::sub3(pl, pu)), quat::len3(quat::sub3(pe, pl)));
            let d = quat::len3(quat::sub3(pe, pu));
            let length = l1 + l2;
            let (a, c) = (quat::sub3(pu, pl), quat::sub3(pe, pl));
            let cos = (a[0] * c[0] + a[1] * c[1] + a[2] * c[2]) / (l1 * l2).max(1e-12);
            let bend = 180.0 - cos.clamp(-1.0, 1.0).acos().to_degrees();
            let reach = if length > 0.0 { d / length } else { 0.0 };
            Some(json!({
                "limb": limb, "distance": d, "length": length, "reach": reach,
                "straight": reach >= STRAIGHT, "bend_deg": bend,
            }))
        })
        .collect::<Vec<Value>>();
    let outside = limits.map(|l| {
        let mut v: Vec<(f64, usize)> = (0..sk.len())
            .filter_map(|i| {
                let over = crate::metrics::limit_excess(l, i, *pose.rotations.get(i)?);
                (over > crate::metrics::LIMIT_SLACK_DEG).then_some((over, i))
            })
            .collect();
        v.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
        v.into_iter()
            .map(|(over, i)| json!({ "bone": name(i), "past_deg": over }))
            .collect::<Vec<Value>>()
    });
    let mut notes = Vec::new();
    if limits.is_none() {
        notes.push("joint limits are not loaded: limits were not checked");
    }
    Ok(json!({
        "frame": frame,
        "bones": bones_out,
        "pairs": pairs_out,
        "limbs": limbs,
        "outside_limits": outside,
        "notes": notes,
    }))
}
