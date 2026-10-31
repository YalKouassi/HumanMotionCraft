//! `--record`: the whole window into a video, one picture for each frame of the animation.
//! It steps through the frames and grabs the window once each is drawn, so no frame can be
//! dropped and a slow machine makes the same file. And `--size`, which it shares with
//! `--screenshot`: a window of exactly that many pixels, whatever the display's scale.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ac_media::{Encoder, Format};
use serde_json::json;

use super::App;

/// Windows drawn before a window that won't take its size is given up on.
const SIZE_TRIES: u32 = 300;
/// Frames drawn after a step before it is grabbed: some of the window lays out from the frame before.
const DRAWS: u32 = 2;

/// What `--record` asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub path: PathBuf,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub fps: Option<f64>,
}

type Picture = Arc<Mutex<Option<([usize; 2], Vec<u8>)>>>;

enum Stage {
    /// Waiting for the window and its thumbnails.
    Start,
    /// Frame `times[next]` is set, and has been drawn this many times.
    Drawing(u32),
    /// Waiting for the picture of `times[next]`.
    Grabbing,
    /// The file is written (or failed): closing.
    Done,
}

pub(super) struct Recording {
    plan: Plan,
    stage: Stage,
    times: Vec<f64>,
    fps: f64,
    next: usize,
    picture: Picture,
    encoder: Option<Encoder>,
    size: [usize; 2],
}

impl Recording {
    pub(super) fn new(plan: Plan) -> Recording {
        Recording {
            plan,
            stage: Stage::Start,
            times: Vec::new(),
            fps: 0.0,
            next: 0,
            picture: Picture::default(),
            encoder: None,
            size: [0, 0],
        }
    }

    /// A grabbed picture into the file. An MP4 needs even sizes: a display's scale can make the
    /// default window odd, so its last row or column goes.
    fn push(&mut self, size: [usize; 2], rgba: &[u8]) -> Result<(), String> {
        if self.encoder.is_none() {
            let (w, h) = (size[0] as u32, size[1] as u32);
            let (w, h) = match Format::of(&self.plan.path)? {
                Format::Mp4 => (w & !1, h & !1),
                Format::Gif => (w, h),
            };
            if let Some(dir) = self.plan.path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
            }
            self.encoder = Some(Encoder::create(&self.plan.path, w, h, self.fps)?);
            self.size = size;
        }
        if size != self.size {
            return Err(format!(
                "the window changed size while recording: {}x{} to {}x{}",
                self.size[0], self.size[1], size[0], size[1]
            ));
        }
        let keep = (size[0] & !1, size[1] & !1);
        let Some(enc) = &mut self.encoder else {
            return Err("no encoder".into());
        };
        match enc.format() {
            Format::Mp4 => enc.push(&cropped(rgba, size[0], keep)),
            Format::Gif => enc.push(rgba),
        }
    }
}

/// The first `keep.1` rows of `keep.0` pixels, from RGBA rows `w` pixels wide.
fn cropped(rgba: &[u8], w: usize, keep: (usize, usize)) -> Vec<u8> {
    rgba.chunks_exact((w * 4).max(1))
        .take(keep.1)
        .flat_map(|row| &row[..keep.0 * 4])
        .copied()
        .collect()
}

impl App {
    /// The window at the size `--size` asked for yet? It asks for it until it is. Sizes are in
    /// the display's pixels here, not egui's points.
    pub(super) fn window_sized(&mut self, ctx: &egui::Context) -> bool {
        let Some((w, h)) = self.window_size else {
            return true;
        };
        let scale = ctx.pixels_per_point();
        let now = ctx.content_rect().size() * scale;
        let now = (now.x.round() as u32, now.y.round() as u32);
        if now == (w, h) {
            return true;
        }
        self.size_tries += 1;
        if self.size_tries > SIZE_TRIES {
            crate::failed(format!(
                "the window stayed at {}x{}, not the {w}x{h} asked for (is it bigger than this display?)",
                now.0, now.1
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return false;
        }
        if self.size_tries % 30 == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                w as f32 / scale,
                h as f32 / scale,
            )));
        }
        false
    }

    /// One frame of `--record`, at the end of drawing the window.
    pub(super) fn recording(&mut self, ctx: &egui::Context) {
        if self.record.is_none() {
            return;
        }
        ctx.request_repaint();
        if let Err(why) = self.record_step(ctx) {
            crate::failed(format!("--record: {why}"));
            if let Some(r) = &mut self.record {
                r.encoder = None;
                r.stage = Stage::Done;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn record_step(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.frames_drawn += 1;
        let settled = self.frames_drawn > 5 && (!self.thumbs.busy || self.frames_drawn > 600);
        let waiting = matches!(self.record.as_ref().map(|r| &r.stage), Some(Stage::Start));
        if waiting && (!settled || !self.window_sized(ctx)) {
            return Ok(());
        }
        let Some(r) = &mut self.record else {
            return Ok(());
        };
        match r.stage {
            Stage::Start => {
                let doc = &self.session.document;
                let fps = r.plan.fps.unwrap_or(doc.clip.fps);
                let last = (doc.clip.length_with(&doc.links) - 1).max(0) as f64;
                let (from, to) = (r.plan.from.unwrap_or(0.0), r.plan.to.unwrap_or(last));
                r.times = ac_media::shots::times(doc.clip.fps, from, to, fps)?;
                r.fps = fps;
                self.playing = false;
                self.show_record_frame()?;
            }
            Stage::Drawing(n) if n + 1 < DRAWS => r.stage = Stage::Drawing(n + 1),
            Stage::Drawing(_) => {
                let slot = r.picture.clone();
                r.stage = Stage::Grabbing;
                ctx.request_screenshot(move |img| {
                    if let Ok(mut s) = slot.lock() {
                        *s = Some((img.size, img.as_raw().to_vec()));
                    }
                });
            }
            Stage::Grabbing => {
                let got = r.picture.lock().map_err(|e| e.to_string())?.take();
                let Some((size, rgba)) = got else {
                    return Ok(());
                };
                r.push(size, &rgba)?;
                r.next += 1;
                if r.next < r.times.len() {
                    self.show_record_frame()?;
                } else {
                    r.stage = Stage::Done;
                    let enc = r.encoder.take().ok_or("no frames")?;
                    let n = enc.finish()?;
                    println!("{} ({n} frames at {} fps)", r.plan.path.display(), r.fps);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Stage::Done => {}
        }
        Ok(())
    }

    fn show_record_frame(&mut self) -> Result<(), String> {
        let Some(r) = &mut self.record else {
            return Ok(());
        };
        let t = *r.times.get(r.next).ok_or("no frames to record")?;
        r.stage = Stage::Drawing(0);
        self.session
            .run("frame.set", &json!({ "frame": t }))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cropping_keeps_the_top_left() {
        // 3x3, each pixel numbered
        let rgba: Vec<u8> = (0..9u8).flat_map(|i| [i, i, i, 255]).collect();
        let kept = cropped(&rgba, 3, (2, 2));
        let firsts: Vec<u8> = kept.chunks(4).map(|p| p[0]).collect();
        assert_eq!(firsts, [0, 1, 3, 4]);
    }
}
