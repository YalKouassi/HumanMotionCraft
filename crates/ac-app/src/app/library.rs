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
//! Thumbnails are drawn on a headless renderer within a few milliseconds a frame, for the rows on
//! screen, and kept in `<library>/.thumbs/` (redrawn when the item's file is newer); an animation's
//! plays while hovered (`hover.rs`). Right-click menus, the keys and the small dialogs are in
//! `menu.rs`.
//!
//! A library can hold thousands of items, so a frame's work doesn't grow with them: the filtered
//! list is worked out when something it depends on changes ([`Listing`]), only the rows on screen
//! are drawn ([`visible_rows`]), and only so many pictures are kept ([`insert_capped`]).

pub mod filter;
pub(in crate::app) mod hover;
pub mod menu;
pub mod preview;
mod row;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::App;
use super::save::Switch;
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
    /// Goes up whenever the window's list of items changes.
    pub(super) generation: u64,
    listing: Option<Arc<Listing>>,
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
    /// Not folded away (a search or a filter opens every section).
    open: bool,
}

/// All a [`Listing`] depends on.
#[derive(PartialEq)]
struct ListKey {
    generation: u64,
    anims: bool,
    query: ac_io::find::Query,
    /// The Changed chip is on (animations).
    changed: bool,
    /// With it on: the open animation, while it has unsaved changes.
    unsaved_open: Option<String>,
    order: Order,
    recent: Vec<String>,
    folded: Vec<String>,
}

/// The list as filtered, sorted and sectioned.
struct Listing {
    key: ListKey,
    items: Vec<Value>,
    sections: Vec<Section>,
    /// The rows that can be reached, in order, each once: what the keys walk.
    order: Vec<(bool, String)>,
    /// Where each id is first drawn: (section, position in it).
    first: HashMap<String, (usize, usize)>,
}

fn build_listing(all: &[Value], key: ListKey) -> Listing {
    let mut items = ac_io::find::find(all, &key.query);
    if key.changed {
        items.retain(|it| {
            it["edited"].as_bool() == Some(true)
                || (key.unsaved_open.is_some() && it["id"].as_str() == key.unsaved_open.as_deref())
        });
    }
    if key.order == Order::Recent {
        items.sort_by_key(|it| {
            it["id"]
                .as_str()
                .and_then(|i| key.recent.iter().position(|r| r == i))
                .unwrap_or(usize::MAX)
        });
    }
    let narrowed = key.query.narrows() || key.changed;
    let kind = if key.anims { "animations" } else { "poses" };
    let mut sections = sections(&items, key.order, key.query.favourites, &key.recent);
    let mut order = Vec::new();
    let mut first = HashMap::new();
    for (si, s) in sections.iter_mut().enumerate() {
        s.open = narrowed || !key.folded.contains(&format!("{kind}/{}", s.key));
        if !s.open {
            continue;
        }
        for (pos, i) in s.rows.iter().enumerate() {
            let id = items
                .get(*i)
                .and_then(|it| it["id"].as_str())
                .unwrap_or_default();
            if !first.contains_key(id) {
                first.insert(id.to_string(), (si, pos));
                order.push((key.anims, id.to_string()));
            }
        }
    }
    Listing {
        key,
        items,
        sections,
        order,
        first,
    }
}

/// The rows of a run of `n`, starting at `top` and `step` apart, that touch `clip_top..clip_bottom`.
fn visible_rows(top: f32, n: usize, step: f32, clip_top: f32, clip_bottom: f32) -> std::ops::Range<usize> {
    if step <= 0.0 {
        return 0..n;
    }
    let row = |y: f32| ((y - top) / step).clamp(0.0, n as f32);
    let start = (row(clip_top).floor() as usize).min(n);
    let end = (row(clip_bottom).ceil() as usize).clamp(start, n);
    start..end
}

/// Keep `value`, and let the oldest go past `max`: pictures of thousands of items don't all stay
/// in memory. One let go that is wanted again is read back from its file.
pub(super) fn insert_capped<T>(
    cache: &mut HashMap<String, T>,
    order: &mut VecDeque<String>,
    key: String,
    value: T,
    max: usize,
) {
    order.retain(|k| *k != key);
    order.push_back(key.clone());
    cache.insert(key, value);
    while cache.len() > max {
        let Some(old) = order.pop_front() else { break };
        cache.remove(&old);
    }
}

/// Take a deleted item out of the window's `library.list` result, and its group when that was the
/// group's last item in either kind (`Library::delete` removes an empty folder).
pub(super) fn forget_item(library: &mut Value, kind: &str, id: &str) {
    let mut group = None;
    if let Some(items) = library[kind].as_array_mut() {
        items.retain(|it| {
            let this = it["id"].as_str() == Some(id);
            if this {
                group = it["group"].as_str().map(str::to_string);
            }
            !this
        });
    }
    let Some(group) = group else { return };
    let same_kind_left = library[kind]
        .as_array()
        .into_iter()
        .flatten()
        .any(|it| it["group"].as_str() == Some(group.as_str()));
    if same_kind_left {
        return;
    }
    let other = if kind == "animations" { "poses" } else { "animations" };
    let other_left = library[other]
        .as_array()
        .into_iter()
        .flatten()
        .any(|it| it["group"].as_str() == Some(group.as_str()));
    if !other_left && let Some(groups) = library["groups"].as_array_mut() {
        groups.retain(|g| g.as_str() != Some(group.as_str()));
    }
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
            open: true,
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
                open: true,
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
            open: true,
        });
    }
    // the groups in the order they first appear
    let mut at: HashMap<&str, usize> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        let g = it["group"].as_str().unwrap_or_default();
        let s = *at.entry(g).or_insert_with(|| {
            out.push(Section {
                key: g.to_string(),
                title: g.to_string(),
                drop: Drop::Group(g.to_string()),
                rows: Vec::new(),
                open: true,
            });
            out.len() - 1
        });
        if let Some(s) = out.get_mut(s) {
            s.rows.push(i);
        }
    }
    out
}

/// Thumbnail cells (points) and how many frames an animation's shows.
const CELL: (u32, u32) = (36, 48);
const ANIM_FRAMES: usize = 4;
/// Time a frame spends on thumbnails (at least one is done each frame).
const THUMB_BUDGET: Duration = Duration::from_millis(8);
/// Thumbnails kept in memory: far more than fit on screen, and about 110 KB each.
const MAX_THUMBS: usize = 512;

pub enum Thumb {
    Ready(egui::TextureHandle),
    Failed(String),
}

#[derive(Default)]
pub struct Thumbs {
    pub cache: HashMap<String, Thumb>,
    /// The cache's keys, oldest first.
    kept: VecDeque<String>,
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

    /// The list for the filters as they are now: the last one, unless something it depends on
    /// changed.
    fn lib_listing(&mut self, kind: &str, anims: bool) -> Arc<Listing> {
        let recent: Vec<String> = if anims {
            self.files.state.recent.clone()
        } else {
            let key = self.lib_key();
            key.and_then(|k| self.shell.prefs.get().library_panel.recent_poses.get(&k).cloned())
                .unwrap_or_default()
        };
        let folded = self.lib_folded();
        let doc = &self.session.document;
        let f = &self.lib_view.filters;
        let changed = anims && f.changed;
        let key = ListKey {
            generation: self.lib_view.generation,
            anims,
            query: f.query(&self.lib_filter, anims),
            changed,
            unsaved_open: doc.clip_id.clone().filter(|_| changed && doc.unsaved()),
            order: f.order,
            recent,
            folded,
        };
        if let Some(l) = &self.lib_view.listing
            && l.key == key
        {
            return l.clone();
        }
        let all = self.library[kind].as_array().map(Vec::as_slice).unwrap_or_default();
        let listing = Arc::new(build_listing(all, key));
        self.files.nav.order = listing.order.clone();
        self.lib_view.listing = Some(listing.clone());
        listing
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
        let parts: Vec<String> = if anims {
            Vec::new()
        } else {
            let mut parts: Vec<String> = self.library[kind]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|it| it["part"].as_str().map(str::to_string))
                .collect();
            parts.sort();
            parts.dedup();
            parts
        };
        self.library_filters(ui, anims, &parts);
        if anims && self.lib_view.filters.source == ac_io::find::Source::Builtin {
            ui.label(
                egui::RichText::new(
                    "For watching and learning. To change one, open it and use Save as: the copy is yours.",
                )
                .small()
                .weak(),
            );
        }
        let listing = self.lib_listing(kind, anims);
        let items = &listing.items;
        let sections = &listing.sections;
        let narrowed = listing.key.query.narrows() || listing.key.changed;
        let everything = !narrowed && self.lib_view.filters.source == ac_io::find::Source::All;
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
                if self.library[kind].as_array().is_some_and(|a| !a.is_empty())
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
        let scroll = std::mem::take(&mut self.files.nav.scroll);
        let mut anchored = false;
        let mut fold: Option<(String, bool)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let clip = ui.clip_rect();
                let gap = ui.spacing().item_spacing.y;
                let step = row::ROW_H + gap;
                for (si, s) in sections.iter().enumerate() {
                    let what = format!("{kind}/{}", s.key);
                    if !s.title.is_empty() {
                        let tag = format!("library.group.{}", s.key);
                        let r = self.lib_header(ui, &tag, &s.title, s.rows.len(), s.open, s.drop.clone());
                        if r.clicked() && !narrowed {
                            fold = Some((what.clone(), s.open));
                        }
                        if self.lib_view.jump.as_deref() == Some(s.key.as_str()) {
                            r.scroll_to_me(Some(egui::Align::TOP));
                            self.lib_view.jump = None;
                        }
                    }
                    if !s.open {
                        continue;
                    }
                    let (left, top, width) = (ui.cursor().left(), ui.cursor().top(), ui.available_width());
                    let seen = visible_rows(top, s.rows.len(), step, clip.top(), clip.bottom());
                    // the rows off screen only take their room
                    let room = |ui: &mut egui::Ui, n: usize| {
                        if n > 0 {
                            ui.allocate_space(egui::vec2(width, n as f32 * step - gap));
                        }
                    };
                    // the selection off screen is still scrolled to
                    let sel = self.files.nav.sel.as_ref().filter(|x| x.0 == anims).map(|x| x.1.as_str());
                    if scroll
                        && let Some(&(ss, pos)) = sel.and_then(|id| listing.first.get(id))
                        && ss == si
                        && !seen.contains(&pos)
                    {
                        let at = egui::pos2(left, top + pos as f32 * step);
                        let rect = egui::Rect::from_min_size(at, egui::vec2(width, row::ROW_H));
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                    room(ui, seen.start);
                    for pos in seen.clone() {
                        let Some(item) = s.rows.get(pos).and_then(|i| items.get(*i)) else {
                            continue;
                        };
                        let id = item["id"].as_str().unwrap_or_default().to_string();
                        let first = listing.first.get(&id) == Some(&(si, pos));
                        let picked = self
                            .files
                            .nav
                            .sel
                            .as_ref()
                            .is_some_and(|x| x.0 == anims && x.1 == id);
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
                        let builtin = item["builtin"].as_bool() == Some(true);
                        self.item_menu(&r, anims, &id, builtin);
                        if r.double_clicked() {
                            self.lib_select(anims, &id);
                            self.lib_activate(anims, &id);
                        } else if r.clicked() {
                            self.lib_select(anims, &id);
                        }
                    }
                    room(ui, s.rows.len() - seen.end);
                }
            });
        if !anchored {
            self.lib_view.preview.anchor = None;
        }
        if let Some((what, f)) = fold {
            self.lib_fold(&what, f);
        }
        let ctx = ui.ctx().clone();
        self.lib_drag_ghost(&ctx);
        self.lib_preview(&ctx, panel, items);
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
                self.lib_delete(true, &id);
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
        let began = Instant::now();
        for (id, anims, kind) in wanted {
            let key = format!("{kind}/{id}");
            // a row in two sections asks twice
            if self.thumbs.cache.contains_key(&key) {
                continue;
            }
            if began.elapsed() >= THUMB_BUDGET {
                break;
            }
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
            let t = &mut self.thumbs;
            insert_capped(&mut t.cache, &mut t.kept, key, thumb, MAX_THUMBS);
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// 6,000 clips in two groups of 3,000: "Clip 0".."Clip 2999" in A, the rest in B.
    fn many() -> Vec<Value> {
        (0..6000)
            .map(|i| {
                let group = if i < 3000 { "A" } else { "B" };
                let name = format!("Clip {i}");
                json!({ "id": format!("{group}/{name}"), "name": name, "group": group })
            })
            .collect()
    }

    fn key() -> ListKey {
        ListKey {
            generation: 0,
            anims: true,
            query: ac_io::find::Query::default(),
            changed: false,
            unsaved_open: None,
            order: Order::Name,
            recent: Vec::new(),
            folded: Vec::new(),
        }
    }

    #[test]
    fn a_listing_of_6000_has_every_row_once_and_a_fold_takes_a_group_out() {
        let all = many();
        let l = build_listing(&all, key());
        assert_eq!(l.sections.len(), 2);
        assert_eq!(l.order.len(), 6000);
        assert_eq!(l.first.get("B/Clip 3000"), Some(&(1, 0)));
        // natural order: Clip 2 before Clip 10
        assert_eq!(l.order[2].1, "A/Clip 2");
        assert_eq!(l.order[10].1, "A/Clip 10");
        let folded = ListKey {
            folded: vec!["animations/A".into()],
            ..key()
        };
        let l = build_listing(&all, folded);
        assert_eq!(l.order.len(), 3000);
        assert!(!l.sections[0].open);
        assert!(!l.first.contains_key("A/Clip 0"));
    }

    #[test]
    fn a_search_opens_a_folded_group_and_recent_rows_count_once() {
        let all = many();
        let k = ListKey {
            query: ac_io::find::Query {
                words: "5999".into(),
                ..Default::default()
            },
            folded: vec!["animations/B".into()],
            ..key()
        };
        let l = build_listing(&all, k);
        assert_eq!(l.order, [(true, "B/Clip 5999".to_string())]);
        let k = ListKey {
            recent: vec!["B/Clip 7".into(), "A/Clip 7".into()],
            ..key()
        };
        // B/Clip 7 doesn't exist, A/Clip 7 does: Recent has one row, and the list stays 6,000
        let l = build_listing(&all, k);
        assert_eq!(l.sections[0].key, RECENT);
        assert_eq!(l.sections[0].rows.len(), 1);
        assert_eq!(l.order.len(), 6000);
        assert_eq!(l.first.get("A/Clip 7"), Some(&(0, 0)));
    }

    #[test]
    fn only_the_rows_on_screen_are_drawn() {
        // rows 60 apart from y 100; on screen 400..1000: (400-100)/60 = 5, (1000-100)/60 = 15
        assert_eq!(visible_rows(100.0, 6000, 60.0, 400.0, 1000.0), 5..15);
        // part of a row counts: (410-100)/60 = 5.17 down to 5, (1010-100)/60 = 15.17 up to 16
        assert_eq!(visible_rows(100.0, 6000, 60.0, 410.0, 1010.0), 5..16);
        // the run is below the screen, above it, and ends on it
        assert_eq!(visible_rows(2000.0, 6000, 60.0, 400.0, 1000.0), 0..0);
        assert_eq!(visible_rows(-1000.0, 10, 60.0, 400.0, 1000.0), 10..10);
        assert_eq!(visible_rows(100.0, 8, 60.0, 400.0, 1000.0), 5..8);
    }

    #[test]
    fn the_oldest_pictures_go_past_the_cap() {
        let (mut cache, mut kept) = (HashMap::new(), VecDeque::new());
        for (i, k) in ["a", "b", "c", "a", "d"].iter().enumerate() {
            insert_capped(&mut cache, &mut kept, k.to_string(), i, 3);
        }
        // a was kept again after c, so b is the oldest when d comes
        assert_eq!(cache.len(), 3);
        assert!(!cache.contains_key("b"));
        assert_eq!(cache["a"], 3);
    }

    #[test]
    fn a_deleted_item_leaves_the_list_and_its_group_only_with_the_last_one() {
        let mut lib = json!({
            "animations": [
                {"id": "A/1", "group": "A"}, {"id": "A/2", "group": "A"}, {"id": "B/1", "group": "B"},
                {"id": "C/1", "group": "C"}
            ],
            "poses": [{"id": "C/p", "group": "C"}],
            "groups": ["A", "B", "C"]
        });
        forget_item(&mut lib, "animations", "A/1");
        assert_eq!(lib["groups"], json!(["A", "B", "C"]));
        assert_eq!(lib["animations"].as_array().unwrap().len(), 3);
        forget_item(&mut lib, "animations", "A/2");
        assert_eq!(lib["groups"], json!(["B", "C"]));
        // C still has a pose: its folder stays in the list
        forget_item(&mut lib, "animations", "C/1");
        assert_eq!(lib["groups"], json!(["B", "C"]));
        // not in the list: nothing changes
        forget_item(&mut lib, "animations", "No/such");
        assert_eq!(lib["animations"].as_array().unwrap().len(), 1);
    }
}
