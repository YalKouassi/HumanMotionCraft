//! The library panel: your animations, the built-ins (to watch and learn from; saving over them is
//! refused, Save as makes your own copy) and poses, with a search box and contact-sheet thumbnails.
//!
//! Thumbnails are drawn on a headless renderer a few per frame, for the rows on screen, and kept
//! in `<library>/.thumbs/` (redrawn when the item's file is newer); an animation's plays while
//! hovered (`hover.rs`). Right-click menus, the keys and the small dialogs are in `menu.rs`.

mod hover;
pub mod menu;

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::App;
use super::save::{Confirm, Switch};
use super::ui_ids::Tag;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Animations,
    Builtin,
    Poses,
}

/// Thumbnail cells (points) and how many frames an animation's shows.
const CELL: (u32, u32) = (36, 48);
const ANIM_FRAMES: usize = 4;
/// Thumbnails drawn per frame at most (each is a few milliseconds).
const PER_FRAME: usize = 2;

pub enum Thumb {
    Ready(egui::TextureHandle),
    Failed(String),
}

#[derive(Default)]
pub struct Thumbs {
    pub cache: HashMap<String, Thumb>,
    /// Asked for this frame, not drawn yet.
    wanted: Vec<(String, bool, String)>,
    /// Some were still waiting at the last draw.
    pub busy: bool,
}

/// "6 animations", "1 pose": counted and named.
fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// The status line for an import's result: what was copied, what was already there, and every
/// file that was skipped, with why.
fn import_status(r: &Value) -> String {
    let len = |v: &Value| v.as_array().map_or(0, Vec::len);
    let (anims, poses) = (&r["animations"], &r["poses"]);
    // library.samples answers with animations only, at the top level
    let (a, ak) = if anims.is_object() {
        (len(&anims["copied"]), len(&anims["kept"]))
    } else {
        (len(&r["copied"]), len(&r["kept"]))
    };
    let (p, pk) = (len(&poses["copied"]), len(&poses["kept"]));
    let mut what = vec![count(a, "animation")];
    if p + pk > 0 {
        what.push(count(p, "pose"));
    }
    let mut s = format!("imported {}", what.join(" and "));
    if let Some(g) = r["group"].as_str() {
        s.push_str(&format!(" (group {g})"));
    }
    if ak + pk > 0 {
        s.push_str(&format!("; {} already in the library, kept", ak + pk));
    }
    let skipped = r["skipped"].as_array().map(Vec::as_slice).unwrap_or_default();
    if !skipped.is_empty() {
        let files: Vec<String> = skipped
            .iter()
            .map(|f| {
                format!(
                    "{}: {}",
                    f["file"].as_str().unwrap_or_default(),
                    f["error"].as_str().unwrap_or_default()
                )
            })
            .collect();
        s.push_str(&format!(
            "; skipped {}: {}",
            count(skipped.len(), "file"),
            files.join("; ")
        ));
    }
    if let Some(cut) = r["cut"].as_str() {
        s.push_str(&format!("; {cut}"));
    }
    s
}

impl App {
    /// Run an import command and say what it did (a refusal is reported by `run`).
    fn import_library(&mut self, id: &str, params: Value) {
        if let Some(r) = self.run(id, params) {
            self.status = import_status(&r);
            self.lib_tab = Tab::Animations;
            self.refresh_library();
        }
    }

    pub(super) fn library_panel(&mut self, ui: &mut egui::Ui) {
        // a click in the panel gives it the keys (arrows, Enter, F2, Delete); a click elsewhere
        // takes them back
        let panel = ui.max_rect();
        if let Some(at) = ui.input(|i| {
            i.pointer
                .any_pressed()
                .then_some(i.pointer.interact_pos())
                .flatten()
        }) {
            self.files.nav.focus = panel.contains(at);
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.lib_tab, Tab::Animations, "Animations")
                .on_hover_text("Your own animations")
                .tag("library.tab.animations", "Animations");
            ui.selectable_value(&mut self.lib_tab, Tab::Builtin, "Built-in")
                .on_hover_text(format!(
                    "The animations that come with {}: open one to watch and learn from it",
                    ac_core::product::NAME
                ))
                .tag("library.tab.builtin", "Built-in");
            ui.selectable_value(&mut self.lib_tab, Tab::Poses, "Poses")
                .tag("library.tab.poses", "Poses");
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.lib_filter)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        )
        .tag("library.search", "Search");
        let (kind, builtin) = match self.lib_tab {
            Tab::Animations => ("animations", Some(false)),
            Tab::Builtin => ("animations", Some(true)),
            Tab::Poses => ("poses", None),
        };
        match self.lib_tab {
            Tab::Builtin => {
                ui.label(
                    egui::RichText::new("For watching and learning. To change one, open it and use Save as: your copy goes under Animations.")
                        .small()
                        .weak(),
                );
            }
            Tab::Animations => {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("+ New animation").tag("library.new", "+ New animation").clicked() {
                        self.switch(Switch::New(
                            json!({ "name": "Untitled", "group": "My animations" }),
                        ));
                    }
                    if ui
                        .add_enabled(self.session.document.unsaved(), egui::Button::new("Save"))
                        .on_hover_text("Save the open animation (Ctrl+S)")
                        .on_disabled_hover_text("The library has every change")
                        .tag("library.save", "Save")
                        .clicked()
                    {
                        self.save();
                    }
                    let mine = self
                        .session
                        .document
                        .clip_id
                        .clone()
                        .filter(|id| self.is_mine(id));
                    if ui
                        .add_enabled(mine.is_some(), egui::Button::new("Delete"))
                        .on_hover_text("Delete the open animation from your library")
                        .on_disabled_hover_text("Open one of your animations to delete it (right-click any of them, too)")
                        .tag("library.delete", "Delete")
                        .clicked()
                        && let Some(id) = mine
                    {
                        self.confirm = Some(Confirm::Delete(id));
                    }
                    ui.menu_button("Import library", |ui| {
                        if ui
                            .button("Samples")
                            .on_hover_text(format!(
                                "The sample animations that come with {}, into the Samples group",
                                ac_core::product::NAME
                            ))
                            .tag("library.import.samples", "Samples")
                            .clicked()
                        {
                            self.import_library("library.samples", json!({}));
                        }
                        if ui
                            .button("From a folder…")
                            .on_hover_text("Animations (*.clip.json) and poses (*.pose.json) from a folder of your own; its subfolders become groups")
                            .tag("library.import.folder", "From a folder…")
                            .clicked()
                            && let Some(dir) = rfd::FileDialog::new()
                                .set_title("Import a library folder")
                                .pick_folder()
                        {
                            self.import_library(
                                "library.import",
                                json!({ "from": dir.to_string_lossy() }),
                            );
                        }
                    })
                    .response
                    .tag("library.import", "Import library");
                });
            }
            Tab::Poses => {
                if ui
                    .button("Save current pose…")
                    .on_hover_text("The pose on screen into the library (the whole body or a part)")
                    .tag("library.save_pose", "Save current pose…")
                    .clicked()
                {
                    self.open_save_pose();
                }
            }
        }
        ui.separator();
        let needle = self.lib_filter.trim().to_lowercase();
        let items: Vec<Value> = self.library[kind]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|it| builtin.is_none_or(|b| it["builtin"].as_bool().unwrap_or(false) == b))
            .filter(|it| {
                needle.is_empty()
                    || ["name", "group", "part"].iter().any(|k| {
                        it[*k]
                            .as_str()
                            .is_some_and(|t| t.to_lowercase().contains(&needle))
                    })
            })
            .cloned()
            .collect();
        if items.is_empty() {
            let empty = match (needle.is_empty(), self.lib_tab) {
                (false, _) => "No match.",
                (true, Tab::Animations) => {
                    "Your animations go here: start a new one, import a library, or open a built-in and Save as copy."
                }
                (true, _) => "Nothing here yet.",
            };
            ui.label(egui::RichText::new(empty).weak());
        }
        let anims = kind == "animations";
        self.files.nav.order = items
            .iter()
            .filter_map(|it| it["id"].as_str().map(|id| (anims, id.to_string())))
            .collect();
        let scroll = std::mem::take(&mut self.files.nav.scroll);
        let row_h = CELL.1 as f32 + 6.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut group = String::new();
                for item in &items {
                    let g = item["group"].as_str().unwrap_or_default();
                    if g != group {
                        group = g.to_string();
                        ui.label(egui::RichText::new(&group).weak().small());
                    }
                    let id = item["id"].as_str().unwrap_or_default().to_string();
                    let open = anims && self.session.document.clip_id.as_deref() == Some(id.as_str());
                    let name = item["name"].as_str().unwrap_or_default();
                    let picked = self
                        .files
                        .nav
                        .sel
                        .as_ref()
                        .is_some_and(|(a, s)| *a == anims && *s == id);
                    let r = ui
                        .horizontal(|ui| {
                            ui.set_min_height(row_h);
                            let size = egui::vec2(
                                (CELL.0 * if anims { ANIM_FRAMES as u32 } else { 1 }) as f32,
                                CELL.1 as f32,
                            );
                            match self.thumbs.cache.get(&format!("{kind}/{id}")) {
                                Some(Thumb::Ready(t)) if anims => {
                                    let big = egui::vec2(CELL.0 as f32 * 2.5, CELL.1 as f32 * 2.5);
                                    hover::strip(ui, t, size, ANIM_FRAMES, big);
                                }
                                Some(Thumb::Ready(t)) => {
                                    ui.add(egui::Image::new((t.id(), size)));
                                }
                                Some(Thumb::Failed(e)) => {
                                    ui.allocate_ui(size, |ui| {
                                        ui.label(egui::RichText::new("no preview").small().weak())
                                    })
                                    .response
                                    .on_hover_text(e.clone());
                                }
                                None => {
                                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                                    ui.painter().rect_filled(rect, 2.0, self.palette().surface.thumb);
                                    if ui.is_rect_visible(rect) {
                                        self.thumbs.wanted.push((id.clone(), anims, kind.to_string()));
                                    }
                                }
                            }
                            ui.vertical(|ui| {
                                let mut text = egui::RichText::new(name);
                                if open {
                                    text = text.strong();
                                }
                                let label = ui
                                    .selectable_label(open || picked, text)
                                    .tag(format!("library.item.{id}"), name);
                                let mut detail = Vec::new();
                                if let Some(n) = item["frames"].as_i64() {
                                    detail.push(format!("{n} frames"));
                                }
                                if let Some(p) = item["part"]
                                    .as_str()
                                    .filter(|p| !p.is_empty() && *p != "Whole body")
                                {
                                    detail.push(p.to_string());
                                }
                                if open && self.session.document.unsaved() {
                                    detail.push("unsaved changes".into());
                                }
                                if item["edited"].as_bool() == Some(true) {
                                    detail.push("edited".into());
                                }
                                if !anims && item["builtin"].as_bool() == Some(true) {
                                    detail.push("built-in".into());
                                }
                                if let Some(e) = item["error"].as_str() {
                                    detail.push(e.to_string());
                                }
                                ui.label(egui::RichText::new(detail.join(" · ")).small().weak());
                                label
                            })
                            .inner
                        })
                        .inner;
                    let clicked = r.clicked();
                    if picked && scroll {
                        r.scroll_to_me(Some(egui::Align::Center));
                    }
                    self.item_menu(&r, anims, &id);
                    if clicked {
                        self.lib_select(anims, &id);
                        // the open one stays as it is (opening it again would drop its edits)
                        self.lib_activate(anims, &id);
                    }
                }
            });
        self.draw_thumbs(ui.ctx());
    }

    /// Draw a few of the thumbnails asked for this frame (from the disk cache when it is fresh).
    fn draw_thumbs(&mut self, ctx: &egui::Context) {
        let wanted = std::mem::take(&mut self.thumbs.wanted);
        self.thumbs.busy = !wanted.is_empty();
        if wanted.is_empty() {
            return;
        }
        for (id, anims, kind) in wanted.into_iter().take(PER_FRAME) {
            let key = format!("{kind}/{id}");
            let thumb = match self
                .thumb_png(&kind, &id, anims)
                .and_then(|png| ac_render::offscreen::unpng(&png))
            {
                Ok((w, h, rgba)) => Thumb::Ready(ctx.load_texture(
                    format!("thumb:{key}"),
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                    egui::TextureOptions::LINEAR,
                )),
                Err(e) => Thumb::Failed(e),
            };
            self.thumbs.cache.insert(key, thumb);
        }
        ctx.request_repaint();
    }

    /// A library item's thumbnail as PNG: from `<library>/.thumbs/` if newer than the item, else
    /// drawn (and kept there).
    fn thumb_png(&mut self, kind: &str, id: &str, anims: bool) -> Result<Vec<u8>, String> {
        let root = self.session.document.library.clone().ok_or("no library")?;
        let lib_kind = if anims {
            ac_io::library::Kind::Animations
        } else {
            ac_io::library::Kind::Poses
        };
        let lib = ac_io::library::Library::new(&root, lib_kind);
        let src = lib.path(id)?;
        let cached = root.join(".thumbs").join(kind).join(format!("{id}.png"));
        let modified = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let (Some(c), Some(s)) = (modified(&cached), modified(&src))
            && c >= s
            && let Ok(png) = std::fs::read(&cached)
        {
            return Ok(png);
        }
        let text = std::fs::read_to_string(&src).map_err(|e| e.to_string())?;
        let sk = self.session.document.skeleton.clone();
        let poses: Vec<(Vec<ac_core::quat::Quat>, [f64; 3])> = if anims {
            let clip = ac_core::clipfile::load_clip(&text)?;
            let links = ac_io::commands::load_links(&lib, &clip)?;
            let last = (clip.length_with(&links) - 1).max(0) as f64;
            let frames: Vec<f64> = (0..ANIM_FRAMES)
                .map(|i| (last * i as f64 / (ANIM_FRAMES - 1) as f64).round())
                .collect();
            let (r, o) = clip.sample_with(&frames, &links)?;
            r.into_iter().zip(o).collect()
        } else {
            let p = ac_core::clipfile::load_pose(&text)?;
            if p.bones != sk.names {
                return Err("a pose for a different rig".into());
            }
            vec![(p.rotations, p.root)]
        };
        let posed = poses
            .iter()
            .map(|(r, o)| sk.fk(r, *o).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()?;
        self.ensure_offscreen()?;
        let off = self.offscreen.as_mut().ok_or("no renderer")?;
        let scale = 2; // drawn at twice the size for sharp high-DPI thumbnails
        let png = off.sheet(
            "three-quarter",
            (CELL.0 * scale, CELL.1 * scale, posed.len() as u32),
            &posed,
            &sk.heads,
            false,
        )?;
        if let Some(dir) = cached.parent() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&cached, &png);
        }
        Ok(png)
    }
}
