//! Every built-in animation's motion quality, against a checked-in table
//! (`tests/quality/builtins.json`), so a bug measured here can't come back once it is fixed. A clip
//! that gets worse fails. So does one that gets better, until its row is lowered: bless with
//! `AC_BLESS_QUALITY=1 cargo test -p ac-cli --test quality_ratchet`, look at the diff and say so in
//! the commit. Numbers are from the core, deterministic on every machine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ac_core::Session;
use serde_json::{Value, json};

/// How far a number may differ before it counts (they are rounded to this).
const TOL: f64 = 0.01;

fn clips(dir: &Path, root: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            clips(&p, root, out);
        } else {
            let rel = p.strip_prefix(root).unwrap();
            let parts: Vec<String> = rel.iter().map(|c| c.to_string_lossy().into_owned()).collect();
            if let Some(id) = parts.join("/").strip_suffix(".clip.json") {
                out.push(id.to_string());
            }
        }
    }
}

fn round(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The most the physics pass adds to the root's speed (m/s) on any frame.
fn root_jump(s: &mut Session) -> f64 {
    let doc = &mut s.document;
    let cache = doc.physics().unwrap();
    let shown = &cache.report.roots;
    let frames: Vec<f64> = (0..shown.len()).map(|f| f as f64).collect();
    let (_, keyed) = doc.clip.sample_with(&frames, &doc.links).unwrap();
    let scale = doc.skeleton.hip_height * doc.clip.fps;
    let speed = |r: &[[f64; 3]], f: usize| {
        let (a, b) = (r[f], r[f + 1]);
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt() * scale
    };
    (0..shown.len().saturating_sub(1))
        .map(|f| speed(shown, f) - speed(&keyed, f))
        .fold(0.0, f64::max)
}

fn measure(s: &mut Session, id: &str) -> Value {
    s.run("clip.open", &json!({ "id": id })).unwrap();
    let one = |s: &mut Session, motion: &str| {
        let m = s.run("clip.metrics", &json!({ "motion": motion })).unwrap();
        let dv = m["keys"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|k| k["dv"].as_f64())
            .fold(0.0, f64::max);
        json!({
            "slide_cm": round(m["sliding_max_cm"].as_f64().unwrap()),
            "penetration_cm": round(m["penetration"]["max_cm"].as_f64().unwrap_or(0.0)),
            "balance_frames": m["balance"]["frames"],
            "pops": m["pops"],
            "worst_dv": round(dv),
        })
    };
    let (shown, keys) = (one(s, "shown"), one(s, "keys"));
    json!({ "shown": shown, "keys": keys, "root_jump_ms": round(root_jump(s)) })
}

/// Where `now` differs from `was`, as readable lines.
fn differences(id: &str, at: &str, was: &Value, now: &Value, out: &mut Vec<String>) {
    match (was.as_object(), now.as_object()) {
        (Some(a), Some(b)) => {
            for (k, v) in b {
                differences(id, &format!("{at}.{k}"), a.get(k).unwrap_or(&Value::Null), v, out);
            }
        }
        _ => {
            let (a, b) = (was.as_f64().unwrap_or(f64::NAN), now.as_f64().unwrap_or(f64::NAN));
            if a.is_nan() || (a - b).abs() > TOL * 1.5 {
                let how = if b > a { "WORSE" } else { "better" };
                out.push(format!("{id}{at}: {how}, {was} -> {now}"));
            }
        }
    }
}

#[test]
fn builtin_motion_quality_only_improves() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let assets = repo.join("assets");
    let root = assets.join("library/animations");
    let mut ids = Vec::new();
    clips(&root, &root, &mut ids);
    ids.sort();
    assert!(!ids.is_empty());
    let chunks: Vec<(usize, Vec<String>)> = ids
        .chunks(ids.len().div_ceil(8))
        .map(<[String]>::to_vec)
        .enumerate()
        .collect();
    let measured: BTreeMap<String, Value> = ac_cli::par_map(&chunks, |(i, part)| {
        let lib = std::env::temp_dir().join(format!(
            "{}-ratchet-{}-{i}",
            ac_core::product::ID,
            std::process::id()
        ));
        let mut s = ac_cli::session(&assets, &lib).unwrap();
        let out: Vec<(String, Value)> = part.iter().map(|id| (id.clone(), measure(&mut s, id))).collect();
        std::fs::remove_dir_all(&lib).ok();
        out
    })
    .into_iter()
    .flatten()
    .collect();

    let table = repo.join("tests/quality/builtins.json");
    if std::env::var_os("AC_BLESS_QUALITY").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(table.parent().unwrap()).unwrap();
        std::fs::write(&table, serde_json::to_string_pretty(&measured).unwrap() + "\n").unwrap();
        return;
    }
    let text = std::fs::read_to_string(&table).expect("no tests/quality/builtins.json: bless it");
    let was: BTreeMap<String, Value> = serde_json::from_str(&text).unwrap();
    let mut diffs = Vec::new();
    for (id, now) in &measured {
        match was.get(id) {
            Some(w) => differences(id, "", w, now, &mut diffs),
            None => diffs.push(format!("{id}: not in the table")),
        }
    }
    diffs.extend(
        was.keys()
            .filter(|id| !measured.contains_key(*id))
            .map(|id| format!("{id}: no such clip")),
    );
    assert!(
        diffs.is_empty(),
        "the motion quality of the built-ins changed (bless with AC_BLESS_QUALITY=1 after looking):\n{}",
        diffs.join("\n")
    );
}
