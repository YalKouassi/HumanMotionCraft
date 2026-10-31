//! Layers, travel, scene boxes and the physics switch (cf-animate's `svc_layers.py`). Here, not in
//! ac-core, because link and travel layers read the library.

use std::collections::BTreeMap;

use ac_core::clip::{Clip, Layer, LayerMode, LayerSource, Surface};
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use ac_core::{Document, edits, parts, travel};
use serde_json::{Map, Value, json};

use crate::commands::{anims, load_links};

type P = Map<String, Value>;
type Out = Result<Value, String>;

const I: ParamSpec = ParamSpec::new("i", K::Integer, "the layer (0: the base)").required();
const PARTS: ParamSpec = ParamSpec::new("parts", K::TextList, "body parts (empty: the whole body)");
const MODE: ParamSpec = ParamSpec::new(
    "mode",
    K::Choice(&["override", "additive"]),
    "replace what is below, or add to it",
);
const GAIT: ParamSpec = ParamSpec::new(
    "gait",
    K::Choice(&["auto", "walk", "run", "crouch"]),
    "which Locomotion clips to walk with",
);
const FADE: ParamSpec =
    ParamSpec::new("fade", K::Integer, "frames the layer fades in and out over").range(0.0, 600.0);
const SURFACE: ParamSpec = ParamSpec::new("i", K::Integer, "the box").required();
/// Most boxes in a scene.
pub const MAX_SURFACES: usize = 64;
/// A box further than this (m) from the origin is a typo.
pub const MAX_REACH: f64 = 1000.0;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("layers.list", "Layers", layers_list)
        .icon("layers")
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.add", "Add layer", layer_add)
        .icon("layer-add")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "source",
                K::Choice(&["keys", "link"]),
                "a layer of keys, or another animation",
            ),
            ParamSpec::new("link", K::Text, "the animation a link layer plays (library id)"),
            ParamSpec::new("name", K::Text, "the layer's name"),
            PARTS,
            MODE,
            ParamSpec::new("offset", K::Number, "frames a link starts later").range(-1e5, 1e5),
        ])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.travel", "Travel between keys", layer_travel)
        .icon("travel")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "from",
                K::Integer,
                "first frame (with `to`; default: the keys around the playhead)",
            )
            .range(0.0, 1e6),
            ParamSpec::new("to", K::Integer, "last frame").range(0.0, 1e6),
            PARTS,
            FADE,
            GAIT,
        ])
        .outputs(crate::returns::LAYER_TRAVEL),
    CommandSpec::new("layer.plan", "Plan travel again", layer_plan)
        .icon("travel")
        .undoable()
        .inputs(&[I])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.edit", "Edit layer", layer_edit)
        .icon("layer")
        .undoable()
        .inputs(&[I])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.set", "Layer settings", layer_set)
        .icon("layer")
        .undoable()
        .inputs(&[
            I,
            ParamSpec::new("name", K::Text, "the layer's name"),
            MODE,
            ParamSpec::new("weight", K::Number, "how much it shows, 0..1").range(0.0, 1.0),
            ParamSpec::new("offset", K::Number, "frames a link starts later").range(-1e5, 1e5),
            ParamSpec::new("speed", K::Number, "playback speed of a link").range(0.01, 100.0),
            ParamSpec::new("loop", K::Bool, "a link repeats"),
            ParamSpec::new("muted", K::Bool, "off"),
            GAIT,
            FADE,
            ParamSpec::new("start", K::Number, "first frame of its range").range(-1e6, 1e6),
            ParamSpec::new("end", K::Number, "last frame of its range").range(-1e6, 1e6),
            ParamSpec::new("whole", K::Bool, "true: no range, the whole animation"),
            ParamSpec::new("waypoints", K::Points, "a travel's path, [[x, y], ...] metres"),
            ParamSpec::new("moves", K::Offsets, "footsteps moved by hand, {\"L0\": [dx, dy]}"),
            PARTS,
        ])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.move", "Move layer", layer_move)
        .icon("layer")
        .undoable()
        .inputs(&[I, ParamSpec::new("to", K::Integer, "its new place").required()])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.delete", "Delete layer", layer_delete)
        .icon("delete")
        .undoable()
        .inputs(&[I])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.bake", "Bake layers down", layer_bake)
        .icon("bake")
        .undoable()
        .inputs(&[I])
        .outputs(crate::returns::LAYER_BAKE),
    CommandSpec::new("surface.add", "Add box", surface_add)
        .icon("box")
        .undoable()
        .inputs(&[
            ParamSpec::new("name", K::Text, "the box's name"),
            ParamSpec::new("width", K::Number, "metres across the body").range(0.05, 100.0),
            ParamSpec::new("depth", K::Number, "metres along the facing").range(0.05, 100.0),
            ParamSpec::new("top", K::Number, "height of its top (m)").range(0.0, 100.0),
        ])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("surface.set", "Box settings", surface_set)
        .icon("box")
        .undoable()
        .inputs(&[
            SURFACE,
            ParamSpec::new("name", K::Text, "the box's name"),
            ParamSpec::new("top", K::Number, "height of its top (m)").range(0.0, 100.0),
            ParamSpec::new("move", K::Vec3, "nudge [dx, dy, 0] metres"),
            ParamSpec::new("size", K::Vec3, "[width, depth, 0] metres around its centre"),
            ParamSpec::new("centre", K::Vec3, "put its middle at [x, y, 0] metres"),
            ParamSpec::new(
                "turn",
                K::Number,
                "how far it is turned about its centre (degrees)",
            )
            .range(-360.0, 360.0),
        ])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("surface.delete", "Delete box", surface_delete)
        .icon("delete")
        .undoable()
        .inputs(&[SURFACE])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("physics.set", "Physics pass", physics_set)
        .icon("physics")
        .undoable()
        .inputs(&[ParamSpec::new("on", K::Bool, "play and export through the physics pass").required()])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("physics.solve", "Physics pass result", physics_solve)
        .icon("physics")
        .inputs(&[ParamSpec::new("frame", K::Number, "the frame whose root to report").range(0.0, 1e5)])
        .outputs(crate::returns::PHYSICS_SOLVE),
    CommandSpec::new("physics.overlay", "Physics overlay", physics_overlay)
        .icon("physics")
        .inputs(&[ParamSpec::new(
            "frame",
            K::Number,
            "the frame to look at (default: the current one)",
        )
        .range(0.0, 1e5)])
        .outputs(crate::returns::PHYSICS_OVERLAY),
];

fn num(p: &P, k: &str) -> Option<f64> {
    p.get(k).and_then(Value::as_f64)
}

fn index(p: &P, k: &str) -> Result<usize, String> {
    num(p, k)
        .filter(|x| *x >= 0.0)
        .map(|x| x as usize)
        .ok_or_else(|| format!("{k} must be 0 or more"))
}

pub(crate) fn layer_at(doc: &Document, p: &P) -> Result<usize, String> {
    let i = index(p, "i")?;
    if i < doc.clip.layers.len() {
        Ok(i)
    } else {
        Err(format!("no layer {i} (there are {})", doc.clip.layers.len()))
    }
}

fn mode_of(p: &P) -> LayerMode {
    match cmd::text(p, "mode") {
        Some("additive") => LayerMode::Additive,
        _ => LayerMode::Override,
    }
}

fn source_name(s: LayerSource) -> &'static str {
    match s {
        LayerSource::Keys => "keys",
        LayerSource::Link => "link",
        LayerSource::Travel => "travel",
    }
}

pub(crate) fn refresh(doc: &mut Document) -> Out {
    let f = doc.frame;
    doc.go_to(f)?;
    layers_list(doc, &P::new())
}

fn layers_list(doc: &mut Document, _: &P) -> Out {
    let sk = doc.skeleton.clone();
    let list: Vec<Value> = doc
        .clip
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            json!({
                "i": i, "name": l.name, "source": source_name(l.source), "link": l.link,
                "part": parts::label(&sk, &doc.custom_parts, l.only.as_deref()),
                "mode": if l.mode == LayerMode::Additive { "additive" } else { "override" },
                "weight": l.weight, "offset": l.offset, "speed": l.speed, "loop": l.loop_,
                "muted": l.muted, "keys": l.keys.len(), "edit": i == doc.clip.edit,
                "start": l.start, "end": l.end, "fade": l.fade, "gait": l.gait,
                "waypoints": l.waypoints, "moves": l.moves, "steps": l.steps,
                "missing": l.source == LayerSource::Link && !doc.links.contains_key(&l.link),
            })
        })
        .collect();
    let surfaces: Vec<Value> = doc
        .clip
        .surfaces
        .iter()
        .map(|s| {
            json!({
                "name": s.name, "min": s.min, "max": s.max, "top": s.top,
                "centre": s.centre(), "turn": s.yaw.to_degrees(),
            })
        })
        .collect();
    let contacts: Vec<Value> = doc
        .clip
        .contacts
        .iter()
        .map(|c| json!({ "bone": c.bone, "from": c.from, "to": c.to, "at": c.at }))
        .collect();
    Ok(json!({
        "layers": list, "edit": doc.clip.edit, "surfaces": surfaces, "contacts": contacts,
        "physics": doc.clip.physics,
    }))
}

fn part_names(p: &P) -> Vec<String> {
    cmd::texts(p, "parts")
        .into_iter()
        .filter(|n| n != "Whole body")
        .collect()
}

fn layer_add(doc: &mut Document, p: &P) -> Out {
    let link = cmd::text(p, "link").unwrap_or_default().to_string();
    let is_link = cmd::text(p, "source") == Some("link");
    let names = cmd::texts(p, "parts");
    let mut lay = Layer::keys("Layer", Vec::new());
    if is_link {
        let lib = anims(doc)?;
        if !lib.exists(&link) {
            return Err(format!("no animation {link:?} in the library"));
        }
        lay.source = LayerSource::Link;
        lay.link = link.clone();
        lay.name = link.split_once('/').map_or(link.as_str(), |x| x.1).to_string();
    }
    if let Some(n) = cmd::text(p, "name") {
        lay.name = n.to_string();
    }
    lay.only = doc.part_bones(&names)?;
    lay.parts = part_names(p);
    lay.mode = mode_of(p);
    lay.offset = num(p, "offset").unwrap_or(0.0);
    doc.clip.layers.push(lay);
    if is_link {
        let lib = anims(doc)?;
        let mut links = load_links(&lib, &doc.clip)?;
        if let Some(me) = &doc.clip_id
            && links.contains_key(me)
        {
            return Err("that animation plays this one: the link would loop".into());
        }
        if doc.project.is_some() {
            // a project plays the clips it carries; the library only fills in what it lacks
            for (id, c) in std::mem::take(&mut doc.links) {
                links.insert(id, c);
            }
        }
        doc.links = links;
        ac_core::evaluation::mark_from_links(doc);
    } else {
        doc.clip.edit = doc.clip.layers.len() - 1;
    }
    refresh(doc)
}

/// The Locomotion clips a gait walks with, from the library.
fn locomotion(doc: &Document, gait: &str) -> Result<Vec<(String, Clip)>, String> {
    let lib = anims(doc)?;
    let mut out = Vec::new();
    for item in lib.items() {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if travel::uses(gait, id) {
            let clip = ac_core::clipfile::load_clip(&lib.read(id)?).map_err(|e| format!("{id}: {e}"))?;
            if clip.bones == doc.skeleton.names && !clip.evaluation_only {
                out.push((id.to_string(), clip));
            }
        }
    }
    Ok(out)
}

fn plan_travel(doc: &mut Document, i: usize) -> Result<(), String> {
    let lay = doc.clip.layers.get(i).ok_or_else(|| format!("no layer {i}"))?;
    if lay.source != LayerSource::Travel {
        return Err(format!("layer {:?} is not a travel layer", lay.name));
    }
    let (a, b) = (lay.start.unwrap_or(0.0) as i64, lay.end.unwrap_or(0.0) as i64);
    let (gait, waypoints, moves) = (lay.gait.clone(), lay.waypoints.clone(), lay.moves.clone());
    let clips = locomotion(doc, &gait)?;
    let db = travel::Database::new(&doc.skeleton, &clips)?;
    let (keys, steps) = travel::travel_keys(
        &doc.skeleton,
        &doc.clip,
        a,
        b,
        i,
        &doc.links,
        &db,
        &waypoints,
        &moves,
    )?;
    let lay = &mut doc.clip.layers[i];
    lay.keys = keys;
    lay.steps = steps
        .into_iter()
        .map(|(s, f, g, at)| json!([s, f, g, at[0], at[1], at[2]]))
        .collect();
    Ok(())
}

pub(crate) fn layer_travel(doc: &mut Document, p: &P) -> Out {
    let (a, b) = match (num(p, "from"), num(p, "to")) {
        (Some(x), Some(y)) => {
            let (x, y) = (x as i64, y as i64);
            (x.min(y), x.max(y))
        }
        (None, None) => {
            let mut base = Clip::new(doc.clip.bones.clone(), doc.clip.fps);
            base.layers[0].keys = doc.clip.base().keys.clone();
            edits::span_around(&base, doc.frame.round() as i64)
                .ok_or("put the playhead between two keys (or give from and to) to walk between them")?
        }
        _ => return Err("give both from and to, or neither".into()),
    };
    let mut lay = Layer::keys(&format!("Travel {a}-{b}"), Vec::new());
    lay.source = LayerSource::Travel;
    lay.only = doc.part_bones(&cmd::texts(p, "parts"))?;
    lay.parts = part_names(p);
    lay.start = Some(a as f64);
    lay.end = Some(b as f64);
    lay.fade = num(p, "fade").map_or(6, |x| x as i64);
    lay.gait = cmd::text(p, "gait").unwrap_or("auto").to_string();
    doc.clip.layers.push(lay);
    plan_travel(doc, doc.clip.layers.len() - 1)?;
    let mut out = refresh(doc)?;
    out["i"] = json!(doc.clip.layers.len() - 1);
    Ok(out)
}

fn layer_plan(doc: &mut Document, p: &P) -> Out {
    let i = layer_at(doc, p)?;
    plan_travel(doc, i)?;
    refresh(doc)
}

fn layer_edit(doc: &mut Document, p: &P) -> Out {
    let i = layer_at(doc, p)?;
    if doc.clip.layers[i].source != LayerSource::Keys {
        return Err("only a keys layer takes keys".into());
    }
    doc.clip.edit = i;
    refresh(doc)
}

fn layer_set(doc: &mut Document, p: &P) -> Out {
    let i = layer_at(doc, p)?;
    let only = if p.contains_key("parts") {
        Some(doc.part_bones(&cmd::texts(p, "parts"))?)
    } else {
        None
    };
    let lay = &mut doc.clip.layers[i];
    if let Some(n) = cmd::text(p, "name") {
        lay.name = n.to_string();
    }
    if p.contains_key("mode") {
        lay.mode = mode_of(p);
    }
    if let Some(w) = num(p, "weight") {
        lay.weight = w;
    }
    if let Some(o) = num(p, "offset") {
        lay.offset = o;
    }
    if let Some(s) = num(p, "speed") {
        lay.speed = s;
    }
    if let Some(b) = p.get("loop").and_then(Value::as_bool) {
        lay.loop_ = b;
    }
    if let Some(b) = p.get("muted").and_then(Value::as_bool) {
        lay.muted = b;
    }
    if let Some(g) = cmd::text(p, "gait") {
        lay.gait = g.to_string();
    }
    if let Some(f) = num(p, "fade") {
        lay.fade = f as i64;
    }
    if p.get("whole").and_then(Value::as_bool) == Some(true) {
        if lay.source == LayerSource::Travel {
            return Err("a travel layer keeps its range".into());
        }
        lay.start = None;
        lay.end = None;
    }
    if let Some(s) = num(p, "start") {
        lay.start = Some(s);
    }
    if let Some(e) = num(p, "end") {
        lay.end = Some(e);
    }
    if let (Some(s), Some(e)) = (lay.start, lay.end)
        && s > e
    {
        return Err(format!("the range starts ({s}) after it ends ({e})"));
    }
    if lay.start.is_some() != lay.end.is_some() {
        return Err("a range needs both start and end".into());
    }
    if let Some(Value::Array(w)) = p.get("waypoints") {
        lay.waypoints = w
            .iter()
            .filter_map(|x| Some([x.get(0)?.as_f64()?, x.get(1)?.as_f64()?]))
            .collect();
    }
    if let Some(Value::Object(m)) = p.get("moves") {
        lay.moves = m
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), [v.get(0)?.as_f64()?, v.get(1)?.as_f64()?])))
            .filter(|(_, d)| d[0] != 0.0 || d[1] != 0.0)
            .collect::<BTreeMap<_, _>>();
    }
    if let Some(only) = only {
        lay.only = only;
        lay.parts = part_names(p);
    }
    let replan = lay.source == LayerSource::Travel
        && ["gait", "parts", "waypoints", "moves", "start", "end"]
            .iter()
            .any(|k| p.contains_key(*k));
    if replan {
        plan_travel(doc, i)?;
    }
    refresh(doc)
}

fn layer_move(doc: &mut Document, p: &P) -> Out {
    let (i, to) = (index(p, "i")?, index(p, "to")?);
    let n = doc.clip.layers.len();
    if !((1..n).contains(&i) && (1..n).contains(&to)) {
        return Err("the base layer stays at the bottom".into());
    }
    let editing = doc.clip.edit;
    let lay = doc.clip.layers.remove(i);
    doc.clip.layers.insert(to, lay);
    // the edited layer follows its layer
    doc.clip.edit = if editing == i {
        to
    } else if i < editing && editing <= to {
        editing - 1
    } else if to <= editing && editing < i {
        editing + 1
    } else {
        editing
    };
    refresh(doc)
}

fn layer_delete(doc: &mut Document, p: &P) -> Out {
    let i = index(p, "i")?;
    if i < 1 || i >= doc.clip.layers.len() {
        return Err("the base layer stays".into());
    }
    doc.clip.layers.remove(i);
    let e = doc.clip.edit;
    doc.clip.edit = if e == i {
        0
    } else if e > i {
        e - 1
    } else {
        e
    };
    refresh(doc)
}

fn layer_bake(doc: &mut Document, p: &P) -> Out {
    let i = index(p, "i")?;
    let links = doc.links.clone();
    let n = edits::bake_down(&mut doc.clip, i, 1, &links)?;
    let mut out = refresh(doc)?;
    out["baked"] = json!(n);
    Ok(out)
}

fn surface_add(doc: &mut Document, p: &P) -> Out {
    if doc.clip.surfaces.len() >= MAX_SURFACES {
        return Err(format!("a scene holds at most {MAX_SURFACES} boxes"));
    }
    let j = doc.joints()?;
    let (l, r) = (doc.bone("upperleg01.L")?, doc.bone("upperleg01.R")?);
    let left = [j[l][0] - j[r][0], j[l][1] - j[r][1]];
    let yaw = (-left[0]).atan2(left[1]);
    let (s, c) = yaw.sin_cos();
    let ahead = [j[0][0] + 0.8 * c, j[0][1] + 0.8 * s];
    let w = num(p, "width").unwrap_or(1.2) / 2.0;
    let d = num(p, "depth").unwrap_or(0.6) / 2.0;
    // the box's half extents, turned to the facing
    let span = [(c * d).abs() + (s * w).abs(), (s * d).abs() + (c * w).abs()];
    let name = cmd::text(p, "name")
        .map(str::to_string)
        .unwrap_or_else(|| format!("Box {}", doc.clip.surfaces.len() + 1));
    doc.clip.surfaces.push(Surface {
        name,
        min: [ahead[0] - span[0], ahead[1] - span[1]],
        max: [ahead[0] + span[0], ahead[1] + span[1]],
        top: num(p, "top").unwrap_or(0.9),
        yaw: 0.0,
    });
    layers_list(doc, &P::new())
}

fn surface_set(doc: &mut Document, p: &P) -> Out {
    let i = index(p, "i")?;
    let n = doc.clip.surfaces.len();
    let s = doc
        .clip
        .surfaces
        .get_mut(i)
        .ok_or_else(|| format!("no box {i} (there are {n})"))?;
    if let Some(name) = cmd::text(p, "name") {
        s.name = name.to_string();
    }
    if let Some(t) = num(p, "top") {
        s.top = t;
    }
    if let Some(m) = cmd::vec3(p, "move") {
        s.min = [s.min[0] + m[0], s.min[1] + m[1]];
        s.max = [s.max[0] + m[0], s.max[1] + m[1]];
    }
    if let Some(z) = cmd::vec3(p, "size") {
        let (cx, cy) = ((s.min[0] + s.max[0]) / 2.0, (s.min[1] + s.max[1]) / 2.0);
        let (sx, sy) = (z[0].max(0.05) / 2.0, z[1].max(0.05) / 2.0);
        s.min = [cx - sx, cy - sy];
        s.max = [cx + sx, cy + sy];
    }
    if let Some(c) = cmd::vec3(p, "centre") {
        if c.iter().any(|x| x.abs() > MAX_REACH) {
            return Err(format!("centre {c:?} is more than {MAX_REACH} m away"));
        }
        let now = s.centre();
        let (dx, dy) = (c[0] - now[0], c[1] - now[1]);
        s.min = [s.min[0] + dx, s.min[1] + dy];
        s.max = [s.max[0] + dx, s.max[1] + dy];
    }
    if let Some(t) = num(p, "turn") {
        s.yaw = t.to_radians();
    }
    if s.min.iter().chain(&s.max).any(|x| x.abs() > MAX_REACH) {
        return Err(format!("the box would be more than {MAX_REACH} m away"));
    }
    layers_list(doc, &P::new())
}

fn surface_delete(doc: &mut Document, p: &P) -> Out {
    let i = index(p, "i")?;
    if i >= doc.clip.surfaces.len() {
        return Err(format!("no box {i}"));
    }
    doc.clip.surfaces.remove(i);
    layers_list(doc, &P::new())
}

/// What the physics pass does to the animation: its flights, the frames it lifted out of the
/// ground, the frames where it held a sliding foot or balanced a still pose (`problems`: runs of
/// them, labelled, to jump to), and (given `frame`) the root it puts there, keyed and solved.
fn physics_solve(doc: &mut Document, p: &P) -> Out {
    let cache = doc.physics()?;
    let r = &cache.report;
    let n = r.roots.len();
    let mut out = json!({
        "frames": n, "flights": r.flights, "lifted": r.lifted, "slid": r.slid, "balanced": r.balanced,
        "lifted_frames": r.lifted_frames, "slid_frames": r.slid_frames,
        "problems": ac_core::problems::problems(r),
    });
    if let Some(f) = num(p, "frame") {
        let f = f.round() as usize;
        if f >= n {
            return Err(format!("frame {f} is past the end ({n} frames)"));
        }
        let (rots, roots) = doc.clip.sample_with(&[f as f64], &doc.links)?;
        let sk = &doc.skeleton;
        let body = ac_core::physics::Body::new(sk)?;
        let check = |rots: &[ac_core::quat::Quat], root| -> Result<Value, String> {
            Ok(
                match ac_core::physics::balance(sk, &body, rots, root, &doc.clip.surfaces)? {
                    Some(b) => json!({ "com": b.com, "inside": b.inside }),
                    None => Value::Null,
                },
            )
        };
        out["keyed"] = json!(roots[0]);
        out["root"] = json!(r.roots[f]);
        out["keyed_balance"] = check(&rots[0], roots[0])?;
        out["balance"] = check(&r.rotations[f], r.roots[f])?;
    }
    Ok(out)
}

/// What the viewport draws over a frame: centre of mass, support area (on the ground under it),
/// contacts (pinned or sliding), the jump arc the frame is in, and how far the physics pass moved
/// the root head from the keyed pose (the ghost).
fn physics_overlay(doc: &mut Document, p: &P) -> Out {
    let frame = num(p, "frame").unwrap_or(doc.frame);
    let o = doc.overlay(frame)?;
    let surfaces = &doc.clip.surfaces;
    Ok(json!({
        "frame": o.frame,
        "physics": doc.physics_shown(),
        "com": o.com,
        "support": o.balance.as_ref().map(|b| b.support.iter().map(|q| [q[0], q[1], ac_core::physics::ground(surfaces, q[0], q[1])]).collect::<Vec<_>>()),
        "inside": o.balance.as_ref().map(|b| b.inside),
        "contacts": o.contacts.iter().map(|c| json!({ "point": c.point, "pinned": c.pinned, "foot": c.foot })).collect::<Vec<_>>(),
        "pinned": o.contacts.iter().filter(|c| c.pinned).count(),
        "flight": o.flight.as_ref().map(|(from, to, arc)| json!({ "from": from, "to": to, "arc": arc })),
        "keyed_moved": o.keyed_moved,
    }))
}

fn physics_set(doc: &mut Document, p: &P) -> Out {
    doc.clip.physics = p.get("on").and_then(Value::as_bool).unwrap_or(true);
    layers_list(doc, &P::new())
}
