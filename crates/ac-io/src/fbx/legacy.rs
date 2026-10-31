//! FBX 6's scene put in the shape of 7's, so one scene reader serves both.
//!
//! Written from the public layout of the format. FBX 6 names its objects instead of numbering them:
//! `Model: "Model::Hips", "Limb"`, with `Properties60` of `Property: name, type, flags, values`, and
//! `Connect: "OO", child, parent`. `GlobalSettings` sits in `Objects`. Its animation is in `Takes`:
//! each `Take` has `LocalTime`, `ReferenceTime` and a `Model` for each animated model, holding
//! `Channel: "Transform"`, then `T`, `R`, `S`, then `X`, `Y`, `Z`, each with a `Default`, a
//! `KeyCount` and `Key: time, value, kind, ...`. A kind is `L` (linear), `C` (constant, with a
//! letter) or `U` (cubic: a tangent mode and its numbers, then a weight mode and its).
//!
//! A take has one layer. An axis with no keys holds its `Default` for the take, which outranks the
//! model's own value: it becomes a curve of one key. A file with no `GlobalSettings` is refused:
//! its axes and unit are never assumed.

use std::collections::BTreeMap;

use super::binary::{Document, Node, Prop};

/// The first version with the scene of 7.x.
pub const FIRST_MODERN: u32 = 7000;
/// `KeyAttrFlags` of a curve with a cubic key.
const CUBIC: i32 = 0x8;

type R<T> = Result<T, String>;

fn node(name: &str, props: Vec<Prop>, children: Vec<Node>) -> Node {
    Node {
        name: name.into(),
        props,
        children,
    }
}

/// An object's name, in binary's `Name\0\x01Class` or a plain `Class::Name`: `Name`.
fn plain(raw: &str) -> &str {
    let name = raw.split("\u{0}\u{1}").next().unwrap_or(raw);
    name.strip_prefix("Model::").unwrap_or(name)
}

/// `Properties60` as `Properties70`: a `Property` has a name, a type and flags before its values; a
/// `P` a name, a type, a label and flags.
fn properties70(props60: &Node) -> Node {
    let entries = props60
        .children_named("Property")
        .map(|p| {
            let mut props = p.props.get(..2).map(<[Prop]>::to_vec).unwrap_or_default();
            props.push(Prop::Str(String::new()));
            props.extend(p.props.get(2..).unwrap_or(&[]).iter().cloned());
            node("P", props, Vec::new())
        })
        .collect();
    node("Properties70", Vec::new(), entries)
}

fn p(name: &str, value: Prop) -> Node {
    let text = |s: &str| Prop::Str(s.to_string());
    node(
        "P",
        vec![text(name), text(""), text(""), text(""), value],
        Vec::new(),
    )
}

fn letter(p: &Prop) -> Option<char> {
    match p {
        Prop::Char(c) => Some(*c as char),
        Prop::Str(s) if s.chars().count() == 1 => s.chars().next(),
        _ => None,
    }
}

/// A channel's keys.
#[derive(Default)]
struct Keys {
    times: Vec<i64>,
    values: Vec<f64>,
    cubic: bool,
}

/// The `Key` records of an axis: time, value, kind and what the kind brings.
fn keys_of(axis: &Node, what: &str) -> R<Keys> {
    let props: Vec<&Prop> = axis.children_named("Key").flat_map(|k| k.props.iter()).collect();
    let mut keys = Keys::default();
    let mut i = 0;
    let numbers = |i: &mut usize, n: usize, about: &str| -> R<()> {
        for _ in 0..n {
            let is_number = props
                .get(*i)
                .is_some_and(|x| letter(x).is_none() && x.as_f64().is_some());
            if !is_number {
                return Err(format!("{what}: a key's {about} should have {n} numbers"));
            }
            *i += 1;
        }
        Ok(())
    };
    while i < props.len() {
        let number_at = |at: usize| props.get(at).filter(|x| letter(x).is_none());
        let time = number_at(i)
            .and_then(|x| x.as_i64())
            .ok_or_else(|| format!("{what}: key {} has no whole time", keys.times.len()))?;
        let value = number_at(i + 1)
            .and_then(|x| x.as_f64())
            .ok_or_else(|| format!("{what}: key {} has no value", keys.times.len()))?;
        let kind = props.get(i + 2).and_then(|x| letter(x));
        i += 3;
        match kind {
            Some('L') => {}
            Some('C') => {
                if props.get(i).is_some_and(|x| letter(x).is_some()) {
                    i += 1;
                }
            }
            Some('U') => {
                keys.cubic = true;
                let tangent = props.get(i).and_then(|x| letter(x));
                i += 1;
                match tangent {
                    Some('s' | 'b') => numbers(&mut i, 2, "tangent")?,
                    Some('t') => numbers(&mut i, 3, "tangent")?,
                    Some('a') => {
                        if props.get(i).is_some_and(|x| letter(x).is_none()) {
                            numbers(&mut i, 2, "tangent")?;
                        }
                    }
                    other => {
                        return Err(format!(
                            "{what}: a cubic key's tangent mode {other:?} is not known"
                        ));
                    }
                }
                let weight = props.get(i).and_then(|x| letter(x));
                i += 1;
                match weight {
                    Some('n') => {}
                    Some('a') => numbers(&mut i, 2, "weight")?,
                    other => {
                        return Err(format!(
                            "{what}: a cubic key's weight mode {other:?} is not known"
                        ));
                    }
                }
            }
            other => return Err(format!("{what}: a key's kind {other:?} is not known")),
        }
        keys.times.push(time);
        keys.values.push(value);
    }
    if let Some(count) = axis.child("KeyCount").and_then(|c| c.i64_at(0))
        && usize::try_from(count).ok() != Some(keys.times.len())
    {
        return Err(format!(
            "{what}: KeyCount says {count} keys and the Key record holds {}",
            keys.times.len()
        ));
    }
    Ok(keys)
}

/// Builds the 7.x objects and connections, numbering them as it goes.
struct Build {
    next: i64,
    objects: Vec<Node>,
    connections: Vec<Node>,
}

impl Build {
    fn id(&mut self) -> i64 {
        self.next += 1;
        self.next
    }

    fn connect(&mut self, kind: &str, child: i64, parent: i64, prop: Option<&str>) {
        let mut props = vec![Prop::Str(kind.into()), Prop::I64(child), Prop::I64(parent)];
        if let Some(prop) = prop {
            props.push(Prop::Str(prop.into()));
        }
        self.connections.push(node("C", props, Vec::new()));
    }

    fn object(&mut self, kind: &str, id: i64, name: &str, children: Vec<Node>) {
        let props = vec![
            Prop::I64(id),
            Prop::Str(format!("{name}\u{0}\u{1}{kind}")),
            Prop::Str(String::new()),
        ];
        self.objects.push(node(kind, props, children));
    }

    fn curve(&mut self, curve_node: i64, axis: &str, keys: &Keys) {
        let id = self.id();
        let mut children = vec![
            node("KeyTime", vec![Prop::I64s(keys.times.clone())], Vec::new()),
            node("KeyValueFloat", vec![Prop::F64s(keys.values.clone())], Vec::new()),
        ];
        if keys.cubic {
            children.push(node("KeyAttrFlags", vec![Prop::I32s(vec![CUBIC])], Vec::new()));
        }
        self.object("AnimationCurve", id, "", children);
        self.connect("OP", id, curve_node, Some(&format!("d|{axis}")));
    }

    /// One take: a stack, its one layer, and a curve node per animated channel.
    fn take(&mut self, take: &Node, models: &BTreeMap<String, i64>) -> R<()> {
        let name = take.str_at(0).unwrap_or("").to_string();
        let stack = self.id();
        let layer = self.id();
        let mut range = Vec::new();
        for (record, start, stop) in [
            ("LocalTime", "LocalStart", "LocalStop"),
            ("ReferenceTime", "ReferenceStart", "ReferenceStop"),
        ] {
            if let Some(n) = take.child(record)
                && let (Some(a), Some(b)) = (n.i64_at(0), n.i64_at(1))
            {
                range.push(p(start, Prop::I64(a)));
                range.push(p(stop, Prop::I64(b)));
            }
        }
        self.object(
            "AnimationStack",
            stack,
            &name,
            vec![node("Properties70", Vec::new(), range)],
        );
        self.object("AnimationLayer", layer, "Layer", Vec::new());
        self.connect("OO", layer, stack, None);

        // read every axis first: an axis with only a Default needs the take's first key time
        let mut channels = Vec::new();
        let mut first_time: Option<i64> = None;
        for model in take.children_named("Model") {
            let model_name = plain(model.str_at(0).unwrap_or(""));
            let Some(&model_id) = models.get(model_name) else {
                continue;
            };
            let transform = model
                .children_named("Channel")
                .find(|c| c.str_at(0) == Some("Transform"));
            let Some(transform) = transform else { continue };
            for channel in transform.children_named("Channel") {
                let prop = match channel.str_at(0) {
                    Some("T") => "Lcl Translation",
                    Some("R") => "Lcl Rotation",
                    Some("S") => "Lcl Scaling",
                    _ => continue,
                };
                let mut axes = Vec::new();
                for axis in channel.children_named("Channel") {
                    let label = axis.str_at(0).unwrap_or("").to_string();
                    if !matches!(label.as_str(), "X" | "Y" | "Z") {
                        continue;
                    }
                    let what = format!("take {name:?}, {model_name} {prop} {label}");
                    let keys = keys_of(axis, &what)?;
                    if let Some(&t) = keys.times.first() {
                        first_time = Some(first_time.map_or(t, |f| f.min(t)));
                    }
                    let default = axis.child("Default").and_then(|d| d.f64_at(0));
                    axes.push((label, keys, default));
                }
                channels.push((model_id, prop, axes));
            }
        }
        let held_at = first_time.unwrap_or(0);
        for (model_id, prop, axes) in channels {
            if axes.iter().all(|(_, k, d)| k.times.is_empty() && d.is_none()) {
                continue;
            }
            let curve_node = self.id();
            self.object("AnimationCurveNode", curve_node, prop, Vec::new());
            self.connect("OO", curve_node, layer, None);
            self.connect("OP", curve_node, model_id, Some(prop));
            for (label, mut keys, default) in axes {
                if keys.times.is_empty() {
                    let Some(d) = default else { continue };
                    keys.times.push(held_at);
                    keys.values.push(d);
                }
                self.curve(curve_node, &label, &keys);
            }
        }
        Ok(())
    }
}

/// `doc`, an FBX 6 file, as the records of a 7.x one.
pub fn modern(doc: &Document) -> R<Document> {
    let version = doc.version;
    let objects = doc.top("Objects").ok_or("no Objects: the file has no scene")?;
    let settings = doc
        .top("GlobalSettings")
        .or_else(|| objects.child("GlobalSettings"))
        .ok_or_else(|| {
            format!("FBX {version} with no GlobalSettings: the file does not say its axes or units")
        })?;
    let settings_props = settings
        .child("Properties60")
        .map(properties70)
        .or_else(|| settings.child("Properties70").cloned())
        .ok_or_else(|| {
            format!(
                "FBX {version}: GlobalSettings has no properties: the file does not say its axes or units"
            )
        })?;

    let mut b = Build {
        next: 0,
        objects: Vec::new(),
        connections: Vec::new(),
    };
    let mut models: BTreeMap<String, i64> = BTreeMap::new();
    for m in objects.children_named("Model") {
        let name = plain(m.str_at(0).unwrap_or("")).to_string();
        if models.contains_key(&name) {
            return Err(format!(
                "two models are named {name:?}, and FBX {version} connects models by name: the file can't be read"
            ));
        }
        let id = b.id();
        let children = m.child("Properties60").map(properties70).into_iter().collect();
        let props = vec![
            Prop::I64(id),
            Prop::Str(format!("{name}\u{0}\u{1}Model")),
            Prop::Str(m.str_at(1).unwrap_or("").to_string()),
        ];
        b.objects.push(node("Model", props, children));
        models.insert(name, id);
    }
    if let Some(connections) = doc.top("Connections") {
        for c in connections.children_named("Connect") {
            if c.str_at(0) != Some("OO") {
                continue;
            }
            let id = |at| c.str_at(at).and_then(|s| models.get(plain(s))).copied();
            if let (Some(child), Some(parent)) = (id(1), id(2)) {
                b.connect("OO", child, parent, None);
            }
        }
    }
    if let Some(takes) = doc.top("Takes") {
        for take in takes.children_named("Take") {
            b.take(take, &models)?;
        }
    }
    Ok(Document {
        version,
        nodes: vec![
            node("GlobalSettings", Vec::new(), vec![settings_props]),
            node("Objects", Vec::new(), b.objects),
            node("Connections", Vec::new(), b.connections),
        ],
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::super::scene;
    use super::*;

    /// A second in FBX ticks: frame 30 at the 30 fps below.
    const SECOND: i64 = 46_186_158_000;

    /// A Y-up 30 fps file of a parent and a child 10 units along X, the parent's T X keyed `keys`.
    fn file(settings: &str, keys: &str) -> String {
        format!(
            r#"; FBX 6.1.0 project file
FBXHeaderExtension:  {{
    FBXVersion: 6100
}}
Objects:  {{
    Model: "Model::Hips", "Limb" {{
        Properties60:  {{
            Property: "Lcl Translation", "Lcl Translation", "A+",1,2,3
            Property: "InheritType", "enum", "",1
        }}
    }}
    Model: "Model::Child", "Limb" {{
        Properties60:  {{
            Property: "Lcl Translation", "Lcl Translation", "A+",10,0,0
            Property: "Lcl Rotation", "Lcl Rotation", "A+",0,0,45
        }}
    }}
    {settings}
}}
Connections:  {{
    Connect: "OO", "Model::Hips", "Model::Scene"
    Connect: "OO", "Model::Child", "Model::Hips"
}}
Takes:  {{
    Current: "Take 001"
    Take: "Take 001" {{
        LocalTime: 0,{SECOND}
        Model: "Model::Hips" {{
            Channel: "Transform" {{
                Channel: "T" {{
                    Channel: "X" {{
                        Default: 0
                        {keys}
                    }}
                    Channel: "Y" {{
                        Default: 7
                    }}
                }}
                LayerType: 1
            }}
        }}
    }}
}}
"#
        )
    }

    const SETTINGS: &str = r#"GlobalSettings:  {
        Properties60:  {
            Property: "UpAxis", "int", "",1
            Property: "FrontAxis", "int", "",2
            Property: "CoordAxis", "int", "",0
            Property: "UnitScaleFactor", "double", "",1
            Property: "TimeMode", "enum", "",6
        }
    }"#;

    fn read(s: &str) -> R<scene::Motion> {
        scene::motion(&super::super::read(s.as_bytes())?)
    }

    #[test]
    fn an_fbx_6_file_reads_as_its_numbers_say() {
        let keys = format!("KeyCount: 2\n Key: \n 0,0,L,\n {SECOND},30,L");
        let m = read(&file(SETTINGS, &keys)).unwrap();
        assert_eq!((m.version, m.frames, m.fps), (6100, 31, 30.0));
        assert_eq!(m.names, ["Hips", "Child"]);
        assert_eq!(m.classes[0], "Limb");
        assert_eq!(m.parents, [None, Some(0)]);
        assert_eq!(m.take, "Take 001");
        // By hand, on frame 15 (half a second): X keys 0 to 30 give 15; Y holds the take's Default
        // 7 over the model's 2; Z has no channel and stays the model's 3. Y up, cm: the rig's
        // (x, y, z) is (15, -3, 7) cm.
        let want = [0.15, -0.03, 0.07];
        for (got, want) in m.positions[15][0].iter().zip(&want) {
            assert!((got - want).abs() < 1e-9, "{:?}", m.positions[15][0]);
        }
        // the child 10 cm along the parent's X, which has no turn
        assert!(
            (m.positions[15][1][0] - 0.25).abs() < 1e-9,
            "{:?}",
            m.positions[15][1]
        );
    }

    #[test]
    fn every_kind_of_key_is_read_or_refused() {
        // constant, then cubic with each form this reader knows; straight lines, with a note
        let t: Vec<i64> = (0..5).map(|i| i * SECOND / 4).collect();
        let keys = format!(
            "KeyCount: 5\n Key: {},0,C,n,{},1,U,s,0,0,n,{},2,U,a,n,{},3,U,s,0,0,a,0.33,0.33,{},4,L",
            t[0], t[1], t[2], t[3], t[4]
        );
        let m = read(&file(SETTINGS, &keys)).unwrap();
        // frame 15 is the middle key: 2 cm
        assert!((m.positions[15][0][0] - 0.02).abs() < 1e-9);
        assert!(m.notes.iter().any(|n| n.contains("cubic keys")), "{:?}", m.notes);
        for (key, says) in [
            ("Key: 0,0,Q", "kind"),
            ("Key: 0,0", "kind"),
            ("Key: 0,0,U,z,n", "tangent mode"),
            ("Key: 0,0,U,s,1,n", "tangent"),
            ("Key: 0,0,U,s,0,0,q", "weight mode"),
            ("Key: 0.5,0,L", "whole time"),
            ("KeyCount: 2\n Key: 0,0,L", "KeyCount"),
        ] {
            let e = read(&file(SETTINGS, key)).unwrap_err();
            assert!(e.contains(says), "{key}: {e}");
        }
    }

    #[test]
    fn what_fbx_6_files_leave_out_is_refused_with_a_reason() {
        let e = read(&file("", "Key: 0,0,L")).unwrap_err();
        assert!(e.contains("6100") && e.contains("no GlobalSettings"), "{e}");
        let e = read(&file("GlobalSettings: {\n}\n", "Key: 0,0,L")).unwrap_err();
        assert!(e.contains("no properties"), "{e}");
        // Two of a name: which is which can't be told
        let twice = file(SETTINGS, "Key: 0,0,L").replace("Model::Child", "Model::Hips");
        let e = read(&twice).unwrap_err();
        assert!(e.contains("two models") && e.contains("Hips"), "{e}");
        // no takes at all
        let none = file(SETTINGS, "Key: 0,0,L");
        let at = none.find("Takes:").unwrap();
        let e = read(&none[..at]).unwrap_err();
        assert!(e.contains("no animation"), "{e}");
    }

    #[test]
    fn a_binary_key_spells_its_kind_in_bytes() {
        // what the binary container gives for Key: time, value, a C byte
        let axis = node(
            "Channel",
            vec![Prop::Str("X".into())],
            vec![node(
                "Key",
                vec![
                    Prop::I64(0),
                    Prop::F64(1.5),
                    Prop::Char(b'L'),
                    Prop::I64(9),
                    Prop::F64(2.5),
                    Prop::Char(b'C'),
                    Prop::Char(b'n'),
                ],
                Vec::new(),
            )],
        );
        let keys = keys_of(&axis, "x").unwrap();
        assert_eq!((keys.times, keys.values), (vec![0, 9], vec![1.5, 2.5]));
        assert!(!keys.cubic);
    }
}
