//! Body parts: named bone sets used wherever a tool works on part of the body (keying, copy and
//! paste, locks, poses, the timeline rows). A part is built from the hierarchy (a bone and
//! everything below it), so it holds for any body shape of the rig.

use std::collections::{BTreeMap, BTreeSet};

use crate::rig::Skeleton;

pub const WHOLE_BODY: &str = "Whole body";
pub const HIPS: &str = "Hips";

/// The parts offered in the tools.
pub const PRESETS: &[&str] = &[
    WHOLE_BODY,
    "Upper body",
    "Lower body",
    "Left arm",
    "Right arm",
    "Left leg",
    "Right leg",
    "Head",
    "Hands",
    "Spine",
];

/// The timeline's part rows: every bone in exactly one row.
pub const ROWS: &[&str] = &[
    "Hips",
    "Spine",
    "Head",
    "Left arm",
    "Right arm",
    "Left leg",
    "Right leg",
];

/// name -> (bones whose subtrees are in, bones whose subtrees are left out)
const TREES: &[(&str, &[&str], &[&str])] = &[
    ("Upper body", &["spine05"], &[]),
    ("Lower body", &["pelvis.L", "pelvis.R"], &[]),
    ("Left arm", &["clavicle.L"], &[]),
    ("Right arm", &["clavicle.R"], &[]),
    ("Left leg", &["pelvis.L"], &[]),
    ("Right leg", &["pelvis.R"], &[]),
    ("Head", &["neck01"], &[]),
    ("Hands", &["wrist.L", "wrist.R"], &[]),
    ("Spine", &["spine05"], &["clavicle.L", "clavicle.R", "neck01"]),
];

/// The user's own part sets: name -> bone names.
pub type Custom = BTreeMap<String, Vec<String>>;

/// `top` and every bone below it (empty when the rig has no such bone).
pub fn subtree(sk: &Skeleton, top: &str) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    let Some(t) = sk.bone(top) else {
        return out;
    };
    out.insert(t);
    for (i, p) in sk.parents.iter().enumerate() {
        if p.is_some_and(|p| out.contains(&p)) {
            out.insert(i);
        }
    }
    out
}

/// Bone indices of a part (a preset, a row, "Hips" or a custom set), in skeleton order.
pub fn bones(sk: &Skeleton, custom: &Custom, part: &str) -> Result<Vec<usize>, String> {
    if part == WHOLE_BODY {
        return Ok((0..sk.len()).collect());
    }
    if part == HIPS {
        return Ok(vec![0]);
    }
    if let Some(names) = custom.get(part) {
        let set: BTreeSet<usize> = names.iter().filter_map(|n| sk.bone(n)).collect();
        return Ok(set.into_iter().collect());
    }
    let (_, keep, drop) = TREES
        .iter()
        .find(|(n, _, _)| *n == part)
        .ok_or_else(|| format!("unknown body part {part:?}"))?;
    let mut s = BTreeSet::new();
    for t in *keep {
        s.extend(subtree(sk, t));
    }
    for t in *drop {
        for b in subtree(sk, t) {
            s.remove(&b);
        }
    }
    if part == "Lower body" {
        s.insert(0);
    }
    Ok(s.into_iter().collect())
}

/// Bones of several parts; `None` for the whole body (no parts, or "Whole body" among them).
pub fn union(sk: &Skeleton, custom: &Custom, parts: &[String]) -> Result<Option<Vec<usize>>, String> {
    let parts: Vec<&String> = parts.iter().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|p| *p == WHOLE_BODY) {
        return Ok(None);
    }
    let mut s = BTreeSet::new();
    for p in parts {
        s.extend(bones(sk, custom, p)?);
    }
    Ok(if s.len() == sk.len() {
        None
    } else {
        Some(s.into_iter().collect())
    })
}

/// The timeline rows a bone set touches.
pub fn rows(sk: &Skeleton, only: Option<&[usize]>) -> Vec<&'static str> {
    let Some(only) = only else {
        return ROWS.to_vec();
    };
    let s: BTreeSet<usize> = only.iter().copied().collect();
    ROWS.iter()
        .copied()
        .filter(|r| bones(sk, &Custom::new(), r).is_ok_and(|b| b.iter().any(|i| s.contains(i))))
        .collect()
}

/// The name of a bone set: a preset when it is one, else the rows it touches.
pub fn label(sk: &Skeleton, custom: &Custom, only: Option<&[usize]>) -> String {
    let Some(only) = only else {
        return WHOLE_BODY.to_string();
    };
    let s: BTreeSet<usize> = only.iter().copied().collect();
    let names = PRESETS
        .iter()
        .skip(1)
        .map(|p| p.to_string())
        .chain(custom.keys().cloned());
    for p in names {
        if bones(sk, custom, &p).is_ok_and(|b| b.into_iter().collect::<BTreeSet<_>>() == s) {
            return p;
        }
    }
    rows(sk, Some(only)).join(" + ")
}

pub fn mirror_name(n: &str) -> String {
    if let Some(stem) = n.strip_suffix(".L") {
        format!("{stem}.R")
    } else if let Some(stem) = n.strip_suffix(".R") {
        format!("{stem}.L")
    } else {
        n.to_string()
    }
}
