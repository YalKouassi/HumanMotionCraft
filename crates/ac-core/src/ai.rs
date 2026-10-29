//! The learned in-betweener, as the core sees it: a model the front end loads (`ac-ai`), the
//! settings a fill runs with, and the [`Filler`] that writes its frames as AI keys.
//!
//! The core does not know how a model works; it hands it the clip and two manual keys and gets
//! back every bone's rotation and the root offset for each frame between them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::clip::{Clip, Links};
use crate::edits::Filler;
use crate::quat::{Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

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
}

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
}

impl AiFiller {
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
    fn fill(&self, clip: &Clip, a: i64, b: i64) -> R<(Vec<Vec<Quat>>, Vec<Vec3>, String)> {
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
        let (r, o) = self.model.fill(
            &self.skeleton,
            clip,
            &self.links,
            a,
            b,
            &self.settings,
            given.as_deref(),
        )?;
        Ok((r, o, self.label()))
    }
}

/// How still (m a frame) and how low (m) a foot must be in both keys for a fill to plant it.
pub const PLANT_STILL: f64 = 0.25;
pub const PLANT_GROUND: f64 = 0.15;

impl crate::document::Document {
    /// A filler with the loaded model and the current settings (`None`: no model loaded).
    pub fn ai_filler(&self) -> Option<AiFiller> {
        self.ai.as_ref().map(|m| AiFiller {
            model: m.clone(),
            skeleton: self.skeleton.clone(),
            links: self.links.clone(),
            settings: self.ai_settings.clone(),
        })
    }

    /// Fill between two manual keys (`None`: the ones around the playhead) with the model, ease it
    /// through a key it shares with another filled span (`ease.rs`), then plant the feet that
    /// stand still in both keys. Returns the span and the model label.
    pub fn ai_fill(&mut self, span: Option<(i64, i64)>) -> R<(i64, i64, String)> {
        let out = self.fill_clip(span)?;
        let f = self.frame;
        self.go_to(f)?;
        Ok(out)
    }

    /// [`Document::ai_fill`] on the clip alone (the pose shown is left as it is).
    fn fill_clip(&mut self, span: Option<(i64, i64)>) -> R<(i64, i64, String)> {
        let filler = self.ai_filler().ok_or_else(|| {
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
        let note = crate::edits::refill(&mut self.clip, a, b, &filler, &self.links)?;
        if !note.is_empty() {
            return Err(note);
        }
        self.ease_borders(a, b)?;
        self.plant_feet(a, b, PLANT_STILL, PLANT_GROUND)?;
        Ok((a, b, filler.label()))
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
        let made = self.fill_clip(span);
        self.ai_settings = settings;
        let filled = std::mem::replace(&mut self.clip, base.clone());
        let (from, to, label) = made?;
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
                self.ease_borders(a, b)?;
                self.plant_feet(a, b, PLANT_STILL, PLANT_GROUND)?;
            }
        }
        let fr = self.frame;
        self.go_to(fr)?;
        Ok(notes)
    }
}
