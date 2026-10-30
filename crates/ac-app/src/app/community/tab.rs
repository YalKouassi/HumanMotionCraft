//! The tab's drawing: search, tag chips and order; the clips, each playing its preview strip,
//! with its stars and downloads; the selected clip's popover (a larger strip that plays, its
//! details, Install or Open, stars and Report); and My uploads. The floating window
//! (`window.rs`) draws the same parts, larger.

use std::time::Duration;

use egui::{Rect, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::SORTS;
use super::strips::Strip;
use crate::app::library::hover;
use crate::app::ui_ids::Tag;

/// A row's picture (points) and the popover's.
const ROW_CELL: (f32, f32) = (42.0, 56.0);
pub(super) const BIG: (f32, f32) = (180.0, 240.0);
const POPOVER_W: f32 = 220.0;
pub const AREA: &str = "community.preview";

/// Draw cell `i` of a strip that plays at a clip's speed (`seconds` long) while on screen.
pub(super) fn playing(
    ui: &mut egui::Ui,
    strip: Option<&Strip>,
    size: egui::Vec2,
    seconds: f64,
) -> egui::Response {
    match strip {
        Some(Strip::Ready(tex, n)) => {
            let n = (*n).max(1);
            let i = (ui.input(|inp| inp.time) * n as f64 / seconds.max(0.1)) as usize % n;
            let uv = hover::cell_uv(tex.size()[0] as f32, n, i);
            if n > 1 {
                ui.ctx().request_repaint_after(Duration::from_millis(40));
            }
            ui.add(
                egui::Image::new((tex.id(), size))
                    .uv(uv)
                    .sense(egui::Sense::click()),
            )
        }
        Some(Strip::Failed(e)) => {
            let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            ui.painter().rect_filled(r, 3.0, ui.visuals().faint_bg_color);
            if size.x >= 80.0 {
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    "no preview",
                    egui::FontId::proportional(12.0),
                    ui.visuals().weak_text_color(),
                );
            }
            resp.on_hover_text(format!("no preview: {e}"))
        }
        None => ui.add_sized(size, egui::Spinner::new()),
    }
}

pub(super) fn stars(rating: f64, ratings: i64) -> String {
    if ratings == 0 {
        "no ratings yet".into()
    } else {
        format!("rated {rating:.1} ({ratings})")
    }
}

pub(super) fn installs(n: i64) -> String {
    if n == 1 {
        "1 install".into()
    } else {
        format!("{n} installs")
    }
}

/// Five stars to click (the font has no star): the hovered one and those before it light up.
fn star_buttons(ui: &mut egui::Ui) -> Option<u8> {
    let size = vec2(18.0, 18.0);
    let rects: Vec<(u8, egui::Response)> = (1..=5u8)
        .map(|s| {
            let r = ui
                .allocate_response(size, egui::Sense::click())
                .on_hover_text(format!("{s} of 5"))
                .tag(format!("community.preview.rate.{s}"), format!("{s} stars"));
            (s, r)
        })
        .collect();
    let lit = rects
        .iter()
        .filter(|(_, r)| r.hovered())
        .map(|(s, _)| *s)
        .max()
        .unwrap_or(0);
    let mut clicked = None;
    for (s, r) in &rects {
        let colour = if *s <= lit {
            egui::Color32::from_rgb(240, 190, 60)
        } else {
            ui.visuals().widgets.inactive.fg_stroke.color
        };
        star(ui.painter(), r.rect.center(), 8.0, colour);
        if r.clicked() {
            clicked = Some(*s);
        }
    }
    clicked
}

/// A filled five-pointed star: ten triangles from its middle.
fn star(painter: &egui::Painter, at: egui::Pos2, radius: f32, colour: egui::Color32) {
    let point = |i: usize| {
        let r = if i.is_multiple_of(2) {
            radius
        } else {
            radius * 0.45
        };
        let a = std::f32::consts::PI * (i as f32 / 5.0 - 0.5);
        at + vec2(a.cos(), a.sin()) * r
    };
    for i in 0..10 {
        painter.add(egui::Shape::convex_polygon(
            vec![at, point(i), point(i + 1)],
            colour,
            egui::Stroke::NONE,
        ));
    }
}

/// Who shared clip `c`.
pub(super) fn author(c: &Value) -> &str {
    c["author"]
        .as_str()
        .filter(|a| !a.is_empty())
        .unwrap_or("someone")
}

/// What was clicked in a clip's details: (what, its value).
pub(super) type Act = Option<(&'static str, Value)>;

impl App {
    pub(in crate::app) fn community_tab(&mut self, ui: &mut egui::Ui) {
        if self.community.floating {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("The community is open in its own window.").weak());
            if ui
                .button("Put it back here")
                .tag("community.dock", "Put it back here")
                .clicked()
            {
                self.community_float(false);
            }
            return;
        }
        self.community_header(ui, false);
        if self.community.show_mine {
            self.community_mine_view(ui);
            return;
        }
        let panel = ui.max_rect();
        if let Some(a) = self.community_list(ui) {
            self.community_popover(ui.ctx(), panel, a);
        }
    }

    /// Search, Refresh and Mine; the order and the common tags (`wide`: the window's, with more
    /// tags and its view switch); what is in flight. A change fetches the list again.
    pub(super) fn community_header(&mut self, ui: &mut egui::Ui, wide: bool) {
        if self.community.asked.is_none() && !self.community.busy("community.browse") {
            self.community_browse();
        }
        let mut changed = false;
        // the buttons from the right, then the search fills what is left: sized from the panel's
        // width, the row would be wider than the panel and the panel would grow every frame
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mine = self.community.show_mine;
                if ui
                    .selectable_label(mine, "Mine")
                    .on_hover_text("What you shared, and how its checks went")
                    .tag("community.mine", "Mine")
                    .clicked()
                {
                    self.community.show_mine = !mine;
                    if !mine {
                        self.community_mine();
                    }
                }
                if ui
                    .button("Refresh")
                    .on_hover_text("Fetch the list again")
                    .tag("community.refresh", "Refresh")
                    .clicked()
                {
                    changed = true;
                }
                let w = if wide { 320.0 } else { ui.available_width() };
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.community.search)
                            .hint_text("Search the community")
                            .desired_width(w),
                    );
                    changed |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    r.tag("community.search", "Search the community");
                });
            });
        });
        if self.community.show_mine {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if wide {
                self.community_view_switch(ui);
            }
            let now = self.community.sort;
            egui::ComboBox::from_id_salt("community.sort")
                .selected_text(SORTS.iter().find(|s| s.0 == now).map_or("", |s| s.1))
                .show_ui(ui, |ui| {
                    for (word, label) in SORTS {
                        if ui
                            .selectable_label(now == *word, *label)
                            .tag(format!("community.sort.{word}"), *label)
                            .clicked()
                        {
                            self.community.sort = word;
                            changed = true;
                        }
                    }
                });
            for t in common_tags(&self.community.list, if wide { 16 } else { 6 }) {
                let on = self.community.tag.as_deref() == Some(t.as_str());
                if ui
                    .selectable_label(on, &t)
                    .tag(format!("community.tag.{t}"), t.clone())
                    .clicked()
                {
                    self.community.tag = if on { None } else { Some(t) };
                    changed = true;
                }
            }
        });
        if changed {
            self.community_browse();
        }
        let working = self.community_working();
        if let Some(why) = self.community.offline.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(why).color(ui.visuals().warn_fg_color).small());
                if ui.small_button("Retry").tag("community.retry", "Retry").clicked() {
                    self.community_browse();
                }
            });
        } else if !working.is_empty() {
            ui.label(egui::RichText::new(working).small().weak());
        }
    }

    /// "Nothing found", when the list came back empty.
    pub(super) fn community_nothing(&self, ui: &mut egui::Ui) {
        let st = &self.community;
        if st.list.is_empty() && st.offline.is_none() && !st.busy("community.browse") {
            ui.label(egui::RichText::new("Nothing found. Try other words, or share one of your own.").weak());
        }
    }

    /// The clips as rows; the selected row's rectangle (for the popover).
    pub(super) fn community_list(&mut self, ui: &mut egui::Ui) -> Option<Rect> {
        self.community_nothing(ui);
        let list = self.community.list.clone();
        let mut anchor = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for c in &list {
                    let id = c["id"].as_str().unwrap_or_default().to_string();
                    let selected = self.community.selected.as_deref() == Some(id.as_str());
                    let r = ui
                        .horizontal(|ui| {
                            let strip = self
                                .community
                                .strips
                                .get(&id, c["preview"].as_str().unwrap_or_default());
                            let seconds = c["seconds"].as_f64().unwrap_or(1.0);
                            let pic = playing(ui, strip, vec2(ROW_CELL.0, ROW_CELL.1), seconds);
                            let text = ui.vertical(|ui| {
                                let title =
                                    egui::RichText::new(c["title"].as_str().unwrap_or_default()).strong();
                                ui.add(egui::Label::new(title).truncate());
                                ui.label(
                                    egui::RichText::new(format!("by {} · {seconds:.1} s", author(c)))
                                        .small()
                                        .weak(),
                                );
                                let mut facts = vec![stars(
                                    c["rating"].as_f64().unwrap_or(0.0),
                                    c["ratings"].as_i64().unwrap_or(0),
                                )];
                                facts.push(installs(c["downloads"].as_i64().unwrap_or(0)));
                                if c["installed"].as_str().is_some_and(|s| !s.is_empty()) {
                                    facts.push("in your library".into());
                                }
                                ui.add(
                                    egui::Label::new(egui::RichText::new(facts.join(" · ")).small())
                                        .truncate(),
                                );
                            });
                            pic.union(text.response)
                        })
                        .inner;
                    let row = ui.interact(
                        r.rect,
                        egui::Id::unique(("community.row", &id)),
                        egui::Sense::click(),
                    );
                    if selected {
                        ui.painter().rect_stroke(
                            r.rect.expand(2.0),
                            3.0,
                            egui::Stroke::new(1.5, ui.visuals().selection.stroke.color),
                            egui::StrokeKind::Outside,
                        );
                        anchor = Some(r.rect);
                    }
                    let row = row.tag(
                        format!("community.row.{id}"),
                        c["title"].as_str().unwrap_or_default(),
                    );
                    if row.clicked() || r.clicked() {
                        self.community.selected = if selected { None } else { Some(id.clone()) };
                    }
                    if row.double_clicked() {
                        self.community_install_or_open(c);
                    }
                    ui.add_space(4.0);
                }
            });
        anchor
    }

    pub(super) fn community_install_or_open(&mut self, c: &Value) {
        match c["installed"].as_str().filter(|s| !s.is_empty()) {
            Some(at) => self.lib_activate(true, at),
            None => {
                self.run("community.install", json!({ "id": c["id"] }));
            }
        }
    }

    /// The selected clip's popover, beside the panel.
    fn community_popover(&mut self, ctx: &egui::Context, panel: Rect, anchor: Rect) {
        let Some((id, c)) = self.community_selected() else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.community.selected = None;
            return;
        }
        let screen = ctx.content_rect();
        let x = if panel.center().x < screen.center().x {
            panel.right() + 6.0
        } else {
            panel.left() - POPOVER_W - 22.0
        };
        let y = anchor
            .top()
            .clamp(screen.top(), (screen.bottom() - 460.0).max(screen.top()));
        let mut act: Act = None;
        egui::Area::new(egui::Id::unique(AREA))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(POPOVER_W);
                    act = self.community_details(ui, &c, &id);
                });
            });
        self.community_act(act, &c, &id);
    }

    /// The clip whose details show.
    pub(super) fn community_selected(&self) -> Option<(String, Value)> {
        let id = self.community.selected.clone()?;
        let c = self
            .community
            .list
            .iter()
            .find(|c| c["id"].as_str() == Some(id.as_str()))
            .cloned()?;
        Some((id, c))
    }

    /// Clip `c`'s details: its title (and a close button), a larger strip that plays, what it
    /// is, Install or Open, the stars and Report.
    pub(super) fn community_details(&mut self, ui: &mut egui::Ui, c: &Value, id: &str) -> Act {
        let mut act: Act = None;
        let seconds = c["seconds"].as_f64().unwrap_or(1.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(c["title"].as_str().unwrap_or_default()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("×")
                    .on_hover_text("Close (Escape)")
                    .tag("community.preview.close", "×")
                    .clicked()
                {
                    act = Some(("close", Value::Null));
                }
            });
        });
        ui.label(egui::RichText::new(format!("by {}", author(c))).small().weak());
        ui.vertical_centered(|ui| {
            let strip = self
                .community
                .strips
                .get(id, c["preview"].as_str().unwrap_or_default());
            playing(ui, strip, vec2(BIG.0, BIG.1), seconds);
        });
        if let Some(d) = c["description"].as_str().filter(|d| !d.is_empty()) {
            ui.label(d);
        }
        let mut facts = vec![
            format!("{} frames", c["frames"].as_i64().unwrap_or(0)),
            format!("{seconds:.1} s at {} fps", c["fps"].as_f64().unwrap_or(0.0)),
        ];
        let tags: Vec<&str> = c["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !tags.is_empty() {
            facts.push(tags.join(", "));
        }
        ui.label(egui::RichText::new(facts.join(" · ")).small());
        let licence = c["licence"].as_str().unwrap_or_default();
        let credit = c["credit"].as_str().unwrap_or_default();
        let lic = if credit.is_empty() {
            licence.to_string()
        } else {
            format!("{licence}, credit: {credit}")
        };
        ui.label(egui::RichText::new(lic).small().weak());
        ui.label(
            egui::RichText::new(stars(
                c["rating"].as_f64().unwrap_or(0.0),
                c["ratings"].as_i64().unwrap_or(0),
            ))
            .small(),
        );
        ui.horizontal(|ui| {
            let installed = c["installed"].as_str().filter(|s| !s.is_empty());
            let (go, tip) = match installed {
                Some(at) => ("Open", format!("It's in your library as {at}")),
                None => ("Install", "Put it in your library, in Community".to_string()),
            };
            let busy = self.community.busy("community.install");
            if ui
                .add_enabled(!busy, egui::Button::new(go))
                .on_hover_text(tip)
                .tag("community.preview.install", go)
                .clicked()
            {
                act = Some(("install", Value::Null));
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Rate").small());
            if let Some(s) = star_buttons(ui) {
                act = Some(("rate", json!(s)));
            }
        });
        ui.collapsing("Report", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.community.report_reason)
                    .hint_text("what's wrong with it")
                    .char_limit(ac_community::MAX_REASON),
            )
            .tag("community.preview.reason", "Reason");
            if ui
                .button("Send report")
                .tag("community.preview.report", "Send report")
                .clicked()
            {
                act = Some(("report", Value::Null));
            }
        });
        act
    }

    /// Do what was clicked in the details.
    pub(super) fn community_act(&mut self, act: Act, c: &Value, id: &str) {
        match act {
            Some(("close", _)) => self.community.selected = None,
            Some(("install", _)) => self.community_install_or_open(c),
            Some(("rate", s)) => {
                self.run("community.rate", json!({ "id": id, "stars": s }));
            }
            Some(("report", _)) => {
                let reason = self.community.report_reason.trim().to_string();
                self.run("community.report", json!({ "id": id, "reason": reason }));
            }
            _ => {}
        }
    }

    pub(super) fn community_mine_view(&mut self, ui: &mut egui::Ui) {
        let Some(mine) = self.community.mine.clone() else {
            ui.label(egui::RichText::new(self.community_working()).small().weak());
            return;
        };
        if mine.is_empty() {
            ui.label(egui::RichText::new("You haven't shared anything yet: right-click one of your animations and choose Share to community…").weak());
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for m in &mine {
                    let id = m["id"].as_str().unwrap_or_default().to_string();
                    let status = m["status"].as_str().unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(m["title"].as_str().unwrap_or_default()).strong());
                            let (said, colour) = match status {
                                "checking" => ("being checked", ui.visuals().weak_text_color()),
                                "published" => ("live", ui.visuals().text_color()),
                                "failed" => ("didn't pass the checks", ui.visuals().error_fg_color),
                                "hidden" => ("hidden after reports", ui.visuals().warn_fg_color),
                                _ => ("removed", ui.visuals().weak_text_color()),
                            };
                            ui.label(egui::RichText::new(said).small().color(colour));
                            if let Some(r) = m["reason"].as_str().filter(|r| !r.is_empty()) {
                                ui.label(egui::RichText::new(r).small());
                            }
                        });
                        if status != "removed" {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .small_button("Remove")
                                    .on_hover_text("Take it out of the community")
                                    .tag(format!("community.mine.remove.{id}"), "Remove")
                                    .clicked()
                                {
                                    self.run("community.remove", json!({ "id": id }));
                                }
                            });
                        }
                    });
                    ui.separator();
                }
            });
    }
}

/// The `n` tags most clips in `list` have (ties by name).
fn common_tags(list: &[Value], n: usize) -> Vec<String> {
    let mut count: Vec<(String, usize)> = Vec::new();
    for t in list
        .iter()
        .flat_map(|c| c["tags"].as_array().cloned().unwrap_or_default())
    {
        let Some(t) = t.as_str() else { continue };
        match count.iter_mut().find(|(k, _)| k == t) {
            Some((_, c)) => *c += 1,
            None => count.push((t.to_string(), 1)),
        }
    }
    count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    count.into_iter().take(n).map(|(t, _)| t).collect()
}
