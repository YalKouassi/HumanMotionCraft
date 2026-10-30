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
//! | limits (degrees past a joint's range) | 1 | 10 | 30 |
//! | pop (m/s velocity change at a key) | 1 | 1.5 | 3 |
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
                "The {} foot slides {:.1} cm while it is planted on {}. Locking the feet holds each planted foot in one spot for its whole contact.",
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
                "The body stands still but its centre of mass is {:.1} cm outside its feet on {} (worst on frame {w}): it would fall. {}",
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
    for s in m.limits.iter().flatten() {
        let (Some(w), Some(sev)) = (
            s.worst_frame,
            severity(s.worst, [metrics::LIMIT_SLACK_DEG, 10.0, 30.0]),
        ) else {
            continue;
        };
        let runs = s.runs();
        let (a, b) = (runs.first().map_or(w, |r| r[0]), runs.last().map_or(w, |r| r[1]));
        let mut batch = vec![("frame.set".to_string(), json!({ "frame": w }))];
        if let Some(q) = s.clamped {
            batch.push((
                "pose.set_bone".to_string(),
                json!({ "bone": s.at, "rotation": q.map(r6) }),
            ));
            batch.push(("key.set".to_string(), json!({})));
        }
        out.push(Finding {
            kind: "limits",
            severity: sev,
            score: s.worst / 30.0,
            frames: [a, b],
            message: format!(
                "{} turns {:.1}° past its range on {} (worst on frame {w}). Turning it back to the edge of its range and keying it fixes the worst frame.",
                s.at,
                s.worst,
                span_text(a, b)
            ),
            batch,
        });
    }
    for k in &m.keys {
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
                "{} changes speed by {:.2} m/s at the key on frame {}: a visible pop. {tail}",
                k.joint, k.dv, k.frame
            ),
            batch,
        });
    }
    let rank = |k: &str| KINDS.iter().position(|x| *x == k).unwrap_or(KINDS.len());
    out.sort_by(|x, y| {
        y.severity
            .cmp(&x.severity)
            .then(y.score.total_cmp(&x.score))
            .then(rank(x.kind).cmp(&rank(y.kind)))
            .then(x.frames[0].cmp(&y.frames[0]))
    });
    out.truncate(limit.min(MAX_FINDINGS));
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

fn r6(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}
