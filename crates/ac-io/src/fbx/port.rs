//! `clip.port`: a resumable bulk port of FBX files onto the rig (roadmap 164000), for a collection
//! too big and too mixed for `clip.import_batch`. The input is a text list of files, one path a
//! line; the output is a folder outside any repository: `animations/<pack> - <action>/*.clip.json` (a library the app opens with --library) and a
//! manifest, one JSON value a line, one line for each source:
//!
//! - `imported`, with its clips, and those left out with why;
//! - `duplicate`, with the source it repeats (same FBX bytes, same bone map and clips in its `.meta`);
//! - `refused`, with the reason.
//!
//! A line is added only once a source is finished, so a run that stops loses nothing, and a source
//! with a line is never looked at again: a clip deleted by hand stays deleted. A file that can't be
//! read right now (the drive is away) gets no line: it is reported, and the next run tries it. With
//! nothing to do the run writes nothing. The pack is the first folder under `root`, the action the
//! file's own folder.
//!
//! `retry_refused` tries again the sources whose latest outcome is `refused`, after an importer fix.
//! A changed outcome is a new line, and the last line of a source is its outcome. One that comes out
//! the same adds nothing. A refused source wrote no clip, so this can't bring back a deleted one.
//!
//! Each clip is turned to start facing -Y when it is more than [`FACING_MOST_DEG`] off, then checked:
//! its values are finite, it faces -Y, its feet aren't swapped, and its feet are on z = 0 unless it
//! has no floor. A clip that fails is left out and listed. Bone lengths are the rig's by construction,
//! since a clip holds only rotations and the root.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::FRAC_PI_2;
use std::io::Write;
use std::path::{Path, PathBuf};

use ac_core::Document;
use ac_core::clip::{Clip, Interp, Key};
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use ac_core::gait;
use ac_core::quat::{self, Quat, Vec3};
use ac_core::retarget;
use ac_core::rig::Skeleton;
use serde_json::{Map, Value, json};

use super::commands::file_part;
use super::import::{Imported, MAX_FILE_BYTES, open_bytes};
use super::unity_meta;

type P = Map<String, Value>;
type Out = Result<Value, String>;
type R<T> = Result<T, String>;

/// The manifest's name in the output folder.
pub const MANIFEST: &str = "port-manifest.jsonl";
/// The folder under the output folder the clips go in.
pub const CLIPS: &str = "animations";
/// Largest list read.
pub const MAX_LIST_BYTES: u64 = 64 << 20;
/// Furthest a clip's first frame may face from -Y and stay as it is, and pass the check.
pub const FACING_MOST_DEG: f64 = 15.0;
/// The share of the importer's side checks from which a clip's feet count as swapped.
pub const SWAPPED_FAIL: f64 = 0.5;
/// How far below z = 0 the floor may be.
pub const BELOW_MOST_M: f64 = 0.03;
/// A floor moved further down than this, by a clip whose feet never hold still, is a hanging clip's.
pub const HANG_MOVED_M: f64 = 0.05;
/// A foot slower than this is holding still (metres a second).
const STILL_M_S: f64 = 0.25;
/// For this many frames in a row.
const STILL_FRAMES: usize = 3;
/// A root staying this close to where it starts is a clip in place.
const IN_PLACE_M: f64 = 0.5;
/// A hip line shorter than this from above (a body on its side) says nothing about where it faces.
const HIP_LINE_MOST_M: f64 = 0.02;
const FIRST_LINE: &str = "{\"port_manifest\":1}";

pub static SPECS: &[CommandSpec] = &[CommandSpec::new("clip.port", "Port a list of FBX files", port)
    .icon("import")
    .inputs(&[
        ParamSpec::new(
            "list",
            K::Text,
            "a text file of .fbx paths, one a line (a relative path is from the list's folder)",
        )
        .required(),
        ParamSpec::new(
            "root",
            K::Text,
            "the folder the paths lie under: the first folder below it is the pack",
        )
        .required(),
        ParamSpec::new(
            "to",
            K::Text,
            "the output folder for the clips and the manifest (made if missing; never inside a repository)",
        )
        .required(),
        ParamSpec::new(
            "limit",
            K::Integer,
            "finish at most this many new sources, then stop",
        ),
        ParamSpec::new(
            "retry_refused",
            K::Bool,
            "try again the sources whose latest outcome is refused; imported and duplicate stay skipped",
        ),
    ])
    .writes()
    .outputs(crate::returns::CLIP_PORT)];

/// What the manifest holds so far.
#[derive(Default)]
struct Book {
    /// A source's latest outcome: a later line replaces an earlier one.
    done: BTreeMap<String, Value>,
    /// A source's content -> the first source with it.
    content: BTreeMap<String, String>,
    /// Clip paths in use, lower case: two sources never share one, on any file system.
    taken: BTreeSet<String>,
    /// How many bytes are whole lines, and whether a cut-off line follows.
    whole: u64,
    torn: bool,
}

impl Book {
    fn add(&mut self, entry: Value) {
        let source = entry["source"].as_str().unwrap_or_default().to_string();
        // what a replaced line said about the source's content is no longer so
        if let Some(c) = self.done.get(&source).and_then(|old| old["content"].as_str())
            && self.content.get(c) == Some(&source)
        {
            self.content.remove(c);
        }
        if let Some(c) = entry["content"].as_str() {
            self.content
                .entry(c.to_string())
                .or_insert_with(|| source.clone());
        }
        for clip in entry["clips"].as_array().into_iter().flatten() {
            if let Some(p) = clip["path"].as_str() {
                self.taken.insert(p.to_lowercase());
            }
        }
        self.done.insert(source, entry);
    }
}

/// Read a manifest. A last line with no newline is a run stopped mid-write: it is dropped, and its
/// source done again. Any other line that can't be read is our own file broken: refused, not guessed.
fn load(path: &Path) -> R<Book> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Book::default()),
        Err(e) => return Err(format!("can't read {}: {e}", path.display())),
    };
    let mut book = Book::default();
    let whole = text.rfind('\n').map_or(0, |i| i + 1);
    book.whole = whole as u64;
    book.torn = whole < text.len();
    for (i, line) in text.get(..whole).unwrap_or("").lines().enumerate() {
        if i == 0 {
            if line != FIRST_LINE {
                return Err(format!("{} is not a port manifest", path.display()));
            }
            continue;
        }
        let entry: Value =
            serde_json::from_str(line).map_err(|e| format!("{} line {}: {e}", path.display(), i + 1))?;
        if !entry["source"].is_string() {
            return Err(format!("{} line {}: no source", path.display(), i + 1));
        }
        book.add(entry);
    }
    Ok(book)
}

/// The manifest, opened only when there is a line to add.
struct Journal<'a> {
    path: &'a Path,
    whole: u64,
    file: Option<std::fs::File>,
}

impl Journal<'_> {
    fn add(&mut self, entry: &Value) -> R<()> {
        let path = self.path;
        let fail = |e: std::io::Error| format!("can't write {}: {e}", path.display());
        if self.file.is_none() {
            // what a stopped run cut off goes first; an append-only handle may not shorten a file
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(path)
                .and_then(|f| f.set_len(self.whole))
                .map_err(fail)?;
            let mut f2 = std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(fail)?;
            if self.whole == 0 {
                f2.write_all(format!("{FIRST_LINE}\n").as_bytes()).map_err(fail)?;
            }
            self.file = Some(f2);
        }
        if let Some(f) = self.file.as_mut() {
            f.write_all(format!("{entry}\n").as_bytes()).map_err(fail)?;
            f.flush().map_err(fail)?;
        }
        Ok(())
    }
}

/// The different lines of a list, in order. Blank lines don't count.
fn read_list(path: &Path) -> R<Vec<String>> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("can't read the list {}: {e}", path.display()))?
        .len();
    if size > MAX_LIST_BYTES {
        return Err(format!(
            "the list {} is {size} bytes, larger than {MAX_LIST_BYTES}",
            path.display()
        ));
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("can't read the list {}: {e}", path.display()))?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for line in text.strip_prefix('\u{feff}').unwrap_or(&text).lines() {
        let line = line.trim();
        if !line.is_empty() && seen.insert(line) {
            out.push(line.to_string());
        }
    }
    Ok(out)
}

/// The repository `path` would lie in, if any: clips and manifests stay out of every repository.
fn repository_of(path: &Path) -> Option<PathBuf> {
    let absolute = std::env::current_dir().map_or_else(|_| path.to_path_buf(), |d| d.join(path));
    absolute
        .ancestors()
        .find(|a| a.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The pack and the action of a file: the first folder under `root`, and the file's own folder.
fn groups(path: &Path, root: &Path) -> R<(String, String)> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| format!("it isn't under the root {}", root.display()))?;
    let folders: Vec<String> = rel
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    let pack = folders.first().map_or("Ungrouped".to_string(), |f| file_part(f));
    let action = match folders.get(1..).and_then(<[String]>::last) {
        Some(f) => file_part(f),
        None => "Other".to_string(),
    };
    Ok((pack, action))
}

/// A reason with what differs from file to file blanked, so like counts with like.
fn reason_group(reason: &str, source: &str) -> String {
    let reason = reason.replace(source, "<file>");
    let reason = reason.split_once("; ").map_or(reason.as_str(), |(a, _)| a);
    let mut out = String::new();
    let mut quoted = false;
    let mut digits = false;
    for c in reason.chars() {
        if c == '"' {
            if !quoted {
                out.push_str("\"..\"");
            }
            quoted = !quoted;
        } else if !quoted {
            if c.is_ascii_digit() {
                if !digits {
                    out.push('#');
                }
                digits = true;
                continue;
            }
            out.push(c);
        }
        digits = false;
    }
    out.chars().take(160).collect()
}

/// How far a yaw (as [`gait::facing`]) is from facing -Y, in degrees.
fn facing_off_deg(yaw: f64) -> f64 {
    gait::wrap(-FRAC_PI_2 - yaw).to_degrees()
}

/// What a clip's floor says.
#[derive(Debug, PartialEq)]
enum Floor {
    /// On z = 0.
    On,
    /// No floor under it; `undo`: the import moved it down, and that move is taken back.
    Hanging { undo: bool },
    /// The feet stay below z = 0.
    Below,
}

/// `found`: the clip's floor now, metres above z = 0. `moved`: how far the import moved it down.
/// `still`: a foot holds still at some point.
fn floor_verdict(found: f64, moved: f64, still: bool) -> Floor {
    if moved > HANG_MOVED_M && !still {
        Floor::Hanging { undo: true }
    } else if found > retarget::FLOOR_MOST_M {
        Floor::Hanging { undo: false }
    } else if found < -BELOW_MOST_M {
        Floor::Below
    } else {
        Floor::On
    }
}

/// Does a foot hold still for [`STILL_FRAMES`] frames in a row?
fn feet_still(skel: &Skeleton, rots: &[Vec<Quat>], roots: &[Vec3], fps: f64) -> R<bool> {
    let feet = ["foot.L", "foot.R"].map(|n| skel.bone(n));
    let [Some(l), Some(r)] = feet else {
        return Err("the rig has no feet".into());
    };
    let most = STILL_M_S / fps.max(1.0);
    let mut last: Option<[Vec3; 2]> = None;
    let mut run = [0usize; 2];
    for (q, &ro) in rots.iter().zip(roots) {
        let (_, pos) = skel.fk(q, ro).map_err(|e| e.to_string())?;
        let now = [
            pos.get(l).copied().unwrap_or_default(),
            pos.get(r).copied().unwrap_or_default(),
        ];
        if let Some(before) = last {
            for k in 0..2 {
                let (Some(a), Some(b), Some(n)) = (before.get(k), now.get(k), run.get_mut(k)) else {
                    continue;
                };
                *n = if quat::len3(quat::sub3(*b, *a)) < most {
                    *n + 1
                } else {
                    0
                };
                if *n + 1 >= STILL_FRAMES {
                    return Ok(true);
                }
            }
        }
        last = Some(now);
    }
    Ok(false)
}

/// The yaw of a pose and how long its hip line is from above.
fn yaw_of(skel: &Skeleton, rots: &[Quat], root: Vec3) -> R<(f64, f64)> {
    let (_, pos) = skel.fk(rots, root).map_err(|e| e.to_string())?;
    let yaw = gait::facing(skel, &pos)?;
    let at = |n: &str| skel.bone(n).and_then(|b| pos.get(b).copied()).unwrap_or_default();
    let (l, r) = (at("upperleg01.L"), at("upperleg01.R"));
    Ok((yaw, (l[0] - r[0]).hypot(l[1] - r[1])))
}

/// A clip that passed, ready to write.
struct Kept {
    clip: Clip,
    entry: Value,
}

/// Turn a clip to face -Y and check it: the clip to keep, or each check it failed, with why.
fn finish(skel: &Skeleton, imported: &Imported) -> R<Result<Kept, Vec<(&'static str, String)>>> {
    let keys = imported.clip.keys();
    let mut rots: Vec<Vec<Quat>> = keys.iter().map(|k| k.rotations.clone()).collect();
    let mut roots: Vec<Vec3> = keys.iter().map(|k| k.root).collect();
    let finite = rots
        .iter()
        .flatten()
        .flatten()
        .chain(roots.iter().flatten())
        .all(|x| x.is_finite());
    if !finite {
        return Ok(Err(vec![("finite", "a value is not finite".to_string())]));
    }
    let (Some(first_rots), Some(&first_root)) = (rots.first().cloned(), roots.first()) else {
        return Ok(Err(vec![("finite", "the clip has no frames".to_string())]));
    };
    let mut fails: Vec<(&'static str, String)> = Vec::new();
    let mut notes: Vec<Value> = imported
        .report
        .get("notes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if imported.swapped >= SWAPPED_FAIL {
        fails.push((
            "feet_swapped",
            format!(
                "the feet look swapped on {:.0}% of the checks",
                imported.swapped * 100.0
            ),
        ));
    }

    // facing
    let (yaw, hip_line) = yaw_of(skel, &first_rots, first_root)?;
    let mut turned = 0.0;
    if hip_line < HIP_LINE_MOST_M {
        notes.push(json!(
            "the hip line is near upright on the first frame, so where it faces can't be read: left as it was"
        ));
    } else if facing_off_deg(yaw).abs() > FACING_MOST_DEG {
        let turn = gait::wrap(-FRAC_PI_2 - yaw);
        let (s, c) = turn.sin_cos();
        turned = turn.to_degrees();
        for r in &mut rots {
            if let Some(q) = r.first_mut() {
                *q = quat::normalize(quat::mul(gait::qz(turn), *q));
            }
        }
        for r in &mut roots {
            let (x, y) = (r[0] - first_root[0], r[1] - first_root[1]);
            r[0] = first_root[0] + c * x - s * y;
            r[1] = first_root[1] + s * x + c * y;
        }
    }
    if let (Some(q), Some(&ro)) = (rots.first(), roots.first()) {
        let (yaw, line) = yaw_of(skel, q, ro)?;
        let off = facing_off_deg(yaw);
        if line >= HIP_LINE_MOST_M && off.abs() > FACING_MOST_DEG {
            fails.push((
                "faces",
                format!("the first frame is {off:.0} degrees from facing -Y"),
            ));
        }
    }

    // the floor
    let found = retarget::floor_offset(skel, &rots, &roots).map_err(|e| e.to_string())?;
    let moved = imported
        .report
        .get("floor")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let still = feet_still(skel, &rots, &roots, imported.clip.fps)?;
    let verdict = floor_verdict(found, moved, still);
    let hanging = matches!(verdict, Floor::Hanging { .. });
    match verdict {
        Floor::Hanging { undo: true } => {
            let back = moved / skel.hip_height;
            for r in &mut roots {
                r[2] += back;
            }
            notes.push(json!(format!(
                "no foot holds still: taken as hanging, and the {moved:.3} m the floor was moved down is taken back"
            )));
        }
        Floor::Below => fails.push((
            "feet_on_floor",
            format!("the floor is {found:.2} m from z = 0, below it"),
        )),
        Floor::Hanging { undo: false } | Floor::On => {}
    }
    if !fails.is_empty() {
        return Ok(Err(fails));
    }

    let start = roots.first().copied().unwrap_or_default();
    let travel = roots
        .iter()
        .map(|r| (r[0] - start[0]).hypot(r[1] - start[1]) * skel.hip_height)
        .fold(0.0, f64::max);
    let mut clip = Clip::new(imported.clip.bones.clone(), imported.clip.fps);
    clip.name.clone_from(&imported.clip.name);
    clip.loop_ = imported.clip.loop_;
    for (f, (q, ro)) in rots.into_iter().zip(roots).enumerate() {
        clip.set_key(Key::new(f as i64, q, ro, Interp::Linear))?;
    }
    let entry = json!({
        "name": clip.name,
        "frames": imported.report.get("frames").cloned().unwrap_or(Value::Null),
        "fps": clip.fps,
        "loop": clip.loop_,
        "take": imported.report.get("source").and_then(|s| s.get("take")).cloned().unwrap_or(Value::Null),
        "turned": turned,
        "floor": moved,
        "hanging": hanging,
        "in_place": travel < IN_PLACE_M,
        "notes": notes,
    });
    Ok(Ok(Kept { clip, entry }))
}

/// The first free clip path for a name in a folder.
fn free_path(taken: &BTreeSet<String>, folder: &str, name: &str) -> String {
    let ext = crate::library::Kind::Animations.ext();
    let mut path = format!("{folder}/{name}{ext}");
    let mut n = 1;
    while taken.contains(&path.to_lowercase()) {
        n += 1;
        path = format!("{folder}/{name} ({n}){ext}");
    }
    path
}

/// What happened to one source, as its manifest line. `Ok(None)`: it can't be read now, so it gets
/// no outcome. `Err`: the output can't be written, which stops the run.
fn one(skel: &Skeleton, line: &str, path: &Path, root: &Path, to: &Path, book: &Book) -> R<Option<Value>> {
    let refuse = |reason: String, content: Option<&str>| {
        let mut e = json!({ "source": line, "outcome": "refused", "reason": reason });
        if let Some(c) = content {
            e["content"] = json!(c);
        }
        Ok(Some(e))
    };
    if !line.to_lowercase().ends_with(".fbx") {
        return refuse("not an .fbx".into(), None);
    }
    let (pack, action) = match groups(path, root) {
        Ok(g) => g,
        Err(e) => return refuse(e, None),
    };
    let Ok(about) = std::fs::metadata(path) else {
        return Ok(None);
    };
    if !about.is_file() {
        return refuse("not a file".into(), None);
    }
    if about.len() > MAX_FILE_BYTES {
        return refuse(
            format!("{} bytes, larger than {MAX_FILE_BYTES}", about.len()),
            None,
        );
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(None);
    };
    let hash = ac_core::hash::of_bytes(&bytes);
    let meta = match unity_meta::beside(path) {
        Ok(m) => m,
        Err(e) => return refuse(e, None),
    };
    // the `.meta`'s meaning, not its bytes: copies of a pack differ in their ids, and a copy cut
    // into other clips is not a duplicate
    let meta_hash = meta.as_ref().map_or(String::new(), |m| {
        ac_core::hash::of_bytes(format!("{m:?}").as_bytes())
    });
    let content = format!("{hash}:{meta_hash}:{}", bytes.len());
    // a source tried again isn't a copy of itself
    if let Some(first) = book.content.get(&content).filter(|first| *first != line) {
        return Ok(Some(json!({
            "source": line,
            "outcome": "duplicate",
            "of": first,
            "content": content,
        })));
    }
    let stem = path
        .file_stem()
        .map_or("imported".to_string(), |s| s.to_string_lossy().to_string());
    let source = match open_bytes(&bytes, &stem, &BTreeMap::new(), meta.as_ref()) {
        Ok(s) => s,
        Err(e) => return refuse(e, Some(&content)),
    };
    drop(bytes);

    let mut made: Vec<(Option<&str>, Result<Imported, String>)> = Vec::new();
    if source.clips().is_empty() {
        made.push((None, source.whole(skel)));
    }
    for c in source.clips() {
        made.push((Some(c.name.as_str()), source.clip(skel, c)));
    }
    let folder = format!("{CLIPS}/{pack} - {action}");
    let mut taken = book.taken.clone();
    let (mut clips, mut left_out): (Vec<Value>, Vec<Value>) = (Vec::new(), Vec::new());
    for (name, imported) in made {
        let label = name.unwrap_or(&stem);
        let imported = match imported {
            Ok(i) => i,
            Err(e) => {
                left_out.push(json!({ "name": label, "reason": e }));
                continue;
            }
        };
        match finish(skel, &imported) {
            Err(e) => left_out.push(json!({ "name": label, "reason": e })),
            Ok(Err(failed)) => {
                let checks: Vec<&str> = failed.iter().map(|(c, _)| *c).collect();
                let why: Vec<&str> = failed.iter().map(|(_, w)| w.as_str()).collect();
                left_out.push(json!({ "name": label, "checks": checks, "reason": why.join("; ") }));
            }
            Ok(Ok(kept)) => {
                let file = name.map_or_else(
                    || file_part(&stem),
                    |c| format!("{} - {}", file_part(&stem), file_part(c)),
                );
                let rel = free_path(&taken, &folder, &file);
                let out = to.join(&rel);
                if let Some(dir) = out.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                craft_atomic::atomic_write(&out, clipfile::save_clip(&kept.clip).as_bytes())
                    .map_err(|e| format!("can't write {}: {e}", out.display()))?;
                taken.insert(rel.to_lowercase());
                let mut entry = kept.entry;
                entry["path"] = json!(rel);
                clips.push(entry);
            }
        }
    }
    if clips.is_empty() {
        let reason = left_out
            .first()
            .and_then(|l| l["reason"].as_str())
            .unwrap_or("it holds no clips")
            .to_string();
        let mut e = json!({
            "source": line,
            "outcome": "refused",
            "reason": format!("no clip kept: {reason}"),
            "content": content,
        });
        e["left_out"] = json!(left_out);
        return Ok(Some(e));
    }
    Ok(Some(json!({
        "source": line,
        "outcome": "imported",
        "content": content,
        "pack": pack,
        "action": action,
        "clips": clips,
        "left_out": left_out,
    })))
}

/// The counts over the list's sources, as the command returns them.
fn summary(book: &Book, sources: &[String]) -> Value {
    let (mut imported, mut duplicate, mut refused, mut clips, mut left) = (0, 0, 0, 0, 0);
    let (mut hanging, mut in_place, mut turned) = (0, 0, 0);
    let mut checks: BTreeMap<String, u64> = BTreeMap::new();
    let mut reasons: BTreeMap<String, u64> = BTreeMap::new();
    for line in sources {
        let Some(e) = book.done.get(line) else { continue };
        match e["outcome"].as_str() {
            Some("imported") => imported += 1,
            Some("duplicate") => duplicate += 1,
            _ => refused += 1,
        }
        if let Some(r) = e["reason"].as_str()
            && e["left_out"].is_null()
        {
            *reasons.entry(reason_group(r, line)).or_default() += 1;
        }
        for c in e["clips"].as_array().into_iter().flatten() {
            clips += 1;
            hanging += u64::from(c["hanging"] == true);
            in_place += u64::from(c["in_place"] == true);
            turned += u64::from(c["turned"].as_f64().is_some_and(|t| t != 0.0));
        }
        for l in e["left_out"].as_array().into_iter().flatten() {
            left += 1;
            for c in l["checks"].as_array().into_iter().flatten() {
                *checks
                    .entry(c.as_str().unwrap_or_default().to_string())
                    .or_default() += 1;
            }
            if let Some(r) = l["reason"].as_str() {
                *reasons.entry(reason_group(r, line)).or_default() += 1;
            }
        }
    }
    json!({
        "sources": sources.len(),
        "imported": imported,
        "duplicate": duplicate,
        "refused": refused,
        "pending": sources.len() - imported - duplicate - refused,
        "clips": clips,
        "left_out": left,
        "hanging": hanging,
        "in_place": in_place,
        "turned": turned,
        "checks": checks,
        "reasons": reasons,
    })
}

fn port(doc: &mut Document, p: &P) -> Out {
    let list = PathBuf::from(cmd::text(p, "list").unwrap_or_default());
    let root = PathBuf::from(cmd::text(p, "root").unwrap_or_default());
    let to = PathBuf::from(cmd::text(p, "to").unwrap_or_default());
    if to.as_os_str().is_empty() {
        return Err("give the folder the clips and the manifest go in (to)".into());
    }
    if let Some(repo) = repository_of(&to) {
        return Err(format!(
            "{} is inside the repository {}: a port's clips and manifest stay outside",
            to.display(),
            repo.display()
        ));
    }
    let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(u64::MAX);
    let retry = p.get("retry_refused").and_then(Value::as_bool).unwrap_or(false);
    let finished = |e: &Value| !(retry && e["outcome"] == "refused");
    let sources = read_list(&list)?;
    let from = list.parent().unwrap_or_else(|| Path::new(""));
    // a wrong root is the caller's slip, not an outcome to keep for every file
    if let Some(line) = sources.iter().find(|l| !from.join(l).starts_with(&root)) {
        return Err(format!("{line} isn't under the root {}", root.display()));
    }
    std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
    let manifest = to.join(MANIFEST);
    let mut book = load(&manifest)?;
    let torn = book.torn;
    let mut journal = Journal {
        path: &manifest,
        whole: book.whole,
        file: None,
    };
    let mut skipped = sources
        .iter()
        .filter(|s| book.done.get(*s).is_some_and(finished))
        .count();
    let mut new = 0u64;
    let mut unreachable: Vec<&str> = Vec::new();
    for line in &sources {
        if book.done.get(line).is_some_and(finished) {
            continue;
        }
        if new >= limit {
            break;
        }
        tracing::info!(source = %line, "clip.port");
        match one(&doc.skeleton, line, &from.join(line), &root, &to, &book)? {
            // tried again and refused the same way: nothing to add, and no use of the limit
            Some(entry) if book.done.get(line) == Some(&entry) => skipped += 1,
            Some(entry) => {
                journal.add(&entry)?;
                book.add(entry);
                new += 1;
            }
            None => unreachable.push(line),
        }
    }
    let mut out = summary(&book, &sources);
    out["manifest"] = json!(manifest.to_string_lossy());
    out["new"] = json!(new);
    out["skipped"] = json!(skipped);
    out["unreachable"] = json!(unreachable);
    out["torn"] = json!(torn);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn facing_is_measured_from_minus_y() {
        // worked by hand: -pi/2 is -Y; 0 is +X, a quarter turn away; -pi/2 + 0.1 rad is 5.73 degrees off
        assert!(facing_off_deg(-FRAC_PI_2).abs() < 1e-9);
        assert!((facing_off_deg(0.0).abs() - 90.0).abs() < 1e-9);
        assert!((facing_off_deg(-FRAC_PI_2 + 0.1).abs() - 5.7296).abs() < 1e-3);
    }

    #[test]
    fn the_floor_verdict_leaves_hanging_clips_up() {
        // levelled, with feet that plant: on the floor, however far it moved
        assert_eq!(floor_verdict(0.0, 0.15, true), Floor::On);
        assert_eq!(floor_verdict(0.0, 0.0, false), Floor::On);
        // moved down with feet that never hold still: hanging, and the move is taken back
        assert_eq!(floor_verdict(0.0, 0.15, false), Floor::Hanging { undo: true });
        // a small move is only noise
        assert_eq!(floor_verdict(0.0, 0.04, false), Floor::On);
        // too far up to have been moved
        assert_eq!(floor_verdict(0.5, 0.0, false), Floor::Hanging { undo: false });
        assert_eq!(floor_verdict(0.5, 0.0, true), Floor::Hanging { undo: false });
        // too far down: a floor the source has, left where it is
        assert_eq!(floor_verdict(-0.5, 0.0, true), Floor::Below);
        assert_eq!(floor_verdict(-0.02, 0.0, true), Floor::On);
    }

    #[test]
    fn the_pack_and_the_action_come_from_the_folders() {
        let root = Path::new("r");
        let g = |p: &str| groups(Path::new(p), root).unwrap();
        assert_eq!(g("r/P/A/x.fbx"), ("P".into(), "A".into()));
        assert_eq!(g("r/P/deep/er/A/x.fbx"), ("P".into(), "A".into()));
        assert_eq!(g("r/P/x.fbx"), ("P".into(), "Other".into()));
        assert_eq!(g("r/x.fbx"), ("Ungrouped".into(), "Other".into()));
        assert!(
            groups(Path::new("s/x.fbx"), root)
                .unwrap_err()
                .contains("under the root")
        );
    }

    #[test]
    fn reasons_group_without_names_or_numbers() {
        let a = reason_group(
            "D:/a b/x.fbx: clip \"Walk\": 12 of 40; the joints: Hips, Spine",
            "D:/a b/x.fbx",
        );
        let b = reason_group("D:/c/y.fbx: clip \"Run\": 3 of 7; the joints: Foo", "D:/c/y.fbx");
        assert_eq!(a, b);
        assert_eq!(a, "<file>: clip \"..\": # of #");
    }

    #[test]
    fn a_clip_path_never_repeats() {
        let mut taken = BTreeSet::new();
        let first = free_path(&taken, "animations/P - A", "Walk");
        assert_eq!(first, "animations/P - A/Walk.clip.json");
        taken.insert(first.to_lowercase());
        assert_eq!(
            free_path(&taken, "animations/P - A", "WALK"),
            "animations/P - A/WALK (2).clip.json"
        );
    }
}
