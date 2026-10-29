//! Library and file commands. The library folder is `Document::library` (set by the front end);
//! paths and names are parameters, so nothing here ever opens a dialog.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ac_core::clip::{Clip, Layer, LayerSource};
use ac_core::clipfile::{self, PoseFile};
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use ac_core::{Document, Pose, parts};
use serde_json::{Map, Value, json};

use crate::library::{Kind, Library};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// How deep link layers may chain (a link to a clip that links to another, ...).
pub const MAX_LINK_DEPTH: usize = 8;
/// The user's part sets, in the library folder.
pub const PARTS_FILE: &str = "parts.json";

const ID: ParamSpec = ParamSpec::new("id", K::Text, "a library id, <group>/<name>");
const NAME: ParamSpec = ParamSpec::new("name", K::Text, "a name");
const GROUP: ParamSpec = ParamSpec::new("group", K::Text, "a group (folder)");
const PARTS: ParamSpec = ParamSpec::new("parts", K::TextList, "body parts (empty: the whole body)");
const PATH: ParamSpec = ParamSpec::new("path", K::Text, "a file path");

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("library.list", "Library", library_list)
        .icon("library")
        .outputs(crate::returns::LIBRARY_LIST),
    CommandSpec::new(
        "library.install",
        "Install built-ins",
        crate::import::library_install,
    )
    .icon("restore")
    .inputs(&[
        ParamSpec::new("from", K::Text, "the built-in library folder").required(),
        ParamSpec::new("overwrite", K::Bool, "replace your edited copies too"),
    ])
    .writes()
    .outputs(crate::returns::LIBRARY_INSTALL),
    CommandSpec::new("clip.new", "New animation", clip_new)
        .icon("new")
        .shortcut("Ctrl+N")
        .undoable()
        .inputs(&[
            NAME,
            GROUP,
            ParamSpec::new("fps", K::Number, "frames per second").range(1.0, 240.0),
            ParamSpec::new("pose", K::Text, "start from this pose (library id)"),
            ParamSpec::new("link", K::Text, "play this animation underneath (library id)"),
            PARTS,
        ])
        .writes()
        .outputs(crate::returns::CLIP_NEW),
    CommandSpec::new("clip.open", "Open animation", clip_open)
        .icon("open")
        .undoable()
        .inputs(&[ID.required()])
        .outputs(crate::returns::CLIP_OPEN),
    CommandSpec::new("clip.save", "Save", clip_save)
        .icon("save")
        .shortcut("Ctrl+S")
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("clip.save_as", "Save as", clip_save_as)
        .icon("save-as")
        .undoable()
        .inputs(&[NAME, GROUP])
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("clip.rename", "Rename animation", clip_rename)
        .icon("rename")
        .undoable()
        .inputs(&[
            ParamSpec::new("to", K::Text, "the new name").required(),
            ParamSpec::new(
                "id",
                K::Text,
                "a library animation to rename (default: the open one)",
            ),
        ])
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("clip.duplicate", "Duplicate animation", clip_duplicate)
        .icon("duplicate")
        .inputs(&[ID])
        .writes()
        .outputs(crate::returns::CLIP_DUPLICATE),
    CommandSpec::new("clip.group", "Move animation to group", clip_group)
        .icon("group")
        .undoable()
        .inputs(&[ID.required(), GROUP])
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("clip.delete", "Delete animation", clip_delete)
        .icon("delete")
        .undoable()
        .inputs(&[ID.required()])
        .writes()
        .outputs(crate::returns::CLIP_DELETE),
    CommandSpec::new("clip.import", "Import animation file", clip_import)
        .icon("import")
        .undoable()
        .inputs(&[PATH.required()])
        .outputs(crate::returns::CLIP_IMPORT),
    CommandSpec::new("clip.write", "Write animation file", clip_write)
        .icon("export")
        .inputs(&[PATH.required()])
        .writes()
        .outputs(crate::returns::CLIP_WRITE),
    CommandSpec::new("pose.save", "Save pose", pose_save)
        .icon("save-pose")
        .inputs(&[NAME.required(), GROUP, PARTS])
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("pose.apply", "Apply pose", pose_apply)
        .icon("pose")
        .undoable()
        .inputs(&[
            ID.required(),
            PARTS,
            ParamSpec::new("key", K::Bool, "key it (default true)"),
        ]),
    CommandSpec::new("anim.block", "Block out animation", anim_block)
        .icon("ai")
        .undoable()
        .inputs(&[
            ParamSpec::new("poses", K::TextList, "library poses to key, in order").required(),
            ParamSpec::new("frames", K::IntList, "the frame of each pose, rising").required(),
            ParamSpec::new(
                "fill",
                K::Choice(&["ai", "plain", "none"]),
                "between the poses: the model (default), plain interpolation, or the keys' own",
            ),
            ParamSpec::new(
                "style",
                K::Text,
                "the AI style for the fills (default: the current setting)",
            ),
            ParamSpec::new("physics", K::Bool, "turn the physics pass on or off"),
        ])
        .outputs(crate::returns::ANIM_BLOCK),
    CommandSpec::new("pose.delete", "Delete pose", pose_delete)
        .icon("delete")
        .inputs(&[ID.required()])
        .writes(),
    CommandSpec::new("pose.rename", "Rename pose", pose_rename)
        .icon("rename")
        .inputs(&[
            ID.required(),
            ParamSpec::new("to", K::Text, "the new name").required(),
        ])
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("pose.group", "Move pose to group", pose_group)
        .icon("group")
        .inputs(&[ID.required(), GROUP])
        .writes()
        .outputs(crate::returns::CLIP_SAVE),
    CommandSpec::new("parts.save", "Save part set", parts_save)
        .icon("parts")
        .undoable()
        .inputs(&[
            NAME.required(),
            PARTS,
            ParamSpec::new("joint", K::Bone, "a joint with everything below it"),
        ])
        .writes()
        .outputs(crate::returns::PARTS_SAVE),
    CommandSpec::new("parts.delete", "Delete part set", parts_delete)
        .icon("delete")
        .undoable()
        .inputs(&[NAME.required()])
        .writes(),
];

pub(crate) fn root(doc: &Document) -> Result<&Path, String> {
    doc.library
        .as_deref()
        .ok_or_else(|| "no library folder is set (start the app with one)".to_string())
}

pub(crate) fn anims(doc: &Document) -> Result<Library, String> {
    Ok(Library::new(root(doc)?, Kind::Animations))
}

pub(crate) fn poses(doc: &Document) -> Result<Library, String> {
    Ok(Library::new(root(doc)?, Kind::Poses))
}

/// `Some(edited)` when `id` is one of the built-ins the app ships (`<assets>/library`): built-ins
/// are for viewing and learning, kept apart from the person's own work. `edited`: the library's
/// copy differs from the shipped one.
pub(crate) fn builtin(doc: &Document, kind: Kind, id: &str) -> Option<bool> {
    let shipped = Library::new(&doc.assets.as_ref()?.join("library"), kind)
        .path(id)
        .ok()?;
    let shipped = std::fs::read(shipped).ok()?;
    let mine = doc
        .library
        .as_ref()
        .and_then(|r| Library::new(r, kind).path(id).ok());
    Some(
        mine.and_then(|m| std::fs::read(m).ok())
            .is_none_or(|b| b != shipped),
    )
}

fn mark_builtin(doc: &Document, kind: Kind, it: &mut Value) {
    let b = it
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| builtin(doc, kind, id));
    it["builtin"] = json!(b.is_some());
    it["edited"] = json!(b == Some(true));
}

fn group_of(p: &P, default: &str) -> String {
    let g = cmd::text(p, "group").unwrap_or(default).trim().to_string();
    if g.is_empty() { "Ungrouped".into() } else { g }
}

/// Load the clips `clip` links to (and theirs), by id, from the library.
pub fn load_links(lib: &Library, clip: &Clip) -> Result<BTreeMap<String, Clip>, String> {
    let mut out = BTreeMap::new();
    let mut todo: Vec<(String, usize)> = links_of(clip).into_iter().map(|l| (l, 1)).collect();
    while let Some((id, depth)) = todo.pop() {
        if out.contains_key(&id) {
            continue;
        }
        if depth > MAX_LINK_DEPTH {
            return Err(format!(
                "links nest deeper than {MAX_LINK_DEPTH} (a link loop?) at {id:?}"
            ));
        }
        // a missing link drives nothing (the layer says so in the UI), it doesn't block opening
        let Ok(text) = lib.read(&id) else { continue };
        let c = clipfile::load_clip(&text).map_err(|e| format!("linked {id}: {e}"))?;
        todo.extend(links_of(&c).into_iter().map(|l| (l, depth + 1)));
        out.insert(id, c);
    }
    Ok(out)
}

fn links_of(clip: &Clip) -> Vec<String> {
    clip.layers
        .iter()
        .filter(|l| l.source == LayerSource::Link && !l.link.is_empty())
        .map(|l| l.link.clone())
        .collect()
}

/// Make `clip` the open animation (bones must match the rig).
pub fn open(doc: &mut Document, clip: Clip, id: Option<String>) -> Result<(), String> {
    if clip.bones != doc.skeleton.names {
        return Err(format!(
            "this animation is for a different rig ({} bones; this rig has {})",
            clip.bones.len(),
            doc.skeleton.len()
        ));
    }
    doc.links = match doc.library.as_deref() {
        Some(r) => load_links(&Library::new(r, Kind::Animations), &clip)?,
        None => BTreeMap::new(),
    };
    doc.clip = clip;
    doc.clip_id = id;
    doc.mark_saved();
    doc.pins.clear();
    doc.go_to(0.0)
}

fn library_list(doc: &mut Document, _: &P) -> Out {
    let (a, p) = (anims(doc)?, poses(doc)?);
    let mut groups: BTreeSet<String> = a.groups().into_iter().collect();
    groups.extend(p.groups());
    let sk = doc.skeleton.clone();
    let poses: Vec<Value> = p
        .items()
        .into_iter()
        .map(|mut it| {
            let only: Option<Vec<usize>> = it.get("only").and_then(Value::as_array).map(|o| {
                o.iter()
                    .filter_map(|b| b.as_str().and_then(|n| sk.bone(n)))
                    .collect()
            });
            it["part"] = json!(parts::label(&sk, &doc.custom_parts, only.as_deref()));
            mark_builtin(doc, Kind::Poses, &mut it);
            it
        })
        .collect();
    let items = a.items();
    let current = doc.clip_id.clone();
    let animations: Vec<Value> = items
        .into_iter()
        .map(|mut it| {
            it["current"] = json!(it.get("id").and_then(Value::as_str) == current.as_deref());
            mark_builtin(doc, Kind::Animations, &mut it);
            it
        })
        .collect();
    Ok(
        json!({ "animations": animations, "poses": poses, "groups": groups, "custom_parts": doc.custom_parts }),
    )
}

fn clip_new(doc: &mut Document, p: &P) -> Out {
    let group = group_of(p, "Ungrouped");
    let name = cmd::text(p, "name").unwrap_or("untitled");
    let fps = p.get("fps").and_then(Value::as_f64).unwrap_or(30.0);
    let id = match doc.library {
        Some(_) => Some(anims(doc)?.free_id(&group, name)?),
        None => None,
    };
    let start = match cmd::text(p, "pose") {
        Some(pid) => {
            let pf = clipfile::load_pose(&poses(doc)?.read(pid)?)?;
            Pose {
                rotations: pf.rotations,
                root: pf.root,
            }
        }
        None => doc.pose.clone(),
    };
    let mut clip = Clip::new(doc.skeleton.names.clone(), fps);
    clip.name = id
        .as_deref()
        .and_then(|i| i.split_once('/'))
        .map_or(name.to_string(), |x| x.1.to_string());
    clip.group = group;
    let link = cmd::text(p, "link").map(str::to_string);
    let mut start = start;
    if let Some(link) = &link {
        let src = clipfile::load_clip(&anims(doc)?.read(link)?)?;
        let (r, o) = src.sample(&[0.0])?;
        start = Pose {
            rotations: r.into_iter().next().unwrap_or_default(),
            root: o.first().copied().unwrap_or_default(),
        };
    }
    open(doc, clip, None)?;
    doc.pose = start;
    doc.set_key(None, None)?;
    if let Some(link) = link {
        let up = {
            let u = cmd::texts(p, "parts");
            if u.is_empty() {
                vec!["Upper body".to_string()]
            } else {
                u
            }
        };
        let only = doc.part_bones(&up)?;
        let mut lay = Layer::keys(link.split_once('/').map_or(link.as_str(), |x| x.1), Vec::new());
        lay.source = LayerSource::Link;
        lay.link = link.clone();
        doc.clip.layers.push(lay);
        let mut mine = Layer::keys("My keys", Vec::new());
        mine.only = only;
        mine.parts = up.into_iter().filter(|x| x != parts::WHOLE_BODY).collect();
        doc.clip.layers.push(mine);
        doc.clip.edit = 2;
        doc.links = load_links(&anims(doc)?, &doc.clip)?;
        doc.go_to(0.0)?;
    }
    if let Some(id) = &id {
        anims(doc)?.write(id, &clipfile::save_clip(&doc.clip))?;
        doc.clip_id = Some(id.clone());
        doc.mark_saved();
    }
    Ok(json!({ "id": doc.clip_id }))
}

fn clip_open(doc: &mut Document, p: &P) -> Out {
    let id = cmd::text(p, "id").unwrap_or_default().to_string();
    let lib = anims(doc)?;
    // by id, or by name anywhere in the library
    let id = if lib.exists(&id) {
        id
    } else {
        lib.items()
            .into_iter()
            .find(|i| i.get("name").and_then(Value::as_str) == Some(id.as_str()))
            .and_then(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
            .ok_or_else(|| format!("no animation {id:?} in the library"))?
    };
    let clip = clipfile::load_clip(&lib.read(&id)?).map_err(|e| format!("{id}: {e}"))?;
    open(doc, clip, Some(id.clone()))?;
    Ok(json!({ "id": id, "length": doc.clip.length() }))
}

fn clip_save(doc: &mut Document, _: &P) -> Out {
    let lib = anims(doc)?;
    let id = match &doc.clip_id {
        Some(id) => id.clone(),
        None => lib.free_id(
            &doc.clip.group,
            if doc.clip.name.is_empty() {
                "untitled"
            } else {
                &doc.clip.name
            },
        )?,
    };
    if builtin(doc, Kind::Animations, &id).is_some() {
        return Err(format!(
            "{id} is a built-in animation (kept for viewing and learning): save your own copy with clip.save_as"
        ));
    }
    lib.write(&id, &clipfile::save_clip(&doc.clip))?;
    doc.clip_id = Some(id.clone());
    doc.mark_saved();
    Ok(json!({ "id": id }))
}

fn clip_save_as(doc: &mut Document, p: &P) -> Out {
    let group = group_of(p, &doc.clip.group.clone());
    let default = format!("{} copy", doc.clip.name);
    let lib = anims(doc)?;
    let id = lib.free_id(&group, cmd::text(p, "name").unwrap_or(&default))?;
    doc.clip.name = id.split_once('/').map(|x| x.1.to_string()).unwrap_or_default();
    doc.clip.group = group;
    doc.clip.origin.clear();
    lib.write(&id, &clipfile::save_clip(&doc.clip))?;
    doc.clip_id = Some(id.clone());
    doc.mark_saved();
    Ok(json!({ "id": id }))
}

fn clip_rename(doc: &mut Document, p: &P) -> Out {
    // another animation than the open one: its file is renamed, the document is left alone
    if let Some(other) = cmd::text(p, "id").filter(|i| doc.clip_id.as_deref() != Some(*i)) {
        let lib = anims(doc)?;
        if !lib.exists(other) {
            return Err(format!("no animation {other:?}"));
        }
        if builtin(doc, Kind::Animations, other).is_some() {
            return Err(format!(
                "{other} is a built-in animation: duplicate it to have one to rename"
            ));
        }
        let new = lib.move_item(other, None, cmd::text(p, "to"))?;
        doc.saved.remove(other);
        return Ok(json!({ "id": new }));
    }
    let id = clip_save(doc, p)?["id"].as_str().unwrap_or_default().to_string();
    let new = anims(doc)?.move_item(&id, None, cmd::text(p, "to"))?;
    doc.clip.name = new.split_once('/').map(|x| x.1.to_string()).unwrap_or_default();
    doc.saved.remove(&id);
    doc.clip_id = Some(new.clone());
    doc.mark_saved();
    Ok(json!({ "id": new }))
}

fn clip_duplicate(doc: &mut Document, p: &P) -> Out {
    let src = match cmd::text(p, "id") {
        Some(id) => id.to_string(),
        None => clip_save(doc, p)?["id"].as_str().unwrap_or_default().to_string(),
    };
    Ok(json!({ "id": anims(doc)?.duplicate(&src)? }))
}

fn clip_group(doc: &mut Document, p: &P) -> Out {
    let src = cmd::text(p, "id").unwrap_or_default().to_string();
    let open_one = doc.clip_id.as_deref() == Some(src.as_str());
    if open_one {
        clip_save(doc, p)?;
    }
    let new = anims(doc)?.move_item(&src, Some(&group_of(p, "Ungrouped")), None)?;
    doc.saved.remove(&src);
    if open_one {
        doc.clip.group = new.split_once('/').map(|x| x.0.to_string()).unwrap_or_default();
        doc.clip_id = Some(new.clone());
        doc.mark_saved();
    }
    Ok(json!({ "id": new }))
}

fn clip_delete(doc: &mut Document, p: &P) -> Out {
    let id = cmd::text(p, "id").unwrap_or_default().to_string();
    let lib = anims(doc)?;
    if !lib.exists(&id) {
        return Err(format!("no animation {id:?}"));
    }
    lib.delete(&id)?;
    doc.saved.remove(&id);
    if doc.clip_id.as_deref() == Some(id.as_str()) {
        // the open one closes with its file (undo brings it back, unsaved, to save again)
        let mut blank = Clip::new(doc.skeleton.names.clone(), doc.clip.fps);
        blank.name = "untitled".into();
        open(doc, blank, None)?;
        return Ok(json!({ "closed": id }));
    }
    Ok(json!({}))
}

fn clip_import(doc: &mut Document, p: &P) -> Out {
    let path = cmd::text(p, "path").unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {path}: {e}"))?;
    let clip = clipfile::load_clip(&text).map_err(|e| format!("{path}: {e}"))?;
    open(doc, clip, None)?;
    Ok(json!({ "length": doc.clip.length() }))
}

fn clip_write(doc: &mut Document, p: &P) -> Out {
    let path = cmd::text(p, "path").unwrap_or_default();
    craft_atomic::atomic_write(Path::new(path), clipfile::save_clip(&doc.clip).as_bytes())
        .map_err(|e| format!("can't write {path}: {e}"))?;
    Ok(json!({ "path": path }))
}

fn pose_save(doc: &mut Document, p: &P) -> Out {
    let group = group_of(p, "Ungrouped");
    let lib = poses(doc)?;
    let id = lib.free_id(&group, cmd::text(p, "name").unwrap_or_default())?;
    let only = doc.part_bones(&cmd::texts(p, "parts"))?;
    let pf = PoseFile {
        name: id.split_once('/').map(|x| x.1.to_string()).unwrap_or_default(),
        group,
        bones: doc.skeleton.names.clone(),
        rotations: doc.pose.rotations.clone(),
        root: doc.pose.root,
        only,
    };
    lib.write(&id, &clipfile::save_pose(&pf))?;
    Ok(json!({ "id": id }))
}

fn pose_apply(doc: &mut Document, p: &P) -> Out {
    let id = cmd::text(p, "id").unwrap_or_default();
    let pf = clipfile::load_pose(&poses(doc)?.read(id)?).map_err(|e| format!("{id}: {e}"))?;
    if pf.bones != doc.skeleton.names {
        return Err(format!("pose {id} is for a different rig"));
    }
    let mut only = pf.only.clone();
    if let Some(ask) = doc.part_bones(&cmd::texts(p, "parts"))? {
        only = Some(match only {
            None => ask,
            Some(o) => o.into_iter().filter(|b| ask.contains(b)).collect(),
        });
    }
    // the pose lands where the character stands
    doc.paste_part(
        &Pose {
            rotations: pf.rotations,
            root: pf.root,
        },
        only.as_deref(),
    );
    if p.get("key").and_then(Value::as_bool).unwrap_or(true) {
        doc.set_key(None, only)?;
    }
    Ok(json!({}))
}

/// Key library poses at frames, fill between each pair and (optionally) set physics: a blocked-out
/// animation in one step (one undo).
fn anim_block(doc: &mut Document, p: &P) -> Out {
    let ids = cmd::texts(p, "poses");
    let frames: Vec<i64> = p
        .get("frames")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect();
    if ids.is_empty() || ids.len() != frames.len() {
        return Err(format!(
            "{} poses and {} frames: give one frame per pose",
            ids.len(),
            frames.len()
        ));
    }
    if frames.windows(2).any(|w| w[1] <= w[0]) || frames.iter().any(|&f| !(0..=10_000_000).contains(&f)) {
        return Err(format!("frames {frames:?} must rise, from 0"));
    }
    let fill = cmd::text(p, "fill").unwrap_or("ai");
    let settings = doc.ai_settings.clone();
    if let Some(st) = cmd::text(p, "style") {
        let known = doc
            .ai
            .as_ref()
            .map(|m| m.info().styles.clone())
            .unwrap_or_default();
        if st != "any" && !known.iter().any(|k| k == st) {
            return Err(format!("style {st:?}: the model knows {known:?}"));
        }
        doc.ai_settings.style = st.to_string();
    }
    if fill == "plain" {
        doc.ai_settings.ai_force = 0.0;
    }
    let out = block(doc, &ids, &frames, fill);
    doc.ai_settings = settings;
    let fills = out?;
    if let Some(on) = p.get("physics").and_then(Value::as_bool) {
        doc.clip.physics = on;
    }
    doc.go_to(frames[0] as f64)?;
    Ok(
        json!({ "keys": ids.len(), "fills": fills, "physics": doc.clip.physics, "frames": frames[frames.len() - 1] + 1 }),
    )
}

fn block(doc: &mut Document, ids: &[String], frames: &[i64], fill: &str) -> Result<Vec<Value>, String> {
    for (id, &f) in ids.iter().zip(frames) {
        doc.go_to(f as f64)?;
        let mut q = P::new();
        q.insert("id".into(), json!(id));
        pose_apply(doc, &q).map_err(|e| format!("pose {id}: {e}"))?;
    }
    let mut fills = Vec::new();
    if fill != "none" {
        for w in frames.windows(2).filter(|w| w[1] - w[0] > 1) {
            let (a, b, model) = doc.ai_fill(Some((w[0], w[1])))?;
            fills.push(json!({ "from": a, "to": b, "model": if fill == "plain" { "plain interpolation".into() } else { model } }));
        }
    }
    Ok(fills)
}

fn pose_delete(doc: &mut Document, p: &P) -> Out {
    let id = cmd::text(p, "id").unwrap_or_default();
    let lib = poses(doc)?;
    if !lib.exists(id) {
        return Err(format!("no pose {id:?}"));
    }
    lib.delete(id)?;
    Ok(json!({}))
}

fn pose_rename(doc: &mut Document, p: &P) -> Out {
    let id = poses(doc)?.move_item(cmd::text(p, "id").unwrap_or_default(), None, cmd::text(p, "to"))?;
    Ok(json!({ "id": id }))
}

fn pose_group(doc: &mut Document, p: &P) -> Out {
    let id = poses(doc)?.move_item(
        cmd::text(p, "id").unwrap_or_default(),
        Some(&group_of(p, "Ungrouped")),
        None,
    )?;
    Ok(json!({ "id": id }))
}

fn save_parts(doc: &Document) -> Result<(), String> {
    let path = root(doc)?.join(PARTS_FILE);
    let text = serde_json::to_string_pretty(&doc.custom_parts).map_err(|e| e.to_string())?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read the user's part sets from the library (missing file: none yet).
pub fn load_parts(doc: &mut Document) -> Result<(), String> {
    let Some(r) = doc.library.as_deref() else {
        return Ok(());
    };
    let path = r.join(PARTS_FILE);
    if !path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    doc.custom_parts = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn parts_save(doc: &mut Document, p: &P) -> Out {
    let name = cmd::text(p, "name").unwrap_or_default().trim().to_string();
    if name.is_empty() || parts::PRESETS.contains(&name.as_str()) || parts::ROWS.contains(&name.as_str()) {
        return Err("give the part set a name of its own".into());
    }
    let sk = doc.skeleton.clone();
    let mut got = BTreeSet::new();
    for part in cmd::texts(p, "parts") {
        got.extend(parts::bones(&sk, &doc.custom_parts, &part)?);
    }
    if let Some(j) = cmd::text(p, "joint") {
        doc.bone(j)?;
        got.extend(parts::subtree(&sk, j));
    }
    if got.is_empty() {
        return Err("pick at least one part or a joint".into());
    }
    let names: Vec<String> = got.iter().filter_map(|&i| sk.names.get(i).cloned()).collect();
    doc.custom_parts.insert(name.clone(), names);
    save_parts(doc)?;
    Ok(json!({ "name": name, "joints": got.len() }))
}

fn parts_delete(doc: &mut Document, p: &P) -> Out {
    let name = cmd::text(p, "name").unwrap_or_default();
    if doc.custom_parts.remove(name).is_none() {
        return Err(format!("no part set {name:?}"));
    }
    save_parts(doc)?;
    Ok(json!({}))
}
