//! The document: one body, one clip, the current frame and the pose shown on it. This is what is
//! saved and undone. Derived data (world matrices, meshes, the played clip) is not stored.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::clip::Clip;
use crate::parts;
use crate::quat::{IDENTITY, Quat, Vec3};
use crate::rig::Skeleton;

/// One pose: a rotation per bone (relative to its parent) and the root offset in hip heights.
#[derive(Debug, Clone, PartialEq)]
pub struct Pose {
    pub rotations: Vec<Quat>,
    pub root: Vec3,
}

impl Pose {
    pub fn rest(bones: usize) -> Pose {
        Pose {
            rotations: vec![IDENTITY; bones],
            root: [0.0; 3],
        }
    }
}

/// A project file the document came from or was last saved to (`ac-io`'s `project.*` commands
/// keep it): its path and what the file holds, so [`Document::unsaved`] compares a project
/// document with its file, not with the library. The format itself is `ac-io`'s business.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFile {
    pub path: std::path::PathBuf,
    pub clip: Clip,
    pub links: BTreeMap<String, Clip>,
    pub ai_settings: crate::ai::FillSettings,
}

/// A copied pose: the pose and the bones it covers (`None`: all).
#[derive(Debug, Clone, PartialEq)]
pub struct Clipboard {
    pub pose: Pose,
    pub only: Option<Vec<usize>>,
    /// Copied from an evaluation-only animation: pasting marks the animation it lands in.
    pub evaluation_only: bool,
}

#[derive(Debug, Clone)]
pub struct Document {
    /// Shared: a rig never changes under a command, so undo snapshots don't copy it.
    pub skeleton: Arc<Skeleton>,
    pub clip: Clip,
    /// The playhead (fractional while scrubbing or playing).
    pub frame: f64,
    /// The pose shown: the clip's sample at `frame`, plus any edit not keyed yet.
    pub pose: Pose,
    /// Bone -> where the full-body solver keeps its head.
    pub pins: BTreeMap<String, Vec3>,
    /// Body parts a drag leaves as they are.
    pub locked: Vec<String>,
    pub custom_parts: parts::Custom,
    pub clipboard: Option<Clipboard>,
    /// Keys copied on the timeline (`keys.copy`), pasted by `keys.paste`.
    pub key_clipboard: Option<Arc<Vec<crate::clip::Key>>>,
    /// The copied keys came from an evaluation-only animation.
    pub keys_evaluation_only: bool,
    /// The library folder files are read from and saved to (set by the front end).
    pub library: Option<std::path::PathBuf>,
    /// The assets folder (rig, mannequin) the front end loaded, for export.
    pub assets: Option<std::path::PathBuf>,
    /// Joint limits for the full-body solver (`None`: unlimited).
    pub limits: Option<Arc<crate::fullbody::Limits>>,
    /// Coupled range-of-motion tables (`assets/rig/ranges.json`; `None`: not loaded).
    pub ranges: Option<Arc<crate::ranges::Ranges>>,
    /// The last physics pass and the clip it ran on (reused while the clip is unchanged).
    pub physics_cache: Option<Arc<PhysicsCache>>,
    /// The learned in-betweener the front end loaded (`None`: AI fill is unavailable).
    pub ai: Option<Arc<dyn crate::ai::Inbetweener>>,
    /// Where poses can be borrowed from (the front end's library; `None`: nowhere), for the
    /// anchor poses of a long AI fill.
    pub pose_source: Option<Arc<dyn crate::posesource::PoseSource>>,
    /// The settings AI fills run with.
    pub ai_settings: crate::ai::FillSettings,
    /// A fill shown but not kept yet (`ai.preview`); stale once the clip changes.
    pub ai_preview: Option<Arc<crate::ai::Preview>>,
    /// A preview a front end worked out off its UI thread, waiting for `ai.preview` to take it
    /// (used only if it was made from exactly this clip, links and settings). Derived, not saved.
    pub ai_precomputed: Option<Arc<crate::ai::Preview>>,
    /// The open animation's library id (`<group>/<name>`); `None` until saved.
    pub clip_id: Option<String>,
    /// Clips the open one links to, by id (loaded with it), so link layers sample without I/O.
    pub links: BTreeMap<String, Clip>,
    /// What the library holds for each animation this session read or wrote, by id: the open
    /// clip has unsaved changes when it differs from its entry. The files are outside the undo
    /// history, so the session carries this across undo, redo and refused commands.
    pub saved: BTreeMap<String, Arc<Clip>>,
    /// The project file this document is (`None`: a library animation, or never saved).
    pub project: Option<Arc<ProjectFile>>,
    /// A character of the person's own and the mapping onto it (`retarget.load`), for the
    /// export onto it and the preview beside the mannequin. Not saved in a project file.
    pub character: Option<Arc<crate::character::Loaded>>,
}

/// Height (m) of the lowest joint above the floor when standing: the toe joints sit inside the foot.
/// A physics pass over a clip, kept so playback does not re-run it every frame.
#[derive(Debug)]
pub struct PhysicsCache {
    pub clip: Clip,
    pub links: BTreeMap<String, Clip>,
    pub report: crate::physics::Report,
}

pub const TOE_LIFT: f64 = 0.02;

/// The rest pose standing on the floor: the root offset lifts the lowest joint to [`TOE_LIFT`].
pub fn standing(skeleton: &Skeleton) -> Pose {
    let mut pose = Pose::rest(skeleton.len());
    let low = skeleton.heads.iter().map(|h| h[2]).fold(f64::INFINITY, f64::min);
    if low.is_finite() && skeleton.hip_height > 0.0 {
        pose.root[2] = (TOE_LIFT - low) / skeleton.hip_height;
    }
    pose
}

impl Document {
    pub fn new(skeleton: Skeleton) -> Document {
        let pose = standing(&skeleton);
        let mut clip = Clip::new(skeleton.names.clone(), 30.0);
        clip.name = "untitled".into();
        Document {
            skeleton: Arc::new(skeleton),
            clip,
            frame: 0.0,
            pose,
            pins: BTreeMap::new(),
            locked: Vec::new(),
            custom_parts: parts::Custom::new(),
            clipboard: None,
            key_clipboard: None,
            keys_evaluation_only: false,
            library: None,
            assets: None,
            limits: None,
            ranges: None,
            physics_cache: None,
            ai: None,
            pose_source: None,
            ai_settings: crate::ai::FillSettings::default(),
            ai_preview: None,
            ai_precomputed: None,
            clip_id: None,
            links: BTreeMap::new(),
            saved: BTreeMap::new(),
            project: None,
            character: None,
        }
    }

    /// The open clip is what the library now holds under `clip_id` (just read or written).
    pub fn mark_saved(&mut self) {
        if let Some(id) = &self.clip_id {
            self.saved.insert(id.clone(), Arc::new(self.clip.clone()));
        }
    }

    /// The open clip has changes the library doesn't hold (a project document: changes its file
    /// doesn't hold). Never saved: any keys or layers count as work to keep.
    pub fn unsaved(&self) -> bool {
        if let Some(p) = &self.project {
            return self.clip != p.clip || self.links != p.links || self.ai_settings != p.ai_settings;
        }
        match &self.clip_id {
            Some(id) => self.saved.get(id).is_none_or(|c| **c != self.clip),
            None => self.clip.layers.len() > 1 || self.clip.layers.iter().any(|l| !l.keys.is_empty()),
        }
    }

    /// The bone a command named, or a refusal that says so (with the closest names).
    pub fn bone(&self, name: &str) -> Result<usize, String> {
        self.skeleton.bone(name).ok_or_else(|| {
            let near = crate::suggest::closest(name, self.skeleton.names.iter().map(String::as_str));
            let fix = if near.is_empty() {
                "rig.info lists the bone names".to_string()
            } else {
                crate::suggest::did_you_mean(&near)
            };
            crate::error::hinted(format!("no bone named {name:?} in the rig"), fix)
        })
    }
}

impl PartialEq for Document {
    fn eq(&self, other: &Document) -> bool {
        Arc::ptr_eq(&self.skeleton, &other.skeleton)
            && self.clip == other.clip
            && self.frame == other.frame
            && self.pose == other.pose
            && self.pins == other.pins
            && self.locked == other.locked
            && self.custom_parts == other.custom_parts
            && self.clipboard == other.clipboard
            && self.key_clipboard == other.key_clipboard
            && self.keys_evaluation_only == other.keys_evaluation_only
            && self.clip_id == other.clip_id
            && self.links == other.links
            && self.character == other.character
    }
}

impl crate::clip::Links for BTreeMap<String, Clip> {
    fn clip(&self, id: &str) -> Option<&Clip> {
        self.get(id)
    }
}
