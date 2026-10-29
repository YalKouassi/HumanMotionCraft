//! What an edit changed, compactly: the `changed` summary every undoable command's result carries,
//! and what `edit.dry_run` and `edit.batch` report.
//!
//! Only what changed appears (nothing changed: `{}`):
//! - `keys`: per layer (by name), the key frames `added`, `removed`, `moved` (`[from, to]`: a key
//!   that left one frame and arrived unchanged on another) and `edited` (same frame, new content);
//! - `frames`: `[first, last]` frame any key change touched;
//! - `layers`: layer names `added`, `removed`, `edited` (its settings, not its keys) and
//!   `reordered` (true when the stack's order changed);
//! - `pose`: the pose shown, `bones` it turned (at most [`MAX_NAMES`] names, then `more`) and
//!   whether the `root` moved;
//! - `settings`: the other document fields that changed (`frame`, `loop`, `fps`, `physics`,
//!   `name`, `group`, `surfaces`, `pins`, `locked`, `parts`, `clipboard`, `clip_id`, `links`,
//!   `edit_layer`, `ai_settings`, `ai_preview`).

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::clip::{Key, Layer};
use crate::document::Document;

/// Most bone names listed in `pose.bones`.
pub const MAX_NAMES: usize = 12;

/// The summary of `before` → `after` (see the module doc).
pub fn summary(before: &Document, after: &Document) -> Value {
    let mut out = Map::new();
    let (keys, span) = key_changes(&before.clip.layers, &after.clip.layers);
    if !keys.is_empty() {
        out.insert("keys".into(), Value::Array(keys));
    }
    if let Some((a, b)) = span {
        out.insert("frames".into(), json!([a, b]));
    }
    let layers = layer_changes(&before.clip.layers, &after.clip.layers);
    if !layers.is_empty() {
        out.insert("layers".into(), Value::Object(layers));
    }
    if let Some(p) = pose_changes(before, after) {
        out.insert("pose".into(), p);
    }
    let settings = setting_changes(before, after);
    if !settings.is_empty() {
        out.insert("settings".into(), json!(settings));
    }
    Value::Object(out)
}

/// Whether the summary says nothing changed.
pub fn is_empty(summary: &Value) -> bool {
    summary.as_object().is_none_or(Map::is_empty)
}

/// Layers paired by name (the first of each name; a renamed layer reads as removed + added).
fn pairs<'a>(
    before: &'a [Layer],
    after: &'a [Layer],
) -> Vec<(&'a str, Option<&'a Layer>, Option<&'a Layer>)> {
    let mut names: Vec<&str> = Vec::new();
    for l in before.iter().chain(after) {
        if !names.contains(&l.name.as_str()) {
            names.push(&l.name);
        }
    }
    names
        .into_iter()
        .map(|n| {
            (
                n,
                before.iter().find(|l| l.name == n),
                after.iter().find(|l| l.name == n),
            )
        })
        .collect()
}

fn key_changes(before: &[Layer], after: &[Layer]) -> (Vec<Value>, Option<(i64, i64)>) {
    let mut out = Vec::new();
    let mut span: Option<(i64, i64)> = None;
    let mut touch = |f: i64| {
        span = Some(span.map_or((f, f), |(a, b)| (a.min(f), b.max(f))));
    };
    for (name, b, a) in pairs(before, after) {
        let (Some(b), Some(a)) = (b, a) else {
            continue;
        };
        if b.keys == a.keys {
            continue;
        }
        let old: BTreeMap<i64, &Key> = b.keys.iter().map(|k| (k.frame, k)).collect();
        let new: BTreeMap<i64, &Key> = a.keys.iter().map(|k| (k.frame, k)).collect();
        let mut added: Vec<i64> = new.keys().filter(|f| !old.contains_key(f)).copied().collect();
        let mut removed: Vec<i64> = old.keys().filter(|f| !new.contains_key(f)).copied().collect();
        let edited: Vec<i64> = new
            .iter()
            .filter(|(f, k)| old.get(f).is_some_and(|o| o != *k))
            .map(|(f, _)| *f)
            .collect();
        // a key that vanished from one frame and appeared unchanged on another moved
        let mut moved = Vec::new();
        removed.retain(|r| {
            let Some(k) = old.get(r) else { return true };
            match added
                .iter()
                .position(|f| new.get(f).is_some_and(|n| same_but_frame(k, n)))
            {
                Some(i) => {
                    moved.push([*r, added.remove(i)]);
                    false
                }
                None => true,
            }
        });
        for f in added
            .iter()
            .chain(&removed)
            .chain(&edited)
            .chain(moved.iter().flatten())
        {
            touch(*f);
        }
        let mut m = Map::new();
        m.insert("layer".into(), json!(name));
        for (k, v) in [("added", &added), ("removed", &removed), ("edited", &edited)] {
            if !v.is_empty() {
                m.insert(k.into(), json!(v));
            }
        }
        if !moved.is_empty() {
            m.insert("moved".into(), json!(moved));
        }
        out.push(Value::Object(m));
    }
    (out, span)
}

fn same_but_frame(a: &Key, b: &Key) -> bool {
    a.rotations == b.rotations
        && a.root == b.root
        && a.interp == b.interp
        && a.source == b.source
        && a.model == b.model
        && a.only == b.only
}

fn layer_changes(before: &[Layer], after: &[Layer]) -> Map<String, Value> {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut edited = Vec::new();
    for (name, b, a) in pairs(before, after) {
        match (b, a) {
            (None, Some(_)) => added.push(name),
            (Some(_), None) => removed.push(name),
            (Some(b), Some(a)) => {
                let mut b = b.clone();
                b.keys.clone_from(&a.keys);
                if b != *a {
                    edited.push(name);
                }
            }
            (None, None) => {}
        }
    }
    let mut out = Map::new();
    for (k, v) in [("added", added), ("removed", removed), ("edited", edited)] {
        if !v.is_empty() {
            out.insert(k.into(), json!(v));
        }
    }
    let order = |ls: &[Layer]| -> Vec<String> { ls.iter().map(|l| l.name.clone()).collect() };
    let (ob, oa) = (order(before), order(after));
    let kept_b: Vec<&String> = ob.iter().filter(|n| oa.contains(n)).collect();
    let kept_a: Vec<&String> = oa.iter().filter(|n| ob.contains(n)).collect();
    if kept_b != kept_a {
        out.insert("reordered".into(), json!(true));
    }
    out
}

fn pose_changes(before: &Document, after: &Document) -> Option<Value> {
    if before.pose == after.pose {
        return None;
    }
    let names = &after.skeleton.names;
    let turned: Vec<&str> = before
        .pose
        .rotations
        .iter()
        .zip(&after.pose.rotations)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .filter_map(|(i, _)| names.get(i).map(String::as_str))
        .collect();
    let mut m = Map::new();
    m.insert(
        "bones".into(),
        json!(turned.iter().take(MAX_NAMES).collect::<Vec<_>>()),
    );
    if turned.len() > MAX_NAMES {
        m.insert("more".into(), json!(turned.len() - MAX_NAMES));
    }
    m.insert("root".into(), json!(before.pose.root != after.pose.root));
    Some(Value::Object(m))
}

fn setting_changes(b: &Document, a: &Document) -> Vec<&'static str> {
    let (cb, ca) = (&b.clip, &a.clip);
    [
        ("frame", b.frame != a.frame),
        ("loop", cb.loop_ != ca.loop_),
        ("fps", cb.fps != ca.fps),
        ("physics", cb.physics != ca.physics),
        ("name", cb.name != ca.name),
        ("group", cb.group != ca.group),
        ("surfaces", cb.surfaces != ca.surfaces),
        ("pins", b.pins != a.pins),
        ("locked", b.locked != a.locked),
        ("parts", b.custom_parts != a.custom_parts),
        ("clipboard", b.clipboard != a.clipboard),
        ("clip_id", b.clip_id != a.clip_id),
        ("links", b.links != a.links),
        ("edit_layer", cb.edit != ca.edit),
        ("ai_settings", b.ai_settings != a.ai_settings),
        (
            "ai_preview",
            match (&b.ai_preview, &a.ai_preview) {
                (None, None) => false,
                (Some(x), Some(y)) => !std::sync::Arc::ptr_eq(x, y),
                _ => true,
            },
        ),
    ]
    .into_iter()
    .filter(|(_, changed)| *changed)
    .map(|(n, _)| n)
    .collect()
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::clip::Interp;
    use crate::rig::Skeleton;

    fn doc() -> Document {
        let sk = Skeleton::new(
            vec!["root".into(), "spine".into()],
            vec![None, Some(0)],
            vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.2]],
            1.0,
        );
        match sk {
            Ok(sk) => Document::new(sk),
            Err(e) => panic!("{e}"),
        }
    }

    fn key(f: i64, z: f64) -> Key {
        Key::new(f, vec![crate::quat::IDENTITY; 2], [0.0, 0.0, z], Interp::Linear)
    }

    #[test]
    fn nothing_changed_is_empty() {
        let d = doc();
        assert!(is_empty(&summary(&d, &d.clone())));
    }

    #[test]
    fn keys_added_moved_edited_and_frames() {
        let mut b = doc();
        let _ = b.clip.set_key(key(0, 0.0));
        let _ = b.clip.set_key(key(10, 0.5));
        let _ = b.clip.set_key(key(20, 1.0));
        let mut a = b.clone();
        a.clip.layers[0].keys = vec![key(0, 0.1), key(15, 0.5), key(20, 1.0), key(30, 2.0)];
        let s = summary(&b, &a);
        assert_eq!(
            s["keys"],
            json!([{ "layer": "Base", "added": [30], "edited": [0], "moved": [[10, 15]] }])
        );
        assert_eq!(s["frames"], json!([0, 30]));
        assert!(s.get("layers").is_none() && s.get("pose").is_none());
    }

    #[test]
    fn layers_pose_and_settings() {
        let b = doc();
        let mut a = b.clone();
        a.clip.layers.push(Layer::keys("Top", Vec::new()));
        a.clip.layers[0].weight = 0.5;
        a.pose.rotations[1] = [0.0, 0.0, 0.6, 0.8];
        a.frame = 4.0;
        a.clip.loop_ = true;
        let s = summary(&b, &a);
        assert_eq!(s["layers"], json!({ "added": ["Top"], "edited": ["Base"] }));
        assert_eq!(s["pose"], json!({ "bones": ["spine"], "root": false }));
        assert_eq!(s["settings"], json!(["frame", "loop"]));
    }
}
