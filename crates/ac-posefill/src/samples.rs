//! The engine's sample clips, an optional download of their own (`posefill.samples`): off by
//! default, behind their licence, from NVIDIA to this machine. Their licence allows evaluating
//! NVIDIA's technology and nothing else, so what this adds to the library is marked evaluation-only
//! ([`ac_core::evaluation`]), which blocks export and community upload.
//!
//! The setup is `tools/posefill/setup_samples.py`, in `samples/` in the pose fill install, run like
//! the install itself. It leaves one `clips/*.json` per clip on the rig; `add` turns those into
//! library clips, and `remove` deletes them with the download, as the licence asks when it ends.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ac_core::Document;
use ac_core::clip::{Clip, Interp, Key};
use ac_core::clipfile;
use ac_core::command::{Kind as K, ParamSpec};
use ac_core::error::hinted;
use ac_helper::install::Kind;
use ac_helper::setup::{Installer, Spec};
use ac_io::library::{Kind as LibKind, Library};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::setup::Licence;

type P = Map<String, Value>;

/// The library group the clips go in.
pub const GROUP: &str = "Evaluation only";
/// A clip's `origin` while it is one of the sample clips (a copy saved under a new name loses it).
pub const ORIGIN: &str = "nvidia-sample-clips";
/// What the person is told before the download and wherever the clips are offered.
pub const NOTICE: &str = "NVIDIA's sample clips are only for evaluating NVIDIA's technology. Check the NVIDIA Sample Data Evaluation License before use: you are responsible for how you use them. Anything made with them is marked evaluation-only, and this app won't export or share it.";
/// What accepting says, in the person's own words.
pub const ACCEPT: &str =
    "I have read the licence, I am an adult, and I'll use these clips only to evaluate NVIDIA's technology";
/// The licence the download accepts.
pub const LICENCE: Licence = Licence {
    part: "the sample clips",
    licence: "NVIDIA Sample Data Evaluation License",
    url: "https://github.com/NVlabs/motionbricks/blob/54fc563aeaaf819a753d72cc9fdc6713989cdd43/assets/motions/LICENSE.txt",
};

static KIND: Kind = Kind {
    folder: "posefill/samples",
    env: "AC_POSEFILL_SAMPLES_DIR",
    what: "the sample clips",
    how: "download them with Sample clips in the Pose fill panel (posefill.samples)",
};
static SPEC: Spec = Spec {
    kind: &KIND,
    script: "tools/posefill/setup_samples.py",
    python: "3.12",
    msvc: false,
    gpu_required: false,
    without_gpu: "",
};
static INSTALLER: Installer = Installer::new(&SPEC);
static STAND_IN: AtomicBool = AtomicBool::new(false);

/// Fixture runs: one made-up clip in place of the download, and nothing outside the library is
/// touched.
pub fn use_stand_in() {
    STAND_IN.store(true, Ordering::Relaxed);
    INSTALLER.use_stand_in();
}

fn stand_in() -> bool {
    STAND_IN.load(Ordering::Relaxed)
}

/// The folder: `samples/` in the pose fill install.
pub fn dir() -> Option<PathBuf> {
    crate::install::dir().map(|d| d.join("samples"))
}

pub const OUT: &[ParamSpec] = &[
    ParamSpec::new(
        "state",
        K::Choice(&[
            "not installed",
            "installing",
            "stopping",
            "installed",
            "stopped",
            "failed",
        ]),
        "where the download is",
    )
    .required(),
    ParamSpec::new("dir", K::Text, "the folder they go in").required(),
    ParamSpec::new("notice", K::Text, "what to check before using these clips").required(),
    ParamSpec::new("accept", K::Text, "what accepting says").required(),
    ParamSpec::new("licence", K::Text, "the licence: {part, licence, url}")
        .json()
        .required(),
    ParamSpec::new("downloaded", K::Integer, "the clips on the rig in the folder").required(),
    ParamSpec::new("in_library", K::Integer, "the sample clips in the library").required(),
    ParamSpec::new("log", K::Text, "the setup's log"),
    ParamSpec::new("seconds", K::Integer, "how long this run has taken"),
    ParamSpec::new("step", K::Integer, "the step working now"),
    ParamSpec::new("steps", K::Integer, "how many steps"),
    ParamSpec::new("label", K::Text, "what the step does"),
    ParamSpec::new("last", K::Text, "the setup's last line"),
    ParamSpec::new("error", K::Text, "why it failed"),
    ParamSpec::new("added", K::TextList, "add: the ids put in the library"),
    ParamSpec::new("removed", K::TextList, "remove: the ids deleted"),
    ParamSpec::new(
        "derived",
        K::TextList,
        "remove: other marked animations and poses still in the library, which the licence wants gone too when it ends",
    ),
];

pub const IN: &[ParamSpec] = &[
    ParamSpec::new(
        "action",
        K::Choice(&["check", "start", "stop", "add", "remove"]),
        "check (default) says where it is; start downloads them (about 115 MB, needs accept); stop ends that, and start carries on; add puts them in the library, in the group Evaluation only; remove deletes the download and the sample clips, edited or not",
    ),
    ParamSpec::new(
        "accept",
        K::Bool,
        "start: the person has read the licence and accepts it, as `accept` says. It is their own answer: ask them, don't set it for them",
    ),
    ParamSpec::new(
        "derived",
        K::Bool,
        "remove: also delete the other marked animations and poses (default: they are listed and left)",
    ),
];

fn library(doc: &Document, kind: LibKind) -> Result<Library, String> {
    let root = doc
        .library
        .as_deref()
        .ok_or_else(|| hinted("no library is open", "start the app with a library"))?;
    Ok(Library::new(root, kind))
}

/// The marked items of a library: the sample clips themselves, and the rest.
fn marked(lib: &Library) -> (Vec<String>, Vec<String>) {
    let (mut own, mut derived) = (Vec::new(), Vec::new());
    for item in lib.items() {
        if item["evaluation_only"] != true || item["builtin"] == true {
            continue;
        }
        let Some(id) = item["id"].as_str() else {
            continue;
        };
        if item["origin"].as_str() == Some(ORIGIN) {
            own.push(id.to_string());
        } else {
            derived.push(id.to_string());
        }
    }
    (own, derived)
}

/// One clip as `convert_samples.py` writes it.
#[derive(Deserialize)]
struct Converted {
    name: String,
    fps: f64,
    rotations: Vec<Vec<[f64; 4]>>,
    root: Vec<[f64; 3]>,
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir.join("clips"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    out
}

/// A two-key clip for the fixtures, standing in for the download.
fn made_up(doc: &Document) -> Converted {
    let rest = vec![[0.0, 0.0, 0.0, 1.0]; doc.skeleton.names.len()];
    Converted {
        name: "Stand-in sample".into(),
        fps: 30.0,
        rotations: vec![rest.clone(), rest],
        root: vec![[0.0, 0.0, 0.0], [0.0, -0.1, 0.0]],
    }
}

fn to_clip(doc: &Document, c: Converted) -> Result<Clip, String> {
    if c.rotations.len() != c.root.len() || c.rotations.iter().any(|r| r.len() != doc.skeleton.names.len()) {
        return Err(format!("{}: not for this rig", c.name));
    }
    let mut clip = Clip::new(doc.skeleton.names.clone(), c.fps);
    clip.group = GROUP.into();
    clip.origin = ORIGIN.into();
    clip.name = c.name;
    clip.evaluation_only = true;
    for (f, (r, o)) in c.rotations.into_iter().zip(c.root).enumerate() {
        clip.set_key(Key::new(f as i64, r, o, Interp::Linear))?;
    }
    Ok(clip)
}

/// Put the converted clips in the library; one already there is left alone.
fn add(doc: &Document, dir: &Path) -> Result<Vec<String>, String> {
    let lib = library(doc, LibKind::Animations)?;
    let mut clips = Vec::new();
    if stand_in() {
        clips.push(made_up(doc));
    } else {
        for path in files(dir) {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            clips.push(serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?);
        }
    }
    if clips.is_empty() {
        return Err(hinted("no sample clips have been downloaded", "run start first"));
    }
    let mut added = Vec::new();
    for c in clips {
        let id = format!("{GROUP}/{}", c.name);
        if lib.exists(&id) {
            continue;
        }
        lib.write(&id, &clipfile::save_clip(&to_clip(doc, c)?))?;
        added.push(id);
    }
    Ok(added)
}

/// Delete the download, the sample clips and, when asked, what was made from them. Returns what
/// was deleted and what was left.
fn remove(doc: &mut Document, dir: &Path, derived: bool) -> Result<(Vec<String>, Vec<String>), String> {
    if INSTALLER.running() {
        return Err(hinted("the download is running", "stop it first"));
    }
    let (anims, poses) = (library(doc, LibKind::Animations)?, library(doc, LibKind::Poses)?);
    let (own, from_anims) = marked(&anims);
    let (own_poses, from_poses) = marked(&poses);
    let (mut gone, mut left) = (Vec::new(), Vec::new());
    for id in own
        .into_iter()
        .chain(if derived { from_anims.clone() } else { Vec::new() })
    {
        anims.delete(&id)?;
        if doc.clip_id.as_deref() == Some(id.as_str()) {
            doc.clip_id = None;
        }
        gone.push(id);
    }
    for id in own_poses
        .into_iter()
        .chain(if derived { from_poses.clone() } else { Vec::new() })
    {
        poses.delete(&id)?;
        gone.push(id);
    }
    if !derived {
        left.extend(from_anims);
        left.extend(from_poses);
    }
    if !stand_in() && dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("can't delete {}: {e}", dir.display()))?;
    }
    Ok((gone, left))
}

fn accepted(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let said =
        json!({ "accepted": ACCEPT, "licence": LICENCE.licence, "url": LICENCE.url, "unix_time": when });
    std::fs::write(dir.join("accepted.json"), said.to_string()).map_err(|e| format!("{}: {e}", dir.display()))
}

/// `posefill.samples`.
pub fn command(doc: &mut Document, p: &P) -> Result<Value, String> {
    let dir = dir().ok_or_else(|| {
        hinted(
            "no data folder is known",
            "set AC_POSEFILL_DIR to the folder pose fill is installed in",
        )
    })?;
    let assets = doc.assets.clone();
    let mut extra = P::new();
    match p.get("action").and_then(Value::as_str).unwrap_or("check") {
        "start" => {
            if p.get("accept").and_then(Value::as_bool) != Some(true) {
                return Err(hinted(
                    format!("these clips need their licence accepted first: {}", LICENCE.url),
                    format!("ask the person, then pass accept: true. {NOTICE}"),
                ));
            }
            crate::install::find()?;
            if !stand_in() {
                accepted(&dir)?;
            }
            INSTALLER.start(&dir, assets.as_deref(), false)?;
        }
        "stop" => INSTALLER.stop()?,
        "add" => {
            extra.insert("added".into(), json!(add(doc, &dir)?));
        }
        "remove" => {
            let derived = p.get("derived").and_then(Value::as_bool).unwrap_or(false);
            let (gone, left) = remove(doc, &dir, derived)?;
            extra.insert("removed".into(), json!(gone));
            extra.insert("derived".into(), json!(left));
        }
        _ => {}
    }
    let mut out = INSTALLER.status(&dir);
    out["notice"] = json!(NOTICE);
    out["accept"] = json!(ACCEPT);
    out["licence"] = serde_json::to_value(LICENCE).map_err(|e| e.to_string())?;
    let on_disk = if stand_in() { 1 } else { files(&dir).len() };
    out["downloaded"] = json!(on_disk);
    out["in_library"] = json!(library(doc, LibKind::Animations).map_or(0, |l| marked(&l).0.len()));
    if let Some(o) = out.as_object_mut() {
        o.extend(extra);
    }
    Ok(out)
}
