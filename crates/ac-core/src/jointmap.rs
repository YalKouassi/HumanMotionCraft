//! Which joint of another skeleton stands for each of the retarget's landmarks, guessed from the
//! joint names and the hierarchy. Shared by the FBX import (`ac-io`) and, later, the export onto
//! a character (roadmap 146000), so it lives on its own.
//!
//! A name is cut into words (`LeftUpLeg` -> left, up, leg; `mixamorig:Spine1` -> spine, 1;
//! `upperleg01.L` -> upperleg, 01, l), its side read from the `left`/`right`/`l`/`r` words, and
//! the rest joined into a key (`upleg`) that is matched against each landmark's known names.
//! Joints with an end, twist, roll, helper or control word are never landmarks. Where names
//! leave a doubt the hierarchy decides: the hips are the joint the two thighs meet under, the
//! spine landmark is the hips' child on the way to the neck, and `Leg` is the knee when a thigh
//! joint exists above it, the thigh when a shin exists below it. Every guess can be corrected by
//! the caller (`clip.import`'s `joints`).

use std::collections::BTreeMap;

use crate::retarget::{LANDMARKS, SPINE};

/// Words that mark a joint as not a landmark (an end site, a twist bone, a control).
const NOT_A_LANDMARK: [&str; 18] = [
    "end", "nub", "tip", "twist", "roll", "helper", "ik", "fk", "ctrl", "control", "prop", "offset",
    "target", "pole", "eff", "effector", "site", "attach",
];
const LEFT: [&str; 3] = ["left", "l", "lft"];
const RIGHT: [&str; 3] = ["right", "r", "rt"];
/// Keys that, with a leading `l`/`r` and no other side word, are one-word sided names (`lfemur`).
const BODY_WORDS: [&str; 22] = [
    "femur", "tibia", "foot", "toes", "toe", "humerus", "radius", "ulna", "hand", "wrist", "clavicle",
    "shoulder", "arm", "forearm", "upleg", "leg", "thigh", "shin", "calf", "knee", "ankle", "elbow",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    None,
}

/// A joint name taken apart.
#[derive(Debug, Clone)]
struct Parsed {
    index: usize,
    side: Side,
    /// The body words joined, lower case, digits and side words dropped.
    key: String,
    excluded: bool,
}

/// The words of a joint name: split at non-alphanumerics, at lower-to-upper case changes and at
/// letter-digit changes; the namespace (`ns:name`) dropped; lower case.
fn words(name: &str) -> Vec<String> {
    let name = name.rsplit(':').next().unwrap_or(name);
    let chars: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev = None;
            continue;
        }
        if let Some(p) = prev {
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            // fooBar, LHip (an initial before a capitalised word), foo1
            let case_change = c.is_uppercase() && (p.is_lowercase() || (p.is_uppercase() && next_lower));
            let kind_change = p.is_ascii_digit() != c.is_ascii_digit();
            if (case_change || kind_change) && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
        prev = Some(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse(index: usize, name: &str) -> Parsed {
    let ws = words(name);
    let mut side = Side::None;
    let mut sides = 0;
    let mut body: Vec<&str> = Vec::new();
    for w in &ws {
        let w = w.as_str();
        if LEFT.contains(&w) {
            side = Side::Left;
            sides += 1;
        } else if RIGHT.contains(&w) {
            side = Side::Right;
            sides += 1;
        } else if !w.chars().all(|c| c.is_ascii_digit()) {
            body.push(w);
        }
    }
    if sides > 1 {
        side = Side::None;
    }
    let excluded = body.iter().any(|w| NOT_A_LANDMARK.contains(w));
    let mut key: String = body.concat();
    // one-word sided names: lfemur, rhand
    if side == Side::None
        && key.len() > 1
        && let Some(rest) = key.strip_prefix('l').or_else(|| key.strip_prefix('r'))
        && BODY_WORDS.contains(&rest)
    {
        side = if key.starts_with('l') {
            Side::Left
        } else {
            Side::Right
        };
        key = rest.to_string();
    }
    Parsed {
        index,
        side,
        key,
        excluded,
    }
}

/// The guess: the joint index of each landmark found, the landmarks not found, and notes on
/// the doubtful choices (Rule 1: a guess that could be wrong is said).
#[derive(Debug, Clone, Default)]
pub struct Guess {
    pub map: BTreeMap<&'static str, usize>,
    pub missing: Vec<&'static str>,
    pub notes: Vec<String>,
}

fn depth(parents: &[Option<usize>], mut i: usize) -> usize {
    let mut d = 0;
    while let Some(Some(p)) = parents.get(i) {
        d += 1;
        i = *p;
        if d > parents.len() {
            break;
        }
    }
    d
}

/// Whether `node` is `ancestor` or below it.
pub fn under(parents: &[Option<usize>], mut node: usize, ancestor: usize) -> bool {
    let mut steps = 0;
    loop {
        if node == ancestor {
            return true;
        }
        match parents.get(node) {
            Some(Some(p)) if steps <= parents.len() => {
                node = *p;
                steps += 1;
            }
            _ => return false,
        }
    }
}

/// The deepest joint both are under.
fn common_ancestor(parents: &[Option<usize>], a: usize, b: usize) -> Option<usize> {
    let mut node = a;
    let mut steps = 0;
    loop {
        if under(parents, b, node) {
            return Some(node);
        }
        match parents.get(node) {
            Some(Some(p)) if steps <= parents.len() => {
                node = *p;
                steps += 1;
            }
            _ => return None,
        }
    }
}

/// The joints whose key is one of `keys` (first key first), on `side`, not excluded; shallowest
/// first, then by index.
fn find(joints: &[Parsed], parents: &[Option<usize>], side: Side, keys: &[&str]) -> Option<usize> {
    for key in keys {
        let mut found: Vec<&Parsed> = joints
            .iter()
            .filter(|j| !j.excluded && j.side == side && j.key == *key)
            .collect();
        found.sort_by_key(|j| (depth(parents, j.index), j.index));
        if let Some(j) = found.first() {
            return Some(j.index);
        }
    }
    None
}

/// Guess the landmark of each joint from `names` (one per joint) and `parents`.
pub fn guess(names: &[String], parents: &[Option<usize>]) -> Guess {
    let joints: Vec<Parsed> = names.iter().enumerate().map(|(i, n)| parse(i, n)).collect();
    let mut out = Guess::default();
    let name = |i: usize| names.get(i).cloned().unwrap_or_default();
    let put = |lm: &'static str, i: Option<usize>, out: &mut Guess| {
        if let Some(i) = i {
            out.map.insert(lm, i);
        }
    };
    for (side, c) in [(Side::Left, "l"), (Side::Right, "r")] {
        let lm = |k: &str| -> &'static str {
            match (c, k) {
                ("l", "shoulder") => "l_shoulder",
                ("l", "elbow") => "l_elbow",
                ("l", "wrist") => "l_wrist",
                ("l", "hip") => "l_hip",
                ("l", "knee") => "l_knee",
                ("l", "ankle") => "l_ankle",
                ("l", "toe") => "l_toe",
                ("r", "shoulder") => "r_shoulder",
                ("r", "elbow") => "r_elbow",
                ("r", "wrist") => "r_wrist",
                ("r", "hip") => "r_hip",
                ("r", "knee") => "r_knee",
                ("r", "ankle") => "r_ankle",
                _ => "r_toe",
            }
        };
        // the upper arm: an arm joint; else a shoulder joint stands for it (a file with no arm joint
        // names the upper arm "shoulder")
        let arm = find(
            &joints,
            parents,
            side,
            &["arm", "upperarm", "uparm", "armupper", "humerus", "upperarmbone"],
        );
        let shoulder = arm.or_else(|| {
            let s = find(&joints, parents, side, &["shoulder", "clavicle", "collar"]);
            if let Some(s) = s {
                out.notes.push(format!(
                    "{} is taken as the {} upper arm: the file has no arm joint under it",
                    name(s),
                    if c == "l" { "left" } else { "right" }
                ));
            }
            s
        });
        put(lm("shoulder"), shoulder, &mut out);
        put(
            lm("elbow"),
            find(
                &joints,
                parents,
                side,
                &[
                    "forearm",
                    "lowerarm",
                    "lowarm",
                    "armlower",
                    "elbow",
                    "radius",
                    "ulna",
                    "forearmbone",
                ],
            ),
            &mut out,
        );
        put(
            lm("wrist"),
            find(&joints, parents, side, &["hand", "wrist", "palm"]),
            &mut out,
        );
        let thigh = find(
            &joints,
            parents,
            side,
            &[
                "upleg",
                "upperleg",
                "thigh",
                "femur",
                "legupper",
                "hip",
                "upperlegbone",
            ],
        );
        let shin = find(
            &joints,
            parents,
            side,
            &[
                "lowerleg",
                "shin",
                "calf",
                "knee",
                "tibia",
                "lowleg",
                "leglower",
                "lowerlegbone",
            ],
        );
        let leg = find(&joints, parents, side, &["leg"]);
        // "Leg" is the knee under a thigh joint, the thigh above a shin joint
        let (hip, knee) = match (thigh, shin, leg) {
            (Some(t), Some(s), _) => (Some(t), Some(s)),
            (Some(t), None, l) => (Some(t), l),
            (None, Some(s), l) => (l, Some(s)),
            (None, None, l) => (l, None),
        };
        put(lm("hip"), hip, &mut out);
        put(lm("knee"), knee, &mut out);
        put(
            lm("ankle"),
            find(&joints, parents, side, &["foot", "ankle"]),
            &mut out,
        );
        put(
            lm("toe"),
            find(
                &joints,
                parents,
                side,
                &["toebase", "toe", "toes", "ball", "foottoe", "toebone"],
            ),
            &mut out,
        );
    }
    // the hips: by name, else where the two thighs meet
    let hips = find(&joints, parents, Side::None, &["hips", "pelvis", "hip"]).or_else(|| {
        let lca = common_ancestor(parents, *out.map.get("l_hip")?, *out.map.get("r_hip")?)?;
        out.notes.push(format!(
            "{} is taken as the hips: the two thighs meet under it",
            name(lca)
        ));
        Some(lca)
    });
    put("hips", hips, &mut out);
    put("neck", find(&joints, parents, Side::None, &["neck"]), &mut out);
    put("head", find(&joints, parents, Side::None, &["head"]), &mut out);
    // the spine landmark: the first spine joint above the hips, else the hips' child towards the neck
    let spine = find(
        &joints,
        parents,
        Side::None,
        &["spine", "chest", "lowerback", "abdomen", "torso", "waist"],
    )
    .filter(|&s| hips.is_none_or(|h| s != h))
    .or_else(|| {
        let (h, n) = (hips?, *out.map.get("neck")?);
        let mut node = n;
        let mut steps = 0;
        while let Some(Some(p)) = parents.get(node) {
            if *p == h {
                return Some(node);
            }
            node = *p;
            steps += 1;
            if steps > parents.len() {
                break;
            }
        }
        None
    });
    put(SPINE, spine, &mut out);
    for lm in LANDMARKS {
        if !out.map.contains_key(lm) {
            out.missing.push(lm);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;

    fn tree(spec: &[(&str, Option<usize>)]) -> (Vec<String>, Vec<Option<usize>>) {
        (
            spec.iter().map(|(n, _)| n.to_string()).collect(),
            spec.iter().map(|(_, p)| *p).collect(),
        )
    }

    #[test]
    fn words_split_names_as_people_read_them() {
        assert_eq!(words("LeftUpLeg"), ["left", "up", "leg"]);
        assert_eq!(words("mixamorig:Spine1"), ["spine", "1"]);
        assert_eq!(words("upperleg01.L"), ["upperleg", "01", "l"]);
        assert_eq!(words("LHipJoint"), ["l", "hip", "joint"]);
        assert_eq!(words("HeadTop_End"), ["head", "top", "end"]);
    }

    #[test]
    fn the_usual_names_are_found_and_the_doubtful_ones_decided_by_the_tree() {
        let (names, parents) = tree(&[
            ("Reference", None),
            ("Hips", Some(0)),
            ("LHipJoint", Some(1)),
            ("LeftUpLeg", Some(2)),
            ("LeftLeg", Some(3)),
            ("LeftFoot", Some(4)),
            ("LeftToeBase", Some(5)),
            ("LeftToe_End", Some(6)),
            ("RightUpLeg", Some(1)),
            ("RightLeg", Some(8)),
            ("RightFoot", Some(9)),
            ("RightToeBase", Some(10)),
            ("Spine", Some(1)),
            ("Spine1", Some(12)),
            ("Neck", Some(13)),
            ("Neck1", Some(14)),
            ("Head", Some(15)),
            ("HeadTop_End", Some(16)),
            ("LeftShoulder", Some(13)),
            ("LeftArm", Some(18)),
            ("LeftArmRoll", Some(19)),
            ("LeftForeArm", Some(20)),
            ("LeftHand", Some(21)),
            ("LeftHandThumb1", Some(22)),
            ("RightShoulder", Some(13)),
            ("RightArm", Some(24)),
            ("RightForeArm", Some(25)),
            ("RightHand", Some(26)),
        ]);
        let g = guess(&names, &parents);
        assert!(g.missing.is_empty(), "{:?}", g.missing);
        let at = |lm: &str| names[g.map[lm]].clone();
        assert_eq!(at("hips"), "Hips");
        assert_eq!(at("spine"), "Spine");
        assert_eq!(at("neck"), "Neck");
        assert_eq!(at("head"), "Head");
        assert_eq!(at("l_shoulder"), "LeftArm");
        assert_eq!(at("l_elbow"), "LeftForeArm");
        assert_eq!(at("l_wrist"), "LeftHand");
        assert_eq!(at("l_hip"), "LeftUpLeg");
        assert_eq!(at("l_knee"), "LeftLeg");
        assert_eq!(at("l_ankle"), "LeftFoot");
        assert_eq!(at("l_toe"), "LeftToeBase");
        assert_eq!(at("r_toe"), "RightToeBase");
        assert!(g.notes.is_empty(), "{:?}", g.notes);
    }

    #[test]
    fn a_shin_makes_leg_the_thigh_and_shoulder_stands_for_a_missing_arm() {
        let (names, parents) = tree(&[
            ("root", None),
            ("LeftLeg", Some(0)),
            ("LeftShin", Some(1)),
            ("LeftFoot", Some(2)),
            ("LeftToe", Some(3)),
            ("RightLeg", Some(0)),
            ("RightShin", Some(5)),
            ("RightFoot", Some(6)),
            ("RightToe", Some(7)),
            ("Chest", Some(0)),
            ("Neck", Some(9)),
            ("Head", Some(10)),
            ("LeftShoulder", Some(9)),
            ("LeftElbow", Some(12)),
            ("LeftWrist", Some(13)),
            ("RightShoulder", Some(9)),
            ("RightElbow", Some(15)),
            ("RightWrist", Some(16)),
        ]);
        let g = guess(&names, &parents);
        assert!(g.missing.is_empty(), "{:?}", g.missing);
        assert_eq!(names[g.map["l_hip"]], "LeftLeg");
        assert_eq!(names[g.map["l_knee"]], "LeftShin");
        assert_eq!(names[g.map["l_shoulder"]], "LeftShoulder");
        assert_eq!(names[g.map["hips"]], "root", "the thighs meet under root");
        assert_eq!(names[g.map["spine"]], "Chest");
        assert_eq!(g.notes.len(), 3, "{:?}", g.notes);
    }

    #[test]
    fn our_own_rig_names_and_one_word_sided_names() {
        let (names, parents) = tree(&[
            ("root", None),
            ("pelvis.L", Some(0)),
            ("upperleg01.L", Some(1)),
            ("lowerleg01.L", Some(2)),
            ("foot.L", Some(3)),
            ("toe3-1.L", Some(4)),
            ("rfemur", Some(0)),
            ("rtibia", Some(6)),
            ("rfoot", Some(7)),
            ("rtoes", Some(8)),
        ]);
        let g = guess(&names, &parents);
        assert_eq!(names[g.map["l_hip"]], "upperleg01.L");
        assert_eq!(names[g.map["l_knee"]], "lowerleg01.L");
        assert_eq!(names[g.map["l_toe"]], "toe3-1.L");
        assert_eq!(names[g.map["r_hip"]], "rfemur");
        assert_eq!(names[g.map["r_knee"]], "rtibia");
        assert_eq!(names[g.map["r_toe"]], "rtoes");
        assert_eq!(names[g.map["hips"]], "root");
        assert!(g.missing.contains(&"neck") && g.missing.contains(&"l_wrist"));
    }
}
