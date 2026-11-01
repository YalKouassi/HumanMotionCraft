//! Where the review window was: the library, the sets' folder and the set open, so a plain launch
//! comes back to them. The window's only; `ac-review run` reads just what it is given, so a script
//! does the same on any machine. The file holds private paths, so it lives beside the app's library,
//! outside any repository.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::training_set::TrainingSet;

type R<T> = Result<T, String>;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Remembered {
    pub library: Option<PathBuf>,
    pub sets: Option<PathBuf>,
    pub set: Option<String>,
}

/// What a launch asks for; what it leaves out, [`Remembered::restore`] may fill in.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Launch {
    pub library: Option<PathBuf>,
    pub sets: Option<PathBuf>,
    pub set: Option<String>,
}

impl Remembered {
    /// `review-session.json` beside the app's library.
    pub fn file() -> PathBuf {
        ac_io::library::app_library_dir().with_file_name("review-session.json")
    }

    /// Nothing remembered when there is no file yet; a file that won't read is an error to say.
    pub fn load(path: &Path) -> R<Remembered> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Remembered::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> R<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        craft_atomic::atomic_write(path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Fill in what `launch` leaves out; what it names wins. The set comes back only with its own
    /// folder. Whatever can't come back is left out and said, in the lines returned.
    /// `default_sets`: the folder a launch with no `--sets` and nothing remembered uses.
    pub fn restore(&self, launch: &mut Launch, default_sets: &Path) -> Vec<String> {
        let mut notes = Vec::new();
        if launch.library.is_none()
            && let Some(library) = &self.library
        {
            if library.is_dir() {
                launch.library = Some(library.clone());
            } else {
                notes.push(format!(
                    "the library you last used isn't there: {}",
                    library.display()
                ));
            }
        }
        let same_folder = launch.sets.is_none() || launch.sets == self.sets;
        let mut folder_gone = false;
        if launch.sets.is_none()
            && let Some(sets) = &self.sets
        {
            if sets.is_dir() {
                launch.sets = Some(sets.clone());
            } else {
                folder_gone = true;
                notes.push(format!(
                    "the sets' folder you last used isn't there: {}",
                    sets.display()
                ));
            }
        }
        if launch.set.is_none()
            && same_folder
            && !folder_gone
            && let Some(name) = &self.set
        {
            let folder = launch.sets.as_deref().unwrap_or(default_sets);
            match TrainingSet::load(folder, name) {
                Ok(_) => launch.set = Some(name.clone()),
                Err(e) => notes.push(format!("the set you last had open, {name}, won't open: {e}")),
            }
        }
        notes
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ac-review-remembered-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A library and a sets' folder holding the set `walk`.
    fn places(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let d = tmp(name);
        let (library, sets) = (d.join("library"), d.join("sets"));
        std::fs::create_dir_all(&library).unwrap();
        TrainingSet::new("walk", true).unwrap().save(&sets).unwrap();
        (d, library, sets)
    }

    #[test]
    fn a_plain_launch_comes_back() {
        let (d, library, sets) = places("plain");
        let was = Remembered {
            library: Some(library.clone()),
            sets: Some(sets.clone()),
            set: Some("walk".into()),
        };
        let mut launch = Launch::default();
        assert!(was.restore(&mut launch, &d.join("default")).is_empty());
        assert_eq!(
            launch,
            Launch {
                library: Some(library),
                sets: Some(sets),
                set: Some("walk".into())
            }
        );
    }

    #[test]
    fn what_a_launch_names_wins() {
        let (d, library, sets) = places("named");
        let other = d.join("other");
        TrainingSet::new("run", true).unwrap().save(&other).unwrap();
        let was = Remembered {
            library: Some(library),
            sets: Some(sets),
            set: Some("walk".into()),
        };
        let named = d.join("named-library");
        let mut launch = Launch {
            library: Some(named.clone()),
            sets: Some(other.clone()),
            set: Some("run".into()),
        };
        assert!(was.restore(&mut launch, &d.join("default")).is_empty());
        assert_eq!(
            launch,
            Launch {
                library: Some(named),
                sets: Some(other),
                set: Some("run".into())
            }
        );
    }

    #[test]
    fn another_folder_does_not_get_the_remembered_set() {
        let (d, _, sets) = places("other");
        let other = d.join("other");
        TrainingSet::new("walk", true).unwrap().save(&other).unwrap();
        let was = Remembered {
            sets: Some(sets),
            set: Some("walk".into()),
            ..Default::default()
        };
        let mut launch = Launch {
            sets: Some(other),
            ..Default::default()
        };
        assert!(was.restore(&mut launch, &d.join("default")).is_empty());
        assert_eq!(launch.set, None);
    }

    #[test]
    fn what_has_gone_is_said() {
        let (d, library, sets) = places("gone");
        let was = Remembered {
            library: Some(d.join("no-library")),
            sets: Some(d.join("no-sets")),
            set: Some("walk".into()),
        };
        let mut launch = Launch::default();
        let notes = was.restore(&mut launch, &d.join("default"));
        assert_eq!(launch, Launch::default());
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(
            notes[0].contains("library") && notes[1].contains("sets' folder"),
            "{notes:?}"
        );

        let was = Remembered {
            library: Some(library),
            sets: Some(sets),
            set: Some("deleted".into()),
        };
        let mut launch = Launch::default();
        let notes = was.restore(&mut launch, &d.join("default"));
        assert_eq!(launch.set, None);
        assert!(launch.sets.is_some());
        assert!(notes.len() == 1 && notes[0].contains("deleted"), "{notes:?}");
    }

    #[test]
    fn it_saves_and_reads_back() {
        let d = tmp("file");
        let path = d.join("deeper").join("review-session.json");
        assert_eq!(Remembered::load(&path).unwrap(), Remembered::default());
        let was = Remembered {
            library: Some(d.join("l")),
            sets: Some(d.join("s")),
            set: Some("walk".into()),
        };
        was.save(&path).unwrap();
        assert_eq!(Remembered::load(&path).unwrap(), was);
        std::fs::write(&path, "not json").unwrap();
        assert!(
            Remembered::load(&path)
                .unwrap_err()
                .contains("review-session.json")
        );
    }
}
