//! The review window: the queue (left), the body (middle), the take and the decision (right), the
//! scrub bar and transport (bottom). Every decision goes through [`Review::run`], as headless.
//!
//! Keys (when no text field has the keyboard): Space play, Left/Right a frame (Shift: 10),
//! Up/Down the take before/after, F the next flag, I/O the selection's start/end, Esc no selection,
//! K ok (keep it all), C cut (the cuts shown plus the selection), X uncut the selection, R reject,
//! Backspace forget the decision.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use ac_review::decisions::Status;
use ac_review::review::{Filter, Review, Show};
use ac_review::store::Motion;
use egui::{Color32, Key, RichText};
use serde_json::{Value, json};

use crate::scrub::{Bar, Change};
use crate::viewer::Viewer;
use crate::{Args, Paths};

const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

pub struct App {
    review: Review,
    filter: Filter,
    queue: Vec<(usize, usize)>,
    current: Option<(usize, usize)>,
    motion: Option<Motion>,
    motion_error: Option<String>,
    frame: usize,
    playing: bool,
    speed: f32,
    clock: f64,
    selection: Option<(usize, usize)>,
    anchor: Option<usize>,
    /// The cuts being edited for the take shown (the decision's, else the scan's).
    working: Vec<[usize; 2]>,
    kind: String,
    tags: String,
    note: String,
    /// After a decision, go on to the next take.
    advance: bool,
    /// The queue scrolls the take shown into view (once, after it changes).
    scroll_to: bool,
    status: String,
    viewer: Viewer,
    unpacking: HashMap<usize, Receiver<Result<(), String>>>,
    unpack_failed: HashMap<usize, String>,
    screenshot: Option<(PathBuf, Arc<AtomicU8>)>,
    frames_drawn: u32,
}

fn badge(s: Option<Status>) -> RichText {
    match s {
        Some(Status::Ok) => RichText::new("OK ").color(Color32::from_rgb(90, 200, 110)),
        Some(Status::Cut) => RichText::new("CUT").color(Color32::from_rgb(230, 150, 60)),
        Some(Status::Reject) => RichText::new("REJ").color(Color32::from_rgb(230, 80, 80)),
        None => RichText::new(" · ").color(Color32::GRAY),
    }
    .monospace()
}

/// `cuts` without the frames `a..=b`.
fn uncut(cuts: &[[usize; 2]], a: usize, b: usize) -> Vec<[usize; 2]> {
    let mut out = Vec::new();
    for &[x, y] in cuts {
        if y < a || x > b {
            out.push([x, y]);
            continue;
        }
        if x < a {
            out.push([x, a - 1]);
        }
        if y > b {
            out.push([b + 1, y]);
        }
    }
    out
}

impl App {
    pub fn new(cc: &eframe::CreationContext, args: Args, paths: Paths) -> Result<App, String> {
        let mut review = Review::open(&paths.data, &paths.decisions)?;
        review.check_rig(&paths.assets.join("rig/default_rig.json"))?;
        let viewer = Viewer::new(cc, &paths.assets)?;
        let mut app = App {
            review,
            filter: Filter::default(),
            queue: Vec::new(),
            current: None,
            motion: None,
            motion_error: None,
            frame: 0,
            playing: false,
            speed: 1.0,
            clock: 0.0,
            selection: None,
            anchor: None,
            working: Vec::new(),
            kind: String::new(),
            tags: String::new(),
            note: String::new(),
            advance: true,
            scroll_to: true,
            status: String::new(),
            viewer,
            unpacking: HashMap::new(),
            unpack_failed: HashMap::new(),
            screenshot: args.screenshot.map(|p| (p, Arc::new(AtomicU8::new(0)))),
            frames_drawn: 0,
        };
        app.refresh();
        if let Some(id) = &args.open {
            match app.review.find(id) {
                Ok(t) => {
                    app.filter.show = Show::All;
                    app.refresh();
                    app.select(t);
                }
                Err(e) => app.status = e,
            }
        } else if let Some(&t) = app.queue.first() {
            app.select(t);
        }
        if let Some(f) = args.frame {
            app.frame = f;
        }
        Ok(app)
    }

    fn refresh(&mut self) {
        self.queue = self.review.queue(&self.filter);
    }

    fn select(&mut self, t: (usize, usize)) {
        self.current = Some(t);
        self.scroll_to = true;
        self.frame = 0;
        self.clock = 0.0;
        self.selection = None;
        self.anchor = None;
        self.motion = None;
        self.motion_error = None;
        let d = self.review.decision(t.0, t.1).cloned();
        self.kind = d.as_ref().and_then(|d| d.kind.clone()).unwrap_or_default();
        self.tags = d.as_ref().map(|d| d.tags.join(", ")).unwrap_or_default();
        self.note = d.as_ref().map(|d| d.note.clone()).unwrap_or_default();
        self.working = match &d {
            Some(d) if d.status == Status::Cut => d.cuts.clone(),
            _ => self
                .review
                .scan(t.0, t.1)
                .cuts
                .iter()
                .map(|&(a, b, _)| [a, b])
                .collect(),
        };
        self.load_motion();
    }

    /// The take's frames, or start unpacking its set (once).
    fn load_motion(&mut self) {
        let Some((s, t)) = self.current else { return };
        let Some(set) = self.review.sets.get(s) else {
            return;
        };
        if !set.unpacked() {
            if !self.unpacking.contains_key(&s) && !self.unpack_failed.contains_key(&s) {
                let job = set.unpack_job();
                let (tx, rx) = channel();
                std::thread::spawn(move || {
                    // the window may have closed; nothing to tell then
                    tx.send(job.run()).ok();
                });
                self.unpacking.insert(s, rx);
            }
            return;
        }
        match set.motion(t) {
            Ok(m) => self.motion = Some(m),
            Err(e) => self.motion_error = Some(e),
        }
    }

    fn poll_unpacking(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        for (&s, rx) in &self.unpacking {
            match rx.try_recv() {
                Ok(r) => done.push((s, r)),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => done.push((s, Err("the unpacking stopped".into()))),
            }
        }
        for (s, r) in done {
            self.unpacking.remove(&s);
            let name = self.review.sets.get(s).map_or(String::new(), |x| x.name.clone());
            match r {
                Ok(()) => self.status = format!("{name} unpacked"),
                Err(e) => {
                    self.status = format!("{name}: {e}");
                    self.unpack_failed.insert(s, e);
                }
            }
            if self.current.is_some_and(|c| c.0 == s) {
                self.load_motion();
            }
        }
        if !self.unpacking.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn frames(&self) -> usize {
        self.motion.as_ref().map_or(0, |m| m.roots.len())
    }

    /// The playhead stays on the take (a `--frame` past its end, a shorter take).
    fn clamp_frame(&mut self) {
        let n = self.frames();
        if n > 0 && self.frame >= n {
            self.frame = n - 1;
        }
    }

    fn id(&self) -> Option<String> {
        self.current.map(|(s, t)| self.review.id(s, t))
    }

    /// Run a review command for the take shown; say what happened.
    fn decide(&mut self, status: Status) {
        let Some(id) = self.id() else { return };
        let mut p = json!({ "id": id, "status": status.word(), "note": self.note, "type": self.kind, "by": "person" });
        let tags: Vec<String> = self
            .tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        p["tags"] = json!(tags);
        if status == Status::Cut {
            let mut cuts = self.working.clone();
            if let Some((a, b)) = self.selection {
                cuts.push([a, b]);
            }
            p["cuts"] = json!(cuts);
        }
        let params = match p {
            Value::Object(m) => m,
            _ => return,
        };
        match self.review.run("review.decide", &params) {
            Ok(saved) => {
                if let Some(c) = saved
                    .get("cuts")
                    .and_then(|c| serde_json::from_value(c.clone()).ok())
                {
                    self.working = c;
                }
                self.selection = None;
                self.status = format!("{id}: {}", status.word());
                self.after_decision();
            }
            Err(e) => self.status = format!("{id}: {e}"),
        }
    }

    fn clear(&mut self) {
        let Some(id) = self.id() else { return };
        let mut p = serde_json::Map::new();
        p.insert("id".into(), json!(id));
        match self.review.run("review.clear", &p) {
            Ok(v) => {
                self.status = if v["cleared"].as_bool() == Some(true) {
                    format!("{id}: decision forgotten (the scan's verdict applies)")
                } else {
                    format!("{id}: nothing decided")
                };
                self.refresh();
            }
            Err(e) => self.status = format!("{id}: {e}"),
        }
    }

    /// The queue changes (a decided take leaves the open list): show the next.
    fn after_decision(&mut self) {
        let at = self.current.and_then(|c| self.queue.iter().position(|&q| q == c));
        self.refresh();
        if !self.advance {
            return;
        }
        let next = match at {
            Some(i) if self.queue.get(i) == self.current.as_ref() => self.queue.get(i + 1),
            Some(i) => self.queue.get(i),
            None => self.queue.first(),
        };
        if let Some(&n) = next {
            self.select(n);
        }
    }

    fn step_take(&mut self, by: isize) {
        let at = self.current.and_then(|c| self.queue.iter().position(|&q| q == c));
        let i = match at {
            Some(i) => i as isize + by,
            None => 0,
        };
        if let Some(&t) = usize::try_from(i).ok().and_then(|i| self.queue.get(i)) {
            self.select(t);
        }
    }

    fn next_flag(&mut self) {
        let Some((s, t)) = self.current else { return };
        let flags = &self.review.scan(s, t).cuts;
        let next = flags
            .iter()
            .map(|c| c.0)
            .find(|&a| a > self.frame)
            .or_else(|| flags.first().map(|c| c.0));
        if let Some(f) = next {
            self.frame = f;
            self.playing = false;
        }
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let n = self.frames();
        let (pressed, shift) = ctx.input(|i| {
            let keys = [
                Key::Space,
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
                Key::F,
                Key::I,
                Key::O,
                Key::Escape,
                Key::K,
                Key::C,
                Key::X,
                Key::R,
                Key::Backspace,
            ];
            (
                keys.into_iter().filter(|k| i.key_pressed(*k)).collect::<Vec<_>>(),
                i.modifiers.shift,
            )
        });
        let step = if shift { 10 } else { 1 };
        for k in pressed {
            match k {
                Key::Space => self.playing = !self.playing,
                Key::ArrowLeft => {
                    self.frame = self.frame.saturating_sub(step);
                    self.playing = false;
                }
                Key::ArrowRight => {
                    self.frame = (self.frame + step).min(n.saturating_sub(1));
                    self.playing = false;
                }
                Key::ArrowUp => self.step_take(-1),
                Key::ArrowDown => self.step_take(1),
                Key::F => self.next_flag(),
                Key::I => {
                    self.selection = Some((
                        self.frame,
                        self.selection.map_or(self.frame, |s| s.1.max(self.frame)),
                    ))
                }
                Key::O => {
                    self.selection = Some((
                        self.selection.map_or(self.frame, |s| s.0.min(self.frame)),
                        self.frame,
                    ))
                }
                Key::Escape => self.selection = None,
                Key::K => self.decide(Status::Ok),
                Key::C => self.decide(Status::Cut),
                Key::X => self.uncut_selection(),
                Key::R => self.decide(Status::Reject),
                Key::Backspace => self.clear(),
                _ => {}
            }
        }
    }

    fn uncut_selection(&mut self) {
        let Some((a, b)) = self.selection else {
            self.status = "select frames first (Shift-drag on the bar, or I and O)".into();
            return;
        };
        self.working = uncut(&self.working, a, b);
        self.selection = None;
        let decided = self
            .current
            .and_then(|(s, t)| self.review.decision(s, t))
            .map(|d| d.status);
        if decided == Some(Status::Cut) {
            if self.working.is_empty() {
                self.decide(Status::Ok);
            } else {
                self.decide(Status::Cut);
            }
        } else {
            self.status = format!("frames {a}–{b} not cut (C saves the cuts)");
        }
    }

    fn play(&mut self, ctx: &egui::Context) {
        let n = self.frames();
        if !self.playing || n == 0 {
            self.clock = self.frame as f64;
            return;
        }
        let fps = self
            .current
            .and_then(|(s, _)| self.review.sets.get(s))
            .map_or(30.0, |s| s.fps);
        let dt = ctx.input(|i| i.stable_dt) as f64;
        self.clock += dt * fps * self.speed as f64;
        if self.clock >= n as f64 {
            self.clock = 0.0;
        }
        self.frame = (self.clock as usize).min(n - 1);
        ctx.request_repaint();
    }

    fn top(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let before = self.filter.clone();
            egui::ComboBox::from_id_salt("show")
                .selected_text(match self.filter.show {
                    Show::Open => "To review",
                    Show::Flagged => "Flagged",
                    Show::Decided => "Decided",
                    Show::All => "All takes",
                    Show::Model => "Model fails",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.show, Show::Open, "To review")
                        .on_hover_text("Flagged by the scan, not decided yet");
                    ui.selectable_value(&mut self.filter.show, Show::Flagged, "Flagged");
                    ui.selectable_value(&mut self.filter.show, Show::Decided, "Decided");
                    ui.selectable_value(&mut self.filter.show, Show::All, "All takes");
                    ui.selectable_value(&mut self.filter.show, Show::Model, "Model fails")
                        .on_hover_text(
                            "The takes the model fails on (python -m ac_train.flag), worst loss first: look, then decide; a high loss is often just hard motion",
                        );
                });
            let sets: Vec<String> = self.review.sets.iter().map(|s| s.name.clone()).collect();
            egui::ComboBox::from_id_salt("set")
                .selected_text(self.filter.set.clone().unwrap_or_else(|| "every set".into()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.set, None, "every set");
                    for s in sets {
                        ui.selectable_value(&mut self.filter.set, Some(s.clone()), s);
                    }
                });
            let reasons = self.review.reasons();
            egui::ComboBox::from_id_salt("reason")
                .selected_text(self.filter.reason.clone().unwrap_or_else(|| "any reason".into()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.reason, None, "any reason");
                    for r in reasons {
                        ui.selectable_value(&mut self.filter.reason, Some(r.clone()), r);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.search)
                    .hint_text("search names and labels")
                    .desired_width(220.0),
            );
            if self.filter != before {
                self.refresh();
            }
            ui.separator();
            ui.label(format!("{} takes", self.queue.len()));
            ui.label(RichText::new(format!("{} decided", self.review.decisions.takes.len())).weak());
            ui.checkbox(&mut self.advance, "next take after deciding");
            if self.filter.show == Show::Model {
                ui.label(RichText::new(&self.review.flags.note).weak());
            }
        });
    }

    fn queue_panel(&mut self, ui: &mut egui::Ui) {
        let row = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        let mut pick = None;
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_to)
            && let Some(i) = self.current.and_then(|c| self.queue.iter().position(|&q| q == c))
        {
            let spacing = ui.spacing().item_spacing.y;
            let above = (ui.available_height() / 3.0).max(0.0);
            area = area.vertical_scroll_offset((i as f32 * (row + spacing) - above).max(0.0));
        }
        area.show_rows(ui, row, self.queue.len(), |ui, range| {
            for i in range {
                let Some(&(s, t)) = self.queue.get(i) else {
                    continue;
                };
                let sc = self.review.scan(s, t);
                let d = self.review.decision(s, t).map(|d| d.status);
                let cut = 100.0 * sc.cut_frames as f64 / sc.frames.max(1) as f64;
                let mut text = format!("{}  {cut:>3.0}%", self.review.id(s, t));
                let mut reasons = sc.reasons();
                if !sc.reject.is_empty() {
                    reasons.insert(0, "REJECTED".into());
                }
                if !reasons.is_empty() {
                    text.push_str(&format!("  {}", reasons.join(" ")));
                }
                if let Some(f) = self.review.flag(s, t) {
                    text.push_str(&format!("  loss {:.3} #{}", f.loss, f.rank));
                }
                ui.horizontal(|ui| {
                    ui.label(badge(d));
                    let on = self.current == Some((s, t));
                    if ui
                        .selectable_label(on, RichText::new(text).monospace().size(12.0))
                        .clicked()
                    {
                        pick = Some((s, t));
                    }
                });
            }
        });
        if let Some(t) = pick {
            self.select(t);
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let Some((s, t)) = self.current else {
            ui.label("Nothing to review here: change the filter.");
            return;
        };
        let (Some(set), id) = (self.review.sets.get(s), self.review.id(s, t)) else {
            return;
        };
        let Some(take) = set.takes.get(t) else { return };
        ui.heading(RichText::new(&take.name).size(16.0));
        ui.label(
            RichText::new(format!(
                "{} · {} frames · {:.1} s",
                set.name,
                take.frames,
                take.frames as f64 / set.fps
            ))
            .weak(),
        );
        if take.label != take.name {
            ui.label(&take.label);
        }
        ui.separator();
        let sc = self.review.scan(s, t).clone();
        ui.strong("The scan");
        if sc.take.is_empty() {
            ui.label(RichText::new("not scanned (python -m ac_train.quality)").weak());
        } else {
            if !sc.reject.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(230, 90, 90),
                    format!("rejects it: {}", sc.reject),
                );
            }
            ui.label(format!(
                "cuts {} of {} frames · floor {:+.1} cm · {} glitch frames · sliding {:.0}% of contacts · floats {:.1} s",
                sc.cut_frames,
                sc.frames,
                sc.floor_m * 100.0,
                sc.glitches,
                sc.skate_share * 100.0,
                sc.float_s
            ));
            let mut jump = None;
            egui::ScrollArea::vertical()
                .id_salt("flags")
                .max_height(140.0)
                .show(ui, |ui| {
                    for (a, b, why) in &sc.cuts {
                        if ui.link(format!("{a}–{b}  {why}")).clicked() {
                            jump = Some(*a);
                        }
                    }
                });
            if let Some(f) = jump {
                self.frame = f;
                self.playing = false;
            }
        }
        ui.separator();
        ui.strong("The model");
        match self.review.flag(s, t).cloned() {
            Some(f) => {
                ui.label(format!("#{} of the takes it fails on: {}", f.rank, f.why));
                if ui.link(format!("worst window at frame {}", f.worst_at)).clicked() {
                    self.frame = f.worst_at;
                    self.playing = false;
                }
                ui.label(
                    RichText::new(
                        "A high loss is often just hard motion, not bad data: look before deciding.",
                    )
                    .weak(),
                );
            }
            None if self.review.flags.present => {
                ui.label(
                    RichText::new(format!(
                        "not among the takes {} fails on",
                        self.review.flags.model
                    ))
                    .weak(),
                );
            }
            None => {
                ui.label(RichText::new(&self.review.flags.note).weak());
            }
        }
        ui.separator();
        ui.strong("Your decision");
        let d = self.review.decision(s, t).cloned();
        match &d {
            Some(d) => {
                ui.horizontal(|ui| {
                    ui.label(badge(Some(d.status)));
                    ui.label(format!("by {}", if d.by.is_empty() { "?" } else { &d.by }));
                    if d.frames != take.frames {
                        ui.colored_label(
                            Color32::YELLOW,
                            format!("reviewed at {} frames: the take changed", d.frames),
                        );
                    }
                });
            }
            None => {
                ui.label(RichText::new("none yet: the scan's verdict applies").weak());
            }
        }
        ui.label(format!(
            "cuts: {}",
            if self.working.is_empty() {
                "none".to_string()
            } else {
                self.working
                    .iter()
                    .map(|[a, b]| format!("{a}–{b}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Keep it all (K)")
                .on_hover_text("The take is good: the scan was wrong")
                .clicked()
            {
                self.decide(Status::Ok);
            }
            if ui
                .button("Cut (C)")
                .on_hover_text("Leave out the cuts shown in orange, plus the selection")
                .clicked()
            {
                self.decide(Status::Cut);
            }
            if ui.button("Uncut selection (X)").clicked() {
                self.uncut_selection();
            }
            if ui
                .button("Reject (R)")
                .on_hover_text("Leave the whole take out")
                .clicked()
            {
                self.decide(Status::Reject);
            }
            if ui
                .add_enabled(d.is_some(), egui::Button::new("Forget (Backspace)"))
                .clicked()
            {
                self.clear();
            }
        });
        ui.add_space(6.0);
        egui::Grid::new("fields").num_columns(2).show(ui, |ui| {
            ui.label("Type");
            ui.add(
                egui::TextEdit::singleline(&mut self.kind)
                    .hint_text("only if the name's is wrong: walk, fight…"),
            );
            ui.end_row();
            ui.label("Style");
            ui.add(egui::TextEdit::singleline(&mut self.tags).hint_text("tired, sneaky, angry…"));
            ui.end_row();
            ui.label("Note");
            ui.add(
                egui::TextEdit::multiline(&mut self.note)
                    .desired_rows(3)
                    .hint_text("e.g. retarget bug, not bad mocap"),
            );
            ui.end_row();
        });
        if let Some(d) = &d
            && ui.button("Save type, style and note").clicked()
        {
            let keep_advance = std::mem::replace(&mut self.advance, false);
            self.decide(d.status);
            self.advance = keep_advance;
        }
        ui.separator();
        ui.label(RichText::new(
            "Space play · Left/Right a frame (Shift: 10) · Up/Down the take · F next flag · I/O select · Esc unselect · Shift-drag the bar to select",
        ).weak().small());
        ui.label(RichText::new(id.to_string()).weak().small());
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        let n = self.frames();
        ui.horizontal(|ui| {
            if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                self.playing = !self.playing;
            }
            ui.label(RichText::new(format!("{:>5} / {}", self.frame, n.saturating_sub(1))).monospace());
            egui::ComboBox::from_id_salt("speed")
                .selected_text(format!("{}×", self.speed))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for s in SPEEDS {
                        ui.selectable_value(&mut self.speed, s, format!("{s}×"));
                    }
                });
            ui.checkbox(&mut self.viewer.follow, "follow");
            if let Some((a, b)) = self.selection {
                ui.label(RichText::new(format!("selected {a}–{b}")).color(Color32::from_rgb(120, 170, 240)));
            }
            ui.separator();
            ui.label(RichText::new(&self.status).weak());
        });
        if n == 0 {
            return;
        }
        let Some((s, t)) = self.current else { return };
        let fps = self.review.sets.get(s).map_or(30.0, |x| x.fps);
        let flags = self.review.scan(s, t).cuts.clone();
        let bar = Bar {
            frames: n,
            frame: self.frame,
            fps,
            flags: &flags,
            cuts: &self.working,
            selection: self.selection,
        };
        match bar.show(ui, &mut self.anchor) {
            Some(Change::Frame(f)) => {
                self.frame = f;
                self.playing = false;
            }
            Some(Change::Select(a, b)) => self.selection = Some((a, b)),
            None => {}
        }
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, state)) = &self.screenshot else {
            return;
        };
        self.frames_drawn += 1;
        match state.load(Ordering::SeqCst) {
            0 if self.frames_drawn > 5 && (self.unpacking.is_empty() || self.frames_drawn > 3000) => {
                state.store(1, Ordering::SeqCst);
                let (path, state) = (path.clone(), state.clone());
                ctx.request_screenshot(move |img| {
                    let ok = ac_render::offscreen::png(img.size[0] as u32, img.size[1] as u32, img.as_raw())
                        .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
                    if let Err(e) = &ok {
                        eprintln!("screenshot: {e}");
                    }
                    state.store(if ok.is_ok() { 2 } else { 3 }, Ordering::SeqCst);
                });
            }
            2 | 3 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_unpacking(&ctx);
        self.keys(&ctx);
        self.play(&ctx);
        self.clamp_frame();
        egui::Panel::top("filters").show(ui, |ui| self.top(ui));
        if !self.review.problems.is_empty() {
            egui::Panel::top("problems").show(ui, |ui| {
                for p in &self.review.problems {
                    ui.colored_label(Color32::YELLOW, p);
                }
            });
        }
        egui::Panel::bottom("transport").show(ui, |ui| self.transport(ui));
        egui::Panel::left("queue")
            .resizable(true)
            .default_size(430.0)
            .show(ui, |ui| self.queue_panel(ui));
        egui::Panel::right("details")
            .resizable(true)
            .default_size(380.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.details(ui))
            });
        let message = match self.current {
            None => "nothing selected".to_string(),
            Some((s, _)) if self.unpacking.contains_key(&s) => format!(
                "unpacking {} (once; up to a minute for the biggest set)…",
                self.review.sets.get(s).map_or("", |x| x.name.as_str())
            ),
            Some((s, _)) if self.unpack_failed.contains_key(&s) => {
                format!(
                    "can't unpack: {}",
                    self.unpack_failed.get(&s).map_or("", String::as_str)
                )
            }
            Some(_) => self.motion_error.clone().unwrap_or_else(|| "loading…".into()),
        };
        let frame_no = self.frame;
        let pose = self
            .motion
            .as_ref()
            .and_then(|m| Some((m.rotations.get(frame_no)?.as_slice(), *m.roots.get(frame_no)?)));
        egui::CentralPanel::default().show(ui, |ui| self.viewer.show(ui, frame, pose, &message));
        self.screenshot(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::uncut;

    #[test]
    fn uncutting_splits_ranges() {
        assert_eq!(uncut(&[[0, 10], [20, 30]], 5, 22), vec![[0, 4], [23, 30]]);
        assert_eq!(uncut(&[[5, 6]], 0, 10), Vec::<[usize; 2]>::new());
        assert_eq!(uncut(&[[0, 10]], 3, 4), vec![[0, 2], [5, 10]]);
    }
}
