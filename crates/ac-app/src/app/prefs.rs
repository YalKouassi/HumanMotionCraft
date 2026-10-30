//! Saved preferences: what the window remembers between launches (not the document, which the
//! library holds). Stored by craft-settings as `config.toml` in the app's config folder, written
//! atomically a moment after a change and on exit; a damaged file is moved aside and announced.
//!
//! **Adding a preference** (other features): give your feature its own section, a struct with
//! `#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]`, `#[serde(default)]` and a
//! `Default` impl, and add one field for it to [`Prefs`] (also `#[serde(default)]`). Read it with
//! `self.shell.prefs.get().<section>` and change it with `self.shell.prefs.update(|p| ...)`; the
//! store saves on its own. Never rename or retype a stored key: add a new one (old files keep
//! loading, unknown keys are kept in the file). A section that fails to parse falls back to its
//! defaults and the user is told.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use craft_keymap::Overrides;
use craft_settings::{Location, Settings, SettingsStore};
use serde::{Deserialize, Serialize};

/// Everything the app remembers. Each section is `#[serde(default)]`, so a file from an older
/// build (or a hand-edited one with keys missing) loads with defaults for the rest.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Prefs {
    pub general: General,
    pub view: View,
    pub layout: Layout,
    /// The AI fill sliders' values, applied (`ai.settings`) at startup.
    pub ai: Ai,
    /// The viewport's viewing aids: onion skin, trails, picker, handles, gizmo axes.
    pub viewport: Viewport,
    /// The timeline's playback and view switches.
    pub timeline: TimelinePrefs,
    /// Files: export and preview choices, the welcome and tips, and per library its recent
    /// animations and the one open last.
    pub files: FilesPrefs,
    /// The library panel: per library its folded groups and recent poses.
    pub library_panel: LibraryPanel,
    /// The user's changes to the default shortcuts (Help > Keyboard shortcuts).
    #[serde(with = "craft_keymap::keymap::overrides_list")]
    pub shortcuts: Overrides,
    /// The user's named shortcut sets (the shortcuts dialog's Set menu; Import makes one).
    pub shortcut_sets: ShortcutSets,
}

/// craft-keymap's shortcut sets, as stored (equal when they store the same).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(transparent)]
pub struct ShortcutSets(pub craft_keymap::Profiles);

impl PartialEq for ShortcutSets {
    fn eq(&self, other: &Self) -> bool {
        self.0.active() == other.0.active() && self.0.sets() == other.0.sets()
    }
}

impl Settings for Prefs {
    const APP_ID: &'static str = ac_core::product::ID;
    const SCHEMA_VERSION: u32 = 1;
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// The colours the overlays and ghosts use (`theme.rs`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Colours {
    #[default]
    Standard,
    /// Okabe–Ito colours: told apart with any colour vision.
    Safe,
}

/// How times are shown: frame numbers, seconds, or timecode (`mm:ss:ff`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Frames,
    Seconds,
    Timecode,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct General {
    /// Frames per second of a new animation.
    pub default_fps: f64,
    /// Seconds between autosaves; 0 turns autosave off. (Stored here; the autosave feature reads
    /// it.)
    pub autosave_secs: u32,
    /// UI scale (egui zoom factor): Ctrl + / Ctrl - / Ctrl 0.
    pub ui_scale: f32,
    pub theme: Theme,
    pub colours: Colours,
    pub units: Units,
    /// The library folder used at the next launch (`None`: the app's own folder). `--library`
    /// wins over it.
    pub library_dir: Option<PathBuf>,
}

impl Default for General {
    fn default() -> General {
        General {
            default_fps: 30.0,
            autosave_secs: 120,
            ui_scale: 1.0,
            theme: Theme::Dark,
            colours: Colours::Standard,
            units: Units::Frames,
            library_dir: None,
        }
    }
}

/// The viewport's camera.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct CameraPrefs {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    /// No perspective (the viewport's 5).
    pub ortho: bool,
}

impl Default for CameraPrefs {
    fn default() -> CameraPrefs {
        let c = ac_render::Camera::default();
        CameraPrefs {
            yaw: c.yaw,
            pitch: c.pitch,
            distance: c.distance,
            target: c.target.to_array(),
            ortho: c.ortho,
        }
    }
}

/// The window's view state, by the names `--ui` takes.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct View {
    /// `drag`, `reach` or `move`.
    pub tool: String,
    /// The physics overlay.
    pub overlay: bool,
    /// The keyed-pose and AI ghosts.
    pub ghost: bool,
    /// The dope sheet's part rows.
    pub dope: bool,
    /// `animations` or `poses` (`builtin`, from older builds: the animations, built-ins only).
    pub library_tab: String,
    /// The library's source switch: `all`, `mine` or `builtin`.
    pub library_source: String,
    /// The library's order: `name`, `recent`, `modified` or `length`.
    pub library_sort: String,
    pub camera: CameraPrefs,
    /// The performance HUD.
    pub perf: bool,
}

impl Default for View {
    fn default() -> View {
        View {
            tool: "drag".into(),
            overlay: true,
            ghost: true,
            dope: false,
            library_tab: "animations".into(),
            library_source: "all".into(),
            library_sort: "name".into(),
            camera: CameraPrefs::default(),
            perf: false,
        }
    }
}

/// The viewport's viewing aids (`app/view_state.rs`), by the names `--ui` takes.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Viewport {
    pub onion: bool,
    pub onion_count: u32,
    pub onion_step: u32,
    /// `frames` (every `onion_step` frames) or `keys` (the nearest keys).
    pub onion_mode: String,
    pub trails: bool,
    /// Frames each side of the playhead (0: the whole animation).
    pub trail_span: u32,
    pub picker: bool,
    #[serde(default)]
    pub range: bool,
    /// `body` or `all` (fingers and face too).
    pub handles: String,
    /// `world` or `local` axes for the rotate gizmo.
    pub gizmo: String,
    pub follow: bool,
}

impl Default for Viewport {
    fn default() -> Viewport {
        Viewport {
            onion: false,
            onion_count: 2,
            onion_step: 2,
            onion_mode: "frames".into(),
            trails: false,
            trail_span: 0,
            picker: false,
            range: false,
            handles: "body".into(),
            gizmo: "world".into(),
            follow: false,
        }
    }
}

/// The timeline's switches (its time format is `general.units`; the dope sheet is `view.dope`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct TimelinePrefs {
    /// Playback speed (0.25 to 2).
    pub speed: f64,
    /// Every frame shown (else real time).
    pub every_frame: bool,
    pub contacts: bool,
    pub curves: bool,
    pub retime: bool,
    /// The curve editor's channels: X, Y, Z angle, then the hips' X, Y, Z position.
    pub curve_channels: [bool; 6],
}

impl Default for TimelinePrefs {
    fn default() -> TimelinePrefs {
        TimelinePrefs {
            speed: 1.0,
            every_frame: false,
            contacts: false,
            curves: false,
            retime: false,
            curve_channels: [true, true, true, false, false, false],
        }
    }
}

/// What the window remembers about files. Recent animations are per library (keyed by its
/// folder), since their ids name items in it; the rest is the person's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct FilesPrefs {
    /// Where .glb files were exported last.
    pub export_dir: Option<String>,
    /// The export's joints: full, game, body or bones.
    pub export_profile: Option<String>,
    /// The bone list (comma-separated) for the "bones" profile.
    pub export_bones: String,
    /// Where preview videos and contact sheets went last.
    pub preview_dir: Option<String>,
    /// mp4 or gif.
    pub preview_format: Option<String>,
    pub welcome_seen: bool,
    pub tips_seen: bool,
    /// Per library folder ([`library_key`]): recent animations and the last one open.
    pub libraries: BTreeMap<String, LibraryPrefs>,
}

/// One library's recent animations (library ids, newest first) and the one open last.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct LibraryPrefs {
    pub recent: Vec<String>,
    pub last_open: Option<String>,
}

/// The library panel, per library folder (by [`library_key`]): the groups folded away
/// (`animations/<group>`, `poses/<group>`, `animations/@starred`, ...) and the poses applied
/// last, newest first. Stars are the library's own (`favourites.json`, `library.favourite`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct LibraryPanel {
    pub folded: BTreeMap<String, Vec<String>>,
    pub recent_poses: BTreeMap<String, Vec<String>>,
}

/// How a library folder is named in [`FilesPrefs::libraries`]: its full path where it can be
/// found, else as given.
pub fn library_key(dir: &Path) -> String {
    let full = std::fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned();
    // Windows' long-path prefix (`\\?\C:\...`) says nothing to a person reading the file
    match full.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => rest.to_string(),
        _ => full,
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Left,
    Right,
}

/// Panel sizes and placement (points).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Layout {
    pub library_width: f32,
    pub inspector_width: f32,
    /// `0`: the timeline's own height.
    pub timeline_height: f32,
    /// Which side the library is on (the inspector takes the other).
    pub library_side: Side,
    pub show_library: bool,
    pub show_inspector: bool,
    pub show_timeline: bool,
    /// Serialized `craft_ui::docking::Workspace<DockPanel>` arrangement (empty: default).
    #[serde(default)]
    pub dock_json: String,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout {
            library_width: 290.0,
            inspector_width: 270.0,
            timeline_height: 0.0,
            library_side: Side::Left,
            show_library: true,
            show_inspector: true,
            show_timeline: true,
            dock_json: String::new(),
        }
    }
}

/// The AI fill sliders (`ai.settings`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Ai {
    pub style: String,
    pub style_force: f64,
    pub ai_force: f64,
    pub trajectory: f64,
    pub head: f64,
}

impl Default for Ai {
    fn default() -> Ai {
        let s = ac_core::ai::FillSettings::default();
        Ai {
            style: s.style,
            style_force: s.style_force,
            ai_force: s.ai_force,
            trajectory: s.trajectory,
            head: s.head,
        }
    }
}

impl Ai {
    pub fn from_settings(s: &ac_core::ai::FillSettings) -> Ai {
        Ai {
            style: s.style.clone(),
            style_force: s.style_force,
            ai_force: s.ai_force,
            trajectory: s.trajectory,
            head: s.head,
        }
    }
}

/// Where the preferences live: `--prefs <dir>`, `--prefs memory`, or (no flag) the app's config
/// folder, except for `--screenshot` and `--control` runs, which keep them in memory so a picture
/// or an agent's session neither depends on nor changes what a person set.
pub fn location(flag: Option<&str>, automated: bool) -> Location {
    match flag {
        Some("memory") => Location::Memory,
        Some(dir) => Location::Dir(PathBuf::from(dir)),
        None if automated => Location::Memory,
        None => Location::Auto,
    }
}

/// Open the store. Never fails: problems come back as messages for the user.
pub fn open(location: Location) -> (SettingsStore<Prefs>, Vec<String>) {
    let (store, report) = SettingsStore::<Prefs>::open(location);
    let mut notes = Vec::new();
    if let Some(moved) = &report.broken_moved_to {
        notes.push(format!(
            "settings: the settings file was damaged and was moved to {}; defaults are used",
            moved.display()
        ));
    }
    for e in &report.section_errors {
        notes.push(format!("settings: {e:?}; that section uses its defaults"));
    }
    if let Some(why) = &report.read_only {
        notes.push(format!("settings: changes won't be saved ({why:?})"));
    }
    (store, notes)
}

/// The UI scale steps Ctrl + and Ctrl - move through.
pub const SCALES: &[f32] = &[
    0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

/// The next scale step up (`up`) or down from `now`.
pub fn step_scale(now: f32, up: bool) -> f32 {
    let now = if now.is_finite() { now } else { 1.0 };
    if up {
        SCALES.iter().copied().find(|s| *s > now + 1e-3).unwrap_or(3.0)
    } else {
        SCALES
            .iter()
            .rev()
            .copied()
            .find(|s| *s < now - 1e-3)
            .unwrap_or(0.5)
    }
}

/// A frame as the user's units show it, at `fps`.
pub fn time_text(frame: f64, fps: f64, units: Units) -> String {
    let fps = if fps.is_finite() && fps > 0.0 { fps } else { 30.0 };
    let frame = if frame.is_finite() { frame } else { 0.0 };
    match units {
        Units::Frames => format!("{}", frame.round() as i64),
        Units::Seconds => format!("{:.2} s", frame / fps),
        Units::Timecode => timecode(frame, fps),
    }
}

/// `mm:ss:ff` (whole frames; minutes go past 59).
pub fn timecode(frame: f64, fps: f64) -> String {
    let fps = if fps.is_finite() && fps > 0.0 { fps } else { 30.0 };
    let f = if frame.is_finite() {
        frame.round().max(0.0)
    } else {
        0.0
    } as i64;
    let per = fps.round().max(1.0) as i64;
    let secs = f / per;
    format!("{:02}:{:02}:{:02}", secs / 60, secs % 60, f % per)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn timecode_counts_whole_frames() {
        assert_eq!(timecode(0.0, 30.0), "00:00:00");
        assert_eq!(timecode(29.0, 30.0), "00:00:29");
        assert_eq!(timecode(30.0, 30.0), "00:01:00");
        assert_eq!(timecode(30.0 * 61.0 + 5.0, 30.0), "01:01:05");
        assert_eq!(timecode(f64::NAN, 30.0), "00:00:00");
        assert_eq!(timecode(10.0, 0.0), "00:00:10");
        assert_eq!(time_text(45.0, 30.0, Units::Seconds), "1.50 s");
        assert_eq!(time_text(45.4, 30.0, Units::Frames), "45");
    }

    #[test]
    fn scale_steps_stay_in_range() {
        assert_eq!(step_scale(1.0, true), 1.1);
        assert_eq!(step_scale(1.0, false), 0.9);
        assert_eq!(step_scale(3.0, true), 3.0);
        assert_eq!(step_scale(0.5, false), 0.5);
        assert_eq!(step_scale(f32::NAN, true), 1.1);
    }

    #[test]
    fn prefs_round_trip_and_old_files_load() {
        let dir = std::env::temp_dir().join(format!(
            "{}-prefs-test-{}",
            ac_core::product::ID,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let (mut store, notes) = open(Location::Dir(dir.clone()));
            assert!(notes.is_empty(), "{notes:?}");
            store.update(|p| {
                p.general.default_fps = 24.0;
                p.view.tool = "reach".into();
                p.layout.library_side = Side::Right;
                p.general.units = Units::Timecode;
                p.viewport.onion = true;
                p.viewport.onion_mode = "keys".into();
                p.viewport.trail_span = 12;
                p.view.camera.ortho = true;
                p.timeline.speed = 0.5;
                p.timeline.curve_channels = [false, true, false, true, false, false];
                p.files.export_dir = Some("D:/out".into());
                let mut mine = Overrides::new();
                mine.insert("app.play", vec!["F9".into()]);
                p.shortcut_sets.0.save_as("Mine", mine, &[]).expect("a set");
                p.files.libraries.insert(
                    library_key(Path::new("C:/some library")),
                    LibraryPrefs {
                        recent: vec!["A/walk".into()],
                        last_open: Some("A/walk".into()),
                    },
                );
            });
            store.flush().expect("save");
        }
        let (store, _) = open(Location::Dir(dir.clone()));
        let p = store.get();
        assert_eq!(p.general.default_fps, 24.0);
        assert_eq!(p.view.tool, "reach");
        assert_eq!(p.layout.library_side, Side::Right);
        assert_eq!(p.general.units, Units::Timecode);
        assert!(p.layout.show_timeline, "a key never written keeps its default");
        assert!(p.viewport.onion && p.viewport.onion_mode == "keys" && p.viewport.trail_span == 12);
        assert!(p.view.camera.ortho);
        assert_eq!(p.timeline.speed, 0.5);
        assert_eq!(
            p.timeline.curve_channels,
            [false, true, false, true, false, false]
        );
        assert_eq!(p.files.export_dir.as_deref(), Some("D:/out"));
        let sets = &p.shortcut_sets.0;
        assert_eq!(sets.get("Mine", &[]).map(|o| o.len()), Some(1), "{sets:?}");
        let lib = &p.files.libraries[&library_key(Path::new("C:/some library"))];
        assert_eq!(lib.recent, ["A/walk"]);
        assert!(
            !library_key(&dir).starts_with(r"\\?\"),
            "keys read as plain paths"
        );
        // a file from an older build: one section, one key
        std::fs::write(
            dir.join("config.toml"),
            "schema_version = 1\n[general]\ndefault_fps = 60.0\n",
        )
        .expect("write");
        let (store, notes) = open(Location::Dir(dir.clone()));
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(store.get().general.default_fps, 60.0);
        assert_eq!(store.get().general.autosave_secs, 120);
        assert_eq!(store.get().view, View::default());
        assert_eq!(store.get().viewport, Viewport::default());
        assert_eq!(store.get().timeline, TimelinePrefs::default());
        assert_eq!(store.get().files, FilesPrefs::default());
        // a section that doesn't parse: defaults for it, and the user is told
        std::fs::write(
            dir.join("config.toml"),
            "schema_version = 1\n[general]\ntheme = \"purple\"\n[view]\ntool = \"move\"\n",
        )
        .expect("write");
        let (store, notes) = open(Location::Dir(dir.clone()));
        assert!(!notes.is_empty(), "a bad value is announced");
        assert_eq!(store.get().general, General::default());
        assert_eq!(store.get().view.tool, "move", "other sections keep their values");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
