//! A humanoid FBX animation -> a clip on the rig: the file's joints are read as world positions
//! ([`super::scene`]), the retarget's landmarks are found among them by name and hierarchy
//! (`ac_core::jointmap`, correctable by the caller), the source is scaled to the rig's legs,
//! retargeted (`ac_core::retarget`) and its floor moved to z = 0. The result says what it did:
//! which joint stood for each landmark, which joints had no landmark, which rig bones stayed at
//! rest (fingers, face, props), the floor offset and how far the hips landed from the source's.
//!
//! A Unity `.meta` beside the file goes first ([`super::unity_meta`]): its humanoid bone map says
//! which joint is which, and the name guess is used only without one, or for a landmark the map
//! leaves out (said in the notes). Its clip ranges cut the take into named clips, each retargeted
//! and levelled on its own, like a file of its own. Open with [`open_file`], then make a clip.
//!
//! Three things real files need are read with a note, and only where one answer is left: a clip
//! naming a take that isn't there, when the file has one take; a `.meta` bone that matches one joint
//! once spaces and case are ignored; a missing neck, from the head's parent if no landmark has it.

use std::collections::BTreeMap;
use std::path::Path;

use ac_core::clip::{Clip, Interp, Key};
use ac_core::jointmap;
use ac_core::quat::{self, Vec3};
use ac_core::retarget::{self, LANDMARKS, Landmarks, SPINE};
use ac_core::rig::Skeleton;
use serde_json::{Map, Value, json};

use super::scene::{self, Motion};
use super::unity_meta::{self, MetaClip, UnityMeta};

/// Largest FBX file read.
pub const MAX_FILE_BYTES: u64 = 512 << 20;
/// Hips that move this far in one frame at a take's start are a setup frame, not motion.
pub const JUMP_M: f64 = 0.25;
/// Most setup frames dropped at the start.
const MAX_JUNK: usize = 5;
/// The share of frames whose toes and knees say the sides are swapped that refuses the import.
const SWAPPED_REFUSE: f64 = 0.9;
/// The share from which the result warns.
const SWAPPED_WARN: f64 = 0.5;

/// What an import made.
#[derive(Debug, Clone)]
pub struct Imported {
    pub clip: Clip,
    /// The command's result besides `length`: fps, frames, joints, unused, at_rest, floor, hips, notes, source.
    pub report: Map<String, Value>,
    /// The share of the checks whose toes and knees say the sides are swapped (0 to 1).
    pub swapped: f64,
}

type R<T> = Result<T, String>;

static NO_BONES: BTreeMap<&'static str, String> = BTreeMap::new();

/// An opened FBX: its sampled motion, which joint stands for each landmark, and the named clips
/// a `.meta` cuts it into (none: the take is one clip). Its clips may name other takes of the file.
#[derive(Debug, Clone)]
pub struct Source {
    /// The take read for what names no other: the first, or the first a clip names.
    motion: Motion,
    /// The other takes the clips name; one that can't be read refuses only its own clips.
    takes: BTreeMap<String, R<Motion>>,
    /// Every take in the file, to say what a stale name could have been.
    take_names: Vec<String>,
    map: BTreeMap<&'static str, usize>,
    given: Vec<String>,
    /// `unity_meta` or `names`: what said which joint is which.
    bone_map: &'static str,
    notes: Vec<String>,
    name: String,
    clips: Vec<MetaClip>,
}

/// A name with its spaces gone and in lower case.
fn loose(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The joints a `.meta` bone's name matches once spaces and case are ignored.
fn loose_matches(names: &[String], bone: &str) -> Vec<usize> {
    let wanted = loose(bone);
    if wanted.is_empty() {
        return Vec::new();
    }
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| loose(n) == wanted)
        .map(|(i, _)| i)
        .collect()
}

/// The joint to read as the neck when only the neck is missing: the head's parent, unless a landmark
/// already has it (the chest, say), since one joint can't be both.
fn neck_from_head(map: &BTreeMap<&'static str, usize>, parents: &[Option<usize>]) -> Option<usize> {
    let head = *map.get("head")?;
    let parent = (*parents.get(head)?)?;
    (!map.values().any(|&i| i == parent)).then_some(parent)
}

/// Open an `.fbx` file and the `.meta` beside it, if any. `joints` corrects the landmark -> joint
/// map.
pub fn open_file(path: &Path, joints: &BTreeMap<String, String>) -> R<Source> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("can't read {}: {e}", path.display()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{} is {size} bytes, larger than {MAX_FILE_BYTES}",
            path.display()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "imported".into());
    let meta = unity_meta::beside(path)?;
    open_bytes(&bytes, &name, joints, meta.as_ref()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read an `.fbx` file into one clip for `skel`: the clip named `clip`, else the first the `.meta`
/// names, else the whole take.
pub fn import_file(
    path: &Path,
    skel: &Skeleton,
    joints: &BTreeMap<String, String>,
    clip: Option<&str>,
) -> R<Imported> {
    open_file(path, joints)?
        .pick(skel, clip)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// How many frames at the start are setup, not motion: frames the hips leave by a jump.
pub fn leading_junk(hips: &[Vec3]) -> usize {
    let most = MAX_JUNK.min(hips.len().saturating_sub(3));
    let mut n = 0;
    while n < most {
        let (Some(a), Some(b)) = (hips.get(n), hips.get(n + 1)) else {
            break;
        };
        if quat::len3(quat::sub3(*b, *a)) <= JUMP_M {
            break;
        }
        n += 1;
    }
    n
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// How many frames say the left and right joints are swapped: the toes (and bent knees) point
/// the other way from the forward the hip line and the trunk give for a right-handed body.
fn swapped_frames(src: &Landmarks) -> (usize, usize) {
    let f = src.frames();
    let at = |name: &str, i: usize| src.get(name).get(i).copied().unwrap_or_default();
    let mut votes = 0;
    let mut swapped = 0;
    for i in 0..f {
        let up = quat::sub3(at("neck", i), at("hips", i));
        let left = quat::sub3(at("l_hip", i), at("r_hip", i));
        let forward = quat::cross(left, up);
        if quat::len3(forward) < 1e-9 {
            continue;
        }
        let toes = quat::add3(
            quat::sub3(at("l_toe", i), at("l_ankle", i)),
            quat::sub3(at("r_toe", i), at("r_ankle", i)),
        );
        if quat::len3(toes) > 1e-6 {
            votes += 1;
            if dot(toes, forward) < 0.0 {
                swapped += 1;
            }
        }
        for (hip, knee, ankle) in [("l_hip", "l_knee", "l_ankle"), ("r_hip", "r_knee", "r_ankle")] {
            let (h, k, a) = (at(hip, i), at(knee, i), at(ankle, i));
            let thigh = quat::sub3(k, h);
            let shin = quat::sub3(a, k);
            let bend = quat::len3(quat::cross(thigh, shin))
                .atan2(dot(thigh, shin))
                .to_degrees();
            if bend > 20.0 {
                let mid = quat::scale3(quat::add3(h, a), 0.5);
                votes += 1;
                if dot(quat::sub3(k, mid), forward) < 0.0 {
                    swapped += 1;
                }
            }
        }
    }
    (swapped, votes)
}

fn joint_list(motion: &Motion) -> String {
    let mut names: Vec<&str> = motion.names.iter().map(String::as_str).collect();
    let more = names.len().saturating_sub(60);
    names.truncate(60);
    format!(
        "{}{}",
        names.join(", "),
        if more > 0 {
            format!(", ... ({more} more)")
        } else {
            String::new()
        }
    )
}

/// The rig's bones left at rest on every frame, grouped by their family (`finger`, `toe`, ...):
/// "finger: 30 of 30 at rest".
fn at_rest(skel: &Skeleton, rots: &[Vec<quat::Quat>]) -> Vec<String> {
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (b, name) in skel.names.iter().enumerate() {
        let family: String = name.chars().take_while(|c| c.is_alphabetic()).collect();
        let still = rots
            .iter()
            .all(|row| row.get(b).is_none_or(|q| (q[3].abs() - 1.0).abs() < 1e-9));
        let e = groups.entry(family).or_insert((0, 0));
        e.1 += 1;
        if still {
            e.0 += 1;
        }
    }
    groups
        .into_iter()
        .filter(|(_, (rest, _))| *rest > 0)
        .map(|(family, (rest, total))| format!("{family}: {rest} of {total} at rest"))
        .collect()
}

/// Import FBX bytes as one clip, the whole take, with no `.meta`.
pub fn import_bytes(
    bytes: &[u8],
    skel: &Skeleton,
    name: &str,
    joints: &BTreeMap<String, String>,
) -> R<Imported> {
    open_bytes(bytes, name, joints, None)?.whole(skel)
}

/// Open FBX bytes with what a `.meta` says. A correction in `joints` outranks the `.meta`, and the
/// `.meta` outranks the guess from names.
pub fn open_bytes(
    bytes: &[u8],
    name: &str,
    joints: &BTreeMap<String, String>,
    meta: Option<&UnityMeta>,
) -> R<Source> {
    let doc = super::read(bytes)?;
    let take_names = scene::take_names(&doc);
    let (motion, takes) = open_takes(&doc, &take_names, meta)?;
    let mut notes = Vec::new();
    let mut guess = jointmap::guess(&motion.names, &motion.parents);
    let stated = meta.map_or(&NO_BONES, |m| &m.bones);
    let bone_map = if stated.is_empty() { "names" } else { "unity_meta" };
    if stated.is_empty() {
        if meta.is_some() {
            notes.push(
                "the .meta beside the file has no humanoid bone map: the joints were guessed from their names"
                    .into(),
            );
        }
        notes.extend(guess.notes.iter().cloned());
    } else {
        let left_out: Vec<String> = guess
            .map
            .iter()
            .filter(|(lm, _)| !stated.contains_key(**lm) && !joints.contains_key(**lm))
            .map(|(lm, &i)| format!("{lm} ({})", motion.names.get(i).map_or("", String::as_str)))
            .collect();
        if !left_out.is_empty() {
            notes.push(format!(
                "the .meta's bone map leaves out {}: guessed from the joints' names",
                left_out.join(", ")
            ));
        }
        let mut read: BTreeMap<usize, &str> = BTreeMap::new();
        for (&lm, bone) in stated {
            if joints.contains_key(lm) {
                continue;
            }
            let index = match motion.names.iter().position(|n| n == bone) {
                Some(i) => i,
                None => {
                    let found = loose_matches(&motion.names, bone);
                    let names: Vec<&str> = found
                        .iter()
                        .filter_map(|&i| motion.names.get(i).map(String::as_str))
                        .collect();
                    match (found.as_slice(), names.as_slice()) {
                        ([i], [name]) => {
                            notes.push(format!(
                                "the .meta says {bone:?} is {lm}: read as the joint {name:?}, ignoring spaces and case"
                            ));
                            *i
                        }
                        ([], _) => {
                            return Err(format!(
                                "the .meta says {bone:?} is {lm}, but the file has no such joint (is the .meta this file's?); correct it with joints; the file's joints: {}",
                                joint_list(&motion)
                            ));
                        }
                        _ => {
                            return Err(format!(
                                "the .meta says {bone:?} is {lm}, and ignoring spaces and case that matches several joints: {}; correct it with joints",
                                names.join(", ")
                            ));
                        }
                    }
                }
            };
            if let Some(other) = read.insert(index, lm) {
                return Err(format!(
                    "the .meta's {other} and {lm} both come to the joint {:?} once spaces and case are ignored; correct it with joints",
                    motion.names.get(index).map_or("", String::as_str)
                ));
            }
            if motion.names.iter().filter(|n| *n == bone).count() > 1 {
                notes.push(format!(
                    "{bone:?} ({lm}) names several joints: the first was read"
                ));
            }
            guess.map.insert(lm, index);
            guess.missing.retain(|m| *m != lm);
        }
    }
    let mut given: Vec<String> = Vec::new();
    for (lm, joint) in joints {
        let lm_static = LANDMARKS
            .iter()
            .copied()
            .chain([SPINE])
            .find(|l| l == lm)
            .ok_or_else(|| {
                format!(
                    "joints: {lm:?} is not a landmark; the landmarks are {}, {SPINE}",
                    LANDMARKS.join(", ")
                )
            })?;
        let index = motion.names.iter().position(|n| n == joint).ok_or_else(|| {
            format!(
                "joints: no joint {joint:?} in the file; its joints: {}",
                joint_list(&motion)
            )
        })?;
        guess.map.insert(lm_static, index);
        guess.missing.retain(|m| *m != lm_static);
        given.push(lm.clone());
    }
    if guess.missing == ["neck"]
        && let Some(parent) = neck_from_head(&guess.map, &motion.parents)
    {
        guess.map.insert("neck", parent);
        guess.missing.clear();
        notes.push(format!(
            "no joint found for neck: the head's parent, {}, is read as the neck",
            motion.names.get(parent).map_or("", String::as_str)
        ));
    }
    if !guess.missing.is_empty() {
        let hint: Vec<String> = guess
            .missing
            .iter()
            .map(|m| format!("\"{m}\": \"<joint>\""))
            .collect();
        return Err(format!(
            "not a humanoid skeleton this import knows: no joint found for {}; name them with joints: {{{}}}; the file's joints: {}",
            guess.missing.join(", "),
            hint.join(", "),
            joint_list(&motion)
        ));
    }
    Ok(Source {
        map: guess.map,
        given,
        bone_map,
        notes,
        name: name.to_string(),
        clips: meta.map(|m| m.clips.clone()).unwrap_or_default(),
        motion,
        takes,
        take_names,
    })
}

/// Sample the takes the `.meta`'s clips name, and the first too if a clip names none, or none it
/// knows. All of them together stay under the cap of one take; past it, a take refuses its clips.
fn open_takes(
    doc: &super::binary::Document,
    take_names: &[String],
    meta: Option<&UnityMeta>,
) -> R<(Motion, BTreeMap<String, R<Motion>>)> {
    let clips = meta.map_or(&[][..], |m| m.clips.as_slice());
    let needs_first = clips.is_empty()
        || clips
            .iter()
            .any(|c| c.take.is_empty() || !take_names.contains(&c.take));
    let mut named: Vec<&str> = Vec::new();
    for c in clips {
        if take_names.contains(&c.take) && !named.contains(&c.take.as_str()) {
            named.push(&c.take);
        }
    }
    let mut base = if needs_first {
        Some(scene::motion(doc)?)
    } else {
        None
    };
    let mut takes: BTreeMap<String, R<Motion>> = BTreeMap::new();
    let mut samples = base
        .as_ref()
        .map_or(0, |m| m.frames.saturating_mul(m.names.len()));
    for name in named {
        if base.as_ref().is_some_and(|m| m.take == name) {
            continue;
        }
        let read = if samples > scene::MAX_SAMPLES {
            Err(format!(
                "the takes read so far hold more than {} joint positions",
                scene::MAX_SAMPLES
            ))
        } else {
            scene::motion_of(doc, Some(name))
        };
        match read {
            Ok(m) => {
                samples = samples.saturating_add(m.frames.saturating_mul(m.names.len()));
                if base.is_none() {
                    base = Some(m);
                } else {
                    takes.insert(name.to_string(), Ok(m));
                }
            }
            Err(e) => {
                takes.insert(name.to_string(), Err(e));
            }
        }
    }
    match base {
        Some(m) => Ok((m, takes)),
        None => Err(takes
            .into_iter()
            .find_map(|(name, read)| read.err().map(|e| format!("take {name:?}: {e}")))
            .unwrap_or_else(|| "the file has no take to read".into())),
    }
}

/// Where a named clip lies among the samples: its first sample and how many, and a note if its
/// range had to be rounded to whole frames or cut to the take. A range with no frame in the take, or
/// only one, is refused.
fn clip_range(motion: &Motion, clip: &MetaClip) -> R<(usize, usize, Option<String>)> {
    let (first, last) = (clip.first.round(), clip.last.round());
    let start = motion.start_frame.round();
    let end = start + motion.frames as f64 - 1.0;
    if last - first < 1.0 {
        return Err(format!(
            "its range {} to {} holds fewer than 2 frames",
            clip.first, clip.last
        ));
    }
    if last < start || first > end {
        return Err(format!(
            "its range {} to {} is outside the take's frames {start} to {end}",
            clip.first, clip.last
        ));
    }
    let (from, to) = (first.max(start), last.min(end));
    if to - from < 1.0 {
        return Err(format!(
            "its range {} to {} holds fewer than 2 of the take's frames {start} to {end}",
            clip.first, clip.last
        ));
    }
    let mut notes = Vec::new();
    if first != clip.first || last != clip.last {
        notes.push(format!(
            "its range {} to {} lies between frames: read as {first} to {last}",
            clip.first, clip.last
        ));
    }
    let mut cut = Vec::new();
    if first < from {
        cut.push(format!("{first} to {}", from - 1.0));
    }
    if last > to {
        cut.push(format!("{} to {last}", to + 1.0));
    }
    if !cut.is_empty() {
        notes.push(format!(
            "its range {first} to {last} runs past the take's frames {start} to {end}: frames {} cut",
            cut.join(" and ")
        ));
    }
    let note = (!notes.is_empty()).then(|| notes.join("; "));
    Ok(((from - start) as usize, (to - from) as usize + 1, note))
}

impl Source {
    /// The named clips; empty: the take is one clip.
    pub fn clips(&self) -> &[MetaClip] {
        &self.clips
    }

    /// The take a clip names, when the file doesn't have it and has one take to read instead.
    fn stale_take<'a>(&self, clip: &'a MetaClip) -> Option<&'a str> {
        (self.take_names.len() == 1 && !clip.take.is_empty() && !self.take_names.contains(&clip.take))
            .then_some(clip.take.as_str())
    }

    /// The take a named clip cuts: its `takeName` picks it among the file's takes. A name the file
    /// doesn't have can only mean its one take; with several it could be any, so it is refused.
    fn motion_for(&self, clip: &MetaClip) -> R<&Motion> {
        if clip.take.is_empty() || clip.take == self.motion.take || self.stale_take(clip).is_some() {
            return Ok(&self.motion);
        }
        match self.takes.get(&clip.take) {
            Some(Ok(m)) => Ok(m),
            Some(Err(e)) => Err(format!("take {:?}: {e}", clip.take)),
            None => Err(format!(
                "it cuts the take {:?}, but the file's takes are {:?}",
                clip.take, self.take_names
            )),
        }
    }

    /// The whole take as one clip, setup frames dropped.
    pub fn whole(&self, skel: &Skeleton) -> R<Imported> {
        self.build(skel, None)
    }

    /// One named clip. Its range is the `.meta`'s, so no frame of it is dropped as setup.
    pub fn clip(&self, skel: &Skeleton, clip: &MetaClip) -> R<Imported> {
        self.build(skel, Some(clip))
            .map_err(|e| format!("clip {:?}: {e}", clip.name))
    }

    /// The clip named `wanted`, else the first named one (all are listed), else the whole take.
    pub fn pick(&self, skel: &Skeleton, wanted: Option<&str>) -> R<Imported> {
        let Some(first) = self.clips.first() else {
            return match wanted {
                Some(w) => Err(format!(
                    "clip {w:?}: the file has no named clips (no clip ranges in a .meta beside it)"
                )),
                None => self.whole(skel),
            };
        };
        let chosen = match wanted {
            Some(w) => self.clips.iter().find(|c| c.name == w).ok_or_else(|| {
                let names: Vec<&str> = self.clips.iter().map(|c| c.name.as_str()).collect();
                format!("no clip {w:?}; the clips are {}", names.join(", "))
            })?,
            None => first,
        };
        let mut out = self.clip(skel, chosen)?;
        if wanted.is_none()
            && self.clips.len() > 1
            && let Some(Value::Array(notes)) = out.report.get_mut("notes")
        {
            notes.push(json!(format!(
                "the .meta cuts this file into {} clips: {:?} was opened; name another with clip",
                self.clips.len(),
                chosen.name
            )));
        }
        let list: Vec<Value> = self
            .clips
            .iter()
            .map(|c| json!({ "name": c.name, "first": c.first, "last": c.last, "loop": c.looping }))
            .collect();
        out.report.insert("clips".into(), Value::Array(list));
        Ok(out)
    }

    fn build(&self, skel: &Skeleton, part: Option<&MetaClip>) -> R<Imported> {
        let motion = match part {
            Some(clip) => self.motion_for(clip)?,
            None => &self.motion,
        };
        let mut notes = motion.notes.clone();
        notes.extend(self.notes.iter().cloned());
        let (first, count) = match part {
            Some(clip) => {
                let (first, count, note) = clip_range(motion, clip)?;
                notes.extend(note);
                if let Some(stale) = self.stale_take(clip) {
                    notes.push(format!(
                        "the .meta's clip names the take {stale:?}, which the file doesn't have: its frames are read from the file's one take, {:?}",
                        motion.take
                    ));
                }
                (first, count)
            }
            None => (0, motion.frames),
        };
        let name = part.map_or(self.name.as_str(), |c| c.name.as_str());

        // the landmarks, setup frames dropped
        let hips_index = self.map.get("hips").copied().unwrap_or(0);
        let hips: Vec<Vec3> = motion
            .positions
            .iter()
            .skip(first)
            .take(count)
            .map(|row| row.get(hips_index).copied().unwrap_or_default())
            .collect();
        let jumps = leading_junk(&hips);
        let junk = if part.is_some() { 0 } else { jumps };
        if junk > 0 {
            notes.push(format!(
                "the first {junk} frame(s) are setup frames (the hips jump more than {JUMP_M} m): dropped"
            ));
        } else if jumps > 0 {
            notes.push(format!(
            "the hips jump more than {JUMP_M} m in the first {jumps} frame(s): kept, the .meta's range includes them"
        ));
        }
        let frames = count.saturating_sub(junk);
        if frames < 2 {
            return Err(format!("only {frames} frame(s) of motion after the setup frames"));
        }
        let mut points: BTreeMap<String, Vec<Vec3>> = BTreeMap::new();
        for (lm, &index) in &self.map {
            let v: Vec<Vec3> = motion
                .positions
                .iter()
                .skip(first + junk)
                .take(frames)
                .map(|row| row.get(index).copied().unwrap_or_default())
                .collect();
            points.insert((*lm).to_string(), v);
        }
        let src = Landmarks::new(points).map_err(|e| e.to_string())?;
        let (swapped, votes) = swapped_frames(&src);
        let mut swapped_share = 0.0;
        if votes > 0 {
            let share = swapped as f64 / votes as f64;
            swapped_share = share;
            let what = format!(
                "the toes and knees point the other way from what the hip line says on {swapped} of {votes} checks: the left and right joints look swapped; check the joints map (l_hip, r_hip, ...) or the file's axis system"
            );
            if share >= SWAPPED_REFUSE {
                return Err(what);
            }
            if share >= SWAPPED_WARN {
                notes.push(what);
            }
        }

        let (scaled, scale) = retarget::scale_to(skel, &src).map_err(|e| e.to_string())?;
        let (rots, roots) = retarget::retarget(skel, &scaled).map_err(|e| e.to_string())?;
        // how far the hips landed from the source's, before the floor moves everything
        let hips_err = retarget::hip_error(skel, &rots, &roots, &scaled).map_err(|e| e.to_string())?;
        let (roots, floor) = retarget::level(skel, &rots, &roots).map_err(|e| e.to_string())?;
        let found = retarget::floor_offset(skel, &rots, &roots).map_err(|e| e.to_string())?;
        if floor == 0.0 && found.abs() > retarget::FLOOR_MOST_M {
            notes.push(format!(
            "the floor is {found:+.2} m from z = 0, more than {} m: left where it is (the body may be on a prop, or the file's unit is off)",
            retarget::FLOOR_MOST_M
        ));
        }
        let rest = at_rest(skel, &rots);
        let unused: Vec<&str> = motion
            .names
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.map.values().any(|v| v == i))
            .map(|(_, n)| n.as_str())
            .collect();
        if !unused.is_empty() {
            notes.push(format!(
            "{} joints have no landmark and were not read (fingers, face, props, twist and end joints): the rig's own stay at rest",
            unused.len()
        ));
        }

        let mut clip = Clip::new(skel.names.clone(), motion.fps);
        clip.name = name.to_string();
        clip.loop_ = part.is_some_and(|c| c.looping);
        for (f, (q, &ro)) in rots.iter().zip(&roots).enumerate() {
            clip.set_key(Key::new(f as i64, q.clone(), ro, Interp::Linear))?;
        }
        let joints_json: Map<String, Value> = self
            .map
            .iter()
            .map(|(lm, &i)| {
                (
                    (*lm).to_string(),
                    json!(motion.names.get(i).cloned().unwrap_or_default()),
                )
            })
            .collect();
        let mut report = Map::new();
        report.insert("fps".into(), json!(motion.fps));
        report.insert("frames".into(), json!(frames));
        report.insert("joints".into(), Value::Object(joints_json));
        report.insert("bone_map".into(), json!(self.bone_map));
        report.insert("given".into(), json!(self.given));
        report.insert("unused".into(), json!(unused));
        report.insert("at_rest".into(), json!(rest));
        report.insert("scale".into(), json!(scale));
        report.insert("floor".into(), json!(floor));
        report.insert(
            "hips".into(),
            json!({ "mid": hips_err[0], "left": hips_err[1], "right": hips_err[2] }),
        );
        report.insert("notes".into(), json!(notes));
        if let Some(c) = part {
            report.insert(
            "clip".into(),
            json!({ "name": c.name, "first": c.first, "last": c.last, "loop": c.looping, "take": motion.take }),
        );
        }
        report.insert(
            "source".into(),
            json!({
                "version": motion.version,
                "up_axis": motion.up_axis,
                "front_axis": motion.front_axis,
                "unit_scale": motion.unit_scale,
                "fps_from": motion.fps_from,
                "take": motion.take,
                "joints": motion.names.len(),
                "frames": motion.frames,
                "dropped": junk,
            }),
        );
        Ok(Imported {
            clip,
            report,
            swapped: swapped_share,
        })
    }
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

    #[test]
    fn setup_frames_are_the_jumping_ones() {
        let still = vec![[0.0, 0.0, 1.0]; 10];
        assert_eq!(leading_junk(&still), 0);
        let mut jump = still.clone();
        jump[0] = [5.0, 0.0, 1.0];
        assert_eq!(leading_junk(&jump), 1);
        assert_eq!(leading_junk(&[[0.0; 3], [9.0; 3]]), 0, "too short to drop any");
    }

    fn motion(frames: usize, start_frame: f64) -> Motion {
        Motion {
            names: vec![],
            classes: vec![],
            parents: vec![],
            fps: 30.0,
            frames,
            start_frame,
            positions: vec![],
            version: 7400,
            up_axis: "+Y".into(),
            front_axis: "+Z".into(),
            unit_scale: 1.0,
            fps_from: "TimeMode".into(),
            take: "Take".into(),
            notes: vec![],
        }
    }

    fn range(take: &str, first: f64, last: f64) -> MetaClip {
        MetaClip {
            name: "c".into(),
            take: take.into(),
            first,
            last,
            looping: false,
        }
    }

    #[test]
    fn a_clips_frames_are_the_files_own_and_both_ends_are_in() {
        let m = motion(40, 0.0);
        assert_eq!(clip_range(&m, &range("Take", 0.0, 39.0)).unwrap(), (0, 40, None));
        assert_eq!(clip_range(&m, &range("", 20.0, 39.0)).unwrap(), (20, 20, None));
        // a take starting at frame 100: frame 120 of the file is sample 20
        let late = motion(40, 100.0);
        assert_eq!(
            clip_range(&late, &range("Take", 120.0, 129.0)).unwrap(),
            (20, 10, None)
        );
        // between frames: rounded, and said
        let (a, n, note) = clip_range(&m, &range("Take", 1.5, 4.0)).unwrap();
        assert_eq!((a, n), (2, 3));
        assert!(note.unwrap().contains("between frames"));
    }

    #[test]
    fn a_bone_matches_loosely_only_where_one_joint_does() {
        let names: Vec<String> = ["Humanoid_Head", "Hips", "Left Arm", "LEFTARM"]
            .map(String::from)
            .to_vec();
        assert_eq!(loose_matches(&names, "Humanoid_ Head"), [0]);
        assert_eq!(loose_matches(&names, "hips"), [1]);
        assert_eq!(loose_matches(&names, "left arm"), [2, 3]);
        assert!(loose_matches(&names, "Head").is_empty(), "a part is no match");
        assert!(loose_matches(&names, "  ").is_empty());
    }

    #[test]
    fn the_neck_is_the_heads_parent_unless_a_landmark_has_it() {
        // 0 hips, 1 chest, 2 upper chest, 3 head
        let parents = [None, Some(0), Some(1), Some(2)];
        let mut map = BTreeMap::from([("hips", 0), ("spine", 1), ("head", 3)]);
        assert_eq!(neck_from_head(&map, &parents), Some(2));
        map.insert("spine", 2);
        assert_eq!(neck_from_head(&map, &parents), None);
        assert_eq!(
            neck_from_head(&BTreeMap::from([("head", 0)]), &parents),
            None,
            "a head with no parent"
        );
        assert_eq!(neck_from_head(&BTreeMap::new(), &parents), None);
    }

    #[test]
    fn a_clip_running_past_its_take_is_cut_to_it_and_says_which_frames() {
        // worked by hand on a take of frames 0 to 39
        let m = motion(40, 0.0);
        let (a, n, note) = clip_range(&m, &range("Take", 30.0, 45.0)).unwrap();
        assert_eq!((a, n), (30, 10));
        assert!(note.unwrap().contains("frames 40 to 45 cut"));
        let (a, n, note) = clip_range(&m, &range("Take", -3.0, 10.0)).unwrap();
        assert_eq!((a, n), (0, 11));
        assert!(note.unwrap().contains("frames -3 to -1 cut"));
        let (a, n, note) = clip_range(&m, &range("Take", -2.0, 41.0)).unwrap();
        assert_eq!((a, n), (0, 40));
        assert!(note.unwrap().contains("frames -2 to -1 and 40 to 41 cut"));
        let (a, n, _) = clip_range(&m, &range("Take", 0.0, 1e300)).unwrap();
        assert_eq!((a, n), (0, 40));
        // a take starting at frame 100, frames 100 to 139: samples 38 and 39 are left
        let (a, n, note) = clip_range(&motion(40, 100.0), &range("Take", 138.0, 150.0)).unwrap();
        assert_eq!((a, n), (38, 2));
        assert!(note.unwrap().contains("frames 140 to 150 cut"));
        // rounded and cut: both said
        let (a, n, note) = clip_range(&m, &range("Take", 37.5, 40.0)).unwrap();
        assert_eq!((a, n), (38, 2));
        let note = note.unwrap();
        assert!(
            note.contains("between frames") && note.contains("frames 40 to 40 cut"),
            "{note}"
        );
    }

    #[test]
    fn a_clip_outside_its_take_or_too_short_is_refused() {
        let m = motion(40, 0.0);
        for (clip, want) in [
            (range("Take", 10.0, 10.0), "fewer than 2"),
            (range("Take", 10.0, 2.0), "fewer than 2"),
            (range("Take", 40.0, 50.0), "outside the take"),
            (range("Take", -5.0, -1.0), "outside the take"),
            // one frame of the take each
            (range("Take", 39.0, 45.0), "fewer than 2"),
            (range("Take", -5.0, 0.0), "fewer than 2"),
        ] {
            let e = clip_range(&m, &clip).unwrap_err();
            assert!(e.contains(want), "{e} should say {want}");
        }
        let late = motion(40, 100.0);
        let e = clip_range(&late, &range("Take", 0.0, 10.0)).unwrap_err();
        assert!(e.contains("100 to 139"), "{e}");
    }
}
