//! Finding things in the library: `library.find` (search words, where an item comes from, its
//! group, length, looping, part and star, and an order) and `library.favourite` (star an item).
//!
//! The filter is [`find`] on the items `library.list` returns, so the window filters its own copy
//! of the list with the same rules the command uses. Stars are kept per library in
//! `<library>/favourites.json` (`{animations: [ids], poses: [ids]}`); a star follows its item when
//! it is renamed or moved to another group ([`rename_favourite`], called by
//! `Library::move_item`), and an item deleted keeps its star in the file, so an undo brings both
//! back.

use std::cmp::Ordering;
use std::path::Path;

use ac_core::Document;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::library::{Kind, Library, natural_cmp};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// The stars, in the library folder.
pub const FAVOURITES_FILE: &str = "favourites.json";
/// Animations up to this long (seconds) are short; longer than [`LONG`] are long.
pub const SHORT: f64 = 2.0;
pub const LONG: f64 = 6.0;
/// Longest search text read (characters).
const MAX_QUERY: usize = 200;

const KIND: ParamSpec = ParamSpec::new(
    "kind",
    K::Choice(&["animations", "poses"]),
    "animations (default) or poses",
);

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("library.find", "Find in library", library_find)
        .icon("search")
        .inputs(&[
            KIND,
            ParamSpec::new(
                "query",
                K::Text,
                "words that must all be in the name, group or part (any case)",
            ),
            ParamSpec::new(
                "source",
                K::Choice(&["all", "mine", "builtin"]),
                "all (default), your own, or the built-ins",
            ),
            ParamSpec::new("group", K::Text, "only this group"),
            ParamSpec::new(
                "length",
                K::Choice(&["short", "medium", "long"]),
                "animations only: short (under 2 s), medium (2 to 6 s) or long (over 6 s)",
            ),
            ParamSpec::new(
                "loop",
                K::Bool,
                "animations only: true for loops only, false for the others",
            ),
            ParamSpec::new("edited", K::Bool, "true: only built-ins changed in your library"),
            ParamSpec::new(
                "part",
                K::Text,
                "poses only: this body part (Whole body, Left arm, ...)",
            ),
            ParamSpec::new("favourites", K::Bool, "true: only starred items"),
            ParamSpec::new(
                "sort",
                K::Choice(&["name", "length", "modified"]),
                "name (by group, the default), length (shortest first) or modified (newest first)",
            ),
        ])
        .outputs(crate::returns::LIBRARY_FIND),
    CommandSpec::new("library.favourite", "Star in library", library_favourite)
        .icon("star")
        .inputs(&[
            ParamSpec::new("id", K::Text, "a library id, <group>/<name>").required(),
            KIND,
            ParamSpec::new("on", K::Bool, "star it (default true) or take the star off"),
        ])
        .writes()
        .outputs(crate::returns::LIBRARY_FAVOURITE),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Source {
    #[default]
    All,
    Mine,
    Builtin,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Length {
    Short,
    Medium,
    Long,
}

impl Length {
    /// The length band of an animation `seconds` long.
    pub fn of(seconds: f64) -> Length {
        if seconds < SHORT {
            Length::Short
        } else if seconds <= LONG {
            Length::Medium
        } else {
            Length::Long
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sort {
    #[default]
    Name,
    Length,
    Modified,
}

/// What to find. The default finds everything, by group.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Query {
    pub words: String,
    pub source: Source,
    pub group: Option<String>,
    pub length: Option<Length>,
    pub looping: Option<bool>,
    pub edited: bool,
    pub part: Option<String>,
    pub favourites: bool,
    pub sort: Sort,
}

impl Query {
    /// Something beyond the source and the order narrows the list.
    pub fn narrows(&self) -> bool {
        !self.words.trim().is_empty()
            || self.group.is_some()
            || self.length.is_some()
            || self.looping.is_some()
            || self.edited
            || self.part.is_some()
            || self.favourites
    }
}

fn flag(it: &Value, k: &str) -> bool {
    it.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn text<'a>(it: &'a Value, k: &str) -> &'a str {
    it.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn seconds(it: &Value) -> f64 {
    it.get("seconds")
        .and_then(Value::as_f64)
        .filter(|s| s.is_finite())
        .unwrap_or(0.0)
}

/// Whether a `library.list` item is one the query finds.
pub fn matches(it: &Value, q: &Query) -> bool {
    let source = match q.source {
        Source::All => true,
        Source::Mine => !flag(it, "builtin"),
        Source::Builtin => flag(it, "builtin"),
    };
    let fields = [text(it, "name"), text(it, "group"), text(it, "part")].map(str::to_lowercase);
    let words = q.words.chars().take(MAX_QUERY).collect::<String>().to_lowercase();
    source
        && words
            .split_whitespace()
            .all(|w| fields.iter().any(|f| f.contains(w)))
        && q.group.as_deref().is_none_or(|g| text(it, "group") == g)
        && q.length
            .is_none_or(|l| it.get("seconds").is_some() && Length::of(seconds(it)) == l)
        && q.looping.is_none_or(|l| flag(it, "loop") == l)
        && (!q.edited || flag(it, "edited"))
        && q.part
            .as_deref()
            .is_none_or(|p| text(it, "part").eq_ignore_ascii_case(p))
        && (!q.favourites || flag(it, "favourite"))
}

/// The items the query finds, in its order (name: as listed, by group then name).
pub fn find(items: &[Value], q: &Query) -> Vec<Value> {
    let mut out: Vec<Value> = items.iter().filter(|it| matches(it, q)).cloned().collect();
    let by_name = |a: &Value, b: &Value| {
        natural_cmp(text(a, "group"), text(b, "group"))
            .then_with(|| natural_cmp(text(a, "name"), text(b, "name")))
    };
    match q.sort {
        Sort::Name => out.sort_by(by_name),
        Sort::Length => out.sort_by(|a, b| {
            seconds(a)
                .partial_cmp(&seconds(b))
                .unwrap_or(Ordering::Equal)
                .then_with(|| by_name(a, b))
        }),
        Sort::Modified => out.sort_by(|a, b| {
            let m = |v: &Value| v.get("modified").and_then(Value::as_u64).unwrap_or(0);
            m(b).cmp(&m(a)).then_with(|| by_name(a, b))
        }),
    }
    out
}

/// Each group of `items` with how many of them are in it, in the order they first appear.
pub fn group_counts(items: &[Value]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for it in items {
        let g = text(it, "group");
        match out.iter_mut().find(|(n, _)| n == g) {
            Some((_, c)) => *c += 1,
            None => out.push((g.to_string(), 1)),
        }
    }
    out
}

/// The query a command's parameters ask for; refuses filters for the other kind.
pub fn query_of(p: &P, kind: Kind) -> Result<Query, String> {
    let poses = kind == Kind::Poses;
    for (k, wrong) in [("length", poses), ("loop", poses), ("part", !poses)] {
        if wrong && p.get(k).is_some_and(|v| !v.is_null()) {
            let other = if poses { "animations" } else { "poses" };
            return Err(format!("{k} is for {other} only"));
        }
    }
    let nonempty = |k: &str| {
        cmd::text(p, k)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Ok(Query {
        words: cmd::text(p, "query").unwrap_or_default().to_string(),
        source: match cmd::text(p, "source") {
            Some("mine") => Source::Mine,
            Some("builtin") => Source::Builtin,
            _ => Source::All,
        },
        group: nonempty("group"),
        length: match cmd::text(p, "length") {
            Some("short") => Some(Length::Short),
            Some("medium") => Some(Length::Medium),
            Some("long") => Some(Length::Long),
            _ => None,
        },
        looping: p.get("loop").and_then(Value::as_bool),
        edited: p.get("edited").and_then(Value::as_bool).unwrap_or(false),
        part: nonempty("part"),
        favourites: p.get("favourites").and_then(Value::as_bool).unwrap_or(false),
        sort: match cmd::text(p, "sort") {
            Some("length") => Sort::Length,
            Some("modified") => Sort::Modified,
            _ => Sort::Name,
        },
    })
}

fn kind_of(p: &P) -> Kind {
    match cmd::text(p, "kind") {
        Some("poses") => Kind::Poses,
        _ => Kind::Animations,
    }
}

fn library_find(doc: &mut Document, p: &P) -> Out {
    let kind = kind_of(p);
    let q = query_of(p, kind)?;
    let list = crate::commands::library_list(doc, &P::new())?;
    let items: Vec<Value> = list
        .get(kind.folder())
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let found = find(&items, &q);
    let groups: Vec<Value> = group_counts(&found)
        .into_iter()
        .map(|(g, n)| json!({ "group": g, "count": n }))
        .collect();
    Ok(json!({ "count": found.len(), "items": found, "groups": groups }))
}

/// The starred ids, by kind.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Favourites {
    pub animations: Vec<String>,
    pub poses: Vec<String>,
}

impl Favourites {
    fn of(&mut self, kind: Kind) -> &mut Vec<String> {
        match kind {
            Kind::Animations => &mut self.animations,
            Kind::Poses => &mut self.poses,
        }
    }

    pub fn has(&self, kind: Kind, id: &str) -> bool {
        let ids = match kind {
            Kind::Animations => &self.animations,
            Kind::Poses => &self.poses,
        };
        ids.iter().any(|i| i == id)
    }
}

/// The library's stars (no file yet: none).
pub fn load_favourites(root: &Path) -> Result<Favourites, String> {
    let path = root.join(FAVOURITES_FILE);
    if !path.exists() {
        return Ok(Favourites::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn save_favourites(root: &Path, f: &Favourites) -> Result<(), String> {
    let path = root.join(FAVOURITES_FILE);
    let text = serde_json::to_string_pretty(f).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Move a star from `old` to `new` (an item renamed or regrouped); nothing when it had none.
pub fn rename_favourite(lib: &Library, old: &str, new: &str) -> Result<(), String> {
    let Some(root) = lib.root.parent() else {
        return Ok(());
    };
    if !root.join(FAVOURITES_FILE).exists() {
        return Ok(());
    }
    let mut f = load_favourites(root)?;
    let ids = f.of(lib.kind);
    if !ids.iter().any(|i| i == old) {
        return Ok(());
    }
    for i in ids.iter_mut() {
        if i == old {
            *i = new.to_string();
        }
    }
    ids.dedup();
    save_favourites(root, &f)
}

/// Mark each `library.list` item of `kind` with `favourite`.
pub fn mark_favourites(f: &Favourites, kind: Kind, items: &mut [Value]) {
    for it in items {
        let on = it
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| f.has(kind, id));
        it["favourite"] = json!(on);
    }
}

fn library_favourite(doc: &mut Document, p: &P) -> Out {
    let kind = kind_of(p);
    let id = cmd::text(p, "id").unwrap_or_default();
    let lib = Library::new(crate::commands::root(doc)?, kind);
    if !lib.exists(id) {
        return Err(format!("no {} {id:?} in the library", kind.folder()));
    }
    let on = p.get("on").and_then(Value::as_bool).unwrap_or(true);
    let root = crate::commands::root(doc)?.to_path_buf();
    let mut f = load_favourites(&root)?;
    let ids = f.of(kind);
    ids.retain(|i| i != id);
    if on {
        ids.push(id.to_string());
    }
    save_favourites(&root, &f)?;
    Ok(json!({ "id": id, "favourite": on }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn items() -> Vec<Value> {
        vec![
            json!({"id": "Falls/Fall and get up 1", "name": "Fall and get up 1", "group": "Falls", "seconds": 7.5, "loop": false, "builtin": true, "modified": 5}),
            json!({"id": "Locomotion/Walk 10", "name": "Walk 10", "group": "Locomotion", "seconds": 1.0, "loop": true, "builtin": true, "edited": true, "modified": 9}),
            json!({"id": "Locomotion/Walk 2", "name": "Walk 2", "group": "Locomotion", "seconds": 2.0, "loop": true, "builtin": false, "favourite": true, "modified": 1}),
        ]
    }

    fn ids(v: &[Value]) -> Vec<&str> {
        v.iter().map(|i| text(i, "id")).collect()
    }

    #[test]
    fn every_word_must_match_somewhere() {
        let q = Query {
            words: "fall GET".into(),
            ..Query::default()
        };
        assert_eq!(ids(&find(&items(), &q)), ["Falls/Fall and get up 1"]);
        let q = Query {
            words: "walk falls".into(),
            ..Query::default()
        };
        assert!(find(&items(), &q).is_empty());
        // a word may match the group
        let q = Query {
            words: "locomotion 2".into(),
            ..Query::default()
        };
        assert_eq!(ids(&find(&items(), &q)), ["Locomotion/Walk 2"]);
    }

    #[test]
    fn length_bands_include_their_edges_in_medium() {
        assert_eq!(Length::of(1.99), Length::Short);
        assert_eq!(Length::of(2.0), Length::Medium);
        assert_eq!(Length::of(6.0), Length::Medium);
        assert_eq!(Length::of(6.01), Length::Long);
    }

    #[test]
    fn filters_and_orders() {
        let all = items();
        let mine = Query {
            source: Source::Mine,
            ..Query::default()
        };
        assert_eq!(ids(&find(&all, &mine)), ["Locomotion/Walk 2"]);
        let q = Query {
            looping: Some(true),
            sort: Sort::Name,
            ..Query::default()
        };
        // natural order: 2 before 10
        assert_eq!(ids(&find(&all, &q)), ["Locomotion/Walk 2", "Locomotion/Walk 10"]);
        let q = Query {
            sort: Sort::Length,
            ..Query::default()
        };
        assert_eq!(ids(&find(&all, &q))[0], "Locomotion/Walk 10");
        let q = Query {
            sort: Sort::Modified,
            ..Query::default()
        };
        assert_eq!(ids(&find(&all, &q))[0], "Locomotion/Walk 10");
        let q = Query {
            favourites: true,
            ..Query::default()
        };
        assert_eq!(ids(&find(&all, &q)), ["Locomotion/Walk 2"]);
        let q = Query {
            edited: true,
            ..Query::default()
        };
        assert_eq!(ids(&find(&all, &q)), ["Locomotion/Walk 10"]);
        assert_eq!(
            group_counts(&find(&all, &Query::default())),
            [("Falls".to_string(), 1), ("Locomotion".to_string(), 2)]
        );
    }
}
