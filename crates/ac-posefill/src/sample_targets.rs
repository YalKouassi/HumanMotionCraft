//! Target poses for a live take made from the optional sample clips (`live.start` with
//! `targets: "samples"`), on this machine and from the person's own download. The same choice as
//! `training/ac_train/posetargets.py` makes from the free sets: for each gait four frames at 30 fps
//! where the hips move steadily inside its speed band, straight ahead and facing the way they go,
//! the left foot leading, and among those nearest its speed the most typical posture.
//!
//! The file goes in the download's folder, so `posefill.samples` remove deletes it with the clips.
//! It is never shipped or trained on. Deterministic: ties go by clip name, then frame.

use std::path::{Path, PathBuf};

use ac_core::quat::{self, Quat, Vec3};
use ac_core::rig::Skeleton;
use serde_json::{Value, json};

const FPS: f64 = 30.0;
const POSES: usize = 4;
const STEADY: usize = 15;
/// The download's plain walking and standing, not its zombies, dances or crouches.
const FOLDERS: &[&str] = &["bs_1d_linear", "idle"];
/// Clips in those folders that aren't a plain stand: they'd make the character stand like this.
const NOT_PLAIN: &[&str] = &["crouch", "warm_up", "legs_relax"];
/// (name, speed aimed at in m/s, the band of mean speeds that qualify), as `posetargets.py`.
const GAITS: &[(&str, f64, f64, f64)] = &[
    ("stand", 0.0, 0.0, 0.08),
    ("walk", 1.3, 1.0, 1.7),
    ("jog", 2.4, 2.0, 2.9),
    ("run", 3.6, 3.2, 5.0),
];
const FACING_DOT: f64 = 0.9;
const TURN_DEG: f64 = 10.0;
const SPEED_SPREAD: f64 = 0.2;
const LEAN_STEP: f64 = 5.0;
const HEIGHT_STEP: f64 = 0.03;

/// One clip on the rig: its file's stem, then a frame's rotations and root offset.
pub struct Source {
    pub name: String,
    pub fps: f64,
    pub rotations: Vec<Vec<Quat>>,
    pub root: Vec<Vec3>,
}

struct Candidate {
    gait: usize,
    name: String,
    frame: usize,
    speed: f64,
    lean: f64,
    height: f64,
    rotations: Vec<Vec<Quat>>,
    root: Vec<Vec3>,
}

/// Whether a clip's file stem (`<folder>__<name>`) is one to choose from.
pub fn wanted(stem: &str) -> bool {
    let plain = !NOT_PLAIN.iter().any(|w| stem.to_lowercase().contains(w));
    plain
        && stem
            .split("__")
            .next()
            .is_some_and(|folder| FOLDERS.contains(&folder))
}

fn yaw(angle: f64) -> Quat {
    [0.0, 0.0, (angle / 2.0).sin(), (angle / 2.0).cos()]
}

fn unit2(v: [f64; 2]) -> [f64; 2] {
    let n = v[0].hypot(v[1]).max(1e-9);
    [v[0] / n, v[1] / n]
}

fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// The ground direction the hips face: the root bone turns the rig's forward (-Y).
fn facing(q: Quat) -> [f64; 2] {
    let f = quat::mat3_vec(&quat::to_mat3(q), [0.0, -1.0, 0.0]);
    unit2([f[0], f[1]])
}

fn candidates(sk: &Skeleton, c: &Source, out: &mut Vec<Candidate>) -> Result<(), String> {
    let n = c.root.len();
    if (c.fps - FPS).abs() > 1e-6 || c.rotations.len() != n || n < POSES + 2 * STEADY + 1 {
        return Ok(());
    }
    let (Some(neck), Some(left), Some(right)) = (sk.bone("neck01"), sk.bone("foot.L"), sk.bone("foot.R"))
    else {
        return Err("this rig has no neck01, foot.L or foot.R".into());
    };
    let hips: Vec<Vec3> = c
        .root
        .iter()
        .map(|r| quat::add3(sk.heads[0], quat::scale3(*r, sk.hip_height)))
        .collect();
    let v: Vec<[f64; 2]> = hips
        .windows(2)
        .map(|w| [(w[1][0] - w[0][0]) * FPS, (w[1][1] - w[0][1]) * FPS])
        .collect();
    let speed: Vec<f64> = v.iter().map(|d| d[0].hypot(d[1])).collect();
    let root_of = |f: usize| c.rotations[f].first().copied().unwrap_or(quat::IDENTITY);
    for f in STEADY..n - POSES - STEADY {
        let (w0, w1) = (f - STEADY, f + POSES + STEADY);
        let s = &speed[w0..w1 - 1];
        let mean = s.iter().sum::<f64>() / s.len() as f64;
        let spread = (s.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
        let top = s.iter().copied().fold(0.0, f64::max);
        for (g, &(name, _, lo, hi)) in GAITS.iter().enumerate() {
            if !(lo..=hi).contains(&mean) {
                continue;
            }
            let heading = if name == "stand" {
                if top > hi {
                    continue;
                }
                facing(root_of(f))
            } else {
                if spread > SPEED_SPREAD * mean {
                    continue;
                }
                let mut sum = [0.0, 0.0];
                for d in &v[f..f + POSES - 1] {
                    sum = [sum[0] + d[0], sum[1] + d[1]];
                }
                let heading = unit2(sum);
                let turn = dot2(unit2(v[w0]), unit2(v[w1 - 2])).clamp(-1.0, 1.0).acos();
                if turn.to_degrees() > TURN_DEG {
                    continue;
                }
                let mut face = [0.0, 0.0];
                for k in f..f + POSES {
                    let d = facing(root_of(k));
                    face = [face[0] + d[0] / POSES as f64, face[1] + d[1] / POSES as f64];
                }
                if dot2(face, heading) < FACING_DOT {
                    continue;
                }
                heading
            };
            let (_, p) = sk.fk(&c.rotations[f], c.root[f]).map_err(|e| e.to_string())?;
            let foot = [p[left][0] - p[right][0], p[left][1] - p[right][1]];
            if name != "stand" && dot2(foot, heading) <= 0.0 {
                continue;
            }
            let trunk = quat::sub3(p[neck], hips[f]);
            let length = (trunk[0] * trunk[0] + trunk[1] * trunk[1] + trunk[2] * trunk[2])
                .sqrt()
                .max(1e-9);
            let lean = (dot2([trunk[0], trunk[1]], heading) / length)
                .clamp(-1.0, 1.0)
                .asin()
                .to_degrees();
            let (rotations, root) = normalised(sk, &c.rotations[f..f + POSES], &hips[f..f + POSES], heading);
            out.push(Candidate {
                gait: g,
                name: c.name.clone(),
                frame: f,
                speed: mean,
                lean,
                height: hips[f][2],
                rotations,
                root,
            });
        }
    }
    Ok(())
}

/// The four poses turned so `heading` becomes -Y, the first pose's hips over the origin.
fn normalised(
    sk: &Skeleton,
    rotations: &[Vec<Quat>],
    hips: &[Vec3],
    heading: [f64; 2],
) -> (Vec<Vec<Quat>>, Vec<Vec3>) {
    let angle = (-1.0f64).atan2(0.0) - heading[1].atan2(heading[0]);
    let (c, s, q) = (angle.cos(), angle.sin(), yaw(angle));
    let turned = rotations
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if let Some(first) = r.first_mut() {
                *first = quat::normalize(quat::mul(q, *first));
            }
            r
        })
        .collect();
    let first = hips.first().copied().unwrap_or_default();
    let root = hips
        .iter()
        .map(|p| {
            let (x, y) = (p[0] - first[0], p[1] - first[1]);
            let at = [c * x - s * y, s * x + c * y, p[2]];
            quat::scale3(quat::sub3(at, sk.heads[0]), 1.0 / sk.hip_height)
        })
        .collect();
    (turned, root)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn rounded(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// The target set from these clips, in the format `tools/posefill/live.py` reads.
pub fn pick(sk: &Skeleton, clips: &[Source]) -> Result<Value, String> {
    let mut all = Vec::new();
    for c in clips {
        candidates(sk, c, &mut all)?;
    }
    let mut targets = Vec::new();
    for (g, &(name, aim, ..)) in GAITS.iter().enumerate() {
        let of: Vec<&Candidate> = all.iter().filter(|c| c.gait == g).collect();
        let lean = median(of.iter().map(|c| c.lean).collect());
        let height = median(of.iter().map(|c| c.height).collect());
        let key = |c: &Candidate| {
            let near = ((c.speed - aim).abs() * 10.0).round();
            let posture = (((c.lean - lean).abs() / LEAN_STEP + (c.height - height).abs() / HEIGHT_STEP)
                * 10.0)
                .round();
            (near as i64, posture as i64)
        };
        let Some(best) = of.iter().min_by(|a, b| {
            key(a)
                .cmp(&key(b))
                .then_with(|| a.name.cmp(&b.name))
                .then(a.frame.cmp(&b.frame))
        }) else {
            continue;
        };
        let poses: Vec<Value> = best
            .rotations
            .iter()
            .zip(&best.root)
            .map(|(r, o)| {
                let r: Vec<Vec<f64>> = r
                    .iter()
                    .map(|q| q.iter().map(|x| rounded(*x)).collect())
                    .collect();
                json!({ "rotations": r, "root": o.iter().map(|x| rounded(*x)).collect::<Vec<f64>>() })
            })
            .collect();
        targets.push(json!({
            "name": name,
            "speed": rounded(best.speed),
            "poses": poses,
            "from": { "clip": best.name, "frames": [best.frame, best.frame + POSES - 1] },
        }));
    }
    if targets.len() < 2 {
        return Err("the sample clips gave fewer than two gaits to aim at".into());
    }
    Ok(json!({
        "format": format!("{}.posetargets", ac_core::product::ID),
        "version": 1,
        "note": "made on this machine from the person's download of NVIDIA's sample clips: evaluation only, never shipped or trained on",
        "evaluation_only": true,
        "fps": FPS,
        "bones": sk.names.len(),
        "targets": targets,
    }))
}

/// The file the set is kept in.
pub fn path(dir: &Path) -> PathBuf {
    dir.join("targets.json")
}

/// Read the converted clips and write the set.
pub fn build(sk: &Skeleton, dir: &Path, files: &[PathBuf]) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Raw {
        fps: f64,
        rotations: Vec<Vec<Quat>>,
        root: Vec<Vec3>,
    }
    let mut clips = Vec::new();
    for f in files {
        let Some(stem) = f.file_stem().and_then(|s| s.to_str()).filter(|s| wanted(s)) else {
            continue;
        };
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let raw: Raw = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", f.display()))?;
        if raw.rotations.iter().any(|r| r.len() != sk.names.len()) {
            return Err(format!("{}: not for this rig", f.display()));
        }
        clips.push(Source {
            name: stem.to_string(),
            fps: raw.fps,
            rotations: raw.rotations,
            root: raw.root,
        });
    }
    let set = pick(sk, &clips)?;
    craft_atomic::atomic_write(&path(dir), set.to_string().as_bytes())
        .map_err(|e| format!("{}: {e}", path(dir).display()))
}
