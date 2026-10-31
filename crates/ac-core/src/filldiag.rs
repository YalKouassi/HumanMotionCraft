//! Why an AI fill might look wrong, measured (`ai.fill`, `ai.fill_all` and `ai.toggle` return it
//! as `diagnostics`): not a confidence score but the things a reviewer looks for around a filled
//! span, as numbers, each with the line past which it is flagged. The measures are the ones
//! `clip.metrics` already makes ([`crate::metrics`]) on the keys the fill wrote (the physics pass
//! is not part of a fill), plus the joints [`Document::keep_possible`] bent back:
//!
//! | measure | what | flagged past |
//! |---|---|---|
//! | `sliding` | per contact span ([`crate::contact`]), how far a planted foot slid (cm) | [`SLIDE_CM`] |
//! | `path` | the root's largest distance (m) from the keys' own path: the clip's interpolation with the span's AI keys taken out | [`PATH_M`] |
//! | `seams` | at each of the two keys, and at each anchor pose of a long fill ([`crate::anchors`]): the largest change of any joint's speed (m/s, `clip.metrics`'s `keys`) and of any bone's spin (rad/s, [`metrics::spin_change`]) across the key | [`POP_SPEED`], [`SPIN_RAD_S`] |
//! | `bent` | the joints `keep_possible` bent back inside the possible limits, and how far past they were (degrees) | [`BENT_DEG`] |
//! | `steps` | the feet landing inside the span: do they take turns, is any step under half the median ([`crate::steps`]; `regular` null: too few steps) | not flagged: only a walk should step evenly, and the baseline (roadmap 153500) comes first |
//!
//! `flags` lists what went past a line, each with its frames and the reason in words (the
//! timeline's marks). `notes` says what could not be measured (a seam on the animation's first or
//! last frame has no frame beyond it; a span with no planted foot has no slide to measure; the
//! range tables may be missing), so a partial result is never silently empty.

use serde_json::{Value, json};

use crate::ai::Filled;
use crate::document::Document;
use crate::metrics::{self, Pop, Slide};
use crate::possible::Bent;
use crate::quat::{self, Vec3};
use crate::steps::{self, Steps};

/// A planted foot sliding more than this (cm) is flagged (`clip.metrics`'s own line).
pub const SLIDE_CM: f64 = metrics::SLIDE_CM;
/// The root further than this (m) from the keys' own path is flagged.
pub const PATH_M: f64 = 0.10;
/// A joint's speed changing by more than this (m/s) across a key is flagged (`clip.metrics`'s
/// own line for a pop).
pub const POP_SPEED: f64 = metrics::POP_SPEED;
/// A bone's spin changing by more than this (rad/s) across a key is flagged.
pub const SPIN_RAD_S: f64 = 3.0;
/// A joint bent back by more than this (degrees) is flagged: every joint bent back is.
pub const BENT_DEG: f64 = 0.0;

/// The two keys of a filled span: what changed across each.
#[derive(Debug, Clone, PartialEq)]
pub struct Seam {
    pub frame: i64,
    /// The largest change of a joint's speed (m/s) and the joint; `None`: no frame on one side.
    pub pop: Option<Pop>,
    /// The largest change of a bone's spin (rad/s) and the bone; `None`: no frame on one side.
    pub spin: Option<(f64, String)>,
}

/// Something past a line: the timeline mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Flag {
    /// `sliding`, `path`, `pop`, `spin` or `bent`.
    pub kind: &'static str,
    pub from: i64,
    pub to: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostics {
    pub from: i64,
    pub to: i64,
    pub sliding: Vec<Slide>,
    /// The root's largest distance (m) from the keys' path, and on which frame (`None`: none).
    pub path_max_m: f64,
    pub path_frame: Option<i64>,
    pub seams: Vec<Seam>,
    pub bent: Vec<Bent>,
    /// `None`: not measured (a note says why).
    pub steps: Option<Steps>,
    pub notes: Vec<String>,
}

type R<T> = Result<T, String>;

/// Measure the fill `f` just kept in `doc`'s clip.
pub fn diagnose(doc: &Document, f: &Filled) -> Diagnostics {
    let (a, b) = (f.from, f.to);
    let mut d = Diagnostics {
        from: a,
        to: b,
        sliding: Vec::new(),
        path_max_m: 0.0,
        path_frame: None,
        seams: Vec::new(),
        bent: f.kept.bent.clone(),
        steps: None,
        notes: Vec::new(),
    };
    let last = (doc.clip.length_with(&doc.links) - 1).max(0);
    // the keys two fills meet at: yours, and each anchor pose between them
    let mut seam_frames = vec![a];
    seam_frames.extend(f.anchors.iter().map(|x| x.frame));
    seam_frames.push(b);
    match metrics::measure(doc, Some(a), Some(b), false) {
        Ok(m) => {
            d.sliding = m.sliding;
            if d.sliding.is_empty() {
                d.notes.push(format!(
                    "no foot is planted on frames {a}–{b}: nothing to measure for foot slide"
                ));
            }
            for &k in &seam_frames {
                d.seams.push(Seam {
                    frame: k,
                    pop: m.keys.iter().find(|p| p.frame == k).cloned(),
                    spin: None,
                });
            }
        }
        Err(e) => {
            d.notes
                .push(format!("foot slide and the seams' speed were not measured: {e}"));
            for &k in &seam_frames {
                d.seams.push(Seam {
                    frame: k,
                    pop: None,
                    spin: None,
                });
            }
        }
    }
    match spins(doc, &seam_frames, last) {
        Ok(sp) => {
            for (s, v) in d.seams.iter_mut().zip(sp) {
                s.spin = v;
            }
        }
        Err(e) => d.notes.push(format!("the seams' spin was not measured: {e}")),
    }
    for s in &d.seams {
        if s.frame == 0 && s.pop.is_none() {
            d.notes.push(
                "key 0 is the animation's first frame: the seam there has no frame before it to measure"
                    .into(),
            );
        } else if s.frame == last && s.pop.is_none() {
            d.notes.push(format!(
                "key {last} is the animation's last frame: the seam there has no frame after it to measure"
            ));
        }
    }
    match path(doc, a, b) {
        Ok(Some((m, fr))) => {
            d.path_max_m = m;
            // a frame is named only with a distance the result shows (rounding noise has none)
            d.path_frame = (r4(m) > 0.0).then_some(fr);
        }
        Ok(None) => {}
        Err(e) => d.notes.push(format!("the root's path was not measured: {e}")),
    }
    match steps::measure(doc, a, b) {
        Ok(s) => d.steps = Some(s),
        Err(e) => d.notes.push(format!("the steps were not measured: {e}")),
    }
    if !f.kept.checked {
        d.notes.push(
            "bent joints were not checked: the range tables are not loaded (assets/rig/ranges.json)".into(),
        );
    }
    d
}

/// The spin change at each of `keys` (`None` for a key with no frame on one side).
fn spins(doc: &Document, keys: &[i64], last: i64) -> R<Vec<Option<(f64, String)>>> {
    let mut out = Vec::with_capacity(keys.len());
    for &k in keys {
        if k <= 0 || k >= last {
            out.push(None);
            continue;
        }
        let fr = [k - 1, k, k + 1].map(|f| f as f64);
        let (rots, _) = doc.clip.sample_with(&fr, &doc.links)?;
        let (Some(p), Some(q), Some(n)) = (rots.first(), rots.get(1), rots.get(2)) else {
            out.push(None);
            continue;
        };
        out.push(
            metrics::spin_change(p, q, n, doc.clip.fps)
                .map(|(dw, i)| (dw, doc.skeleton.names.get(i).cloned().unwrap_or_default())),
        );
    }
    Ok(out)
}

/// The root's largest distance (m) from the keys' own path over the frames between the keys, and
/// where; `None` when there are no frames between.
fn path(doc: &Document, a: i64, b: i64) -> R<Option<(f64, i64)>> {
    if b - a < 2 {
        return Ok(None);
    }
    let frames: Vec<f64> = (a + 1..b).map(|f| f as f64).collect();
    let (_, filled) = doc.clip.sample_with(&frames, &doc.links)?;
    let mut keys_only = doc.clip.clone();
    crate::edits::clear_ai(&mut keys_only, a, b)?;
    let (_, keys) = keys_only.sample_with(&frames, &doc.links)?;
    let hip = doc.skeleton.hip_height;
    let mut best: Option<(f64, i64)> = None;
    for (k, (x, y)) in filled.iter().zip(&keys).enumerate() {
        let d = quat::len3(quat::sub3(*x as Vec3, *y)) * hip;
        if best.is_none_or(|(m, _)| d > m) {
            best = Some((d, a + 1 + k as i64));
        }
    }
    Ok(best)
}

fn side(foot: &str) -> &'static str {
    if foot == "L" { "left" } else { "right" }
}

fn span_text(a: i64, b: i64) -> String {
    if a == b {
        format!("frame {a}")
    } else {
        format!("frames {a}–{b}")
    }
}

impl Diagnostics {
    /// What went past a line, in frame order.
    pub fn flags(&self) -> Vec<Flag> {
        let mut out = Vec::new();
        for s in &self.sliding {
            if s.slide_cm > SLIDE_CM {
                out.push(Flag {
                    kind: "sliding",
                    from: s.from,
                    to: s.to,
                    text: format!(
                        "The {} foot slides {:.1} cm while it is planted on {} (over {SLIDE_CM:.0} cm).",
                        side(s.foot),
                        s.slide_cm,
                        span_text(s.from, s.to)
                    ),
                });
            }
        }
        if let Some(fr) = self.path_frame
            && self.path_max_m > PATH_M
        {
            out.push(Flag {
                kind: "path",
                from: fr,
                to: fr,
                text: format!(
                    "The root is {:.2} m from the keys' own path on frame {fr} (over {PATH_M:.2} m).",
                    self.path_max_m
                ),
            });
        }
        for s in &self.seams {
            if let Some(p) = &s.pop
                && p.dv > POP_SPEED
            {
                out.push(Flag {
                    kind: "pop",
                    from: s.frame,
                    to: s.frame,
                    text: format!(
                        "{} changes speed by {:.2} m/s across the key on frame {} (over {POP_SPEED:.0} m/s): the fill meets the key with a jump.",
                        p.joint, p.dv, s.frame
                    ),
                });
            }
            if let Some((dw, bone)) = &s.spin
                && *dw > SPIN_RAD_S
            {
                out.push(Flag {
                    kind: "spin",
                    from: s.frame,
                    to: s.frame,
                    text: format!(
                        "{bone} changes spin by {dw:.2} rad/s across the key on frame {} (over {SPIN_RAD_S:.0} rad/s): the fill meets the key with a snap.",
                        s.frame
                    ),
                });
            }
        }
        for b in &self.bent {
            if b.past_deg > BENT_DEG {
                let (from, to) = (
                    b.frames.first().copied().unwrap_or(b.worst_frame),
                    b.frames.last().copied().unwrap_or(b.worst_frame),
                );
                out.push(Flag {
                    kind: "bent",
                    from,
                    to,
                    text: format!(
                        "{} {} went {:.0}° past what a body can do on {} (worst on frame {}); it was bent back to the limit.",
                        b.joint,
                        b.component,
                        b.past_deg,
                        span_text(from, to),
                        b.worst_frame
                    ),
                });
            }
        }
        out.sort_by(|x, y| x.from.cmp(&y.from).then(x.to.cmp(&y.to)).then(x.kind.cmp(y.kind)));
        out
    }

    /// The `diagnostics` of a fill's result.
    pub fn json(&self) -> Value {
        let seams: Vec<Value> = self
            .seams
            .iter()
            .map(|s| {
                json!({
                    "frame": s.frame,
                    "dv": s.pop.as_ref().map(|p| r4(p.dv)),
                    "joint": s.pop.as_ref().map(|p| p.joint.clone()),
                    "dw": s.spin.as_ref().map(|(dw, _)| r4(*dw)),
                    "bone": s.spin.as_ref().map(|(_, b)| b.clone()),
                })
            })
            .collect();
        json!({
            "from": self.from,
            "to": self.to,
            "sliding": self.sliding.iter().map(|s| json!({
                "foot": s.foot, "from": s.from, "to": s.to, "slide_cm": r4(s.slide_cm),
            })).collect::<Vec<_>>(),
            "sliding_max_cm": r4(self.sliding.iter().map(|s| s.slide_cm).fold(0.0, f64::max)),
            "path": { "max_m": r4(self.path_max_m), "frame": self.path_frame },
            "seams": seams,
            "bent": self.bent.iter().map(Bent::json).collect::<Vec<_>>(),
            "steps": self.steps.as_ref().map(|s| json!({
                "contacts": s.contacts.iter().map(|c| json!({
                    "foot": c.foot, "from": c.from, "to": c.to, "x": r4(c.at[0]), "y": r4(c.at[1]),
                })).collect::<Vec<_>>(),
                "lengths_m": s.lengths.iter().map(|l| r4(*l)).collect::<Vec<_>>(),
                "median_m": s.median_m.map(r4),
                "alternate": s.alternate,
                "short": s.short,
                "regular": s.regular,
            })),
            "flags": self.flags().iter().map(|f| json!({
                "kind": f.kind, "from": f.from, "to": f.to, "text": f.text,
            })).collect::<Vec<_>>(),
            "notes": self.notes,
        })
    }
}

/// Round for a result: 0.1 mm, 0.1 mm/s, 0.0001 rad/s.
fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// A fill with nothing wrong has no flags; each line flags only what is past it.
    #[test]
    fn flags_follow_the_lines() {
        let mut d = Diagnostics {
            from: 0,
            to: 10,
            sliding: vec![
                Slide {
                    foot: "L",
                    from: 2,
                    to: 8,
                    slide_cm: SLIDE_CM,
                },
                Slide {
                    foot: "R",
                    from: 3,
                    to: 9,
                    slide_cm: SLIDE_CM + 0.5,
                },
            ],
            path_max_m: PATH_M,
            path_frame: Some(5),
            seams: vec![
                Seam {
                    frame: 0,
                    pop: None,
                    spin: None,
                },
                Seam {
                    frame: 10,
                    pop: Some(Pop {
                        frame: 10,
                        dv: POP_SPEED + 0.25,
                        joint: "root".into(),
                    }),
                    spin: Some((SPIN_RAD_S, "head".into())),
                },
            ],
            bent: vec![Bent {
                joint: "knee.L".into(),
                component: "flexion",
                past_deg: 3.0,
                worst_frame: 4,
                frames: vec![3, 4],
            }],
            steps: None,
            notes: Vec::new(),
        };
        // in frame order: (from, to, kind)
        let kinds: Vec<&str> = d.flags().iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["bent", "sliding", "pop"]);
        let f = d.flags();
        assert_eq!((f[0].from, f[0].to), (3, 4));
        assert_eq!((f[1].from, f[1].to), (3, 9));
        assert!(f[1].text.contains("right foot slides 2.5 cm"), "{}", f[1].text);
        assert!(
            f[2].text.contains("root changes speed by 1.25 m/s"),
            "{}",
            f[2].text
        );
        d.path_max_m = PATH_M + 0.01;
        d.seams[1].spin = Some((SPIN_RAD_S + 1.0, "head".into()));
        let kinds: Vec<&str> = d.flags().iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["bent", "sliding", "path", "pop", "spin"]);
        let v = d.json();
        assert_eq!(v["seams"][0]["dv"], Value::Null);
        assert_eq!(v["seams"][1]["bone"], "head");
        assert_eq!(v["bent"][0]["spans"], json!([[3, 4]]));
    }
}
