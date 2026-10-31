//! What a Unity `.meta` beside an FBX says: which of the file's bones is which humanoid bone
//! (`humanDescription.human`) and how one take is cut into named clips (`clipAnimations`: frame
//! ranges and loop flags). Read line by line: this is not a YAML reader, only the block mappings
//! and lists these two sections use. A `.meta` is input like the FBX itself, so sizes are capped
//! and a section that can't be read is refused with its line.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;

/// Largest `.meta` read.
pub const MAX_META_BYTES: u64 = 64 << 20;
/// Most clips one `.meta` may cut a take into.
pub const MAX_CLIPS: usize = 4096;
/// Longest clip name.
pub const MAX_CLIP_NAME: usize = 256;

/// The humanoid bone name -> the retarget's landmark. Fingers, eyes, jaw, shoulders and the upper
/// chests have no landmark. The landmark `l_shoulder` is the upper arm, as in `ac_core::jointmap`.
const HUMAN: [(&str, &str); 18] = [
    ("Hips", "hips"),
    ("Spine", ac_core::retarget::SPINE),
    ("Neck", "neck"),
    ("Head", "head"),
    ("LeftUpperArm", "l_shoulder"),
    ("LeftLowerArm", "l_elbow"),
    ("LeftHand", "l_wrist"),
    ("LeftUpperLeg", "l_hip"),
    ("LeftLowerLeg", "l_knee"),
    ("LeftFoot", "l_ankle"),
    ("LeftToes", "l_toe"),
    ("RightUpperArm", "r_shoulder"),
    ("RightLowerArm", "r_elbow"),
    ("RightHand", "r_wrist"),
    ("RightUpperLeg", "r_hip"),
    ("RightLowerLeg", "r_knee"),
    ("RightFoot", "r_ankle"),
    ("RightToes", "r_toe"),
];

type R<T> = Result<T, String>;

/// One named clip: frames as the `.meta` states them, in the file's own timeline, both ends in.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaClip {
    pub name: String,
    /// The take it cuts; empty when the `.meta` doesn't say.
    pub take: String,
    pub first: f64,
    pub last: f64,
    pub looping: bool,
}

/// What a `.meta` holds for the import. Both parts may be empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnityMeta {
    /// Landmark -> the file's bone.
    pub bones: BTreeMap<&'static str, String>,
    pub clips: Vec<MetaClip>,
}

struct Line<'a> {
    number: usize,
    indent: usize,
    dash: bool,
    text: &'a str,
}

/// The `.meta` beside `fbx` (`walk.fbx.meta`), if there is one.
pub fn beside(fbx: &Path) -> R<Option<UnityMeta>> {
    let mut path = fbx.as_os_str().to_owned();
    path.push(".meta");
    let path = Path::new(&path);
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("can't read {}: {e}", path.display())),
    };
    if size > MAX_META_BYTES {
        return Err(format!(
            "{} is {size} bytes, larger than {MAX_META_BYTES}",
            path.display()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: not UTF-8 text: {e}", path.display()))?;
    parse(text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn lines(text: &str) -> Vec<Line<'_>> {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let text = raw.trim();
            if text.is_empty() || text.starts_with('#') {
                return None;
            }
            Some(Line {
                number: i + 1,
                indent: raw.len() - raw.trim_start_matches(' ').len(),
                dash: text == "-" || text.starts_with("- "),
                text,
            })
        })
        .collect()
}

/// `key: value` or `key:`.
fn field(text: &str) -> Option<(&str, &str)> {
    if let Some((k, v)) = text.split_once(": ") {
        return Some((k.trim(), v.trim()));
    }
    text.strip_suffix(':').map(|k| (k.trim(), ""))
}

/// The lines under line `at`: deeper ones, and a list written at the key's own indent.
fn block(all: &[Line], at: usize) -> Range<usize> {
    let indent = all.get(at).map_or(0, |l| l.indent);
    let mut end = at + 1;
    while all
        .get(end)
        .is_some_and(|l| l.indent > indent || (l.indent == indent && l.dash))
    {
        end += 1;
    }
    at + 1..end
}

/// The line of `key` directly in `range`, whose first line sets the indent.
fn child(all: &[Line], range: &Range<usize>, key: &str) -> Option<usize> {
    let indent = all.get(range.start)?.indent;
    range.clone().find(|&i| {
        all.get(i)
            .is_some_and(|l| l.indent == indent && !l.dash && field(l.text).is_some_and(|(k, _)| k == key))
    })
}

/// The list items of `range` with their line, each with the scalar fields directly on it. Values
/// are left raw.
fn items<'a>(all: &[Line<'a>], range: &Range<usize>) -> Vec<(usize, BTreeMap<&'a str, &'a str>)> {
    let Some(indent) = all.get(range.start).map(|l| l.indent) else {
        return Vec::new();
    };
    let mut out: Vec<(usize, BTreeMap<&'a str, &'a str>)> = Vec::new();
    for l in range.clone().filter_map(|i| all.get(i)) {
        if l.indent == indent && l.dash {
            let mut m = BTreeMap::new();
            if let Some((k, v)) = l.text.strip_prefix("- ").and_then(field) {
                m.insert(k, v);
            }
            out.push((l.number, m));
        } else if l.indent == indent + 2
            && !l.dash
            && let (Some((_, m)), Some((k, v))) = (out.last_mut(), field(l.text))
        {
            m.entry(k).or_insert(v);
        }
    }
    out
}

/// A scalar: plain, `'single'` (`''` is a quote) or `"double"` (with the usual escapes).
fn unquote(v: &str, line: usize) -> R<String> {
    let bad = || format!("line {line}: can't read the string {v}");
    if let Some(rest) = v.strip_prefix('\'') {
        let inner = rest.strip_suffix('\'').ok_or_else(bad)?;
        return Ok(inner.replace("''", "'"));
    }
    let Some(rest) = v.strip_prefix('"') else {
        return Ok(v.to_string());
    };
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                return if chars.next().is_none() {
                    Ok(out)
                } else {
                    Err(bad())
                };
            }
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('t') => out.push('\t'),
                _ => return Err(bad()),
            },
            c => out.push(c),
        }
    }
    Err(bad())
}

fn number(fields: &BTreeMap<&str, &str>, key: &str, clip: &str, line: usize) -> R<f64> {
    let v = fields
        .get(key)
        .ok_or_else(|| format!("line {line}: clip {clip:?} has no {key}"))?;
    v.parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("line {line}: clip {clip:?}: {key} is {v}, not a finite number"))
}

fn flag(fields: &BTreeMap<&str, &str>, key: &str, clip: &str, line: usize) -> R<bool> {
    match fields.get(key).copied() {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(v) => Err(format!("line {line}: clip {clip:?}: {key} is {v}, not 0 or 1")),
    }
}

/// Read a `.meta`'s text.
pub fn parse(text: &str) -> R<UnityMeta> {
    let all = lines(text);
    let importer = all
        .iter()
        .position(|l| l.indent == 0 && l.text == "ModelImporter:")
        .ok_or("not a model's .meta: it has no ModelImporter section")?;
    let body = block(&all, importer);
    let mut meta = UnityMeta::default();

    if let Some(section) = child(&all, &body, "humanDescription") {
        let section = block(&all, section);
        if let Some(human) = child(&all, &section, "human") {
            let list = block(&all, human);
            for (line, f) in items(&all, &list) {
                let (Some(bone), Some(human)) = (f.get("boneName"), f.get("humanName")) else {
                    return Err(format!("line {line}: a human entry needs boneName and humanName"));
                };
                let human = unquote(human, line)?;
                let Some(&(_, lm)) = HUMAN.iter().find(|(h, _)| *h == human) else {
                    continue;
                };
                let bone = unquote(bone, line)?;
                if bone.is_empty() {
                    return Err(format!("line {line}: {human} names no bone"));
                }
                if let Some((other, _)) = meta.bones.iter().find(|(l, b)| **l != lm && **b == bone) {
                    return Err(format!(
                        "line {line}: bone {bone:?} is stated for both {lm} and {other}"
                    ));
                }
                if meta.bones.get(lm).is_some_and(|b| *b != bone) {
                    return Err(format!(
                        "line {line}: {human} is stated twice, with different bones"
                    ));
                }
                meta.bones.insert(lm, bone);
            }
        }
    }

    if let Some(section) = child(&all, &body, "animations") {
        let section = block(&all, section);
        if let Some(clips) = child(&all, &section, "clipAnimations") {
            let list = block(&all, clips);
            let mut names: BTreeMap<String, usize> = BTreeMap::new();
            for (line, f) in items(&all, &list) {
                if meta.clips.len() >= MAX_CLIPS {
                    return Err(format!("more than {MAX_CLIPS} clips"));
                }
                let raw = f
                    .get("name")
                    .ok_or_else(|| format!("line {line}: a clip has no name"))?;
                let name = unquote(raw, line)?.trim().to_string();
                if name.is_empty() || name.len() > MAX_CLIP_NAME || name.chars().any(char::is_control) {
                    return Err(format!(
                        "line {line}: a clip's name must be one line of 1 to {MAX_CLIP_NAME} bytes"
                    ));
                }
                if let Some(first) = names.insert(name.to_lowercase(), line) {
                    return Err(format!(
                        "line {line}: two clips are named {name:?} (the other on line {first})"
                    ));
                }
                let take = match f.get("takeName") {
                    Some(t) => unquote(t, line)?,
                    None => String::new(),
                };
                let first = number(&f, "firstFrame", &name, line)?;
                let last = number(&f, "lastFrame", &name, line)?;
                // loopTime is what the importer's own option writes; loop is its older spelling
                let looping = flag(&f, "loopTime", &name, line)? || flag(&f, "loop", &name, line)?;
                meta.clips.push(MetaClip {
                    name,
                    take,
                    first,
                    last,
                    looping,
                });
            }
        }
    }
    Ok(meta)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;

    const META: &str = "fileFormatVersion: 2\r
guid: 0123\r
ModelImporter:\r
  serializedVersion: 19\r
  animations:\r
    legacyGenerateAnimations: 4\r
    clipAnimations:\r
    - serializedVersion: 16\r
      name: 'It''s: walking'\r
      takeName: Take 001\r
      firstFrame: 12.5\r
      lastFrame: 30\r
      loop: 0\r
      loopTime: 1\r
      curves:\r
      - name: not a clip\r
        curve: 1\r
      events: []\r
      transformMask:\r
      - path: \r
        weight: 1\r
    - name: \"Run \\\"fast\\\"\"\r
      firstFrame: 31\r
      lastFrame: 40\r
      loop: 1\r
    isReadable: 1\r
  humanDescription:\r
    serializedVersion: 2\r
    human:\r
    - boneName: Bone 00\r
      humanName: Hips\r
      limit:\r
        min: {x: 0, y: 0, z: 0}\r
    - boneName: ns:Thumb\r
      humanName: Left Thumb Proximal\r
    - boneName: ns:Thigh\r
      humanName: LeftUpperLeg\r
    skeleton:\r
    - name: Bone 00\r
      parentName: root\r
";

    #[test]
    fn the_bone_map_and_the_clips_are_read_and_nested_lists_are_not_taken_for_them() {
        let m = parse(META).unwrap();
        assert_eq!(m.bones.len(), 2, "the finger has no landmark: {:?}", m.bones);
        assert_eq!(m.bones["hips"], "Bone 00");
        assert_eq!(m.bones["l_hip"], "ns:Thigh");
        assert_eq!(
            m.clips,
            [
                MetaClip {
                    name: "It's: walking".into(),
                    take: "Take 001".into(),
                    first: 12.5,
                    last: 30.0,
                    looping: true,
                },
                MetaClip {
                    name: "Run \"fast\"".into(),
                    take: String::new(),
                    first: 31.0,
                    last: 40.0,
                    looping: true,
                },
            ]
        );
    }

    #[test]
    fn empty_sections_give_an_empty_meta() {
        let m = parse(
            "ModelImporter:\n  animations:\n    clipAnimations: []\n    isReadable: 1\n  humanDescription:\n    human: []\n    skeleton: []\n",
        )
        .unwrap();
        assert_eq!(m, UnityMeta::default());
        assert_eq!(parse("ModelImporter:\n  a: 1\n").unwrap(), UnityMeta::default());
    }

    fn clip(body: &str) -> R<UnityMeta> {
        parse(&format!(
            "ModelImporter:\n  animations:\n    clipAnimations:\n{body}"
        ))
    }

    fn human(body: &str) -> R<UnityMeta> {
        parse(&format!(
            "ModelImporter:\n  humanDescription:\n    human:\n{body}"
        ))
    }

    #[test]
    fn what_cant_be_read_is_refused_with_its_reason() {
        for (got, want) in [
            (parse("DefaultImporter:\n  a: 1\n"), "no ModelImporter"),
            (parse(""), "no ModelImporter"),
            (clip("    - firstFrame: 0\n      lastFrame: 1\n"), "no name"),
            (clip("    - name: A\n      lastFrame: 1\n"), "no firstFrame"),
            (clip("    - name: A\n      firstFrame: 0\n"), "no lastFrame"),
            (
                clip("    - name: A\n      firstFrame: x\n      lastFrame: 1\n"),
                "not a finite number",
            ),
            (
                clip("    - name: A\n      firstFrame: .inf\n      lastFrame: 1\n"),
                "not a finite number",
            ),
            (
                clip("    - name: A\n      firstFrame: NaN\n      lastFrame: 1\n"),
                "not a finite number",
            ),
            (
                clip("    - name: A\n      firstFrame: 0\n      lastFrame: 1\n      loopTime: 2\n"),
                "not 0 or 1",
            ),
            (
                clip("    - name: 'A\n      firstFrame: 0\n      lastFrame: 1\n"),
                "can't read the string",
            ),
            (
                clip("    - name: \"A\\q\"\n      firstFrame: 0\n      lastFrame: 1\n"),
                "can't read the string",
            ),
            (
                clip("    - name: \"  \"\n      firstFrame: 0\n      lastFrame: 1\n"),
                "must be one line",
            ),
            (
                clip(
                    "    - name: Walk\n      firstFrame: 0\n      lastFrame: 1\n    - name: WALK\n      firstFrame: 2\n      lastFrame: 3\n",
                ),
                "two clips are named",
            ),
            (human("    - boneName: A\n"), "needs boneName and humanName"),
            (
                human("    - boneName: ''\n      humanName: Hips\n"),
                "names no bone",
            ),
            (
                human("    - boneName: A\n      humanName: Hips\n    - boneName: B\n      humanName: Hips\n"),
                "stated twice",
            ),
            (
                human("    - boneName: A\n      humanName: Hips\n    - boneName: A\n      humanName: Head\n"),
                "is stated for both",
            ),
        ] {
            let e = got.unwrap_err();
            assert!(e.contains(want), "{e} should say {want}");
        }
    }

    #[test]
    fn too_many_clips_are_refused() {
        let mut body = String::new();
        for i in 0..=MAX_CLIPS {
            body += &format!("    - name: C{i}\n      firstFrame: 0\n      lastFrame: 1\n");
        }
        assert!(clip(&body).unwrap_err().contains("more than"));
    }
}
