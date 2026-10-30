//! The library panel: animations and poses, each yours and the built-ins (to watch and learn
//! from; saving over them is refused, Save as makes your own copy), with a search box, filters
//! and contact-sheet thumbnails.
//!
//! The list is grouped under headers that fold away (remembered per library) with a count each;
//! Starred and Recent come first. Searching or a filter opens every group that has a match and
//! leaves out the rest. A click selects an item and shows its preview beside the panel
//! (`preview.rs`); a double-click or Enter opens it (an animation) or applies it (a pose).
//! Filters and the order are in `filter.rs`; rows, headers and dragging to a group in `row.rs`.
//!
//! Thumbnails are drawn on a headless renderer a few per frame, for the rows on screen, and kept
//! in `<library>/.thumbs/` (redrawn when the item's file is newer); an animation's plays while
//! hovered (`hover.rs`). Right-click menus, the keys and the small dialogs are in `menu.rs`.

pub mod filter;
pub(in crate::app) mod hover;
pub mod menu;
pub mod preview;
mod row;

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::App;
use super::save::{Confirm, Switch};
use super::ui_ids::Tag;
use filter::Order;
use row::Drop;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Animations,
    Poses,
    Community,
}

/// The panel's own state: filters, the group to scroll to, the preview.
#[derive(Default)]
pub struct View {
    pub filters: filter::Filters,
    /// Scroll this group's header into view on the next draw.
    jump: Option<String>,
    pub preview: preview::Preview,
    /// The library folder and its key in the preferences (worked out once).
    key: Option<(PathBuf, String)>,
}

/// Recent items shown at the top, and poses remembered.
const RECENT_SHOWN: usize = 5;
pub const MAX_RECENT_POSES: usize = 10;
/// The sections that aren't groups ('@' is never in a group's name).
const STARRED: &str = "@starred";
const RECENT: &str = "@recent";

/// A run of rows (indices into the items) under one header; no title: the whole list in an
/// order other than by name.
struct Section {
    key: String,
    title: String,
    drop: Drop,
    rows: Vec<usize>,
}

/// The list's sections: by name, Starred (unless only starred ones are shown) and Recent, then
/// each group; in any other order, one list.
fn sections(items: &[Value], order: Order, stars_only: bool, recent: &[String]) -> Vec<Section> {
    if !order.grouped() {
        return vec![Section {
            key: String::new(),
            title: String::new(),
            drop: Drop::Nothing,
            rows: (0..items.len()).collect(),
        }];
    }
    let id = |i: &usize| items.get(*i).and_then(|it| it["id"].as_str());
    let mut out = Vec::new();
    if !stars_only {
        let rows: Vec<usize> = (0..items.len())
            .filter(|i| {
                items
                    .get(*i)
                    .is_some_and(|it| it["favourite"].as_bool() == Some(true))
            })
            .collect();
        if !rows.is_empty() {
            out.push(Section {
                key: STARRED.into(),
                title: "Starred".into(),
                drop: Drop::Star,
                rows,
            });
        }
    }
    let rows: Vec<usize> = recent
        .iter()
        .filter_map(|r| (0..items.len()).find(|i| id(i) == Some(r.as_str())))
        .take(RECENT_SHOWN)
        .collect();
    if !rows.is_empty() {
        out.push(Section {
            key: RECENT.into(),
            title: "Recent".into(),
            drop: Drop::Nothing,
            rows,
        });
    }
    for (g, _) in ac_io::find::group_counts(items) {
        let rows = (0..items.len())
            .filter(|i| {
                items
                    .get(*i)
                    .is_some_and(|it| it["group"].as_str() == Some(g.as_str()))
            })
            .collect();
        out.push(Section {
            key: g.clone(),
            title: g.clone(),
            drop: Drop::Group(g),
            rows,
        });
    }
    out
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
            self.lib_reveal(Tab::Animations);
            self.refresh_library();
        }
    }

    /// The library folder's key in the preferences.
    fn lib_key(&mut self) -> Option<String> {
        let dir = self.session.document.library.clone()?;
        if self.lib_view.key.as_ref().is_none_or(|(d, _)| *d != dir) {
            self.lib_view.key = Some((dir.clone(), super::prefs::library_key(&dir)));
        }
        self.lib_view.key.as_ref().map(|k| k.1.clone())
    }

    /// Show a tab (after making or importing something there): the built-ins-only switch would
    /// hide your new item, so it goes back to all.
    pub(super) fn lib_reveal(&mut self, tab: Tab) {
        self.lib_tab = tab;
        if self.lib_view.filters.source == ac_io::find::Source::Builtin {
            self.lib_view.filters.source = ac_io::find::Source::All;
        }
    }

    /// A pose was applied: first in this library's recent poses.
    pub(super) fn lib_pose_used(&mut self, id: &str) {
        let Some(key) = self.lib_key() else { return };
        self.shell.prefs.update(|p| {
            let r = p.library_panel.recent_poses.entry(key).or_default();
            r.retain(|x| x != id);
            r.insert(0, id.to_string());
            r.truncate(MAX_RECENT_POSES);
        });
    }

    /// The folded groups (`<kind>/<group>`) of this library.
    fn lib_folded(&mut self) -> Vec<String> {
        let Some(key) = self.lib_key() else {
            return Vec::new();
        };
        self.shell
            .prefs
            .get()
            .library_panel
            .folded
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    fn lib_fold(&mut self, what: &str, fold: bool) {
        let Some(key) = self.lib_key() else { return };
        let had = self
            .shell
            .prefs
            .get()
            .library_panel
            .folded
            .get(&key)
            .is_some_and(|f| f.iter().any(|x| x == what));
        if had == fold {
            return;
        }
        self.shell.prefs.update(|p| {
            let f = p.library_panel.folded.entry(key).or_default();
            f.retain(|x| x != what);
            if fold {
                f.push(what.to_string());
            }
        });
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
            // the preview popover is the library's too
            let on_preview = ui
                .ctx()
                .layer_id_at(at)
                .is_some_and(|l| l.id == egui::Id::unique(preview::AREA));
            self.files.nav.focus = panel.contains(at) || on_preview;
        }
        let total = |k: &str| self.library[k].as_array().map_or(0, Vec::len);
        let (n_anims, n_poses) = (total("animations"), total("poses"));
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.lib_tab,
                Tab::Animations,
                format!("Animations {n_anims}"),
            )
            .on_hover_text(format!(
                "Your own animations and the ones that come with {} (to watch and learn from)",
                ac_core::product::NAME
            ))
            .tag("library.tab.animations", "Animations");
            ui.selectable_value(&mut self.lib_tab, Tab::Poses, format!("Poses {n_poses}"))
                .tag("library.tab.poses", "Poses");
            ui.selectable_value(&mut self.lib_tab, Tab::Community, "Community")
                .on_hover_text("Animations other people shared: watch, install and rate them")
                .tag("library.tab.community", "Community");
            let floating = self.community.floating;
            if crate::app::community::window::float_button(ui, floating).clicked() {
                self.community_float(!floating);
            }
        });
        if self.lib_tab == Tab::Community {
            self.community_tab(ui);
            return;
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.lib_filter)
                .hint_text("Search: every word must match")
                .desired_width(f32::INFINITY),
        )
        .tag("library.search", "Search");
        let anims = self.lib_tab == Tab::Animations;
        let kind = if anims { "animations" } else { "poses" };
        if anims {
            self.library_buttons(ui);
        } else if ui
            .button("Save current pose…")
            .on_hover_text("The pose on screen into the library (the whole body or a part)")
            .tag("library.save_pose", "Save current pose…")
            .clicked()
        {
            self.open_save_pose();
        }
        let all: Vec<Value> = self.library[kind].as_array().cloned().unwrap_or_default();
        let mut parts: Vec<String> = all
            .iter()
            .filter_map(|it| it["part"].as_str().map(str::to_string))
            .collect();
        parts.sort();
        parts.dedup();
        self.library_filters(ui, anims, &parts);
        let f = &self.lib_view.filters;
        if anims && f.source == ac_io::find::Source::Builtin {
            ui.label(
                egui::RichText::new(
                    "For watching and learning. To change one, open it and use Save as: the copy is yours.",
                )
                .small()
                .weak(),
            );
        }
        let q = f.query(&self.lib_filter, anims);
        let mut items = ac_io::find::find(&all, &q);
        let unsaved = self.session.document.unsaved();
        let open_id = self.session.document.clip_id.clone();
        if anims && f.changed {
            items.retain(|it| {
                it["edited"].as_bool() == Some(true)
                    || (unsaved && it["id"].as_str().is_some_and(|i| open_id.as_deref() == Some(i)))
            });
        }
        let order = f.order;
        let narrowed = q.narrows() || (anims && f.changed);
        let stars_only = f.stars;
        let everything = !narrowed && f.source == ac_io::find::Source::All;
        let recent: Vec<String> = if anims {
            self.files.state.recent.clone()
        } else {
            let key = self.lib_key();
            key.and_then(|k| self.shell.prefs.get().library_panel.recent_poses.get(&k).cloned())
                .unwrap_or_default()
        };
        if order == Order::Recent {
            items.sort_by_key(|it| {
                it["id"]
                    .as_str()
                    .and_then(|i| recent.iter().position(|r| r == i))
                    .unwrap_or(usize::MAX)
            });
        }
        let sections = sections(&items, order, stars_only, &recent);
        ui.separator();
        if items.is_empty() {
            let empty = match (everything, anims) {
                (false, _) => "No match.",
                (true, true) => {
                    "Your animations go here: start a new one, import a library, or open a built-in and Save as copy."
                }
                (true, false) => "Nothing here yet.",
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(empty).weak());
                if !all.is_empty()
                    && ui
                        .small_button("Show everything")
                        .on_hover_text("Clear the search and every filter")
                        .tag("library.show_all", "Show everything")
                        .clicked()
                {
                    self.lib_filter.clear();
                    let order = self.lib_view.filters.order;
                    self.lib_view.filters = filter::Filters {
                        order,
                        ..Default::default()
                    };
                }
            });
        }
        let groups: Vec<String> = sections
            .iter()
            .filter(|s| matches!(s.drop, Drop::Group(_)))
            .map(|s| s.key.clone())
            .collect();
        if groups.len() > 1 {
            egui::ComboBox::from_id_salt("library.jump")
                .selected_text(egui::RichText::new("Go to group").small())
                .show_ui(ui, |ui| {
                    for g in &groups {
                        if ui
                            .selectable_label(false, g)
                            .tag(format!("library.jump.{g}"), g.as_str())
                            .clicked()
                        {
                            self.lib_view.jump = Some(g.clone());
                        }
                    }
                })
                .response
                .on_hover_text("Scroll to a group (and unfold it)")
                .tag("library.jump", "Go to group");
        }
        if let Some(g) = self.lib_view.jump.clone() {
            self.lib_fold(&format!("{kind}/{g}"), false);
        }
        let folded = self.lib_folded();
        let scroll = std::mem::take(&mut self.files.nav.scroll);
        let mut order_keys: Vec<(bool, String)> = Vec::new();
        let mut anchored = false;
        let mut fold: Option<(String, bool)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let clip = ui.clip_rect();
                for s in &sections {
                    let what = format!("{kind}/{}", s.key);
                    let open = narrowed || !folded.contains(&what);
                    if !s.title.is_empty() {
                        let tag = format!("library.group.{}", s.key);
                        let r = self.lib_header(ui, &tag, &s.title, s.rows.len(), open, s.drop.clone());
                        if r.clicked() && !narrowed {
                            fold = Some((what.clone(), open));
                        }
                        if self.lib_view.jump.as_deref() == Some(s.key.as_str()) {
                            r.scroll_to_me(Some(egui::Align::TOP));
                            self.lib_view.jump = None;
                        }
                    }
                    if !open {
                        continue;
                    }
                    for &i in &s.rows {
                        let Some(item) = items.get(i) else { continue };
                        let id = item["id"].as_str().unwrap_or_default().to_string();
                        let key = (anims, id.clone());
                        let first = !order_keys.contains(&key);
                        if first {
                            order_keys.push(key.clone());
                        }
                        let picked = self.files.nav.sel.as_ref() == Some(&key);
                        let r = self.lib_row(ui, item, anims, picked, &s.key);
                        if picked && first {
                            if scroll {
                                r.scroll_to_me(Some(egui::Align::Center));
                            }
                            if clip.intersects(r.rect) {
                                self.lib_view.preview.anchor = Some(r.rect);
                                anchored = true;
                            }
                        }
                        self.item_menu(&r, anims, &id);
                        if r.double_clicked() {
                            self.lib_select(anims, &id);
                            self.lib_activate(anims, &id);
                        } else if r.clicked() {
                            self.lib_select(anims, &id);
                        }
                    }
                }
            });
        if !anchored {
            self.lib_view.preview.anchor = None;
        }
        if let Some((what, f)) = fold {
            self.lib_fold(&what, f);
        }
        self.files.nav.order = order_keys;
        let ctx = ui.ctx().clone();
        self.lib_drag_ghost(&ctx);
        self.lib_preview(&ctx, panel, &items);
        self.draw_thumbs(&ctx);
    }

    /// The animations tab's buttons: new, save, delete, import.
    fn library_buttons(&mut self, ui: &mut egui::Ui) {
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
                .thumb_png(&kind, &id, anims, ANIM_FRAMES, CELL, "")
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

    /// A library item's picture as PNG, `frames` cells of `cell` points (an animation's frames
    /// spread over it): from `<library>/.thumbs/<sub>/` if newer than the item, else drawn (and
    /// kept there).
    pub(in crate::app) fn thumb_png(
        &mut self,
        kind: &str,
        id: &str,
        anims: bool,
        frames: usize,
        cell: (u32, u32),
        sub: &str,
    ) -> Result<Vec<u8>, String> {
        let root = self.session.document.library.clone().ok_or("no library")?;
        let lib_kind = if anims {
            ac_io::library::Kind::Animations
        } else {
            ac_io::library::Kind::Poses
        };
        let lib = ac_io::library::Library::new(&root, lib_kind);
        let src = lib.path(id)?;
        let cached = root
            .join(".thumbs")
            .join(sub)
            .join(kind)
            .join(format!("{id}.png"));
        let modified = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let (Some(c), Some(s)) = (modified(&cached), modified(&src))
            && c >= s
            && let Ok(png) = std::fs::read(&cached)
        {
            return Ok(png);
        }
        let text = std::fs::read_to_string(&src).map_err(|e| e.to_string())?;
        let png = self.strip_png(&text, anims, frames, cell, &lib)?;
        if let Some(dir) = cached.parent() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&cached, &png);
        }
        Ok(png)
    }

    /// An animation's (or a pose's) picture as PNG from its file's text: `frames` cells of `cell`
    /// points side by side, spread over the animation. Link layers find their clips in `lib` (a
    /// missing one drives nothing).
    pub(in crate::app) fn strip_png(
        &mut self,
        text: &str,
        anims: bool,
        frames: usize,
        cell: (u32, u32),
        lib: &ac_io::library::Library,
    ) -> Result<Vec<u8>, String> {
        let sk = self.session.document.skeleton.clone();
        let poses: Vec<(Vec<ac_core::quat::Quat>, [f64; 3])> = if anims {
            let clip = ac_core::clipfile::load_clip(text)?;
            let links = ac_io::commands::load_links(lib, &clip)?;
            let last = (clip.length_with(&links) - 1).max(0) as f64;
            let n = frames.max(2);
            let frames: Vec<f64> = (0..n)
                .map(|i| (last * i as f64 / (n - 1) as f64).round())
                .collect();
            let (r, o) = clip.sample_with(&frames, &links)?;
            r.into_iter().zip(o).collect()
        } else {
            let p = ac_core::clipfile::load_pose(text)?;
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
        off.sheet(
            "three-quarter",
            (cell.0 * scale, cell.1 * scale, posed.len() as u32),
            &posed,
            &sk.heads,
            false,
        )
    }
}
