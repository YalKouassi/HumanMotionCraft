//! What a training set adds up to: minutes per label, and clips and minutes that are the owner's
//! own or purchased, from `sources.json` beside the sets (`{"<pack>": "mine|purchased"}`). A pack
//! not marked counts as purchased. That file names packs, so it is refused inside a git checkout.
//!
//! Totals count the clips kept, without their cuts. A public set is warned about every purchased or
//! unmarked clip it holds, kept or rejected, as its file names them either way.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::training_set::{self, ClipStatus, TrainingSet};

type R<T> = Result<T, String>;

pub const FILE: &str = "sources.json";
pub const NO_LABEL: &str = "(no label)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Mine,
    Purchased,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sources(pub BTreeMap<String, Source>);

impl Sources {
    /// The file beside the sets; none yet is no marks. One that won't read is an error.
    pub fn load(folder: &Path) -> R<Sources> {
        let path = folder.join(FILE);
        if !path.exists() {
            return Ok(Sources::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text)
            .map(Sources)
            .map_err(|e| format!("{}: {e}", path.display()))
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
        let mut text = serde_json::to_string_pretty(&self.0).map_err(|e| e.to_string())?;
        text.push('\n');
        craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// The pack of a clip's id: `<set>/<take>`'s set, `library:<group>/<name>`'s group.
pub fn pack_of(id: &str) -> &str {
    let id = id.strip_prefix(training_set::LIBRARY).unwrap_or(id);
    id.split_once('/').map_or(id, |(pack, _)| pack)
}

/// A clip's pack as `sources` names it, the same rule as `ac_train.trainset.pack_of`: a library
/// group, or the longest mark it starts with, or its start before ` - ` (a port writes
/// `<pack> - <action>`).
pub fn marked_pack<'a>(id: &'a str, sources: &'a Sources) -> &'a str {
    let group = pack_of(id);
    if !id.starts_with(training_set::LIBRARY) {
        return group;
    }
    sources
        .0
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
    pub mine: Count,
    /// Unmarked ones included.
    pub purchased: Count,
    /// Unmarked takes of the training sets (`<set>/<take>`): their licences are the registry's,
    /// and `ac_train` refuses a non-commercial one in a public set.
    pub free: Count,
    /// Of `purchased`, those whose pack isn't marked.
    pub unmarked: Count,
    pub rejected: usize,
    /// Clips kept that aren't here (a library not given): counted, with no minutes.
    pub missing: usize,
    /// Every pack the set holds: `mine`, `purchased` or `unmarked`.
    pub packs: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

/// `length`: a clip's frames and frame rate, if it is here.
pub fn totals(set: &TrainingSet, sources: &Sources, length: impl Fn(&str) -> Option<(usize, f64)>) -> Totals {
    let mut out = Totals::default();
    let (mut held_purchased, mut held_unmarked) = (0, 0);
    for (id, c) in &set.clips {
        let pack = marked_pack(id, sources);
        let source = sources.0.get(pack).copied();
        let free = source.is_none() && !id.starts_with(training_set::LIBRARY);
        let word = match source {
            Some(Source::Mine) => "mine",
            Some(Source::Purchased) => "purchased",
            None if free => "free",
            None => "unmarked",
        };
        out.packs.insert(pack.to_string(), word.to_string());
        match source {
            Some(Source::Mine) => {}
            Some(Source::Purchased) => held_purchased += 1,
            None if free => {}
            None => held_unmarked += 1,
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
        match source {
            Some(Source::Mine) => out.mine.add(minutes),
            Some(Source::Purchased) => out.purchased.add(minutes),
            None if free => out.free.add(minutes),
            None => {
                out.purchased.add(minutes);
                out.unmarked.add(minutes);
            }
        }
    }
    if set.public && held_purchased + held_unmarked > 0 {
        out.warnings.push(format!(
            "this public set holds {held_purchased} purchased and {held_unmarked} unmarked clips: only clips marked mine or free may be public"
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

    fn sources() -> Sources {
        Sources(
            [
                ("G".to_string(), Source::Mine),
                ("cmu".to_string(), Source::Purchased),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn near(a: f64, seconds: f64) {
        assert!(
            (a - seconds / 60.0).abs() < 1e-12,
            "{a} minutes, wanted {seconds} s"
        );
    }

    #[test]
    fn a_library_clip_takes_the_longest_mark_or_its_start() {
        let mut marks = sources();
        marks.0.insert("G - extra".to_string(), Source::Purchased);
        assert_eq!(marked_pack("library:G - extra - walk/a", &marks), "G - extra");
        assert_eq!(marked_pack("library:G - run/a", &marks), "G");
        assert_eq!(marked_pack("library:other - run/a", &marks), "other");
        assert_eq!(marked_pack("G - run/a", &marks), "G - run");
    }

    #[test]
    fn an_unmarked_rig_take_is_free_and_an_unmarked_library_clip_is_not() {
        let mut set = TrainingSet::new("t", false).unwrap();
        for id in ["lafan/a", "library:other/a"] {
            set.clips.insert(id.to_string(), SetClip::new(id));
        }
        let t = totals(&set, &sources(), |_| Some((30, 60.0)));
        assert_eq!(t.free.clips, 1);
        assert_eq!(t.unmarked.clips, 1);
        assert_eq!(t.purchased.clips, 1);
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
        let t = totals(&s, &sources(), length);
        // walk: a 1 s + b 1 s + gone 0 s, 3 clips
        assert_eq!(t.labels["walk"].clips, 3);
        near(t.labels["walk"].minutes, 2.0);
        near(t.labels["tired"].minutes, 1.0);
        near(t.labels["run"].minutes, 10.0);
        near(t.labels[NO_LABEL].minutes, 2.0);
        assert!(!t.labels.contains_key("hop"));
        // mine: G's a and b, 2 s
        assert_eq!(t.mine.clips, 2);
        near(t.mine.minutes, 2.0);
        // purchased: cmu's e 10 s, unmarked H's d 2 s and Z's gone 0 s
        assert_eq!(t.purchased.clips, 3);
        near(t.purchased.minutes, 12.0);
        assert_eq!(t.unmarked.clips, 2);
        near(t.unmarked.minutes, 2.0);
        assert_eq!((t.rejected, t.missing), (1, 1));
        assert_eq!(t.packs["G"], "mine");
        assert_eq!(t.packs["cmu"], "purchased");
        assert_eq!(t.packs["H"], "unmarked");
        assert!(t.warnings.is_empty(), "a private set may hold anything");
    }

    #[test]
    fn a_public_set_warns_of_a_purchased_or_an_unmarked_clip_only() {
        let warned = |id: &str, status| {
            let mut s = TrainingSet::new("x", true).unwrap();
            s.clips.extend([clip(id, status, &[], &[])]);
            totals(&s, &sources(), length).warnings
        };
        let w = warned("cmu/e", ClipStatus::Keep);
        assert!(w[0].contains("1 purchased and 0 unmarked"), "{w:?}");
        let w = warned("library:H/d", ClipStatus::Keep);
        assert!(w[0].contains("0 purchased and 1 unmarked"), "{w:?}");
        // its file names it even when rejected
        assert_eq!(warned("cmu/e", ClipStatus::Reject).len(), 1);
        assert!(warned("library:G/a", ClipStatus::Keep).is_empty());
    }

    #[test]
    fn the_sources_file_stays_out_of_a_checkout_and_says_when_it_is_bad() {
        let dir = std::env::temp_dir().join(format!("ac-review-sources-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (out, repo) = (dir.join("out"), dir.join("repo"));
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(Sources::load(&out).unwrap(), Sources::default());
        sources().save(&out).unwrap();
        assert_eq!(Sources::load(&out).unwrap(), sources());
        assert!(sources().save(&repo).unwrap_err().contains("names packs"));
        assert!(!repo.join(FILE).exists());
        std::fs::write(out.join(FILE), r#"{"G": "stolen"}"#).unwrap();
        assert!(Sources::load(&out).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
