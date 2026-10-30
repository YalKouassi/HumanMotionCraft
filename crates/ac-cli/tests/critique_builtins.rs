//! `clip.critique` over every built-in animation: it agrees with `clip.possible`, the clean clips stay
//! clean, and every fix it offers does what it says when it is really run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use ac_core::Session;
use serde_json::{Value, json};

/// Clips with no medium or high finding: a review that flags these can't be trusted. The hand-keyed
/// walk on the spot, and the recorded walks.
const CLEAN: [&str; 7] = [
    "Locomotion/Walk",
    "Locomotion/Walk 12",
    "Locomotion/Walk 13",
    "Locomotion/Walk 14",
    "Locomotion/Walk 15",
    "Locomotion/Walk 16",
    "Locomotion/Walk 17",
];
/// A number may rise this much (rounding) and not count as worse: the critique's own margin.
const WORSE: f64 = 0.05;

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

/// Slide (the sum over the feet), penetration, balance and capture, the numbers a fix mustn't worsen.
fn numbers(s: &mut Session) -> [f64; 4] {
    let m = s.run("clip.metrics", &json!({})).unwrap();
    let max = |k: &str| m[k]["max_cm"].as_f64().unwrap_or(0.0);
    let slide = m["sliding"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["slide_cm"].as_f64())
        .sum();
    [slide, max("penetration"), max("balance"), max("capture")]
}

fn pop_at(s: &mut Session, frame: &Value) -> f64 {
    let c = s.run("clip.critique", &json!({ "limit": 200 })).unwrap();
    c["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["kind"] == "pop" && f["frames"][0] == *frame)
        .and_then(|f| f["score"].as_f64())
        .unwrap_or(0.0)
}

#[test]
fn the_review_of_every_builtin_can_be_trusted() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = std::env::temp_dir().join("ac-critique-builtins");
    let mut s = ac_cli::session(&repo.join("assets"), &lib).unwrap();
    let root = repo.join("assets/library/animations");
    let mut ids = Vec::new();
    clips(&root, &root, &mut ids);
    ids.sort();
    assert!(ids.len() > 50, "{} clips", ids.len());
    let mut problems = Vec::new();
    let mut tried = 0;
    let mut tally = [0; 4];
    for id in &ids {
        s.run("clip.open", &json!({ "id": id })).unwrap();
        let c = s.run("clip.critique", &json!({ "limit": 200 })).unwrap();
        let findings = c["findings"].as_array().unwrap().clone();
        let worst = ["high", "medium", "low"]
            .into_iter()
            .position(|sev| findings.iter().any(|f| f["severity"] == sev))
            .unwrap_or(3);
        tally[worst] += 1;
        for f in findings.iter().filter(|f| f["severity"] == "high") {
            println!("high: {id}: {}", f["kind"]);
        }

        if CLEAN.contains(&id.as_str()) {
            for f in findings.iter().filter(|f| f["severity"] != "low") {
                problems.push(format!("{id} should be clean: {}", f["message"]));
            }
        }

        let level = s.run("clip.possible", &json!({})).unwrap()["level"].clone();
        let limit = findings.iter().find(|f| f["kind"] == "limits");
        let agrees = match (level.as_str().unwrap(), limit) {
            ("typical", None) => true,
            ("unusual", Some(f)) => f["severity"] == "low",
            ("impossible", Some(f)) => f["severity"] == "high",
            _ => false,
        };
        if !agrees {
            problems.push(format!("{id}: clip.possible says {level}, the review {limit:?}"));
        }

        for f in &findings {
            let batch = f["batch"].as_array().unwrap();
            if batch.is_empty() {
                continue;
            }
            tried += 1;
            let frame = &f["frames"][0];
            let before = numbers(&mut s);
            let pop = pop_at(&mut s, frame);
            s.run("edit.batch", &json!({ "commands": batch })).unwrap();
            let after = numbers(&mut s);
            let pop_after = pop_at(&mut s, frame);
            s.run("edit.undo", &json!({})).unwrap();
            let own = match f["kind"].as_str().unwrap() {
                "sliding" => Some(0),
                "penetration" => Some(1),
                "balance" => Some(2),
                "capture" => Some(3),
                _ => None,
            };
            let lessened = match own {
                Some(k) => after[k] < before[k],
                None => pop_after < pop,
            };
            if !lessened {
                problems.push(format!(
                    "{id}: the fix of {f} didn't help: {before:?} to {after:?}"
                ));
            }
            for (k, (b, a)) in before.iter().zip(&after).enumerate() {
                if Some(k) != own && *a > *b + WORSE {
                    problems.push(format!("{id}: the fix of {f} made number {k} worse: {b} to {a}"));
                }
            }
        }
    }
    println!(
        "worst finding per clip: high {}, medium {}, low {}, none {}; {tried} fixes tried",
        tally[0], tally[1], tally[2], tally[3]
    );
    assert!(tried > 0, "no fix was offered on any clip, so none was tried");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
