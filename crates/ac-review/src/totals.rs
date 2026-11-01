//! What a training set adds up to: minutes per label, and per usage label, from `usage.json` beside
//! the sets (`{"packs": {"<pack>": "<label>"}, "clips": {"<id>": "<label>"}}`). A clip's own label
//! wins over its pack's. A library clip with neither is `just-me`, so a forgotten one can't leave;
//! a rig take with neither is `share-freely`, its licence being the registry's. That file names
//! packs, so it is refused inside a git checkout.
//!
//! Totals count the clips kept, without their cuts. A public set is warned about every `just-me`
//! clip it holds, kept or rejected, as its file names them either way.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::training_set::{self, ClipStatus, TrainingSet};

type R<T> = Result<T, String>;

pub const FILE: &str = "usage.json";
pub const NO_LABEL: &str = "(no label)";

/// What may leave the machine: the clip, and a model trained on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Usage {
    ShareFreely,
    TrainOnly,
    JustMe,
}

impl Usage {
    pub const ALL: [Usage; 3] = [Usage::ShareFreely, Usage::TrainOnly, Usage::JustMe];

    pub fn word(self) -> &'static str {
        match self {
            Usage::ShareFreely => "share-freely",
            Usage::TrainOnly => "train-only",
            Usage::JustMe => "just-me",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Usage::ShareFreely => "Share freely",
            Usage::TrainOnly => "Train only",
            Usage::JustMe => "Just me",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Usage::ShareFreely => "Anyone can have the clip, and models made from it can be released.",
            Usage::TrainOnly => "Never shared, but models made from it can be released.",
            Usage::JustMe => "The clip and any model made from it stay on this machine.",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usages {
    #[serde(default)]
    pub packs: BTreeMap<String, Usage>,
    #[serde(default)]
    pub clips: BTreeMap<String, Usage>,
}

impl Usages {
    /// The file beside the sets; none yet is no marks. One that won't read is an error.
    pub fn load(folder: &Path) -> R<Usages> {
        let path = folder.join(FILE);
        if !path.exists() {
            return Ok(Usages::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, folder: &Path) -> R<()> {
        let path = folder.join(FILE);
        if let Some(repo) = training_set::git_checkout(&path)? {
            return Err(format!(
                "{FILE} names packs: it can't be kept inside the git checkout {}; keep the sets outside any repository",
                repo.display()
            ));
        }
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// A clip's label, and whether anyone chose it.
    pub fn of(&self, id: &str) -> (Usage, bool) {
        let marked = self
            .clips
            .get(id)
            .or_else(|| self.packs.get(marked_pack(id, self)))
            .copied();
        match marked {
            Some(u) => (u, true),
            None if id.starts_with(training_set::LIBRARY) => (Usage::JustMe, false),
            None => (Usage::ShareFreely, false),
        }
    }
}

/// The pack of a clip's id: `<set>/<take>`'s set, `library:<group>/<name>`'s group.
pub fn pack_of(id: &str) -> &str {
    let id = id.strip_prefix(training_set::LIBRARY).unwrap_or(id);
    id.split_once('/').map_or(id, |(pack, _)| pack)
}

/// A clip's pack as `usages` names it, the same rule as `ac_train.trainset.pack_of`: a library
/// group, or the longest mark it starts with, or its start before ` - ` (a port writes
/// `<pack> - <action>`).
pub fn marked_pack<'a>(id: &'a str, usages: &'a Usages) -> &'a str {
    let group = pack_of(id);
    if !id.starts_with(training_set::LIBRARY) {
        return group;
    }
    usages
        .packs
        .keys()
        .filter(|k| group == k.as_str() || group.starts_with(&format!("{k} - ")))
        .max_by_key(|k| k.len())
        .map_or_else(|| group.split(" - ").next().unwrap_or(group), String::as_str)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Count {
    pub clips: usize,
    pub minutes: f64,
}

impl Count {
    fn add(&mut self, minutes: f64) {
        self.clips += 1;
        self.minutes += minutes;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    /// Clips kept, by label; a clip with two labels counts under both.
    pub labels: BTreeMap<String, Count>,
    pub share_freely: Count,
    pub train_only: Count,
    /// Unmarked library clips included.
    pub just_me: Count,
    /// Of `just_me`, the library clips nobody has labelled.
    pub unmarked: Count,
    pub rejected: usize,
    /// Clips kept that aren't here (a library not given): counted, with no minutes.
    pub missing: usize,
    /// Every pack the set holds: its label's word, or `unmarked`.
    pub packs: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

/// `length`: a clip's frames and frame rate, if it is here.
pub fn totals(set: &TrainingSet, usages: &Usages, length: impl Fn(&str) -> Option<(usize, f64)>) -> Totals {
    let mut out = Totals::default();
    let (mut held, mut held_unmarked) = (0, 0);
    for (id, c) in &set.clips {
        let pack = marked_pack(id, usages);
        let (usage, chosen) = usages.of(id);
        let word = match usages.packs.get(pack) {
            Some(u) => u.word(),
            None if id.starts_with(training_set::LIBRARY) => "unmarked",
            None => Usage::ShareFreely.word(),
        };
        out.packs.insert(pack.to_string(), word.to_string());
        if usage == Usage::JustMe {
            held += 1;
            if !chosen {
                held_unmarked += 1;
            }
        }
        if c.status == ClipStatus::Reject {
            out.rejected += 1;
            continue;
        }
        let minutes = match length(id) {
            Some((frames, fps)) if fps > 0.0 => {
                let cut: usize = c.cuts.iter().map(|[a, b]| b - a + 1).sum();
                frames.saturating_sub(cut) as f64 / fps / 60.0
            }
            _ => {
                out.missing += 1;
                0.0
            }
        };
        if c.labels.is_empty() {
            out.labels.entry(NO_LABEL.to_string()).or_default().add(minutes);
        }
        for l in &c.labels {
            out.labels.entry(l.clone()).or_default().add(minutes);
        }
        match usage {
            Usage::ShareFreely => out.share_freely.add(minutes),
            Usage::TrainOnly => out.train_only.add(minutes),
            Usage::JustMe => {
                out.just_me.add(minutes);
                if !chosen {
                    out.unmarked.add(minutes);
                }
            }
        }
    }
    if set.public && held > 0 {
        out.warnings.push(format!(
            "this public set holds {held} Just me clips ({held_unmarked} not labelled yet): training refuses it until they are rejected or relabelled"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::training_set::SetClip;

    fn clip(id: &str, status: ClipStatus, labels: &[&str], cuts: &[[usize; 2]]) -> (String, SetClip) {
        (
            id.to_string(),
            SetClip {
                id: id.to_string(),
                status,
                labels: labels.iter().map(|l| l.to_string()).collect(),
                cuts: cuts.to_vec(),
            },
        )
    }

    fn length(id: &str) -> Option<(usize, f64)> {
        match id {
            "library:G/a" => Some((30, 30.0)),  // 1 s
            "library:G/b" => Some((60, 30.0)),  // 2 s
            "library:G/c" => Some((45, 15.0)),  // 3 s
            "library:H/d" => Some((120, 60.0)), // 2 s
            "cmu/e" => Some((300, 30.0)),       // 10 s
            _ => None,
        }
    }

    fn usages() -> Usages {
        Usages {
            packs: [
                ("G".to_string(), Usage::TrainOnly),
                ("cmu".to_string(), Usage::JustMe),
            ]
            .into_iter()
            .collect(),
            clips: BTreeMap::new(),
        }
    }

    fn near(a: f64, seconds: f64) {
        assert!(
            (a - seconds / 60.0).abs() < 1e-12,
            "{a} minutes, wanted {seconds} s"
        );
    }

    #[test]
    fn a_library_clip_takes_the_longest_mark_or_its_start() {
        let mut marks = usages();
        marks.packs.insert("G - extra".to_string(), Usage::JustMe);
        assert_eq!(marked_pack("library:G - extra - walk/a", &marks), "G - extra");
        assert_eq!(marked_pack("library:G - run/a", &marks), "G");
        assert_eq!(marked_pack("library:other - run/a", &marks), "other");
        assert_eq!(marked_pack("G - run/a", &marks), "G - run");
    }

    #[test]
    fn an_unmarked_rig_take_is_shared_and_an_unmarked_library_clip_is_just_me() {
        let mut set = TrainingSet::new("t", false).unwrap();
        for id in ["lafan/a", "library:other/a"] {
            set.clips.insert(id.to_string(), SetClip::new(id));
        }
        let t = totals(&set, &usages(), |_| Some((30, 60.0)));
        assert_eq!(t.share_freely.clips, 1);
        assert_eq!(t.unmarked.clips, 1);
        assert_eq!(t.just_me.clips, 1);
    }

    #[test]
    fn a_clips_own_label_wins_over_its_packs() {
        let mut u = usages();
        u.clips.insert("library:G/a".to_string(), Usage::JustMe);
        assert_eq!(u.of("library:G/a"), (Usage::JustMe, true));
        assert_eq!(u.of("library:G/b"), (Usage::TrainOnly, true));
        assert_eq!(u.of("library:H/b"), (Usage::JustMe, false));
    }

    #[test]
    fn hand_worked_totals() {
        use ClipStatus::{Keep, Reject};
        let mut s = TrainingSet::new("x", false).unwrap();
        s.clips.extend([
            clip("library:G/a", Keep, &["walk"], &[]),
            // 60 frames less 30 cut: 1 s
            clip("library:G/b", Keep, &["walk", "tired"], &[[0, 29]]),
            // rejected: in no total
            clip("library:G/c", Reject, &["hop"], &[]),
            clip("library:H/d", Keep, &[], &[]),
            clip("cmu/e", Keep, &["run"], &[]),
            // not here: one clip with no minutes
            clip("library:Z/gone", Keep, &["walk"], &[]),
        ]);
        let t = totals(&s, &usages(), length);
        // walk: a 1 s + b 1 s + gone 0 s, 3 clips
        assert_eq!(t.labels["walk"].clips, 3);
        near(t.labels["walk"].minutes, 2.0);
        near(t.labels["tired"].minutes, 1.0);
        near(t.labels["run"].minutes, 10.0);
        near(t.labels[NO_LABEL].minutes, 2.0);
        assert!(!t.labels.contains_key("hop"));
        // train only: G's a and b, 2 s
        assert_eq!(t.train_only.clips, 2);
        near(t.train_only.minutes, 2.0);
        // just me: cmu's e 10 s, unmarked H's d 2 s and Z's gone 0 s
        assert_eq!(t.just_me.clips, 3);
        near(t.just_me.minutes, 12.0);
        assert_eq!(t.unmarked.clips, 2);
        near(t.unmarked.minutes, 2.0);
        assert_eq!((t.rejected, t.missing), (1, 1));
        assert_eq!(t.packs["G"], "train-only");
        assert_eq!(t.packs["cmu"], "just-me");
        assert_eq!(t.packs["H"], "unmarked");
        assert!(t.warnings.is_empty(), "a private set may hold anything");
    }

    #[test]
    fn a_public_set_warns_of_a_just_me_clip_only() {
        let warned = |id: &str, status| {
            let mut s = TrainingSet::new("x", true).unwrap();
            s.clips.extend([clip(id, status, &[], &[])]);
            totals(&s, &usages(), length).warnings
        };
        let w = warned("cmu/e", ClipStatus::Keep);
        assert!(w[0].contains("1 Just me clips (0 not labelled"), "{w:?}");
        let w = warned("library:H/d", ClipStatus::Keep);
        assert!(w[0].contains("1 Just me clips (1 not labelled"), "{w:?}");
        // its file names it even when rejected
        assert_eq!(warned("cmu/e", ClipStatus::Reject).len(), 1);
        assert!(warned("library:G/a", ClipStatus::Keep).is_empty());
    }

    #[test]
    fn the_usage_file_stays_out_of_a_checkout_and_says_when_it_is_bad() {
        let dir = std::env::temp_dir().join(format!("ac-review-usage-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (out, repo) = (dir.join("out"), dir.join("repo"));
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(Usages::load(&out).unwrap(), Usages::default());
        usages().save(&out).unwrap();
        assert_eq!(Usages::load(&out).unwrap(), usages());
        assert!(usages().save(&repo).unwrap_err().contains("names packs"));
        assert!(!repo.join(FILE).exists());
        std::fs::write(out.join(FILE), r#"{"packs": {"G": "stolen"}}"#).unwrap();
        assert!(Usages::load(&out).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
