//! Poses no body can make: a foot turned round on the shin, a knee bent the wrong way, a hip
//! twisted past what any recording reaches.
//!
//! Three levels, from `assets/rig/ranges.json`:
//!
//! - **typical**: inside the published clinical limits (`crate::ranges`, the pairs' `hard`);
//! - **unusual**: past them but possible: athletes, dancers, splits, arms swung far back. Kept,
//!   with a warning;
//! - **impossible**: past a `possible` limit (ours: past anything in the reviewed recordings with
//!   a margin). Built-in clips never have it, and an AI fill bends such a joint back to the limit
//!   ([`Document::keep_possible`]) and says so.

use serde_json::{Value, json};

use crate::anatomy::{self, Joint};
use crate::clip::{Interp, LayerMode};
use crate::document::Document;
use crate::quat::Quat;
use crate::ranges::Ranges;
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

/// How far inside a limit a bent-back angle is put (degrees), so it reads inside, not on it.
pub const MARGIN_DEG: f64 = 0.5;

/// One joint angle past its possible limit.
#[derive(Debug, Clone, PartialEq)]
pub struct Breach {
    /// `"ankle.L"`, or `"trunk"` for a joint with no side.
    pub joint: String,
    pub component: &'static str,
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
}

impl Breach {
    /// Degrees past the limit.
    pub fn past(&self) -> f64 {
        (self.lo - self.value).max(self.value - self.hi).max(0.0)
    }

    pub fn json(&self) -> Value {
        json!({
            "joint": self.joint,
            "component": self.component,
            "value_deg": r2(self.value),
            "lo": self.lo,
            "hi": self.hi,
            "past_deg": r2(self.past()),
        })
    }
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[derive(Debug, Clone)]
struct Rule {
    comp: usize,
    lo: f64,
    hi: f64,
    max_swing: Option<f64>,
}

impl Rule {
    fn holds(&self, a: &[f64; 3]) -> bool {
        self.max_swing.is_none_or(|m| a[0].hypot(a[1]) < m)
    }

    fn breached(&self, a: &[f64; 3]) -> bool {
        let v = a.get(self.comp).copied().unwrap_or(0.0);
        self.holds(a) && !(self.lo..=self.hi).contains(&v)
    }
}

#[derive(Debug, Clone)]
struct Checked {
    name: String,
    joint: Joint,
    rules: Vec<Rule>,
}

/// The possible limits resolved on a rig, every joint and side.
#[derive(Debug, Clone)]
pub struct Possible {
    joints: Vec<Checked>,
}

impl Possible {
    pub fn new(sk: &Skeleton, ranges: &Ranges) -> R<Possible> {
        let mut joints: Vec<Checked> = Vec::new();
        for &j in anatomy::JOINTS {
            let rules: Vec<Rule> = ranges
                .possible
                .iter()
                .filter(|b| b.joint == j)
                .map(|b| {
                    Ok(Rule {
                        comp: anatomy::component(&b.component)?,
                        lo: b.lo,
                        hi: b.hi,
                        max_swing: b.max_swing,
                    })
                })
                .collect::<R<_>>()?;
            if rules.is_empty() {
                continue;
            }
            let sided = anatomy::sided(j);
            for side in if sided { &["L", "R"][..] } else { &["L"][..] } {
                joints.push(Checked {
                    name: if sided {
                        format!("{j}.{side}")
                    } else {
                        j.to_string()
                    },
                    joint: Joint::new(sk, j, side)?,
                    rules: rules.clone(),
                });
            }
        }
        Ok(Possible { joints })
    }

    /// Every joint angle of a pose past its limit.
    pub fn breaches(&self, sk: &Skeleton, rotations: &[Quat]) -> R<Vec<Breach>> {
        let mut out = Vec::new();
        for c in &self.joints {
            let a = c.joint.angles(sk, rotations)?;
            for rule in c.rules.iter().filter(|r| r.breached(&a)) {
                out.push(breach(c, rule, &a));
            }
        }
        Ok(out)
    }

    /// Bend every impossible joint of a pose back inside its limits ([`MARGIN_DEG`] in), leaving
    /// its other angles and every other joint as they were. Returns what was past (the values
    /// before) and what is still past after (a limit that only holds at a small swing can end up
    /// outside its hold, so this is checked, not assumed).
    pub fn fix(&self, sk: &Skeleton, rotations: &mut [Quat]) -> R<(Vec<Breach>, Vec<Breach>)> {
        let (mut fixed, mut left) = (Vec::new(), Vec::new());
        for c in &self.joints {
            let a = c.joint.angles(sk, rotations)?;
            let past: Vec<&Rule> = c.rules.iter().filter(|r| r.breached(&a)).collect();
            if past.is_empty() {
                continue;
            }
            let mut want = a;
            for rule in &past {
                fixed.push(breach(c, rule, &a));
                if let Some(v) = want.get_mut(rule.comp) {
                    let m = MARGIN_DEG.min(0.5 * (rule.hi - rule.lo));
                    *v = v.clamp(rule.lo + m, rule.hi - m);
                }
            }
            c.joint.set_angles(sk, rotations, want)?;
            let b = c.joint.angles(sk, rotations)?;
            left.extend(
                c.rules
                    .iter()
                    .filter(|r| r.breached(&b))
                    .map(|r| breach(c, r, &b)),
            );
        }
        Ok((fixed, left))
    }
}

fn breach(c: &Checked, rule: &Rule, a: &[f64; 3]) -> Breach {
    Breach {
        joint: c.name.clone(),
        component: anatomy::COMPONENTS.get(rule.comp).copied().unwrap_or("?"),
        value: a.get(rule.comp).copied().unwrap_or(0.0),
        lo: rule.lo,
        hi: rule.hi,
    }
}

/// `[[first, last], ...]` runs of sorted frames.
pub fn runs(frames: &[i64]) -> Vec<[i64; 2]> {
    let mut out: Vec<[i64; 2]> = Vec::new();
    for &f in frames {
        match out.last_mut() {
            Some(r) if r[1] + 1 == f => r[1] = f,
            _ => out.push([f, f]),
        }
    }
    out
}

/// `"12–17, 30"`: runs as people read them.
pub fn runs_text(frames: &[i64]) -> String {
    runs(frames)
        .iter()
        .map(|[a, b]| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}–{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl Document {
    /// The possible limits on this document's rig (`None`: the range tables aren't loaded).
    pub fn possible(&self) -> R<Option<Possible>> {
        self.ranges
            .as_deref()
            .map(|r| Possible::new(&self.skeleton, r))
            .transpose()
    }

    /// After an AI fill of `a..b`: bend every impossible joint on the frames between back inside
    /// its limits (the keys there are the fill's own). Returns notes saying what was bent and
    /// anything still past; none when the fill was possible throughout.
    pub fn keep_possible(&mut self, a: i64, b: i64) -> R<Vec<String>> {
        Ok(self.keep_possible_report(a, b)?.notes)
    }

    /// [`Document::keep_possible`], with the joints bent back as data as well as in words.
    pub fn keep_possible_report(&mut self, a: i64, b: i64) -> R<Kept> {
        let Some(possible) = self.possible()? else {
            return Ok(Kept {
                notes: vec![
                    "the range tables are not loaded (assets/rig/ranges.json): the fill was not checked for impossible poses"
                        .into(),
                ],
                checked: false,
                bent: Vec::new(),
            });
        };
        let additive = self
            .clip
            .layers
            .get(self.clip.edit)
            .is_some_and(|l| l.mode == LayerMode::Additive);
        if b - a < 2 {
            return Ok(Kept {
                notes: Vec::new(),
                checked: true,
                bent: Vec::new(),
            });
        }
        let sk = self.skeleton.clone();
        let frames: Vec<f64> = (a + 1..b).map(|f| f as f64).collect();
        let (mut rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        let mut changed = std::collections::BTreeSet::new();
        let (mut bent, mut still) = (Vec::new(), Vec::new());
        // per joint and component: the worst breach and every frame it was bent back on
        let mut worst: Vec<Bent> = Vec::new();
        for (k, r) in rots.iter_mut().enumerate() {
            let f = a + 1 + k as i64;
            if additive {
                if !possible.breaches(&sk, r)?.is_empty() {
                    still.push(f);
                }
                continue;
            }
            let (fixed, left) = possible.fix(&sk, r)?;
            if !fixed.is_empty() {
                changed.insert(k);
                bent.push(f);
                for x in fixed {
                    match worst
                        .iter_mut()
                        .find(|w| w.joint == x.joint && w.component == x.component)
                    {
                        Some(w) => {
                            if x.past() > w.past_deg {
                                w.past_deg = x.past();
                                w.worst_frame = f;
                            }
                            w.frames.push(f);
                        }
                        None => worst.push(Bent {
                            joint: x.joint.clone(),
                            component: x.component,
                            past_deg: x.past(),
                            worst_frame: f,
                            frames: vec![f],
                        }),
                    }
                }
            }
            if !left.is_empty() {
                still.push(f);
            }
        }
        self.write_solved(&changed, a + 1, &rots, &roots, "possible")?;
        let mut notes = Vec::new();
        if !bent.is_empty() {
            worst.sort_by(|x, y| {
                y.past_deg
                    .total_cmp(&x.past_deg)
                    .then(x.joint.cmp(&y.joint))
                    .then(x.component.cmp(y.component))
            });
            let what: Vec<String> = worst
                .iter()
                .take(4)
                .map(|w| format!("{} {} {:.0}° past", w.joint, w.component, w.past_deg))
                .collect();
            notes.push(format!(
                "the AI fill made impossible poses on frames {}; they were bent back to the possible limit ({})",
                runs_text(&bent),
                what.join(", ")
            ));
        }
        if !still.is_empty() {
            notes.push(if additive {
                format!(
                    "frames {} of the fill are impossible poses; on an additive layer they are not bent back: check them in the Range panel",
                    runs_text(&still)
                )
            } else {
                format!(
                    "frames {} of the fill are still impossible poses after bending them back: check them in the Range panel",
                    runs_text(&still)
                )
            });
        }
        Ok(Kept {
            notes,
            checked: true,
            bent: worst,
        })
    }
}

/// One joint angle an AI fill made impossible and [`Document::keep_possible`] bent back: how far
/// past its limit it went at worst, on which frame, and every frame it was bent on.
#[derive(Debug, Clone, PartialEq)]
pub struct Bent {
    /// `"ankle.R"`, or `"trunk"` for a joint with no side.
    pub joint: String,
    pub component: &'static str,
    /// Degrees past the limit at worst.
    pub past_deg: f64,
    pub worst_frame: i64,
    /// Frames it was bent back on, in order.
    pub frames: Vec<i64>,
}

impl Bent {
    pub fn json(&self) -> Value {
        json!({
            "joint": self.joint,
            "component": self.component,
            "past_deg": r2(self.past_deg),
            "worst_frame": self.worst_frame,
            "spans": runs(&self.frames),
        })
    }
}

/// What [`Document::keep_possible_report`] did: the notes in words, whether the fill was checked
/// at all (the range tables may be missing), and each joint bent back, worst first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Kept {
    pub notes: Vec<String>,
    pub checked: bool,
    pub bent: Vec<Bent>,
}

/// Most passes [`Document::fix_impossible`] makes over the frames between keys (a key added
/// changes the curve next to it).
const FIX_PASSES: usize = 3;

impl Document {
    /// `clip.fix_impossible`: bend every impossible joint on the keys of the edited layer between
    /// `from` and `to` back inside its possible limit; then, where the curve between two keys still
    /// swings past a limit, add a key there with the pose bent back.
    pub fn fix_impossible(&mut self, from: Option<i64>, to: Option<i64>) -> R<Value> {
        let possible = self.possible()?.ok_or_else(|| {
            crate::error::hinted(
                "the range-of-motion tables are not loaded (assets/rig/ranges.json)",
                "build them with python -m ac_train.ranges, or start with --assets <dir>",
            )
        })?;
        let layer = self.clip.layers.get(self.clip.edit).ok_or("no layer to edit")?;
        if layer.mode == LayerMode::Additive {
            return Err(crate::error::hinted(
                "the edited layer is additive: its keys add to the layers below",
                "edit the layer that sets the impossible joints (layer.select), then run this again",
            ));
        }
        let (a, b) = (from.unwrap_or(0), to.unwrap_or(i64::MAX));
        if b < a {
            return Err(format!("the range {a}..{b} is empty"));
        }
        let keys: Vec<(i64, Interp)> = layer
            .keys
            .iter()
            .filter(|k| (a..=b).contains(&k.frame))
            .map(|k| (k.frame, k.interp))
            .collect();
        let (Some(&(lo, _)), Some(&(hi, _))) = (keys.first(), keys.last()) else {
            return Ok(json!({ "keys": 0, "fixed": [], "added": [], "left": 0,
                "notes": [format!("the edited layer has no keys between {a} and {}", if b == i64::MAX { "the end".to_string() } else { b.to_string() })] }));
        };
        if hi - lo + 1 > crate::metrics::MAX_FRAMES {
            return Err(format!(
                "{} frames is more than {} at once",
                hi - lo + 1,
                crate::metrics::MAX_FRAMES
            ));
        }
        let sk = self.skeleton.clone();
        let one = std::collections::BTreeSet::from([0]);
        let mut fixed = Vec::new();
        for &(f, _) in &keys {
            let (r, o) = self.clip.sample_with(&[f as f64], &self.links)?;
            let (Some(mut r), Some(o)) = (r.into_iter().next(), o.into_iter().next()) else {
                continue;
            };
            let (was, _) = possible.fix(&sk, &mut r)?;
            if !was.is_empty() {
                self.write_solved(&one, f, &[r], &[o], "possible")?;
                fixed.push(
                    json!({ "frame": f, "breaches": was.iter().map(Breach::json).collect::<Vec<_>>() }),
                );
            }
        }
        // the curve between the keys
        let frames: Vec<f64> = (lo..=hi).map(|f| f as f64).collect();
        let mut added = Vec::new();
        for _ in 0..FIX_PASSES {
            let (rots, roots) = self.clip.sample_with(&frames, &self.links)?;
            let mut any = false;
            for ((f, mut r), o) in (lo..).zip(rots).zip(roots) {
                if keys.iter().any(|k| k.0 == f) || added.contains(&f) {
                    continue;
                }
                let (was, _) = possible.fix(&sk, &mut r)?;
                if was.is_empty() {
                    continue;
                }
                let interp = keys
                    .iter()
                    .rev()
                    .find(|k| k.0 < f)
                    .map_or(Interp::Smooth, |k| k.1);
                let r: Vec<Quat> = r.into_iter().map(crate::quat::normalize).collect();
                self.clip.set_key(crate::clip::Key::new(f, r, o, interp))?;
                added.push(f);
                any = true;
            }
            if !any {
                break;
            }
        }
        added.sort_unstable();
        // what plays now (another layer may set a joint)
        let (rots, _) = self.clip.sample_with(&frames, &self.links)?;
        let mut left = Vec::new();
        let mut notes = Vec::new();
        for (f, r) in (lo..).zip(&rots) {
            if let Some(w) = possible.breaches(&sk, r)?.first() {
                left.push(f);
                if notes.len() < 8 {
                    notes.push(format!(
                        "frame {f}: {} {} is still {:.0}° past (another layer sets it?)",
                        w.joint,
                        w.component,
                        w.past()
                    ));
                }
            }
        }
        let fr = self.frame;
        self.go_to(fr)?;
        Ok(json!({ "keys": keys.len(), "fixed": fixed, "added": added, "left": left.len(), "notes": notes }))
    }
}

/// The three levels, mildest first.
pub const LEVELS: &[&str] = &["typical", "unusual", "impossible"];

/// What a level means, for people.
pub fn level_text(level: &str) -> &'static str {
    match level {
        "impossible" => {
            "Impossible: a joint is past anything a body can do (a foot turned round, a knee bent the wrong way). Fix or delete it."
        }
        "unusual" => {
            "Past the clinical limits but within realistic movement: unusual or athletic (dancers, gymnasts, sprinters). Fine to keep."
        }
        _ => "Inside the clinical limits: a typical range of motion.",
    }
}

/// Per joint angle: the frames past its possible limit and the worst of them.
#[derive(Debug)]
struct Acc {
    frames: Vec<i64>,
    worst: Option<(i64, Breach)>,
}

fn acc_json(frames: &[i64], worst: Option<(i64, f64)>) -> Value {
    json!({
        "frames": frames.len(),
        "spans": runs(frames),
        "max_deg": r2(worst.map_or(0.0, |w| w.1)),
        "worst_frame": worst.map(|w| w.0),
    })
}

/// `clip.possible`: every joint of frames `from..=to` against the possible limits, and every
/// range pair against the clinical ones: the worst level, where and how far.
pub fn measure(doc: &Document, from: Option<i64>, to: Option<i64>, shown: bool) -> R<Value> {
    let Some(ranges) = doc.ranges.as_deref() else {
        return Err(crate::error::hinted(
            "the range-of-motion tables are not loaded (assets/rig/ranges.json)",
            "build them with python -m ac_train.ranges, or start with --assets <dir>",
        ));
    };
    let sk = &doc.skeleton;
    let possible = Possible::new(sk, ranges)?;
    let mut notes = Vec::new();
    let (a, b, poses) = crate::ranges::poses(doc, from, to, shown, &mut notes)?;

    let mut per: Vec<(String, Acc)> = Vec::new();
    let (mut bad, mut bad_worst) = (Vec::new(), None::<(i64, f64)>);
    let (mut odd, mut odd_worst) = (Vec::new(), None::<(i64, f64)>);
    let mut probes = Vec::new();
    for p in &ranges.pairs {
        for &s in if p.sided { &["L", "R"][..] } else { &["L"][..] } {
            probes.push((p, crate::ranges::Probe::new(sk, p, s)?));
        }
    }
    for (f, pose) in (a..).zip(&poses) {
        let br = possible.breaches(sk, &pose.rotations)?;
        if let Some(m) = br.iter().map(Breach::past).reduce(f64::max) {
            bad.push(f);
            if bad_worst.is_none_or(|w| m > w.1) {
                bad_worst = Some((f, m));
            }
        }
        for x in br {
            let key = format!("{}.{}", x.joint, x.component);
            let slot = match per.iter().position(|(k, _)| *k == key) {
                Some(i) => i,
                None => {
                    per.push((
                        key,
                        Acc {
                            frames: Vec::new(),
                            worst: None,
                        },
                    ));
                    per.len() - 1
                }
            };
            if let Some((_, acc)) = per.get_mut(slot) {
                acc.frames.push(f);
                if acc.worst.as_ref().is_none_or(|w| x.past() > w.1.past()) {
                    acc.worst = Some((f, x));
                }
            }
        }
        let mut past = 0.0_f64;
        for (spec, probe) in &probes {
            let (x, y) = probe.point(sk, &pose.rotations)?;
            past = past.max(crate::ranges::judge(spec, x, y).past_hard);
        }
        if past > crate::metrics::LIMIT_SLACK_DEG {
            odd.push(f);
            if odd_worst.is_none_or(|w| past > w.1) {
                odd_worst = Some((f, past));
            }
        }
    }
    let worst_past = |a: &Acc| a.worst.as_ref().map_or(0.0, |w| w.1.past());
    per.sort_by(|x, y| worst_past(&y.1).total_cmp(&worst_past(&x.1)));
    let breaches: Vec<Value> = per
        .iter()
        .filter_map(|(_, acc)| {
            let (wf, w) = acc.worst.as_ref()?;
            let mut v = w.json();
            if let Some(o) = v.as_object_mut() {
                o.insert("frames".into(), json!(acc.frames.len()));
                o.insert("spans".into(), json!(runs(&acc.frames)));
                o.insert("worst_frame".into(), json!(wf));
            }
            Some(v)
        })
        .collect();
    let level = if !bad.is_empty() {
        "impossible"
    } else if !odd.is_empty() {
        "unusual"
    } else {
        "typical"
    };
    Ok(json!({
        "from": a,
        "to": b,
        "frames": b - a + 1,
        "motion": if shown { "shown" } else { "keys" },
        "level": level,
        "message": level_text(level),
        "impossible": acc_json(&bad, bad_worst),
        "breaches": breaches,
        "unusual": acc_json(&odd, odd_worst),
        "possible_note": ranges.possible_note,
        "notes": notes,
    }))
}
