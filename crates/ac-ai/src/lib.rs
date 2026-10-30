//! The learned in-betweener: our network (first written in cf-animate, the earlier Python prototype),
//! run in plain Rust (no ML runtime), and the fill around it (a port of the prototype's `fill`).
//!
//! A fill looks at [`Model::context`] frames ending on key `a`, and at key `b`; it turns that window
//! so key `a` stands at the origin facing -Y, encodes each solved bone as the first two columns of
//! its rotation matrix (6D) plus the root position in hip heights, and asks the network for a change
//! to the plain interpolation (slerp / lerp) between the keys. Bones the network does not solve
//! (fingers, toes, twist bones, the head) keep the keys' own spline.
//!
//! The card's `commercial` and `warning` travel with every model.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod net;

use std::path::Path;

use ac_core::ai::{Contacts, FillSettings, FillWithContacts, Inbetweener, ModelInfo};
use ac_core::clip::{Clip, Interp, Links};
use ac_core::quat::{self, Quat, Vec3};
use ac_core::rig::Skeleton;
use serde_json::Value;

pub use net::Net;

type R<T> = Result<T, String>;
type M3 = [[f64; 3]; 3];

/// The model a fresh install uses: best on every gap length of the four cf-animate trained, and
/// the only one that fills around layer-driven parts.
pub const DEFAULT_TAG: &str = "parts";

#[derive(Debug)]
pub struct Model {
    info: ModelInfo,
    net: NetBox,
    /// FNV-1a of `weights.json` then `weights.bin` (`session.info`).
    hash: String,
}

/// The network behind `Debug` (its weights are not worth printing).
struct NetBox(Net);

impl std::fmt::Debug for NetBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Net({} features)", self.0.feat)
    }
}

impl Model {
    /// Load `<dir>` (a folder with `weights.json` and `weights.bin`).
    pub fn load(dir: &Path) -> R<Model> {
        let (net, card) = Net::load(dir)?;
        let s = |k: &str| card[k].as_str().map(str::to_string);
        let list = |k: &str| -> Vec<String> {
            card[k]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        };
        let bones = list("bones");
        if bones.len() * 6 + 3 != net.feat {
            return Err(format!(
                "model card: {} bones do not make {} features",
                bones.len(),
                net.feat
            ));
        }
        let styles = list("styles");
        if styles.len() != net.styles {
            return Err(format!(
                "model card: {} styles, the network has {}",
                styles.len(),
                net.styles
            ));
        }
        if net.given != 0 && net.given != bones.len() {
            return Err(format!(
                "model card: a given mask of {} for {} bones",
                net.given,
                bones.len()
            ));
        }
        let info = ModelInfo {
            tag: s("tag").unwrap_or_else(|| {
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }),
            bones,
            styles,
            given: net.given != 0,
            context: card["context"].as_u64().unwrap_or(10) as usize,
            max_gap: card["max_gap"].as_u64().unwrap_or(45) as usize,
            commercial: card["commercial"].as_bool().unwrap_or(false),
            warning: s("warning").unwrap_or_default(),
        };
        if info.context == 0 || info.context + info.max_gap + 1 > 56 {
            return Err("model card: context and max_gap do not fit the network".into());
        }
        let mut hash = ac_core::hash::Fnv::default();
        for f in ["weights.json", "weights.bin"] {
            let bytes = std::fs::read(dir.join(f)).map_err(|e| format!("{}: {e}", dir.join(f).display()))?;
            hash = hash.bytes(&bytes);
        }
        Ok(Model {
            info,
            net: NetBox(net),
            hash: hash.hex(),
        })
    }

    /// The default model under `<assets>/models` ([`DEFAULT_TAG`], else the first by name).
    pub fn load_default(assets: &Path) -> R<Model> {
        Model::load_default_in(&assets.join("models"))
    }

    /// The default model in a models folder (`<root>/<tag>/weights.json`): [`DEFAULT_TAG`], else
    /// the first by name.
    pub fn load_default_in(root: &Path) -> R<Model> {
        let root = root.to_path_buf();
        let pick = if root.join(DEFAULT_TAG).join("weights.json").is_file() {
            root.join(DEFAULT_TAG)
        } else {
            let mut dirs: Vec<_> = std::fs::read_dir(&root)
                .map_err(|e| format!("{}: {e}", root.display()))?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("weights.json").is_file())
                .collect();
            dirs.sort();
            dirs.into_iter()
                .next()
                .ok_or_else(|| format!("no model under {}", root.display()))?
        };
        Model::load(&pick)
    }

    pub fn net(&self) -> &Net {
        &self.net.0
    }
}

// ---- rotations, as the reference computes them ------------------------------------------------

fn q2m(q: Quat) -> M3 {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let [x, y, z, w] = [q[0] / n, q[1] / n, q[2] / n, q[3] / n];
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

fn m2q(r: &M3) -> Quat {
    let sq = |v: f64| v.max(0.0).sqrt() / 2.0;
    let w = sq(1.0 + r[0][0] + r[1][1] + r[2][2]);
    let x = sq(1.0 + r[0][0] - r[1][1] - r[2][2]).copysign(r[2][1] - r[1][2]);
    let y = sq(1.0 - r[0][0] + r[1][1] - r[2][2]).copysign(r[0][2] - r[2][0]);
    let z = sq(1.0 - r[0][0] - r[1][1] + r[2][2]).copysign(r[1][0] - r[0][1]);
    let n = (x * x + y * y + z * z + w * w).sqrt();
    [x / n, y / n, z / n, w / n]
}

fn mm(a: &M3, b: &M3) -> M3 {
    let mut c = [[0.0; 3]; 3];
    for (i, row) in c.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    c
}

fn mv(a: &M3, v: Vec3) -> Vec3 {
    [0, 1, 2].map(|i| a[i][0] * v[0] + a[i][1] * v[1] + a[i][2] * v[2])
}

fn yaw(a: f64) -> M3 {
    let (s, c) = a.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// The body's facing (radians about Z) from the hip line, the root's +X (yaw 0 at rest).
fn heading(root: &M3) -> f64 {
    root[1][0].atan2(root[0][0])
}

fn to_6d(m: &M3) -> [f64; 6] {
    [m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1]]
}

fn from_6d(d: &[f64]) -> M3 {
    let norm = |v: Vec3| {
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let x = norm([d[0], d[1], d[2]]);
    let b = [d[3], d[4], d[5]];
    let xb = x[0] * b[0] + x[1] * b[1] + x[2] * b[2];
    let y = norm([b[0] - xb * x[0], b[1] - xb * x[1], b[2] - xb * x[2]]);
    let z = quat::cross(x, y);
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

/// The reference's batched slerp (nearly equal quaternions: lerp).
fn slerp(a: Quat, b: Quat, t: f64) -> Quat {
    let d0: f64 = (0..4).map(|k| a[k] * b[k]).sum();
    let b = if d0 < 0.0 { b.map(|v| -v) } else { b };
    let d = d0.abs().min(1.0 - 1e-7);
    let th = d.acos();
    let s = th.sin().max(1e-7);
    let (wa, wb) = if d > 0.9995 {
        (1.0 - t, t)
    } else {
        (((1.0 - t) * th).sin() / s, (t * th).sin() / s)
    };
    let q = [0, 1, 2, 3].map(|k| wa * a[k] + wb * b[k]);
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| v / n)
}

/// One frame's features: 6D per solved bone, then the root position / hip height.
fn features(rot: &[M3], pos: Vec3, hip: f64, out: &mut Vec<f32>) {
    for m in rot {
        out.extend(to_6d(m).map(|v| v as f32));
    }
    out.extend(pos.map(|v| (v / hip) as f32));
}

impl Inbetweener for Model {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    fn hash(&self) -> Option<String> {
        Some(self.hash.clone())
    }

    fn fill(
        &self,
        sk: &Skeleton,
        clip: &Clip,
        links: &dyn Links,
        a: i64,
        b: i64,
        s: &FillSettings,
        given: Option<&[String]>,
    ) -> R<(Vec<Vec<Quat>>, Vec<Vec3>)> {
        let (r, o, _) = self.fill_contacts(sk, clip, links, a, b, s, given)?;
        Ok((r, o))
    }

    fn fill_contacts(
        &self,
        sk: &Skeleton,
        clip: &Clip,
        links: &dyn Links,
        a: i64,
        b: i64,
        s: &FillSettings,
        given: Option<&[String]>,
    ) -> R<FillWithContacts> {
        let info = &self.info;
        let gap = b - a - 1;
        if gap < 1 || gap > info.max_gap as i64 {
            return Err(format!(
                "AI fill needs 1..{} frames between the keys, not {gap}",
                info.max_gap
            ));
        }
        for (name, v, lo, hi) in [
            ("style_force", s.style_force, 0.0, 3.0),
            ("ai_force", s.ai_force, 0.0, 1.5),
            ("trajectory", s.trajectory, 0.0, 1.0),
            ("head", s.head, 0.0, 1.0),
        ] {
            if !(lo..=hi).contains(&v) {
                return Err(format!("{name} {v} is outside {lo}..{hi}"));
            }
        }
        if s.style != "any" && !info.styles.contains(&s.style) {
            return Err(format!("style {:?}: the model knows {:?}", s.style, info.styles));
        }
        let gap = gap as usize;
        let ctx = info.context;
        let hip = sk.hip_height;
        // the window: context frames ending on a (frame 0 repeated before the start), the gap, b
        let frames: Vec<f64> = (0..ctx as i64)
            .map(|i| (a - ctx as i64 + 1 + i).max(0) as f64)
            .chain((a + 1..=b).map(|f| f as f64))
            .collect();
        let t = frames.len();
        let (rots, roots) = clip.sample_with(&frames, links)?;
        let idx: Vec<usize> = info
            .bones
            .iter()
            .map(|n| sk.bone(n).ok_or_else(|| format!("the rig has no {n}")))
            .collect::<R<_>>()?;
        let ia = ctx - 1;
        // to key a's frame: facing -Y, standing over the origin
        let mut rot: Vec<Vec<M3>> = rots
            .iter()
            .map(|r| idx.iter().map(|&i| q2m(r[i])).collect())
            .collect();
        let pos: Vec<Vec3> = roots.iter().map(|o| o.map(|v| v * hip)).collect();
        let face = heading(&rot[ia][0]);
        let (to_key, from_key) = (yaw(-face), yaw(face));
        for r in &mut rot {
            r[0] = mm(&to_key, &r[0]);
        }
        let origin = [pos[ia][0], pos[ia][1], 0.0];
        let pos_c: Vec<Vec3> = pos.iter().map(|p| mv(&to_key, quat::sub3(*p, origin))).collect();
        // the plain interpolation between the keys
        let ib = t - 1;
        let qa: Vec<Quat> = rot[ia].iter().map(m2q).collect();
        let qb: Vec<Quat> = rot[ib].iter().map(m2q).collect();
        let g = (ib - ia) as f64;
        let (mut known, mut interp) = (
            Vec::with_capacity(t * self.net().feat),
            Vec::with_capacity(t * self.net().feat),
        );
        for i in 0..t {
            features(&rot[i], pos_c[i], hip, &mut known);
            if i > ia && i < ib {
                let u = (i - ia) as f64 / g;
                let r: Vec<M3> = qa.iter().zip(&qb).map(|(x, y)| q2m(slerp(*x, *y, u))).collect();
                let p = [0, 1, 2].map(|k| pos_c[ia][k] + (pos_c[ib][k] - pos_c[ia][k]) * u);
                features(&r, p, hip, &mut interp);
            } else {
                let r: Vec<M3> = rot[i].iter().map(|m| q2m(m2q(m))).collect();
                features(&r, pos_c[i], hip, &mut interp);
            }
        }
        let mask: Vec<f32> = (0..t)
            .map(|i| if i < ctx || i == ib { 1.0 } else { 0.0 })
            .collect();
        let ttg: Vec<f32> = (0..t)
            .map(|i| ((ib as f32 - i as f32) / info.max_gap as f32).max(0.0))
            .collect();
        let gmask: Option<Vec<f32>> = given.map(|gv| {
            info.bones
                .iter()
                .map(|n| if gv.contains(n) { 1.0 } else { 0.0 })
                .collect()
        });
        if gmask.is_some() && !info.given {
            return Err(format!(
                "model {} cannot fill around parts another layer drives",
                info.tag
            ));
        }
        let net = self.net();
        // the keys' own root path, with stepped keys smoothed (a Hold kept the root still and
        // dropped it into the next key in one frame)
        let keys_path: Option<Vec<Vec3>> = if s.trajectory > 0.0 {
            let mut smooth = clip.clone();
            let e = smooth.edit;
            if let Some(l) = smooth.layers.get_mut(e) {
                l.keys
                    .iter_mut()
                    .filter(|k| k.interp == Interp::Step)
                    .for_each(|k| k.interp = Interp::Smooth);
            }
            let fr: Vec<f64> = (a + 1..b).map(|f| f as f64).collect();
            Some(smooth.sample_with(&fr, links)?.1)
        } else {
            None
        };
        // a model trained with the path is given it on the gap frames, in key a's frame (141000)
        let path_in: Option<(Vec<f32>, Vec<f32>)> = keys_path.as_ref().filter(|_| net.path).map(|kp| {
            let mut p = vec![0.0f32; t * 3];
            let mut pm = vec![0.0f32; t];
            for (k, o) in kp.iter().enumerate().take(gap) {
                let c = mv(&to_key, quat::sub3(o.map(|v| v * hip), origin));
                let i = ctx + k;
                (0..3).for_each(|j| p[i * 3 + j] = (c[j] / hip) as f32);
                pm[i] = 1.0;
            }
            (p, pm)
        });
        let path = path_in.as_ref().map(|(p, pm)| (p.as_slice(), pm.as_slice()));
        let styled = s.style != "any" && s.style_force != 0.0;
        let any = (!info.styles.is_empty()).then_some(0);
        let blend = |free: &[f32], cond: &[f32]| -> Vec<f32> {
            free.iter()
                .zip(cond)
                .map(|(f, c)| f + s.style_force as f32 * (c - f))
                .collect()
        };
        let (mut pred, logits) = if styled {
            let st = info.styles.iter().position(|x| *x == s.style);
            let (cond, lc) =
                net.forward_contacts(&known, &interp, &mask, &ttg, st, gmask.as_deref(), path)?;
            let (free, lf) =
                net.forward_contacts(&known, &interp, &mask, &ttg, any, gmask.as_deref(), path)?;
            let logits = match (lf, lc) {
                (Some(f), Some(c)) => Some(blend(&f, &c)),
                _ => None,
            };
            (blend(&free, &cond), logits)
        } else {
            net.forward_contacts(&known, &interp, &mask, &ttg, any, gmask.as_deref(), path)?
        };
        // the feet the network says are planted on the gap frames (none for the plain
        // interpolation: ai_force 0)
        let contacts: Option<Contacts> = logits.filter(|_| s.ai_force > 0.0).map(|l| {
            (ctx..ctx + gap)
                .map(|i| {
                    [
                        l.get(i * 2).is_some_and(|&v| v > 0.0),
                        l.get(i * 2 + 1).is_some_and(|&v| v > 0.0),
                    ]
                })
                .collect()
        });
        let af = s.ai_force as f32;
        pred.iter_mut()
            .zip(&interp)
            .for_each(|(p, i)| *p = i + af * (*p - i));
        if let Some(gm) = &gmask {
            let fm = net.given_features(gm);
            let f = net.feat;
            for (j, p) in pred.iter_mut().enumerate() {
                let m = fm[j % f];
                *p = *p * (1.0 - m) + known[j] * m;
            }
        }
        // back to the clip's frame, gap frames only
        let f = net.feat;
        let nb = info.bones.len();
        let mut out_r: Vec<Vec<Quat>> = rots[ctx..ctx + gap].to_vec();
        let spline_r = out_r.clone();
        let mut out_o: Vec<Vec3> = Vec::with_capacity(gap);
        for (k, fr) in out_r.iter_mut().enumerate() {
            let row: Vec<f64> = pred[(ctx + k) * f..(ctx + k + 1) * f]
                .iter()
                .map(|&v| f64::from(v))
                .collect();
            for (j, &bone) in idx.iter().enumerate() {
                let mut m = from_6d(&row[j * 6..j * 6 + 6]);
                if j == 0 {
                    m = mm(&from_key, &m);
                }
                fr[bone] = m2q(&m);
            }
            let p = [row[nb * 6] * hip, row[nb * 6 + 1] * hip, row[nb * 6 + 2] * hip];
            out_o.push(quat::add3(mv(&from_key, p), origin).map(|v| v / hip));
        }
        if let Some(path) = &keys_path {
            // the blend is what puts the root on the path
            for (o, p) in out_o.iter_mut().zip(path) {
                *o = [0, 1, 2].map(|k| o[k] + s.trajectory * (p[k] - o[k]));
            }
        }
        if s.head > 0.0 {
            for n in ["neck01", "neck02", "neck03", "head"] {
                if let Some(i) = sk.bone(n) {
                    for (fr, sp) in out_r.iter_mut().zip(&spline_r) {
                        fr[i] = quat::slerp(fr[i], sp[i], s.head);
                    }
                }
            }
        }
        Ok((out_r, out_o, contacts))
    }
}

/// The card fields a front end shows next to the AI controls.
pub fn describe(m: &dyn Inbetweener) -> Value {
    serde_json::to_value(m.info()).unwrap_or(Value::Null)
}

/// The default model under `<assets>/models`, loaded once per process and shared. `Ok(None)`:
/// there is no model (AI fill is then unavailable); `Err`: there is one and it is broken.
pub fn shared(assets: &Path) -> R<Option<std::sync::Arc<dyn Inbetweener>>> {
    shared_in(&assets.join("models"))
}

/// Whether a models folder holds a model (`<root>/<tag>/weights.json`).
pub fn has_model(root: &Path) -> bool {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|e| e.path().join("weights.json").is_file())
}

/// [`shared`] for a models folder: the bundled `<assets>/models`, or a person's own (a model
/// trained on data that may not be shipped stays there, never in the repo).
pub fn shared_in(root: &Path) -> R<Option<std::sync::Arc<dyn Inbetweener>>> {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<BTreeMap<std::path::PathBuf, Arc<Model>>>> = OnceLock::new();
    let root = root.to_path_buf();
    if !has_model(&root) {
        return Ok(None);
    }
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .map_err(|_| "the model cache is poisoned")?;
    if let Some(m) = cache.get(&root) {
        return Ok(Some(m.clone()));
    }
    let m = Arc::new(Model::load_default_in(&root)?);
    cache.insert(root, m.clone());
    Ok(Some(m))
}
