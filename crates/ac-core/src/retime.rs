//! Retiming: insert or delete frames at a frame with ripple (everything after slides), and scale
//! keys about a pivot.
//!
//! Insert and delete ripple through the whole animation: every keys layer, link layers' start
//! offsets, layer ranges and the markers. Scaling moves the given keys of the edited layer only.
//! In-between fills (AI keys) slide with their keys; a fill the edit stretched or cut is named in
//! the notes, so the person can fill it again.

use std::collections::BTreeSet;

use crate::clip::{Clip, KeySource, LayerSource};

/// Latest frame a key may land on (the clip file's cap).
pub const MAX_FRAME: i64 = crate::clipfile::MAX_FRAME;

type R<T> = Result<T, String>;

/// What a retime did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Retimed {
    /// Keys that changed frame (every layer).
    pub moved: usize,
    /// Keys removed (deleted frames), as `layer name: frame`.
    pub removed: Vec<String>,
    /// Markers inside deleted frames, removed with them.
    pub markers_removed: Vec<String>,
    /// What the person should know (fills to redo).
    pub notes: Vec<String>,
}

/// The AI fills of the edited layer whose length a retime of frames `from..to` changes: those
/// between manual keys `a < to` and `b >= from` (an insert passes `to == from == at`).
fn fills_touched(clip: &Clip, from: i64, to: i64) -> Vec<String> {
    let manual = crate::edits::manual_frames(clip);
    let ai: Vec<i64> = clip
        .keys()
        .iter()
        .filter(|k| k.source == KeySource::Ai)
        .map(|k| k.frame)
        .collect();
    manual
        .windows(2)
        .filter_map(|w| match w {
            [a, b] if *a < to && from <= *b && ai.iter().any(|f| a < f && f < b) => {
                Some(format!("the in-between {a}–{b} changed length: fill it again"))
            }
            _ => None,
        })
        .collect()
}

/// Insert `count` empty frames before frame `at`: everything on `at` or later moves `count` later.
pub fn insert(clip: &mut Clip, at: i64, count: i64) -> R<Retimed> {
    if count < 1 {
        return Err("insert at least one frame".into());
    }
    let last = clip
        .layers
        .iter()
        .flat_map(|l| l.keys.last())
        .map(|k| k.frame)
        .chain(clip.markers.iter().map(|m| m.frame))
        .max()
        .unwrap_or(0);
    if last >= at && last.saturating_add(count) > MAX_FRAME {
        return Err(format!("frame {last} would move past {MAX_FRAME}"));
    }
    let mut out = Retimed {
        notes: fills_touched(clip, at, at),
        ..Retimed::default()
    };
    let shift = |f: f64| if f >= at as f64 { f + count as f64 } else { f };
    for lay in &mut clip.layers {
        for k in lay.keys.iter_mut().filter(|k| k.frame >= at) {
            k.frame += count;
            out.moved += 1;
        }
        if lay.source == LayerSource::Link {
            lay.offset = shift(lay.offset);
        }
        lay.start = lay.start.map(shift);
        lay.end = lay.end.map(shift);
    }
    for m in clip.markers.iter_mut().filter(|m| m.frame >= at) {
        m.frame += count;
    }
    Ok(out)
}

/// Delete frames `at..at+count`: keys and markers on them go, everything after moves `count`
/// earlier. Refused when the base layer would lose every key.
pub fn delete(clip: &mut Clip, at: i64, count: i64) -> R<Retimed> {
    if count < 1 {
        return Err("delete at least one frame".into());
    }
    let end = at.saturating_add(count);
    let inside = |f: i64| at <= f && f < end;
    if clip.base().keys.iter().all(|k| inside(k.frame)) && !clip.base().keys.is_empty() {
        return Err(format!(
            "frames {at}–{} hold every key of the base layer: an animation keeps at least one",
            end - 1
        ));
    }
    let mut out = Retimed {
        notes: fills_touched(clip, at, end),
        ..Retimed::default()
    };
    let shift = |f: f64| {
        if f >= end as f64 {
            f - count as f64
        } else if f >= at as f64 {
            at as f64
        } else {
            f
        }
    };
    for lay in &mut clip.layers {
        let name = lay.name.clone();
        out.removed.extend(
            lay.keys
                .iter()
                .filter(|k| inside(k.frame))
                .map(|k| format!("{name}: {}", k.frame)),
        );
        lay.keys.retain(|k| !inside(k.frame));
        for k in lay.keys.iter_mut().filter(|k| k.frame >= end) {
            k.frame -= count;
            out.moved += 1;
        }
        if lay.source == LayerSource::Link {
            lay.offset = shift(lay.offset);
        }
        lay.start = lay.start.map(shift);
        lay.end = lay.end.map(shift);
    }
    out.markers_removed = clip
        .markers
        .iter()
        .filter(|m| inside(m.frame))
        .map(|m| m.name.clone())
        .collect();
    clip.markers.retain(|m| !inside(m.frame));
    for m in clip.markers.iter_mut().filter(|m| m.frame >= end) {
        m.frame -= count;
    }
    if !out.markers_removed.is_empty() {
        out.notes.push(format!(
            "markers removed with the frames: {}",
            out.markers_removed.join(", ")
        ));
    }
    Ok(out)
}

/// Scale the edited layer's keys on `frames` about `pivot`: a key on `f` goes to
/// `round(pivot + (f - pivot) * factor)`. Refused as a whole when two keys would land on one
/// frame, one would land on a key that stays, or one would go before frame 0.
pub fn scale(clip: &mut Clip, frames: &[i64], pivot: f64, factor: f64) -> R<Retimed> {
    if !(factor.is_finite() && factor > 0.0) {
        return Err(format!("scale factor {factor} must be above 0"));
    }
    if !pivot.is_finite() {
        return Err(format!("pivot {pivot} is not a frame"));
    }
    let chosen: BTreeSet<i64> = frames.iter().copied().collect();
    if chosen.is_empty() {
        return Err("no keys to scale".into());
    }
    let have: BTreeSet<i64> = clip.keys().iter().map(|k| k.frame).collect();
    if let Some(f) = chosen.iter().find(|f| !have.contains(f)) {
        return Err(format!("no key on frame {f}"));
    }
    let to = |f: i64| (pivot + (f as f64 - pivot) * factor).round();
    let mut landed: BTreeSet<i64> = BTreeSet::new();
    for &f in &chosen {
        let t = to(f);
        if !(0.0..=MAX_FRAME as f64).contains(&t) {
            return Err(format!(
                "key {f} would land on frame {t}, outside 0..={MAX_FRAME}"
            ));
        }
        let t = t as i64;
        if !landed.insert(t) {
            return Err(format!(
                "two keys would land on frame {t}: scale by more, or select fewer keys"
            ));
        }
        if have.contains(&t) && !chosen.contains(&t) {
            return Err(format!(
                "key {f} would land on the key at {t}, which is not scaled"
            ));
        }
    }
    let mut out = Retimed::default();
    let e = clip.edit;
    let lay = clip.layers.get_mut(e).ok_or("no layer to edit")?;
    let mut ai = false;
    for k in lay.keys.iter_mut().filter(|k| chosen.contains(&k.frame)) {
        let t = to(k.frame) as i64;
        if t != k.frame {
            out.moved += 1;
            ai |= k.source == KeySource::Ai;
        }
        k.frame = t;
    }
    lay.keys.sort_by_key(|k| k.frame);
    if ai {
        out.notes
            .push("AI in-between keys were scaled with the rest: fill again for a fresh in-between".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::clip::{Interp, Key};

    fn clip(frames: &[i64]) -> Clip {
        let mut c = Clip::new(vec!["root".into()], 30.0);
        for &f in frames {
            c.set_key(Key::new(f, vec![[0.0, 0.0, 0.0, 1.0]], [0.0; 3], Interp::Smooth))
                .unwrap();
        }
        c
    }

    fn frames(c: &Clip) -> Vec<i64> {
        c.keys().iter().map(|k| k.frame).collect()
    }

    #[test]
    fn insert_and_delete_ripple() {
        let mut c = clip(&[0, 10, 20]);
        crate::markers::add(&mut c, 15, "hit").unwrap();
        insert(&mut c, 10, 5).unwrap();
        assert_eq!(frames(&c), [0, 15, 25]);
        assert_eq!(c.markers.first().unwrap().frame, 20);
        let r = delete(&mut c, 14, 7).unwrap(); // frames 14..=20: key 15 and the marker go
        assert_eq!(frames(&c), [0, 18]);
        assert_eq!(r.markers_removed, ["hit"]);
        assert!(delete(&mut c, 0, 100).is_err());
    }

    #[test]
    fn scale_about_a_pivot_refuses_collisions() {
        let mut c = clip(&[10, 12, 20]);
        scale(&mut c, &[10, 20], 10.0, 1.5).unwrap();
        assert_eq!(frames(&c), [10, 12, 25]);
        assert!(scale(&mut c, &[10, 12], 10.0, 0.1).is_err());
        assert!(scale(&mut c, &[25], 0.0, 0.48).is_err()); // lands on 12
        assert!(scale(&mut c, &[11], 0.0, 2.0).is_err());
    }
}
