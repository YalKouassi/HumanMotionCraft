//! Groups of clips: the takes of a pack that differ only by a trailing number are one row, kept or
//! rejected together (`review.set_group`, one undo step); a single clip can then be overridden.

use std::collections::BTreeMap;

use crate::review::Review;
use crate::training_set::{ClipStatus, LIBRARY};

/// A take's name without its trailing number: `walk_cmu_01_02` -> `walk_cmu`, `Run (2)` -> `Run`.
/// A name of nothing but numbers stands for itself.
pub fn stem(name: &str) -> &str {
    let s = name.trim_end_matches(|c: char| c.is_ascii_digit() || "_- .()[]".contains(c));
    if s.is_empty() { name } else { s }
}

/// The pack a set's or a clip's name belongs to: `library:<group>` and `<set>` alike.
pub fn pack(set: &str) -> &str {
    set.strip_prefix(LIBRARY).unwrap_or(set)
}

/// The key of a take's group.
pub fn group_key(set: &str, take: &str) -> String {
    format!("{set}/{}", group_name(set, take))
}

/// A group's name: a library clip's `<source group> - <action>` goes by its source group, so a
/// pack's zombie or sword clips are one row. Anything else goes by its stem.
pub fn group_name<'a>(set: &str, take: &'a str) -> &'a str {
    match take.split_once(" - ") {
        Some((source, _)) if set.starts_with(LIBRARY) => stem(source),
        _ => stem(take),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// `<set>/<stem>`.
    pub key: String,
    pub pack: String,
    pub name: String,
    pub takes: Vec<(usize, usize)>,
    pub minutes: f64,
    /// In the training set open.
    pub keep: usize,
    pub reject: usize,
}

impl Review {
    /// The groups of these takes, by key.
    pub fn groups(&self, takes: &[(usize, usize)]) -> Vec<Group> {
        let mut out: BTreeMap<String, Group> = BTreeMap::new();
        for &(s, t) in takes {
            let Some(set) = self.sets.get(s) else { continue };
            let Some(take) = set.takes.get(t) else { continue };
            let key = group_key(&set.name, &take.name);
            let g = out.entry(key.clone()).or_insert_with(|| Group {
                key,
                pack: pack(&set.name).to_string(),
                name: group_name(&set.name, &take.name).to_string(),
                takes: Vec::new(),
                minutes: 0.0,
                keep: 0,
                reject: 0,
            });
            g.takes.push((s, t));
            g.minutes += take.frames as f64 / take.fps / 60.0;
            let clip = self
                .training
                .active
                .as_ref()
                .and_then(|a| a.clips.get(&self.id(s, t)));
            match clip.map(|c| c.status) {
                Some(ClipStatus::Keep) => g.keep += 1,
                Some(ClipStatus::Reject) => g.reject += 1,
                None => {}
            }
        }
        out.into_values().collect()
    }

    /// Every take of a group, whatever the queue shows.
    pub fn group_takes(&self, key: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (si, s) in self.sets.iter().enumerate() {
            for (ti, t) in s.takes.iter().enumerate() {
                if group_key(&s.name, &t.name) == key {
                    out.push((si, ti));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trailing_number_is_not_part_of_the_name() {
        assert_eq!(stem("walk_cmu_01_02"), "walk_cmu");
        assert_eq!(stem("Run (2)"), "Run");
        assert_eq!(stem("hop"), "hop");
        assert_eq!(stem("hop2back"), "hop2back");
        assert_eq!(stem("0012"), "0012");
    }

    #[test]
    fn a_library_clip_groups_by_its_source_and_a_rig_take_by_its_stem() {
        assert_eq!(group_key("library:P", "Zombie - ZombieRun"), "library:P/Zombie");
        assert_eq!(
            group_key("library:P", "Zombie - ZombieRun (2)"),
            "library:P/Zombie"
        );
        assert_eq!(group_key("library:P", "Run (2)"), "library:P/Run");
        assert_eq!(group_key("cmu", "a - b_01"), "cmu/a - b");
    }

    #[test]
    fn a_library_group_and_a_set_name_their_pack_alike() {
        assert_eq!(pack("library:G"), "G");
        assert_eq!(pack("cmu"), "cmu");
    }
}
