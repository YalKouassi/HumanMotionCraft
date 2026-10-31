//! AI fills over the whole animation: the gaps between your keys, filling every one of them (one
//! undo; with a model that takes several keys in a window, neighbouring gaps that fit one window
//! in one pass, `gaps::passes`), and switching one span between the model and plain interpolation.

use serde_json::{Map, Value, json};

use super::returns;
use crate::command::{CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::edits;
use crate::gaps::{self, Gap};

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("ai.gaps", "AI gaps", ai_gaps)
        .icon("ai")
        .outputs(returns::AI_GAPS),
    CommandSpec::new("ai.fill_all", "AI fill all gaps", ai_fill_all)
        .icon("ai")
        .undoable()
        .inputs(&[ParamSpec::new(
            "refill",
            Kind::Bool,
            "also fill again the gaps the model filled before (default: only empty gaps)",
        )])
        .outputs(returns::AI_FILL_ALL),
    CommandSpec::new("ai.toggle", "Switch AI / plain", ai_toggle)
        .icon("ai")
        .shortcut("Shift+A")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "from",
                Kind::Integer,
                "your key before the gap (default: around the playhead)",
            )
            .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "your key after the gap").range(0.0, 1e7),
        ])
        .outputs(returns::AI_TOGGLE),
];

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// Every gap between your keys, the one the playhead is in, and the longest the model fills.
fn ai_gaps(doc: &mut Document, _: &P) -> Out {
    let all = gaps::gaps(doc);
    let at = gaps::gap_at(doc, doc.key_frame());
    Ok(json!({ "gaps": all, "at": at, "max_gap": gaps::max_gap(doc) }))
}

/// Fill every gap the model can take (one undo step). Gaps it can't take are listed with the
/// reason; none filled at all is a refusal that says why. A model trained with several keys in
/// a window (`ai.info`'s `multi_key`) takes neighbouring gaps that fit one window in one pass
/// (`passes` lists them), so they agree at the keys they share without an ease.
fn ai_fill_all(doc: &mut Document, p: &P) -> Out {
    let refill = p.get("refill").and_then(Value::as_bool).unwrap_or(false);
    let mut filled: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut model = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut anchors: Vec<Value> = Vec::new();
    let mut diagnostics: Vec<Value> = Vec::new();
    let mut passes: Vec<Value> = Vec::new();
    let mut ready: Vec<(i64, i64)> = Vec::new();
    for g in gaps::gaps(doc) {
        if let Some(why) = &g.refused {
            skipped.push(json!({ "from": g.from, "to": g.to, "reason": why }));
            continue;
        }
        if g.filled && !refill {
            skipped.push(json!({ "from": g.from, "to": g.to, "reason": "filled already" }));
            continue;
        }
        if g.filled && edits::generated_in(&doc.clip, g.from, g.to) {
            skipped.push(
                json!({ "from": g.from, "to": g.to, "reason": "made from text (text.fill makes it again)" }),
            );
            continue;
        }
        ready.push((g.from, g.to));
    }
    let multi = doc.ai.as_ref().is_some_and(|m| m.info().multi_key);
    let plan: Vec<Vec<i64>> = match (multi, gaps::max_gap(doc)) {
        (true, Some(max)) => gaps::passes(&ready, max),
        _ => ready.iter().map(|&(a, b)| vec![a, b]).collect(),
    };
    for keys in plan {
        let (Some(&a), Some(&b)) = (keys.first(), keys.last()) else {
            continue;
        };
        if keys.len() > 2 {
            let f = doc.ai_fill_keys(&keys)?;
            notes.extend(f.notes.iter().map(|n| format!("{}–{}: {n}", f.from, f.to)));
            for w in keys.windows(2) {
                let &[p, q] = w else { continue };
                let gap = crate::ai::Filled {
                    from: p,
                    to: q,
                    model: f.model.clone(),
                    hash: f.hash.clone(),
                    notes: Vec::new(),
                    kept: f.kept.clone(),
                    anchors: Vec::new(),
                    segment: None,
                };
                diagnostics.push(crate::filldiag::diagnose(doc, &gap).json());
                filled.push(json!([p, q]));
            }
            model = f.model;
            passes.push(json!(keys));
            continue;
        }
        let f = doc.ai_fill(Some((a, b)))?;
        notes.extend(f.notes.iter().map(|n| format!("{}–{}: {n}", f.from, f.to)));
        anchors.extend(f.anchors.iter().map(|an| {
            json!({
                "from": f.from, "to": f.to, "frame": an.frame, "id": an.id,
                "source_frame": an.source_frame,
            })
        }));
        diagnostics.push(crate::filldiag::diagnose(doc, &f).json());
        model = f.model;
        filled.push(json!([f.from, f.to]));
    }
    if filled.is_empty() {
        let why: Vec<String> = skipped
            .iter()
            .map(|s| {
                format!(
                    "{}–{}: {}",
                    s["from"],
                    s["to"],
                    s["reason"].as_str().unwrap_or_default()
                )
            })
            .collect();
        return Err(if why.is_empty() {
            "nothing to fill: the animation needs at least two keys of yours".into()
        } else {
            format!("nothing to fill ({})", why.join("; "))
        });
    }
    Ok(json!({
        "filled": filled, "skipped": skipped, "model": model, "notes": notes, "anchors": anchors,
        "diagnostics": diagnostics, "passes": passes,
    }))
}

/// The span: `from` and `to`, else the gap the playhead is in.
fn span(doc: &Document, p: &P) -> Result<Gap, String> {
    let all = gaps::gaps(doc);
    match (
        p.get("from").and_then(Value::as_i64),
        p.get("to").and_then(Value::as_i64),
    ) {
        (None, None) => gaps::gap_at(doc, doc.key_frame())
            .ok_or_else(|| "put the playhead between two of your keys".to_string()),
        (Some(a), Some(b)) => all
            .into_iter()
            .find(|g| g.from == a && g.to == b)
            .ok_or_else(|| format!("{a} and {b} are not neighbouring keys of yours")),
        _ => Err("give both `from` and `to`, or neither".into()),
    }
}

/// A span the model filled goes back to plain interpolation; a plain one is filled by the model.
fn ai_toggle(doc: &mut Document, p: &P) -> Out {
    let g = span(doc, p)?;
    if g.filled {
        let n = edits::clear_ai(&mut doc.clip, g.from, g.to)?;
        doc.go_to(doc.frame)?;
        return Ok(json!({ "from": g.from, "to": g.to, "mode": "plain", "cleared": n }));
    }
    if let Some(why) = g.refused {
        return Err(why);
    }
    let f = doc.ai_fill(Some((g.from, g.to)))?;
    let d = crate::filldiag::diagnose(doc, &f).json();
    Ok(json!({
        "from": f.from, "to": f.to, "mode": "ai", "model": f.model, "notes": f.notes,
        "anchors": f.anchors, "diagnostics": d,
    }))
}
