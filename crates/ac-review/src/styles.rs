//! What each style means, from `training/review/styles.json`: what it should hold, what belongs
//! elsewhere, and the words that suggest a clip for it before anyone has labelled it. A suggestion
//! only puts a clip in front of a person; it is never a label.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const FILE: &str = include_str!("../../../training/review/styles.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Style {
    pub name: String,
    /// `base` or `add-on`.
    pub kind: String,
    pub about: String,
    pub includes: Vec<String>,
    pub leaves_out: Vec<String>,
    pub words: Vec<String>,
    pub not_words: Vec<String>,
}

#[derive(Deserialize)]
struct File {
    not_in_base: Vec<String>,
    styles: Vec<Style>,
}

fn file() -> &'static File {
    static PARSED: OnceLock<File> = OnceLock::new();
    PARSED.get_or_init(|| {
        serde_json::from_str(FILE).unwrap_or_else(|e| File {
            not_in_base: Vec::new(),
            styles: vec![Style {
                name: "styles.json".into(),
                kind: "base".into(),
                about: format!("styles.json won't read: {e}"),
                includes: Vec::new(),
                leaves_out: Vec::new(),
                words: Vec::new(),
                not_words: Vec::new(),
            }],
        })
    })
}

pub fn all() -> &'static [Style] {
    &file().styles
}

pub fn named(name: &str) -> Option<&'static Style> {
    all().iter().find(|s| s.name == name)
}

/// A name's words, lower case: `LurchWalk2Back_L` -> zombie, walk, root, l.
pub fn tokens(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut lower = false;
    for c in name.chars() {
        if !c.is_alphabetic() {
            out.push(std::mem::take(&mut word));
            lower = false;
            continue;
        }
        if c.is_uppercase() && lower {
            out.push(std::mem::take(&mut word));
        }
        lower = c.is_lowercase();
        word.extend(c.to_lowercase());
    }
    out.push(word);
    out.retain(|w| !w.is_empty());
    out
}

/// Whether a clip's name and description suggest it for this style: a word of the style starts a
/// word of the name, and no word against it does. A base style also leaves out props and acted
/// styles, wherever the name says them, the pack's name included.
pub fn suggests(style: &Style, text: &str) -> bool {
    let words = tokens(text);
    let has = |list: &[String]| {
        list.iter()
            .any(|w| words.iter().any(|t| t.starts_with(w.as_str())))
    };
    has(&style.words) && !has(&style.not_words) && !(style.kind == "base" && has(&file().not_in_base))
}

/// Whether a name says a prop or an acted style, so it is left out of the base and of `unsorted`.
pub fn names_a_prop(text: &str) -> bool {
    let words = tokens(text);
    file()
        .not_in_base
        .iter()
        .any(|w| words.iter().any(|t| t.starts_with(w.as_str())))
}

/// Keys 1 to 9 send a clip to these, in this order.
pub const SEND_TO: [&str; 9] = [
    "walk",
    "run",
    "idle",
    "jump",
    "crouch",
    "gesture",
    "climb",
    "zombie",
    "acrobatic",
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn style(name: &str) -> &'static Style {
        named(name).unwrap()
    }

    #[test]
    fn the_file_reads_and_every_style_says_what_it_holds() {
        let file: File = serde_json::from_str(FILE).unwrap();
        assert!(file.styles.len() >= 12);
        for s in &file.styles {
            assert!(
                matches!(s.kind.as_str(), "base" | "add-on" | "to-sort"),
                "{}",
                s.name
            );
            assert!(
                !s.about.is_empty() && !s.includes.is_empty() && !s.leaves_out.is_empty(),
                "{}",
                s.name
            );
            assert!(!s.words.is_empty() || s.kind == "to-sort", "{}", s.name);
        }
    }

    #[test]
    fn every_key_sends_to_a_style() {
        for name in SEND_TO {
            assert!(named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn names_split_into_words() {
        assert_eq!(tokens("LurchWalk2Back_L"), ["lurch", "walk", "back", "l"]);
        assert_eq!(tokens("Some_Group - FPS Run"), ["some", "group", "fps", "run"]);
    }

    #[test]
    fn suggestions_follow_the_descriptions() {
        assert!(suggests(style("walk"), "P - Moves/SideStep_Left"));
        assert!(
            !suggests(style("walk"), "P - Moves/CrouchWalk"),
            "a crouched walk is crouch"
        );
        assert!(suggests(style("crouch"), "P - Moves/CrouchWalk"));
        assert!(
            !suggests(style("walk"), "P - Sword Pack/Walk"),
            "a prop stays out of the base"
        );
        assert!(!suggests(style("walk"), "P - Moves/ZombieWalk"));
        assert!(suggests(style("zombie"), "P/ZombieStep2"));
        assert!(
            !suggests(style("zombie"), "P/ZombieEat"),
            "feeding isn't a way of moving"
        );
        assert!(suggests(style("acrobatic"), "cmu/flip_01 cartwheel"));
        assert!(!suggests(style("acrobatic"), "P - Roller/RollerFlip"));
        assert!(!suggests(style("acrobatic"), "P - Office/OfficePageFlip"));
    }
}
