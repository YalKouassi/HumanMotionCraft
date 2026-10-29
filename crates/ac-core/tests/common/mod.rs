#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::{Document, Session, Skeleton};
use serde_json::Value;

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn skeleton() -> Skeleton {
    let text = std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).expect("baked rig");
    Skeleton::from_json(&text).expect("rig parses")
}

/// A session over the baked rig with ac-core's own commands (the one setup every core test uses;
/// front ends and the CLI tests use `ac_io::setup::SessionBuilder`).
pub fn session() -> Session {
    Session::new(Document::new(skeleton()))
}

pub fn golden(name: &str) -> Vec<Value> {
    let path = repo().join("tests/fixtures/golden").join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let v: Value = serde_json::from_str(&text).expect("golden parses");
    assert!(ac_core::product::is_format(
        v["format"].as_str().unwrap_or_default(),
        "golden"
    ));
    v["cases"].as_array().expect("cases").clone()
}

pub fn f(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

pub fn arr<const N: usize>(v: &Value) -> [f64; N] {
    let a = v.as_array().expect("array");
    assert_eq!(a.len(), N);
    std::array::from_fn(|i| f(&a[i]))
}

pub fn list<const N: usize>(v: &Value) -> Vec<[f64; N]> {
    v.as_array().expect("array").iter().map(arr::<N>).collect()
}

/// Largest absolute difference, reported with where it happened.
pub fn assert_close(case: &str, what: &str, got: &[f64], want: &[f64], tol: f64) {
    assert_eq!(got.len(), want.len(), "{case}: {what} length");
    let (i, d) = got
        .iter()
        .zip(want)
        .map(|(a, b)| (a - b).abs())
        .enumerate()
        .fold((0, 0.0f64), |m, (i, d)| if d > m.1 { (i, d) } else { m });
    assert!(
        d <= tol,
        "{case}: {what}[{i}] differs by {d:e} (tolerance {tol:e}): got {} want {}",
        got[i],
        want[i]
    );
}
