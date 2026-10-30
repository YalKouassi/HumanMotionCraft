//! The learned in-betweener, as the core sees it: a model the front end loads (`ac-ai`), the
//! settings a fill runs with, and the [`Filler`] that writes its frames as AI keys.
//!
//! The core does not know how a model works; it hands it the clip and two manual keys and gets
//! back every bone's rotation and the root offset for each frame between them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::anchors::{self, Anchor, Segment};
use crate::clip::{Clip, Interp, Key, KeySource, Links};
use crate::edits::{self, Filler};
use crate::posesource::PoseSource;
use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

/// A fill kept in the clip: its span, the model label, and what [`Document::keep_possible`] said
/// (impossible poses the model made, bent back): in words (`notes`) and as data (`kept`).
#[derive(Debug, Clone, PartialEq)]
pub struct Filled {
    pub from: i64,
    pub to: i64,
    pub model: String,
    pub notes: Vec<String>,
    pub kept: crate::possible::Kept,
    /// The anchor poses placed inside a gap over the model's limit (`crate::anchors`; empty:
    /// the model took the gap in one), and the library segment they came from.
    pub anchors: Vec<Anchor>,
    pub segment: Option<Segment>,
}

/// A fill worked out but not kept: `clip` is `base` with the span `from..to` filled.
#[derive(Debug)]
pub struct Preview {
    pub base: Clip,
    pub links: BTreeMap<String, Clip>,
    pub clip: Clip,
    pub from: i64,
    pub to: i64,
    /// The model label ("parts walk"), or "plain interpolation".
    pub label: String,
    pub plain: bool,
    /// The settings it was worked out with (before `plain` set the AI force to 0).
    pub settings: FillSettings,
    /// What the fill's check for impossible poses said ([`Filled::notes`]).
    pub notes: Vec<String>,
}

/// How a fill runs (cf-animate's AI panel).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FillSettings {
    /// A style the model knows ("any": none).
    pub style: String,
    /// Style guidance: 0 no style, 1 the style, above 1 stronger (0..3).
    pub style_force: f64,
    /// How much of the network's change to the plain interpolation to keep (0..1.5).
    pub ai_force: f64,
    /// Pull the root onto the keys' own path (0..1).
    pub trajectory: f64,
    /// Keep the neck and head on the keys' spline (0..1).
    pub head: f64,
}

impl Default for FillSettings {
    fn default() -> Self {
        FillSettings {
            style: "any".into(),
            style_force: 1.0,
            ai_force: 1.0,
            trajectory: 0.0,
            head: 0.0,
        }
    }
}

/// What a model is (its card).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelInfo {
    pub tag: String,
    /// The bones the network sets (the rest follow the keys' spline).
    pub bones: Vec<String>,
    /// Styles it was trained with ("any" first), empty when it has none.
    pub styles: Vec<String>,
    /// Whether it can fill around bones another layer drives.
    pub given: bool,
    /// Frames of motion it looks at before the first key.
    pub context: usize,
    /// Most frames it fills between two keys.
    pub max_gap: usize,
    /// Whether its training data allows commercial use.
    pub commercial: bool,
    pub warning: String,
}

pub trait Inbetweener: Send + Sync + std::fmt::Debug {
    fn info(&self) -> &ModelInfo;

    /// A hash of the weights (`session.info` shows it, so two runs can be told apart); `None`
    /// when the model gives none.
    fn hash(&self) -> Option<String> {
        None
    }

    /// Rotations (every bone) and root offsets (hip heights) for frames `a+1 .. b-1`. `given`:
    /// the model's bones another layer drives (kept as the clip has them).
    #[allow(clippy::too_many_arguments)]
    fn fill(
        &self,
        sk: &Skeleton,
        clip: &Clip,
        links: &dyn Links,
        a: i64,
        b: i64,
        settings: &FillSettings,
        given: Option<&[String]>,
    ) -> R<(Vec<Vec<Quat>>, Vec<Vec3>)>;

    /// [`Inbetweener::fill`], and the feet the model says are planted on each filled frame
    /// (`[left, right]`), when it says so (`None`: a model without foot contacts).
    #[allow(clippy::too_many_arguments)]
    fn fill_contacts(
        &self,
        sk: &Skeleton,
        clip: &Clip,
        links: &dyn Links,
        a: i64,
        b: i64,
        settings: &FillSettings,
        given: Option<&[String]>,
    ) -> R<FillWithContacts> {
        let (r, o) = self.fill(sk, clip, links, a, b, settings, given)?;
        Ok((r, o, None))
    }
}

/// Per filled frame, whether the left and the right foot is planted.
pub type Contacts = Vec<[bool; 2]>;

/// What [`Inbetweener::fill_contacts`] gives: rotations, root offsets and the foot contacts.
pub type FillWithContacts = (Vec<Vec<Quat>>, Vec<Vec3>, Option<Contacts>);

/// The style a key's model label names ("parts walk": "walk"), or "any" when it names none.
pub fn label_style(label: &str) -> &str {
    label.split_once(' ').map_or("any", |(_, style)| style)
}

/// A style as people read it: "walk" is "Walk", "arms-above-head" is "Arms above head", "any"
/// is "Any style". Styles are the labels of the model's training data (`training/ac_train/labels.py`).
pub fn style_name(style: &str) -> String {
    let mut c = style.chars();
    match c.next() {
        _ if style == "any" => "Any style".into(),
        Some(first) => first
            .to_uppercase()
            .chain(c)
            .map(|ch| if ch == '-' { ' ' } else { ch })
            .collect(),
        None => "Any style".into(),
    }
}

/// A model with what one fill needs, owned (so it can run while the clip is borrowed mutably).
pub struct AiFiller {
    pub model: Arc<dyn Inbetweener>,
    pub skeleton: Arc<Skeleton>,
    pub links: BTreeMap<String, Clip>,
    pub settings: FillSettings,
    /// The foot contacts of each span it filled (models that predict them), for
    /// [`Document::plant_steps`].
    pub contacts: std::sync::Mutex<BTreeMap<(i64, i64), Contacts>>,
    /// Where anchor poses come from for a gap over the model's limit (`None`: such a gap is
    /// refused): the document's [`PoseSource`], unless the fill asked for no anchors.
    pub source: Option<Arc<dyn PoseSource>>,
    /// The anchors placed in each long span it filled, by span.
    pub placed: std::sync::Mutex<BTreeMap<(i64, i64), Placed>>,
}

/// What a long fill placed: its anchors, the library segment they came from, and the notes of
/// planning them.
#[derive(Debug, Clone, Default)]
pub struct Placed {
    pub anchors: Vec<Anchor>,
    pub segment: Option<Segment>,
    pub notes: Vec<String>,
}

impl AiFiller {
    /// The foot contacts the model gave for the span `a..b` it filled last (`None`: none).
    pub fn contacts_of(&self, a: i64, b: i64) -> Option<Contacts> {
        self.contacts.lock().ok().and_then(|mut m| m.remove(&(a, b)))
    }

    /// The anchors placed in the span `a..b` it filled last (`None`: the model took it in one).
    pub fn placed_of(&self, a: i64, b: i64) -> Option<Placed> {
        self.placed.lock().ok().and_then(|mut m| m.remove(&(a, b)))
    }

    /// Most frames the model fills in one span.
    pub fn max_gap(&self) -> i64 {
        i64::try_from(self.model.info().max_gap)
            .unwrap_or(i64::MAX)
            .min(edits::MAX_FILL)
    }

    /// A gap over the model's limit: anchor poses from the library split it into spans the model
    /// takes, each filled in turn on a copy of the clip (so the second span sees the first as its
    /// context), and the frames between the keys are read back from that copy. The anchors are
    /// kept under [`AiFiller::placed_of`] for the caller to tag, ease and plant around.
    fn fill_long(&self, clip: &Clip, a: i64, b: i64) -> R<(Vec<Vec<Quat>>, Vec<Vec3>, String)> {
        let max = self.max_gap();
        let Some(source) = &self.source else {
            return Err(crate::error::hinted(
                format!(
                    "the gap {a}–{b} holds {} frames, more than the model fills at once ({max}), and there is no library to take anchor poses from",
                    b - a - 1
                ),
                "run with a library (the app has one; ac --library <folder>), or key a pose inside the gap",
            ));
        };
        let sk = &self.skeleton;
        let bones: Vec<usize> = self
            .model
            .info()
            .bones
            .iter()
            .filter_map(|n| sk.bone(n))
            .collect();
        let plan = anchors::plan(sk, clip, &self.links, &bones, source.as_ref(), (a, b), max)?;
        let mut tmp = clip.clone();
        let additive = tmp
            .layers
            .get(tmp.edit)
            .is_some_and(|l| l.mode == crate::clip::LayerMode::Additive);
        let at: Vec<f64> = plan.anchors.iter().map(|x| x.frame as f64).collect();
        let below = if additive {
            Some(tmp.sample_below(&at, tmp.edit, &self.links)?)
        } else {
            None
        };
        for (i, (an, pose)) in plan.anchors.iter().zip(&plan.poses).enumerate() {
            let (mut r, mut o) = (pose.rotations.clone(), pose.root);
            if let Some((br, bo)) = &below {
                // an additive layer keys the turn on top of the layers below
                for (q, base) in r.iter_mut().zip(br.get(i).into_iter().flatten()) {
                    *q = quat::normalize(quat::mul(quat::conj(*base), *q));
                }
                if let Some(base) = bo.get(i) {
                    o = quat::sub3(o, *base);
                }
            }
            let mut k = Key::new(an.frame, r, o, Interp::Smooth);
            k.source = KeySource::Ai;
            k.model = anchors::tag(an);
            tmp.set_key(k)?;
        }
        let mut bounds = vec![a];
        bounds.extend(plan.anchors.iter().map(|x| x.frame));
        bounds.push(b);
        for w in bounds.windows(2) {
            let &[p, q] = w else { continue };
            let note = edits::refill(&mut tmp, p, q, self, &self.links)?;
            if !note.is_empty() {
                return Err(note);
            }
        }
        let frames: Vec<f64> = (a + 1..b).map(|f| f as f64).collect();
        let (r, o) = tmp.sample_with(&frames, &self.links)?;
        if let Ok(mut m) = self.placed.lock() {
            m.insert(
                (a, b),
                Placed {
                    anchors: plan.anchors,
                    segment: Some(plan.segment),
                    notes: plan.notes,
                },
            );
        }
        Ok((r, o, self.label()))
    }

    /// The key label: the tag, and the style when the model honours one.
    pub fn label(&self) -> String {
        let info = self.model.info();
        let s = &self.settings.style;
        if s != "any" && info.styles.iter().any(|x| x == s) && self.settings.style_force != 0.0 {
            format!("{} {s}", info.tag)
        } else {
            info.tag.clone()
        }
    }
}

impl Filler for AiFiller {
    fn long(&self) -> bool {
        self.source.is_some()
    }

    fn fill(&self, clip: &Clip, a: i64, b: i64) -> R<(Vec<Vec<Quat>>, Vec<Vec3>, String)> {
        if b - a - 1 > self.max_gap() {
            return self.fill_long(clip, a, b);
        }
        let info = self.model.info();
        // a layer that drives some parts: the model's other bones come from the layers it sits on
        let lay = clip.layers.get(clip.edit).ok_or("no layer to edit")?;
        let given: Option<Vec<String>> = match (&lay.only, info.given) {
            (Some(only), true) => {
                let mine: Vec<&str> = only
                    .iter()
                    .filter_map(|&i| clip.bones.get(i).map(String::as_str))
                    .collect();
                let g: Vec<String> = info
                    .bones
                    .iter()
                    .filter(|n| !mine.contains(&n.as_str()))
                    .cloned()
                    .collect();
                (!g.is_empty()).then_some(g)
            }
            _ => None,
        };
        let (r, o, contacts) = self.model.fill_contacts(
            &self.skeleton,
            clip,
            &self.links,
            a,
            b,
            &self.settings,
            given.as_deref(),
        )?;
        if let Ok(mut m) = self.contacts.lock() {
            match contacts {
                Some(c) => m.insert((a, b), c),
                None => m.remove(&(a, b)),
            };
        }
        Ok((r, o, self.label()))
    }
}

/// How still (m a frame) and how low (m) a foot must be in both keys for a fill to plant it.
pub const PLANT_STILL: f64 = 0.25;
pub const PLANT_GROUND: f64 = 0.15;

impl crate::document::Document {
    /// A filler with the loaded model and the current settings (`None`: no model loaded). A gap
    /// over the model's limit gets anchor poses from the document's library.
    pub fn ai_filler(&self) -> Option<AiFiller> {
        self.ai_filler_with(true)
    }

    /// [`Document::ai_filler`]; `anchors`: whether a gap over the model's limit gets anchor
    /// poses (`false`: it is refused).
    pub fn ai_filler_with(&self, anchors: bool) -> Option<AiFiller> {
        self.ai.as_ref().map(|m| AiFiller {
            model: m.clone(),
            skeleton: self.skeleton.clone(),
            links: self.links.clone(),
            settings: self.ai_settings.clone(),
            contacts: Default::default(),
            source: self.pose_source.clone().filter(|_| anchors),
            placed: Default::default(),
        })
    }

    /// Fill between two manual keys (`None`: the ones around the playhead) with the model, ease it
    /// through a key it shares with another filled span (`ease.rs`), plant the steps the model
    /// says happen inside the gap ([`Document::plant_steps`], models with foot contacts), then the
    /// feet that stand still in both keys, and bend any impossible pose the model made back inside
    /// the possible limits (`crate::possible`). A gap over the model's limit gets anchor poses from
    /// the library first (`crate::anchors`), and each span between them is treated as a fill of
    /// its own. Returns the span, the model label, the anchors and notes.
    pub fn ai_fill(&mut self, span: Option<(i64, i64)>) -> R<Filled> {
        self.ai_fill_with(span, true)
    }

    /// [`Document::ai_fill`]; `anchors: false` refuses a gap over the model's limit instead.
    pub fn ai_fill_with(&mut self, span: Option<(i64, i64)>, anchors: bool) -> R<Filled> {
        let out = self.fill_clip(span, anchors)?;
        let f = self.frame;
        self.go_to(f)?;
        Ok(out)
    }

    /// [`Document::ai_fill`] on the clip alone (the pose shown is left as it is).
    fn fill_clip(&mut self, span: Option<(i64, i64)>, anchors: bool) -> R<Filled> {
        let filler = self.ai_filler_with(anchors).ok_or_else(|| {
            crate::error::hinted(
                "no AI model is loaded (assets/models)",
                "install a model under assets/models (ai.info says what is loaded); ai.preview with mode \"plain\" interpolates without one",
            )
        })?;
        let (a, b) = match span {
            Some(s) => s,
            None => crate::edits::span_around(&self.clip, self.key_frame())
                .ok_or("AI fill works between two of your keys: put the playhead between them")?,
        };
        let m = crate::edits::manual_frames(&self.clip);
        if !m.contains(&a) || !m.contains(&b) || m.iter().any(|&f| a < f && f < b) {
            return Err(format!("{a} and {b} are not neighbouring keys of yours"));
        }
        if !anchors && b - a - 1 > filler.max_gap() {
            return Err(crate::error::hinted(
                format!(
                    "gap {} frames (max {}) and anchors \"none\" was asked for",
                    b - a - 1,
                    filler.max_gap()
                ),
                "leave `anchors` out (or \"auto\") to place anchor poses from the library inside it, or key a pose inside the gap",
            ));
        }
        let note = crate::edits::refill(&mut self.clip, a, b, &filler, &self.links)?;
        if !note.is_empty() {
            return Err(note);
        }
        let (placed, placing) = self.finish_fill(&filler, a, b)?;
        let kept = self.keep_possible_report(a, b)?;
        let mut notes = kept.notes.clone();
        notes.extend(placing);
        Ok(Filled {
            from: a,
            to: b,
            model: filler.label(),
            notes,
            kept,
            anchors: placed.anchors,
            segment: placed.segment,
        })
    }

    /// After `refill` wrote the span `a..b`: tag the anchor keys of a long fill, ease through
    /// the keys at both ends and through each anchor, and plant the feet of every span between
    /// them. Returns what was placed and the notes of placing it.
    fn finish_fill(&mut self, filler: &AiFiller, a: i64, b: i64) -> R<(Placed, Vec<String>)> {
        let placed = filler.placed_of(a, b).unwrap_or_default();
        let e = self.clip.edit;
        for an in &placed.anchors {
            if let Some(k) = self
                .clip
                .layers
                .get_mut(e)
                .and_then(|l| l.keys.iter_mut().find(|k| k.frame == an.frame))
            {
                k.model = anchors::tag(an);
            }
        }
        let mut bounds = vec![a];
        bounds.extend(placed.anchors.iter().map(|x| x.frame));
        bounds.push(b);
        self.ease_borders(a, b)?;
        for an in &placed.anchors {
            crate::ease::ease_through(&mut self.clip, an.frame, crate::ease::EASE_FRAMES)?;
        }
        // the steps the model saw, over the whole gap at once: a foot down across an anchor is
        // one contact (planted on one spot), not two meeting there with a jump
        let spans: Vec<Option<Contacts>> = bounds
            .windows(2)
            .map(|w| match w {
                &[p, q] => filler.contacts_of(p, q),
                _ => None,
            })
            .collect();
        if spans.iter().all(Option::is_some) {
            let mut merged: Contacts = Vec::with_capacity((b - a - 1).max(0) as usize);
            let mut last: Option<[bool; 2]> = None;
            for c in spans.iter().flatten() {
                if let (Some(prev), Some(next)) = (last, c.first()) {
                    merged.push([prev[0] && next[0], prev[1] && next[1]]);
                }
                merged.extend(c.iter().copied());
                last = c.last().copied().or(last);
            }
            self.plant_steps(a, b, &merged, PLANT_GROUND)?;
        }
        for w in bounds.windows(2) {
            let &[p, q] = w else { continue };
            self.plant_feet(p, q, PLANT_STILL, PLANT_GROUND)?;
        }
        let notes = placed.notes.clone();
        Ok((placed, notes))
    }

    /// Work out a fill without keeping it: the clip as it would be is stored as the preview (a
    /// ghost to compare), and the clip itself is unchanged. `plain`: the plain interpolation
    /// (the network's change dropped), to compare against.
    ///
    /// A preview worked out elsewhere from this very document (`ai_precomputed`, the app's
    /// worker thread) is used as it is when its clip, links, settings, span and mode all match:
    /// the same inputs give the same fill, so the result is identical, only sooner.
    pub fn ai_preview(&mut self, span: Option<(i64, i64)>, plain: bool) -> R<Arc<Preview>> {
        if let Some(p) = self.ai_precomputed.take()
            && span == Some((p.from, p.to))
            && p.plain == plain
            && p.settings == self.ai_settings
            && p.base == self.clip
            && p.links == self.links
        {
            self.ai_preview = Some(p.clone());
            return Ok(p);
        }
        let base = self.clip.clone();
        let settings = self.ai_settings.clone();
        if plain {
            self.ai_settings.ai_force = 0.0;
        }
        let made = self.fill_clip(span, true);
        self.ai_settings = settings;
        let filled = std::mem::replace(&mut self.clip, base.clone());
        let Filled {
            from,
            to,
            model: label,
            notes,
            kept: _,
            anchors: _,
            segment: _,
        } = made?;
        let p = Arc::new(Preview {
            base,
            links: self.links.clone(),
            clip: filled,
            from,
            to,
            label: if plain {
                "plain interpolation".into()
            } else {
                label
            },
            plain,
            settings: self.ai_settings.clone(),
            notes,
        });
        self.ai_preview = Some(p.clone());
        Ok(p)
    }

    /// The preview, while the clip is still the one it was made from.
    pub fn live_preview(&self) -> Option<&Arc<Preview>> {
        self.ai_preview
            .as_ref()
            .filter(|p| p.base == self.clip && p.links == self.links)
    }

    /// The previewed pose at `frame`, inside the previewed span (the ghost).
    pub fn preview_pose(&self, frame: f64) -> Option<crate::document::Pose> {
        let p = self.live_preview()?;
        if !(p.from as f64 <= frame && frame <= p.to as f64) {
            return None;
        }
        let (r, o) = p.clip.sample_with(&[frame], &p.links).ok()?;
        Some(crate::document::Pose {
            rotations: r.into_iter().next()?,
            root: *o.first()?,
        })
    }

    /// Keep the preview: the clip becomes the previewed one.
    pub fn ai_accept(&mut self) -> R<Arc<Preview>> {
        let p = match (&self.ai_preview, self.live_preview()) {
            (None, _) => {
                return Err(crate::error::hinted(
                    "no AI preview to keep",
                    "run ai.preview first",
                ));
            }
            (Some(_), None) => {
                return Err(crate::error::hinted(
                    "the animation changed since the preview",
                    "run ai.preview again",
                ));
            }
            (_, Some(p)) => p.clone(),
        };
        self.clip = p.clip.clone();
        self.ai_preview = None;
        let f = self.frame;
        self.go_to(f)?;
        Ok(p)
    }

    /// After a manual key on the playhead changed: refill the AI spans on both sides of it (they
    /// were made for the old key). Nothing happens without a model or without AI keys there. A
    /// span made from text is left as it is; the returned notes say so.
    pub fn resync_ai(&mut self) -> R<Vec<String>> {
        let f = self.key_frame();
        let m = crate::edits::manual_frames(&self.clip);
        if !m.contains(&f) {
            return Ok(Vec::new());
        }
        let prev = m.iter().copied().filter(|&x| x < f).max();
        let next = m.iter().copied().filter(|&x| x > f).min();
        let mut notes = Vec::new();
        for (a, b) in [(prev, Some(f)), (Some(f), next)] {
            if let (Some(a), Some(b)) = (a, b)
                && crate::edits::generated_in(&self.clip, a, b)
            {
                notes.push(crate::edits::generated_note(a, b));
            }
        }
        let Some(filler) = self.ai_filler() else {
            return Ok(notes);
        };
        let ai: Vec<i64> = self
            .clip
            .keys()
            .iter()
            .filter(|k| k.source == crate::clip::KeySource::Ai)
            .map(|k| k.frame)
            .collect();
        for (a, b) in [(prev, Some(f)), (Some(f), next)] {
            let (Some(a), Some(b)) = (a, b) else { continue };
            if ai.iter().any(|&x| a < x && x < b)
                && !crate::edits::generated_in(&self.clip, a, b)
                && crate::edits::refill(&mut self.clip, a, b, &filler, &self.links)?.is_empty()
            {
                let (_, placing) = self.finish_fill(&filler, a, b)?;
                notes.extend(self.keep_possible(a, b)?);
                notes.extend(placing);
            }
        }
        let fr = self.frame;
        self.go_to(fr)?;
        Ok(notes)
    }
}
