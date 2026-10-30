//! A review of an animation (`clip.critique`): the problems [`crate::metrics`] measures, ranked
//! into findings an agent can act on. Each finding names its frames, says what is wrong in plain
//! words, and suggests a fix as a command with its parameters, plus the whole sequence as `batch`
//! (`[{id, params}]`, ready for `edit.batch`, which makes it one undo step).
//!
//! Severity, by how far past the line the problem goes:
//!
//! | kind | low from | medium from | high from |
//! |---|---|---|---|
//! | sliding (cm a planted foot slides) | 2 | 4 | 10 |
//! | penetration (cm below the ground) | 0.1 | 0.5 | 2 |
//! | balance (cm outside the support, still frames) | 0 | 2 | 5 |
//! | capture (cm to the side of the feet, moving frames) | 8 | 15 | 30 |
//! | limits (`clip.possible`'s level) | unusual | | impossible |
//! | pop (m/s a body joint's change of speed at a key stands out from the frames near it) | 1 | 1.5 | 3 |
//!
//! A suggested fix is tried on a copy first ([`MAX_CHECKED`] of them at most): it is offered only if
//! it lessens its own finding and worsens no other kind. Otherwise the finding says it has none.
//!
//! Findings are ordered by severity, then by how far past the high line they go (`score`:
//! amount / high line), then by kind and first frame, so the order is deterministic.

use serde_json::{Value, json};

use crate::document::Document;
use crate::metrics::{self, Metrics};

/// Most findings returned.
pub const MAX_FINDINGS: usize = 200;

struct Finding {
    kind: &'static str,
    severity: usize,
    score: f64,
    frames: [i64; 2],
    message: String,
    batch: Vec<(String, Value)>,
}

/// 0 low, 1 medium, 2 high; `None` under the low line.
fn severity(x: f64, lines: [f64; 3]) -> Option<usize> {
    if x >= lines[2] {
        Some(2)
    } else if x >= lines[1] {
        Some(1)
    } else if x > lines[0] || (lines[0] == 0.0 && x > 0.0) {
        Some(0)
    } else {
        None
    }
}

/// Most fixes tried on a copy.
pub const MAX_CHECKED: usize = 20;
/// How much (in each kind's unit) a number may rise and not count as worse: rounding, not a change.
const WORSE: f64 = 0.05;

/// One number per kind, as `KINDS` orders them; a kind with several findings counts each's own.
#[derive(Clone, Copy)]
struct Amounts([f64; 6]);

fn amounts(doc: &Document, m: &Metrics) -> Amounts {
    let slide = m.sliding.iter().map(|s| s.slide_cm).fold(0.0, f64::max);
    let past = crate::possible::measure(doc, Some(m.from), Some(m.to), m.shown).map_or(0.0, |v| {
        v["impossible"]["max_deg"].as_f64().unwrap_or(0.0) + v["unusual"]["max_deg"].as_f64().unwrap_or(0.0)
    });
    let pop = m.spikes.iter().map(|k| k.dv).fold(0.0, f64::max);
    Amounts([
        m.penetration.worst,
        slide,
        m.balance.worst,
        m.capture.worst,
        past,
        pop,
    ])
}

/// What a fix should lessen: the sum for sliding (a fix for one foot needn't move the other's worst);
/// this key's own for a pop.
fn own_amount(f: &Finding, m: &Metrics, all: &Amounts, kind: usize) -> f64 {
    match f.kind {
        "sliding" => m.sliding.iter().map(|s| s.slide_cm).sum(),
        "pop" => m
            .spikes
            .iter()
            .find(|k| k.frame == f.frames[0])
            .map_or(0.0, |k| k.dv),
        _ => all.0.get(kind).copied().unwrap_or(0.0),
    }
}

/// Whether a finding's batch, run on a copy, lessens the finding and worsens no other kind.
fn helps(doc: &Document, m: &Metrics, f: &Finding) -> bool {
    if f.batch.is_empty() {
        return false;
    }
    let mut copy = doc.clone();
    for (id, params) in &f.batch {
        let spec = crate::commands::AREAS
            .iter()
            .flat_map(|area| area.iter())
            .find(|s| s.id == *id);
        let Some(spec) = spec else {
            return false;
        };
        let Ok(checked) = crate::command::check(spec, params) else {
            return false;
        };
        if (spec.run)(&mut copy, &checked).is_err() {
            return false;
        }
    }
    let Ok(after) = metrics::measure(&copy, Some(m.from), Some(m.to), m.shown) else {
        return false;
    };
    let (before_all, after_all) = (amounts(doc, m), amounts(&copy, &after));
    let kind = KINDS.iter().position(|k| *k == f.kind).unwrap_or(KINDS.len());
    if own_amount(f, &after, &after_all, kind) >= own_amount(f, m, &before_all, kind) - WORSE {
        return false;
    }
    before_all
        .0
        .iter()
        .zip(&after_all.0)
        .enumerate()
        .all(|(k, (b, a))| k == kind || *a <= *b + WORSE)
}

const SEVERITY: [&str; 3] = ["low", "medium", "high"];
const KINDS: [&str; 6] = ["penetration", "sliding", "balance", "capture", "limits", "pop"];

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

/// The findings for `m`, worst first (at most `limit`).
pub fn findings(doc: &Document, m: &Metrics, limit: usize) -> Vec<Value> {
    let mut out: Vec<Finding> = Vec::new();
    // turning the physics pass on fixes what plays, not the keys: suggested only when what plays
    // was measured and the pass is off
    let physics_off = m.shown && !doc.clip.physics;
    for s in &m.sliding {
        let lines = [metrics::SLIDE_CM, 4.0, 10.0];
        let Some(sev) = severity(s.slide_cm, lines) else {
            continue;
        };
        out.push(Finding {
            kind: "sliding",
            severity: sev,
            score: s.slide_cm / lines[2],
            frames: [s.from, s.to],
            message: format!(
                "The {} foot slides {:.1} cm while it is planted on {}. The keys carry it across the floor while it holds weight. Locking the feet holds each planted foot in one spot for its whole contact.",
                side(s.foot),
                s.slide_cm,
                span_text(s.from, s.to)
            ),
            batch: vec![("feet.lock".into(), json!({}))],
        });
    }
    let p = &m.penetration;
    if let (Some(w), Some(sev)) = (p.worst_frame, severity(p.worst, [0.1, 0.5, 2.0])) {
        let runs = p.runs();
        let (a, b) = (runs.first().map_or(w, |r| r[0]), runs.last().map_or(w, |r| r[1]));
        let (message, batch) = if physics_off {
            (
                format!(
                    "{} goes {:.1} cm into the ground (worst on frame {w}, {} frames in all). The physics pass lifts every frame out of the ground; it is off.",
                    p.at,
                    p.worst,
                    p.frames.len()
                ),
                vec![("physics.set".to_string(), json!({ "on": true }))],
            )
        } else {
            (
                format!(
                    "{} goes {:.1} cm into the ground on {} (worst on frame {w}). Lift what is below the ground on that frame and key it.",
                    p.at,
                    p.worst,
                    span_text(a, b)
                ),
                std::iter::once(("frame.set".to_string(), json!({ "frame": w })))
                    .chain(p.below.iter().map(|(bone, d)| {
                        // out of the ground, and half a centimetre clear of it
                        (
                            "pose.drag".to_string(),
                            json!({ "bone": bone, "by": [0.0, 0.0, r4(d + 0.005)] }),
                        )
                    }))
                    .chain(std::iter::once(("key.set".to_string(), json!({}))))
                    .collect(),
            )
        };
        out.push(Finding {
            kind: "penetration",
            severity: sev,
            score: p.worst / 2.0,
            frames: [a, b],
            message,
            batch,
        });
    }
    let bal = &m.balance;
    if let (Some(w), Some(sev)) = (bal.worst_frame, severity(bal.worst, [0.0, 2.0, 5.0])) {
        let runs = bal.runs();
        let (a, b) = (runs.first().map_or(w, |r| r[0]), runs.last().map_or(w, |r| r[1]));
        let fix = bal.fix.unwrap_or([0.0, 0.0]);
        let batch = if physics_off {
            vec![("physics.set".to_string(), json!({ "on": true }))]
        } else {
            vec![
                ("frame.set".to_string(), json!({ "frame": w })),
                (
                    "pose.drag".to_string(),
                    json!({ "bone": "root", "by": [r4(fix[0]), r4(fix[1]), 0.0] }),
                ),
                ("key.set".to_string(), json!({})),
            ]
        };
        out.push(Finding {
            kind: "balance",
            severity: sev,
            score: bal.worst / 5.0,
            frames: [a, b],
            message: format!(
                "The body stands still but its centre of mass is {:.1} cm outside its feet on {} (worst on frame {w}): the hips are past the feet and nothing is catching it. {}",
                bal.worst,
                span_text(a, b),
                if physics_off {
                    "The physics pass balances still frames; it is off."
                } else {
                    "Move the hips over the feet on that frame and key it."
                }
            ),
            batch,
        });
    }
    let cap = &m.capture;
    let tolerated = crate::weight::TOLERATED * 100.0;
    if let (Some(w), Some(sev)) = (cap.worst_frame, severity(cap.worst, [tolerated, 15.0, 30.0])) {
        let runs = cap.runs();
        let (a, b) = (runs.first().map_or(w, |r| r[0]), runs.last().map_or(w, |r| r[1]));
        out.push(Finding {
            kind: "capture",
            severity: sev,
            score: cap.worst / 30.0,
            frames: [a, b],
            message: format!(
                "The body's weight is heading {:.1} cm to the side of its feet on {} (worst on frame {w}): the feet it has or is about to have can't catch it. Shift the hips over them.",
                cap.worst,
                span_text(a, b),
            ),
            batch: vec![("hips.transfer".to_string(), json!({}))],
        });
    }
    out.extend(limit_finding(doc, m));
    for k in &m.spikes {
        let Some(sev) = severity(k.dv, [metrics::POP_SPEED, 1.5, 3.0]) else {
            continue;
        };
        let smooth = doc
            .clip
            .keys()
            .iter()
            .find(|key| key.frame == k.frame)
            .is_some_and(|key| key.interp == crate::clip::Interp::Smooth);
        let (tail, batch) = if smooth {
            (
                "The key is smooth already: an in-between key before or after it eases the change.",
                vec![("frame.set".to_string(), json!({ "frame": k.frame }))],
            )
        } else {
            (
                "Smooth interpolation eases into and out of it.",
                vec![
                    ("frame.set".to_string(), json!({ "frame": k.frame })),
                    ("key.interp".to_string(), json!({ "interp": "smooth" })),
                ],
            )
        };
        out.push(Finding {
            kind: "pop",
            severity: sev,
            score: k.dv / 3.0,
            frames: [k.frame, k.frame],
            message: format!(
                "{} changes speed {:.2} m/s more than the frames around it at the key on frame {}: a visible pop, because the motion turns on the key instead of through it. {tail}",
                k.joint, k.dv, k.frame
            ),
            batch,
        });
    }
    finish(doc, m, out, limit)
}

/// What `clip.possible` says of the joints, so the two never disagree: unusual is low, impossible
/// is high, typical is no finding. If it can't say, the finding says that.
fn limit_finding(doc: &Document, m: &Metrics) -> Option<Finding> {
    let v = match crate::possible::measure(doc, Some(m.from), Some(m.to), m.shown) {
        Ok(v) => v,
        Err(e) => {
            return Some(Finding {
                kind: "limits",
                severity: 0,
                score: 0.0,
                frames: [m.from, m.to],
                message: format!("The joints' range could not be checked: {e}"),
                batch: Vec::new(),
            });
        }
    };
    let level = v["level"].as_str()?;
    let (part, sev) = match level {
        "impossible" => ("impossible", 2),
        "unusual" => ("unusual", 0),
        _ => return None,
    };
    let w = v[part]["worst_frame"].as_i64()?;
    let spans = v[part]["spans"].as_array().cloned().unwrap_or_default();
    let (a, b) = (
        spans.first().and_then(|r| r[0].as_i64()).unwrap_or(w),
        spans.last().and_then(|r| r[1].as_i64()).unwrap_or(w),
    );
    let deg = v[part]["max_deg"].as_f64().unwrap_or(0.0);
    let message = if sev == 2 {
        let joint = v["breaches"][0]["joint"].as_str().unwrap_or("A joint");
        format!(
            "{joint} goes {deg:.1}° past what a body can do on {} (worst on frame {w}): no recording reaches it. Bend it back on that frame and key it.",
            span_text(a, b)
        )
    } else {
        format!(
            "A joint goes {deg:.1}° past the usual clinical range on {} (worst on frame {w}). A body can do this: athletes and dancers do. Check it is meant.",
            span_text(a, b)
        )
    };
    Some(Finding {
        kind: "limits",
        severity: sev,
        score: deg / 30.0,
        frames: [a, b],
        message,
        batch: Vec::new(),
    })
}

/// Rank the findings, keep the worst `limit`, and offer only the fixes that help.
fn finish(doc: &Document, m: &Metrics, mut out: Vec<Finding>, limit: usize) -> Vec<Value> {
    let rank = |k: &str| KINDS.iter().position(|x| *x == k).unwrap_or(KINDS.len());
    out.sort_by(|x, y| {
        y.severity
            .cmp(&x.severity)
            .then(y.score.total_cmp(&x.score))
            .then(rank(x.kind).cmp(&rank(y.kind)))
            .then(x.frames[0].cmp(&y.frames[0]))
    });
    out.truncate(limit.min(MAX_FINDINGS));
    for (i, f) in out.iter_mut().enumerate() {
        if f.batch.is_empty() {
            continue;
        }
        if i >= MAX_CHECKED {
            f.batch.clear();
            f.message.push_str(" (Its fix was not tried: too many findings.)");
        } else if !helps(doc, m, f) {
            f.batch.clear();
            f.message
                .push_str(" (The usual fix was tried on a copy and didn't help, so none is offered.)");
        }
    }
    out.into_iter()
        .map(|f| {
            // the suggested command: the step that changes something (not the frame.set before it)
            let main = f
                .batch
                .iter()
                .find(|(id, _)| id != "frame.set")
                .or_else(|| f.batch.first())
                .cloned();
            json!({
                "kind": f.kind,
                "severity": SEVERITY.get(f.severity).copied().unwrap_or("low"),
                "score": r4(f.score),
                "frames": f.frames,
                "message": f.message,
                "command": main.as_ref().map(|m| m.0.clone()),
                "params": main.map(|m| m.1),
                "batch": f.batch.into_iter().map(|(id, params)| json!({ "id": id, "params": params })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}
