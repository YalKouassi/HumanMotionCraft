//! Community clips' preview strips: the frames side by side as one PNG, as the library's popover
//! draws them (`library::preview`). They are downloaded off the UI thread, a few at a time, for
//! the rows on screen. A clip without a strip (or one on the fixtures' fake server) is drawn here
//! from its file instead, like a library thumbnail.

use std::collections::HashMap;
use std::sync::mpsc;

use super::super::App;
use crate::app::library::preview::{CELL, FRAMES};

/// Downloads at once, and strips drawn here per frame.
const AT_ONCE: usize = 4;
const DRAWN_PER_FRAME: usize = 1;
/// The gap between cells in a strip (pixels; `offscreen::grid`).
const GAP: f32 = 2.0;

pub enum Strip {
    /// The texture and its number of cells.
    Ready(egui::TextureHandle, usize),
    Failed(String),
}

enum Got {
    Png(Vec<u8>),
    /// The clip's file, to draw.
    Text(String),
}

#[derive(Default)]
pub struct Strips {
    pub cache: HashMap<String, Strip>,
    /// Asked for this frame: (clip id, preview address).
    wanted: Vec<(String, String)>,
    loading: HashMap<String, mpsc::Receiver<Result<Got, String>>>,
    /// Downloaded clip files waiting to be drawn.
    to_draw: Vec<(String, String)>,
}

impl Strips {
    pub fn loading(&self) -> bool {
        !self.wanted.is_empty() || !self.loading.is_empty() || !self.to_draw.is_empty()
    }

    /// The strip of clip `id` (asked for when it isn't here yet).
    pub fn get(&mut self, id: &str, preview: &str) -> Option<&Strip> {
        if !self.cache.contains_key(id)
            && !self.loading.contains_key(id)
            && !self.wanted.iter().any(|w| w.0 == id)
        {
            self.wanted.push((id.to_string(), preview.to_string()));
        }
        self.cache.get(id)
    }
}

/// How many cells a strip `w` × `h` pixels holds (cells keep the popover's shape).
pub fn cells(w: usize, h: usize) -> usize {
    let cell = h as f32 * CELL.0 as f32 / CELL.1 as f32;
    (((w as f32 + GAP) / (cell + GAP)).round() as usize).max(1)
}

impl App {
    pub(super) fn strips_poll(&mut self, ctx: &egui::Context) {
        let st = &mut self.community.strips;
        let wanted = std::mem::take(&mut st.wanted);
        for (id, url) in wanted {
            if st.loading.len() >= AT_ONCE {
                break;
            }
            let (tx, rx) = mpsc::channel();
            let wake = ctx.clone();
            let cid = id.clone();
            let spawned = std::thread::Builder::new()
                .name("community-strip".into())
                .spawn(move || {
                    let got = if url.starts_with("https://") || url.starts_with("http://") {
                        ac_community::http::get_bytes(&url).map(Got::Png)
                    } else {
                        ac_community::clip_text(&cid).map(Got::Text)
                    };
                    let _ = tx.send(got);
                    wake.request_repaint();
                });
            match spawned {
                Ok(_) => {
                    st.loading.insert(id, rx);
                }
                Err(e) => {
                    st.cache.insert(id, Strip::Failed(e.to_string()));
                }
            }
        }
        let mut done = Vec::new();
        st.loading.retain(|id, rx| match rx.try_recv() {
            Err(mpsc::TryRecvError::Empty) => true,
            Ok(r) => {
                done.push((id.clone(), r));
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                done.push((id.clone(), Err("the download stopped".into())));
                false
            }
        });
        for (id, r) in done {
            match r {
                Ok(Got::Png(png)) => {
                    let s = texture(ctx, &id, &png);
                    self.community.strips.cache.insert(id, s);
                }
                Ok(Got::Text(text)) => self.community.strips.to_draw.push((id, text)),
                Err(e) => {
                    self.community.strips.cache.insert(id, Strip::Failed(e));
                }
            }
        }
        for _ in 0..DRAWN_PER_FRAME {
            let Some((id, text)) = self.community.strips.to_draw.pop() else {
                break;
            };
            let s = match self.community_strip_png(&text) {
                Ok(png) => texture(ctx, &id, &png),
                Err(e) => Strip::Failed(e),
            };
            self.community.strips.cache.insert(id, s);
            ctx.request_repaint();
        }
    }

    /// A strip like the popover's, drawn from a clip's file.
    pub(super) fn community_strip_png(&mut self, text: &str) -> Result<Vec<u8>, String> {
        let root = self.session.document.library.clone().ok_or("no library")?;
        let lib = ac_io::library::Library::new(&root, ac_io::library::Kind::Animations);
        self.strip_png(text, true, FRAMES, CELL, &lib)
    }
}

fn texture(ctx: &egui::Context, id: &str, png: &[u8]) -> Strip {
    match ac_render::offscreen::unpng(png) {
        Ok((w, h, rgba)) => {
            let (w, h) = (w as usize, h as usize);
            let tex = ctx.load_texture(
                format!("community:{id}"),
                egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba),
                egui::TextureOptions::LINEAR,
            );
            Strip::Ready(tex, cells(w, h))
        }
        Err(e) => Strip::Failed(format!("the preview isn't a picture: {e}")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_popover_strip_has_its_cells() {
        // 12 cells of 240 × 320 (drawn at twice the size) and 11 gaps of 2
        assert_eq!(super::cells(12 * 240 + 11 * 2, 320), 12);
        assert_eq!(super::cells(240, 320), 1);
    }
}
