//! Named markers on the timeline: a frame and a name, at most one marker a frame, kept sorted by
//! frame. They belong to the animation (saved in its clip file, undone with it).

use crate::clip::Clip;

/// Most markers one animation holds.
pub const MAX_MARKERS: usize = 10_000;
/// Longest marker name (bytes).
pub const MAX_NAME: usize = 200;
/// Latest frame a marker may sit on (the clip file's own cap).
pub const MAX_FRAME: i64 = 10_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub frame: i64,
    pub name: String,
}

type R<T> = Result<T, String>;

/// A name that may be stored: trimmed, not empty, not too long, one line.
pub fn clean_name(name: &str) -> R<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("a marker needs a name".into());
    }
    if n.len() > MAX_NAME {
        return Err(format!("marker names are at most {MAX_NAME} bytes"));
    }
    if n.contains(['\n', '\r']) {
        return Err("a marker name is one line".into());
    }
    Ok(n.to_string())
}

fn check_frame(frame: i64) -> R<()> {
    if (0..=MAX_FRAME).contains(&frame) {
        Ok(())
    } else {
        Err(format!("frame {frame} is outside 0..={MAX_FRAME}"))
    }
}

fn index(clip: &Clip, frame: i64) -> R<usize> {
    clip.markers
        .iter()
        .position(|m| m.frame == frame)
        .ok_or_else(|| format!("no marker on frame {frame}"))
}

/// The name a new marker gets when none is given: "Marker n", the first n not taken.
pub fn default_name(clip: &Clip) -> String {
    (1..)
        .map(|n| format!("Marker {n}"))
        .find(|n| !clip.markers.iter().any(|m| &m.name == n))
        .unwrap_or_else(|| "Marker".into())
}

pub fn add(clip: &mut Clip, frame: i64, name: &str) -> R<()> {
    check_frame(frame)?;
    let name = clean_name(name)?;
    if clip.markers.len() >= MAX_MARKERS {
        return Err(format!("an animation holds at most {MAX_MARKERS} markers"));
    }
    if let Some(m) = clip.markers.iter().find(|m| m.frame == frame) {
        return Err(format!("frame {frame} already has the marker {:?}", m.name));
    }
    clip.markers.push(Marker { frame, name });
    clip.markers.sort_by_key(|m| m.frame);
    Ok(())
}

pub fn rename(clip: &mut Clip, frame: i64, name: &str) -> R<()> {
    let name = clean_name(name)?;
    let i = index(clip, frame)?;
    if let Some(m) = clip.markers.get_mut(i) {
        m.name = name;
    }
    Ok(())
}

pub fn move_to(clip: &mut Clip, frame: i64, to: i64) -> R<()> {
    check_frame(to)?;
    let i = index(clip, frame)?;
    if to != frame
        && let Some(m) = clip.markers.iter().find(|m| m.frame == to)
    {
        return Err(format!("frame {to} already has the marker {:?}", m.name));
    }
    if let Some(m) = clip.markers.get_mut(i) {
        m.frame = to;
    }
    clip.markers.sort_by_key(|m| m.frame);
    Ok(())
}

pub fn delete(clip: &mut Clip, frame: i64) -> R<()> {
    let i = index(clip, frame)?;
    clip.markers.remove(i);
    Ok(())
}

/// The markers as JSON (`[{frame, name}]`, by frame).
pub fn json(clip: &Clip) -> serde_json::Value {
    clip.markers
        .iter()
        .map(|m| serde_json::json!({ "frame": m.frame, "name": m.name }))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn clip() -> Clip {
        Clip::new(vec!["root".into()], 30.0)
    }

    #[test]
    fn markers_stay_sorted_and_one_a_frame() {
        let mut c = clip();
        add(&mut c, 20, "B").unwrap();
        add(&mut c, 5, " A ").unwrap();
        assert_eq!(c.markers.first().unwrap().name, "A");
        assert!(add(&mut c, 20, "C").is_err());
        assert!(move_to(&mut c, 5, 20).is_err());
        move_to(&mut c, 5, 30).unwrap();
        assert_eq!(c.markers.iter().map(|m| m.frame).collect::<Vec<_>>(), [20, 30]);
        assert_eq!(default_name(&c), "Marker 1");
        delete(&mut c, 20).unwrap();
        assert!(delete(&mut c, 20).is_err());
        assert!(rename(&mut c, 30, "  ").is_err());
        assert!(add(&mut c, -1, "x").is_err());
    }
}
