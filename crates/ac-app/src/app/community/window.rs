//! The community in a window of its own, opened by the pop-out button beside the library's
//! Community tab: as big as one likes, with the clips as a gallery of large playing cards (or the
//! tab's rows) and the selected clip's details at the side. Closing it puts the community back
//! in the library's tab.

use egui::vec2;

use super::super::App;
use super::tab::{Act, BIG, author, installs, playing, stars};
use crate::app::library::Tab;
use crate::app::ui_ids::Tag;

/// The gallery's card widths (points); the default.
pub const CARD_MIN: f32 = 100.0;
pub const CARD_MAX: f32 = 320.0;
pub const CARD: f32 = 160.0;
/// The details pane beside the clips.
const SIDE_W: f32 = 230.0;

/// How the window lays out the clips.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Gallery,
    List,
}

impl View {
    pub fn name(self) -> &'static str {
        match self {
            View::Gallery => "gallery",
            View::List => "list",
        }
    }

    pub fn parse(s: &str) -> Result<View, String> {
        match s {
            "gallery" => Ok(View::Gallery),
            "list" => Ok(View::List),
            _ => Err(format!("{s:?}: gallery or list")),
        }
    }
}

/// The pop-out button (the font has no such symbol): a box with an arrow leaving its corner.
/// Lit while the window is open; a click opens or closes it.
pub fn float_button(ui: &mut egui::Ui, floating: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(22.0, 20.0), egui::Sense::click());
    let r = r
        .on_hover_text(if floating {
            "Put the community back in the library"
        } else {
            "Open the community in its own window: as big as you like, with a gallery"
        })
        .tag("library.community.float", "Community in its own window");
    let v = ui.visuals();
    if floating {
        ui.painter().rect_filled(rect, 3.0, v.selection.bg_fill);
    } else if r.hovered() {
        ui.painter()
            .rect_filled(rect, 3.0, v.widgets.hovered.weak_bg_fill);
    }
    let colour = if r.hovered() || floating {
        v.strong_text_color()
    } else {
        v.widgets.inactive.fg_stroke.color
    };
    let stroke = egui::Stroke::new(1.3, colour);
    let c = rect.center();
    // the box, open at its top-right corner
    let (l, t, rt, b) = (c.x - 6.0, c.y - 5.0, c.x + 6.0, c.y + 6.0);
    let p = ui.painter();
    p.line_segment([egui::pos2(c.x - 1.0, t), egui::pos2(l, t)], stroke);
    p.line_segment([egui::pos2(l, t), egui::pos2(l, b)], stroke);
    p.line_segment([egui::pos2(l, b), egui::pos2(rt, b)], stroke);
    p.line_segment([egui::pos2(rt, b), egui::pos2(rt, c.y + 1.0)], stroke);
    // the arrow, out of the corner
    let tip = egui::pos2(rt + 1.0, t - 2.0);
    p.line_segment([egui::pos2(c.x - 1.0, c.y + 2.0), tip], stroke);
    p.line_segment([tip, egui::pos2(tip.x - 5.0, tip.y)], stroke);
    p.line_segment([tip, egui::pos2(tip.x, tip.y + 5.0)], stroke);
    r
}

impl App {
    /// Open the window (the library's tab goes back to the animations) or put the community
    /// back in the library's tab.
    pub(in crate::app) fn community_float(&mut self, on: bool) {
        let was = self.community.floating;
        self.community.floating = on;
        if on && self.lib_tab == Tab::Community {
            self.lib_tab = Tab::Animations;
        }
        if was && !on {
            self.lib_tab = Tab::Community;
        }
    }

    /// The window, while it is open (every frame).
    pub(in crate::app) fn community_window(&mut self, ctx: &egui::Context) {
        if !self.community.floating {
            return;
        }
        let mut open = true;
        let screen = ctx.content_rect();
        egui::Window::new("Community")
            .id(egui::Id::unique("community.window"))
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_size([
                (screen.width() * 0.7).min(1100.0),
                (screen.height() * 0.75).min(760.0),
            ])
            .min_size([420.0, 300.0])
            .show(ctx, |ui| self.community_window_body(ui));
        if !open {
            self.community_float(false);
        }
    }

    fn community_window_body(&mut self, ui: &mut egui::Ui) {
        self.community_header(ui, true);
        ui.separator();
        if self.community.show_mine {
            self.community_mine_view(ui);
            return;
        }
        let selected = self.community_selected();
        if selected.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.community.selected = None;
        }
        let side = if selected.is_some() { SIDE_W } else { 0.0 };
        let size = ui.available_size();
        ui.horizontal_top(|ui| {
            let clips_w = (size.x - side - if side > 0.0 { 16.0 } else { 0.0 }).max(120.0);
            ui.allocate_ui_with_layout(
                vec2(clips_w, size.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| match self.community.view {
                    View::Gallery => self.community_gallery(ui),
                    View::List => {
                        self.community_list(ui);
                    }
                },
            );
            if let Some((id, c)) = selected {
                ui.separator();
                let mut act: Act = None;
                ui.allocate_ui_with_layout(
                    vec2(side, size.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("community.window.details")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(side - 8.0);
                                act = self.community_details(ui, &c, &id);
                            });
                    },
                );
                self.community_act(act, &c, &id);
            }
        });
    }

    /// Gallery or list, and the cards' size (the window's header).
    pub(super) fn community_view_switch(&mut self, ui: &mut egui::Ui) {
        for (v, label, tip) in [
            (View::Gallery, "Gallery", "Large cards that play"),
            (View::List, "List", "One row each, as in the library"),
        ] {
            if ui
                .selectable_label(self.community.view == v, label)
                .on_hover_text(tip)
                .tag(format!("community.view.{}", v.name()), label)
                .clicked()
            {
                self.community.view = v;
            }
        }
        if self.community.view == View::Gallery {
            ui.add(
                egui::Slider::new(&mut self.community.card, CARD_MIN..=CARD_MAX)
                    .show_value(false)
                    .text("size"),
            )
            .on_hover_text("How big the cards are")
            .tag("community.card_size", "Card size");
        }
        ui.separator();
    }

    /// The clips as cards that wrap to the window's width, each playing its strip.
    fn community_gallery(&mut self, ui: &mut egui::Ui) {
        self.community_nothing(ui);
        let list = self.community.list.clone();
        let w = self.community.card.clamp(CARD_MIN, CARD_MAX);
        let h = w * BIG.1 / BIG.0;
        egui::ScrollArea::vertical()
            .id_salt("community.gallery")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    for c in &list {
                        let id = c["id"].as_str().unwrap_or_default().to_string();
                        let title = c["title"].as_str().unwrap_or_default();
                        let selected = self.community.selected.as_deref() == Some(id.as_str());
                        let seconds = c["seconds"].as_f64().unwrap_or(1.0);
                        let card = ui.allocate_ui_with_layout(
                            vec2(w, h + 54.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(w);
                                ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
                                let strip = self
                                    .community
                                    .strips
                                    .get(&id, c["preview"].as_str().unwrap_or_default());
                                let pic = playing(ui, strip, vec2(w, h), seconds);
                                ui.add(egui::Label::new(egui::RichText::new(title).strong()).truncate());
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("by {} · {seconds:.1} s", author(c)))
                                            .small()
                                            .weak(),
                                    )
                                    .truncate(),
                                );
                                let mut facts = vec![
                                    stars(
                                        c["rating"].as_f64().unwrap_or(0.0),
                                        c["ratings"].as_i64().unwrap_or(0),
                                    ),
                                    installs(c["downloads"].as_i64().unwrap_or(0)),
                                ];
                                if c["installed"].as_str().is_some_and(|s| !s.is_empty()) {
                                    facts.push("in your library".into());
                                }
                                ui.add(
                                    egui::Label::new(egui::RichText::new(facts.join(" · ")).small())
                                        .truncate(),
                                );
                                pic
                            },
                        );
                        let rect = card.response.rect;
                        let hit = ui
                            .interact(
                                rect,
                                egui::Id::unique(("community.card", &id)),
                                egui::Sense::click(),
                            )
                            .tag(format!("community.card.{id}"), title);
                        if selected || hit.hovered() {
                            let colour = if selected {
                                ui.visuals().selection.stroke.color
                            } else {
                                ui.visuals().widgets.hovered.bg_stroke.color
                            };
                            ui.painter().rect_stroke(
                                rect.expand(3.0),
                                4.0,
                                egui::Stroke::new(1.5, colour),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if hit.clicked() || card.inner.clicked() {
                            self.community.selected = if selected { None } else { Some(id.clone()) };
                        }
                        if hit.double_clicked() || card.inner.double_clicked() {
                            self.community_install_or_open(c);
                        }
                    }
                });
            });
    }
}
