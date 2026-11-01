//! The review window: the queue (left), the body (middle), the take and the decision (right), the
//! scrub bar and transport (bottom). Every decision goes through [`Review::run`], as headless.
//!
//! Three views of the clips: the list; groups (K and R the whole group, Enter opens it to override
//! single clips, Esc closes it); the gallery of thumbnails (click, Ctrl-click and Shift-click pick,
//! K and R all picked, double-click opens). Both act on the training set open.
//!
//! Keys (when no text field has the keyboard): Space play, Left/Right a frame (Shift: 10),
//! Up/Down the take before/after, F the next flag, I/O the selection's start/end, Esc no selection,
//! K ok (keep it all), C cut (the cuts shown plus the selection), X uncut the selection, R reject,
//! Backspace forget the decision, Ctrl+Z undo a training set's last change. With a training set open
//! K C R and Backspace act on that set; with none, on the decisions file.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use ac_review::decisions::Status;
use ac_review::groups::Group;
use ac_review::review::{Filter, Review, Show};
use ac_review::set_commands::StyleRow;
use ac_review::store::Motion;
use ac_review::styles;
use ac_review::thumbs::{Figure, Job, Thumbs, figure};
use ac_review::totals::Totals;
use ac_review::totals::{Usage, Usages};
use ac_review::training_set::{ClipStatus, SetClip};
use egui::{Color32, Key, RichText};
use serde_json::{Value, json};

use crate::scrub::{Bar, Change};
use crate::viewer::Viewer;
use crate::{Args, Paths};

const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
const CELL: egui::Vec2 = egui::vec2(120.0, 150.0);
const ROW: f32 = 58.0;

/// The least time a clip stays up when the list plays through.
const MIN_SHOWN: f64 = 1.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    List,
    Groups,
    Gallery,
    Styles,
}

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
    /// The labels in the training set open.
    labels: String,
    /// The training sets in the folder, and the name for New and Duplicate.
    set_names: Vec<String>,
    new_set: String,
    view: View,
    /// The open set's live totals, or why not.
    totals: Option<Result<Totals, String>>,
    groups: Vec<Group>,
    /// The group row with the keyboard.
    group_at: usize,
    /// The gallery's picked clips, the last one clicked, and the labels to give them.
    picked: BTreeSet<(usize, usize)>,
    last_pick: Option<(usize, usize)>,
    mass_labels: String,
    thumbs: Thumbs,
    /// Sets known to be unpacked, so that a thumbnail needn't ask the disk each frame.
    ready: HashSet<usize>,
    /// After a decision, go on to the next take.
    advance: bool,
    /// Play the list through, clip after clip.
    play_all: bool,
    /// Seconds the current clip has been on screen in `play_all`.
    shown_for: f64,
    style_rows: Vec<StyleRow>,
    usages: Usages,
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
        let mut review = crate::open_review(&args, &paths)?;
        let set_names = review.set_names();
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
            labels: String::new(),
            set_names,
            new_set: String::new(),
            view: match args.view.as_deref() {
                Some("groups") => View::Groups,
                Some("gallery") => View::Gallery,
                Some("styles") => View::Styles,
                _ => View::List,
            },
            totals: None,
            groups: Vec::new(),
            group_at: 0,
            picked: BTreeSet::new(),
            last_pick: None,
            mass_labels: String::new(),
            thumbs: Thumbs::new(),
            ready: HashSet::new(),
            advance: true,
            play_all: false,
            shown_for: 0.0,
            style_rows: Vec::new(),
            usages: Usages::default(),
            scroll_to: true,
            status: String::new(),
            viewer,
            unpacking: HashMap::new(),
            unpack_failed: HashMap::new(),
            screenshot: args.screenshot.map(|p| (p, Arc::new(AtomicU8::new(0)))),
            frames_drawn: 0,
        };
        if args.library.is_some() {
            // a library's clips have no scan, so "to review" would show none of them
            app.filter.show = Show::All;
        }
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
        if let Some(style) = args.style {
            app.view = View::Styles;
            app.open_style(style);
        }
        Ok(app)
    }

    fn refresh(&mut self) {
        self.queue = self.review.queue(&self.filter);
        self.usages = self
            .review
            .folder()
            .and_then(|f| Usages::load(&f))
            .unwrap_or_default();
        self.groups = self.review.groups(&self.queue);
        self.group_at = self.group_at.min(self.groups.len().saturating_sub(1));
        self.totals = self
            .review
            .training
            .active
            .is_some()
            .then(|| self.review.set_totals());
    }

    fn select(&mut self, t: (usize, usize)) {
        self.current = Some(t);
        self.scroll_to = true;
        self.frame = 0;
        self.clock = 0.0;
        self.shown_for = 0.0;
        self.selection = None;
        self.anchor = None;
        self.motion = None;
        self.motion_error = None;
        let d = self.review.decision(t.0, t.1).cloned();
        self.kind = d.as_ref().and_then(|d| d.kind.clone()).unwrap_or_default();
        self.tags = d.as_ref().map(|d| d.tags.join(", ")).unwrap_or_default();
        self.note = d.as_ref().map(|d| d.note.clone()).unwrap_or_default();
        let in_set = self.set_clip(t).cloned();
        self.labels = in_set.as_ref().map(|c| c.labels.join(", ")).unwrap_or_default();
        let active = self.review.training.active.is_some();
        self.working = match (&in_set, &d) {
            (Some(c), _) if !c.cuts.is_empty() => c.cuts.clone(),
            (Some(c), _) if c.status == ClipStatus::Keep => Vec::new(),
            (None, Some(d)) if !active && d.status == Status::Cut => d.cuts.clone(),
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

    fn set_clip(&self, t: (usize, usize)) -> Option<&SetClip> {
        self.review
            .training
            .active
            .as_ref()?
            .clips
            .get(&self.review.id(t.0, t.1))
    }

    /// A set command from the window; what it returned, or the refusal said.
    fn set_command(&mut self, command: &str, params: Value) -> Option<Value> {
        let Value::Object(p) = params else { return None };
        match self.review.run(command, &p) {
            Ok(v) => {
                self.set_names = self.review.set_names();
                self.refresh();
                Some(v)
            }
            Err(e) => {
                self.status = e;
                None
            }
        }
    }

    /// The set open changed: the clip shown shows that set's decision.
    fn reselect(&mut self) {
        if let Some(t) = self.current {
            let frame = self.frame;
            self.select(t);
            self.frame = frame;
        }
    }

    fn labels_typed(&self) -> Vec<String> {
        self.labels
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// K C R with a training set open: the clip's status, cuts and labels in that set.
    fn decide_in_set(&mut self, id: String, status: Status) {
        let mut p = json!({ "id": id, "labels": self.labels_typed() });
        match status {
            Status::Ok => {
                p["status"] = json!("keep");
                p["cuts"] = json!([]);
            }
            Status::Reject => p["status"] = json!("reject"),
            Status::Cut => {
                let mut cuts = self.working.clone();
                if let Some((a, b)) = self.selection {
                    cuts.push([a, b]);
                }
                if cuts.is_empty() {
                    self.status = "cut what? select frames first".into();
                    return;
                }
                p["status"] = json!("keep");
                p["cuts"] = json!(cuts);
            }
        }
        if let Some(style) = &self.filter.style
            && status != Status::Reject
        {
            p["add_labels"] = json!([style]);
        }
        let before = self.queue.clone();
        if self.set_command("review.set_clip", p).is_some() {
            // the next take is found from where this one was
            self.queue = before;
            self.selection = None;
            self.status = format!("{id}: {} in the set", status.word());
            self.after_decision();
            if !self.advance {
                self.reselect();
            }
        }
    }

    fn undo(&mut self) {
        if let Some(v) = self.set_command("review.undo", json!({})) {
            self.status = format!("undid {} clips", v["undone"]);
            self.reselect();
        }
    }

    /// The take's frames, or start unpacking its set (once).
    fn load_motion(&mut self) {
        let Some((s, t)) = self.current else { return };
        if !self.ensure_unpacked(s) {
            return;
        }
        let Some(set) = self.review.sets.get(s) else {
            return;
        };
        match set.motion(t) {
            Ok(m) => self.motion = Some(m),
            Err(e) => self.motion_error = Some(e),
        }
    }

    /// Whether a set's rotations are ready; if not, start unpacking them (once).
    fn ensure_unpacked(&mut self, s: usize) -> bool {
        if self.unpacking.contains_key(&s) || self.unpack_failed.contains_key(&s) {
            return false;
        }
        let Some(set) = self.review.sets.get(s) else {
            return false;
        };
        if set.unpacked() {
            return true;
        }
        let job = set.unpack_job();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            // the window may have closed; nothing to tell then
            tx.send(job.run()).ok();
        });
        self.unpacking.insert(s, rx);
        false
    }

    /// Queue the figures of the clips on screen, and only those.
    fn ask_thumbs(&mut self, on_screen: &[(usize, usize)], ctx: &egui::Context) {
        let sets: BTreeSet<usize> = on_screen.iter().map(|k| k.0).collect();
        for s in sets {
            if !self.ready.contains(&s) && self.ensure_unpacked(s) {
                self.ready.insert(s);
            }
        }
        let (review, ready, skeleton) = (&self.review, &self.ready, self.viewer.skeleton.clone());
        self.thumbs.show(on_screen, |(s, t)| {
            if !ready.contains(&s) {
                return None;
            }
            let pose = review.sets.get(s)?.pose_job(t).ok()?;
            let sk = skeleton.clone();
            let job: Job = Box::new(move || {
                let (rots, root) = pose.run()?;
                figure(&sk, &rots, root)
            });
            Some(job)
        });
        if self.thumbs.busy() {
            ctx.request_repaint();
        }
    }

    /// K or R on the gallery's picked clips: one undo step.
    fn decide_picked(&mut self, status: &str) {
        if self.picked.is_empty() {
            self.status = "pick clips first".into();
            return;
        }
        let ids = self.picked_ids();
        if let Some(v) = self.set_command("review.set_clip", json!({ "ids": ids, "status": status })) {
            self.status = format!("{} clips: {status}", v["changed"]);
            self.reselect();
        }
    }

    fn picked_ids(&self) -> Vec<String> {
        self.picked.iter().map(|&(s, t)| self.review.id(s, t)).collect()
    }

    /// K or R on the group row with the keyboard: one undo step.
    fn decide_group(&mut self, status: &str) {
        let Some(key) = self.groups.get(self.group_at).map(|g| g.key.clone()) else {
            return;
        };
        let p = json!({ "group": key, "status": status });
        if let Some(v) = self.set_command("review.set_group", p) {
            self.status = format!("{key}: {} clips {status}", v["changed"]);
            self.reselect();
        }
    }

    fn open_group(&mut self, at: usize) {
        let Some(g) = self.groups.get(at) else { return };
        self.group_at = at;
        self.filter.group = Some(g.key.clone());
        self.refresh();
        if let Some(&t) = self.queue.first() {
            self.select(t);
        }
    }

    fn open_style(&mut self, name: String) {
        self.filter.mine = styles::named(&name).is_some_and(|s| s.kind != "add-on");
        self.filter.style = Some(name);
        self.filter.show = Show::All;
        self.advance = true;
        self.refresh();
        if let Some(&t) = self.queue.first() {
            self.select(t);
        }
        self.play_all = true;
        self.playing = true;
    }

    fn close_style(&mut self) {
        self.filter.style = None;
        self.filter.mine = false;
        self.play_all = false;
        self.playing = false;
        self.refresh();
        self.style_rows = self.review.style_rows();
    }

    fn keep_style(&mut self) {
        let Some(style) = self.filter.style.clone() else {
            return;
        };
        let p = json!({ "style": style, "mine": self.filter.mine });
        if let Some(v) = self.set_command("review.style_keep", p) {
            self.status = format!("{} clips kept as {style}", v["changed"]);
            self.reselect();
        }
    }

    /// Send the clip on screen to a style, in place of any labels it had, and move on.
    fn send_to(&mut self, style: &str) {
        let Some(id) = self.id() else { return };
        let before = self.queue.clone();
        let p = json!({ "id": id, "status": "keep", "labels": [style] });
        if self.set_command("review.set_clip", p).is_some() {
            self.queue = before;
            self.status = format!("sent to {style}");
            self.after_decision();
        }
    }

    fn usage_buttons(&mut self, ui: &mut egui::Ui, ids: Vec<String>, current: Option<Usage>) {
        let mut chosen = None;
        for u in Usage::ALL {
            if ui
                .selectable_label(current == Some(u), u.title())
                .on_hover_text(u.meaning())
                .clicked()
            {
                chosen = Some(u);
            }
        }
        if let Some(u) = chosen
            && self
                .set_command("review.usage", json!({ "ids": ids, "usage": u.word() }))
                .is_some()
        {
            self.status = format!("{} clips: {}", ids.len(), u.title());
            self.refresh();
        }
    }

    fn styles_panel(&mut self, ui: &mut egui::Ui) {
        if self.style_rows.is_empty() {
            self.style_rows = self.review.style_rows();
        }
        if self.review.training.active.is_none() {
            ui.colored_label(
                Color32::YELLOW,
                "Open or make a training set first: Keep and Reject go in it.",
            );
        }
        let mut open = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for kind in ["to-sort", "base", "add-on"] {
                    ui.heading(match kind {
                        "to-sort" => "To sort",
                        "base" => "Base styles, your clips",
                        _ => "Add-ons, your clips and the free sets",
                    });
                    for r in self.style_rows.iter().filter(|r| r.style.kind == kind) {
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                if ui
                                    .button(RichText::new(format!("Play {}", r.style.name)).strong())
                                    .clicked()
                                {
                                    open = Some(r.style.name.clone());
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "{} clips · {:.1} min · {} labelled",
                                        r.clips, r.minutes, r.labelled
                                    ))
                                    .weak(),
                                );
                            });
                            ui.label(&r.style.about);
                            ui.label(
                                RichText::new(format!("Holds: {}", r.style.includes.join("; "))).small(),
                            );
                            ui.label(
                                RichText::new(format!("Not: {}", r.style.leaves_out.join("; ")))
                                    .small()
                                    .weak(),
                            );
                        });
                    }
                }
            });
        if let Some(name) = open {
            self.open_style(name);
        }
    }

    fn style_header(&mut self, ui: &mut egui::Ui) {
        let Some(st) = self.filter.style.as_deref().and_then(styles::named) else {
            return;
        };
        ui.heading(&st.name);
        ui.label(&st.about);
        ui.label(RichText::new(format!("Holds: {}", st.includes.join("; "))).small());
        ui.label(
            RichText::new(format!("Not: {}", st.leaves_out.join("; ")))
                .small()
                .weak(),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Back to the styles (Esc)").clicked() {
                self.close_style();
            }
            if ui.selectable_label(self.play_all, "Play all").clicked() {
                self.play_all = !self.play_all;
                self.playing = self.play_all;
            }
            if ui
                .button("Keep the rest")
                .on_hover_text("Everything still in this list is kept with this label. Ctrl+Z undoes it.")
                .clicked()
            {
                self.keep_style();
            }
        });
        let mut send = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("Send to:");
            for (i, name) in styles::SEND_TO.iter().enumerate() {
                if ui.small_button(format!("{} {name}", i + 1)).clicked() {
                    send = Some(*name);
                }
            }
        });
        if let Some(name) = send {
            self.send_to(name);
        }
        if ui.checkbox(&mut self.filter.mine, "My clips only").changed() {
            self.refresh();
        }
        ui.label(
            RichText::new(
                "R rejects and moves on · 1 to 9 send it to a style · I and O mark frames, C cuts them · K keeps · Up and Down skip",
            )
            .weak()
            .small(),
        );
        ui.separator();
    }

    fn close_group(&mut self) {
        let was = self.filter.group.take();
        self.refresh();
        if let Some(i) = self.groups.iter().position(|g| Some(&g.key) == was.as_ref()) {
            self.group_at = i;
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
        if self.review.training.active.is_some() {
            self.decide_in_set(id, status);
            return;
        }
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
        if self.review.training.active.is_some() {
            if let Some(v) = self.set_command("review.set_remove", json!({ "id": id })) {
                self.status = format!("{id}: {} removed from the set", v["changed"]);
            }
            return;
        }
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
        let (pressed, shift, command) = ctx.input(|i| {
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
                Key::Z,
                Key::Enter,
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            (
                keys.into_iter().filter(|k| i.key_pressed(*k)).collect::<Vec<_>>(),
                i.modifiers.shift,
                i.modifiers.command,
            )
        });
        let step = if shift { 10 } else { 1 };
        let rows = self.view == View::Groups && self.filter.group.is_none();
        let gallery = self.view == View::Gallery;
        let opened = self.view == View::Groups && self.filter.group.is_some();
        for k in pressed {
            match k {
                Key::K if rows => self.decide_group("keep"),
                Key::R if rows => self.decide_group("reject"),
                Key::ArrowUp if rows => self.group_at = self.group_at.saturating_sub(1),
                Key::ArrowDown if rows => {
                    self.group_at = (self.group_at + 1).min(self.groups.len().saturating_sub(1))
                }
                Key::Enter if rows => self.open_group(self.group_at),
                Key::Escape if opened && self.selection.is_none() => self.close_group(),
                Key::Escape if self.filter.style.is_some() && self.selection.is_none() => self.close_style(),
                Key::K | Key::R | Key::C | Key::X | Key::Backspace
                    if self.view == View::Styles && self.filter.style.is_none() =>
                {
                    self.status = "open a style first".into()
                }
                Key::K if gallery => self.decide_picked("keep"),
                Key::R if gallery => self.decide_picked("reject"),
                Key::Escape if gallery => self.picked.clear(),
                Key::C | Key::X | Key::Backspace if rows || gallery => {
                    self.status = "open a single clip for that".into()
                }
                Key::Num1
                | Key::Num2
                | Key::Num3
                | Key::Num4
                | Key::Num5
                | Key::Num6
                | Key::Num7
                | Key::Num8
                | Key::Num9 => {
                    let i = [
                        Key::Num1,
                        Key::Num2,
                        Key::Num3,
                        Key::Num4,
                        Key::Num5,
                        Key::Num6,
                        Key::Num7,
                        Key::Num8,
                        Key::Num9,
                    ]
                    .iter()
                    .position(|x| *x == k);
                    if self.filter.style.is_some()
                        && let Some(name) = i.and_then(|i| styles::SEND_TO.get(i))
                    {
                        self.send_to(name);
                    }
                }
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
                Key::C if !command => self.decide(Status::Cut),
                Key::X => self.uncut_selection(),
                Key::R => self.decide(Status::Reject),
                Key::Backspace => self.clear(),
                Key::Z if command => self.undo(),
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
        let decided = self.current.and_then(|(s, t)| self.review.status_of(s, t));
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
            .and_then(|(s, t)| self.review.sets.get(s)?.takes.get(t))
            .map_or(30.0, |t| t.fps);
        let dt = ctx.input(|i| i.stable_dt) as f64;
        self.clock += dt * fps * self.speed as f64;
        self.shown_for += dt;
        if self.clock >= n as f64 {
            self.clock = 0.0;
            // a clip of a few frames repeats until it can be seen
            if self.play_all && self.shown_for >= MIN_SHOWN {
                let at = self.current.and_then(|c| self.queue.iter().position(|&q| q == c));
                match at.and_then(|i| self.queue.get(i + 1)).copied() {
                    Some(next) => self.select(next),
                    None => {
                        self.play_all = false;
                        self.playing = false;
                        self.status = "end of the list".into();
                    }
                }
                ctx.request_repaint();
                return;
            }
        }
        self.frame = (self.clock as usize).min(n - 1);
        ctx.request_repaint();
    }

    /// A clip's figure in `rect`: 2 m of height fill it, the feet on its floor.
    fn paint_figure(painter: &egui::Painter, rect: egui::Rect, fig: &Figure) {
        let (scale, centre) = ac_review::thumbs::fit(fig, [rect.width(), rect.height()]);
        let at = |p: [f32; 2]| {
            egui::pos2(
                rect.center().x + (p[0] - centre[0]) * scale,
                rect.center().y - (p[1] - centre[1]) * scale,
            )
        };
        for [a, b] in fig {
            painter.line_segment([at(*a), at(*b)], egui::Stroke::new(2.5, Color32::LIGHT_GRAY));
        }
    }

    fn thumb(&self, painter: &egui::Painter, rect: egui::Rect, k: (usize, usize)) {
        match self.thumbs.get(k) {
            Some(Ok(fig)) => App::paint_figure(painter, rect, fig),
            // the viewer says why when the clip is opened
            Some(Err(_)) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "can't read",
                    egui::FontId::proportional(11.0),
                    Color32::LIGHT_RED,
                );
            }
            None => {
                painter.rect_filled(rect.shrink(rect.height() / 3.0), 2.0, Color32::from_gray(45));
            }
        }
    }

    fn groups_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("K keep · R reject the whole group · Enter opens it · Ctrl+Z undo")
                .weak()
                .small(),
        );
        let (mut pick, mut open) = (None, None);
        let mut on_screen = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW, self.groups.len(), |ui, range| {
                for i in range {
                    let Some(g) = self.groups.get(i) else { continue };
                    let width = ui.available_width();
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW), egui::Sense::click());
                    let p = ui.painter_at(rect);
                    if i == self.group_at {
                        p.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
                    }
                    if let Some(&k) = g.takes.first() {
                        on_screen.push(k);
                        let box_ = egui::Rect::from_min_size(rect.min, egui::vec2(ROW, ROW));
                        self.thumb(&p, box_, k);
                    }
                    let x = rect.left() + ROW + 6.0;
                    let text = Color32::LIGHT_GRAY;
                    p.text(
                        egui::pos2(x, rect.top() + 6.0),
                        egui::Align2::LEFT_TOP,
                        &g.name,
                        egui::FontId::proportional(14.0),
                        text,
                    );
                    p.text(
                        egui::pos2(x, rect.top() + 28.0),
                        egui::Align2::LEFT_TOP,
                        format!(
                            "{} · {} clips · {:.1} min · {} kept, {} rejected",
                            g.pack,
                            g.takes.len(),
                            g.minutes,
                            g.keep,
                            g.reject
                        ),
                        egui::FontId::proportional(11.0),
                        Color32::GRAY,
                    );
                    if resp.double_clicked() {
                        open = Some(i);
                    } else if resp.clicked() {
                        pick = Some(i);
                    }
                }
            });
        if let Some(i) = pick {
            self.group_at = i;
        }
        if let Some(i) = open {
            self.open_group(i);
        }
        let ctx = ui.ctx().clone();
        self.ask_thumbs(&on_screen, &ctx);
    }

    fn gallery(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} of {} picked", self.picked.len(), self.queue.len()));
            if ui.button("All").clicked() {
                self.picked = self.queue.iter().copied().collect();
            }
            if ui.button("None").clicked() {
                self.picked.clear();
            }
            if ui.button("Keep (K)").clicked() {
                self.decide_picked("keep");
            }
            if ui.button("Reject (R)").clicked() {
                self.decide_picked("reject");
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.mass_labels)
                    .hint_text("labels")
                    .desired_width(160.0),
            );
            for (button, key) in [("Add labels", "add_labels"), ("Remove labels", "remove_labels")] {
                if ui.button(button).clicked() {
                    let labels: Vec<String> = self
                        .mass_labels
                        .split(',')
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect();
                    let p = json!({ "ids": self.picked_ids(), key: labels });
                    if let Some(v) = self.set_command("review.set_clip", p) {
                        self.status = format!("labels changed on {} clips", v["changed"]);
                        self.reselect();
                    }
                }
            }
        });
        let gap = ui.spacing().item_spacing;
        let cols = ((ui.available_width() / (CELL.x + gap.x)) as usize).max(1);
        let rows = self.queue.len().div_ceil(cols);
        let mut click = None;
        let mut open = None;
        let mut on_screen = Vec::new();
        let mods = ui.input(|i| i.modifiers);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, CELL.y, rows, |ui, range| {
                for r in range {
                    ui.horizontal(|ui| {
                        for c in 0..cols {
                            let Some(&k) = self.queue.get(r * cols + c) else {
                                break;
                            };
                            on_screen.push(k);
                            let (rect, resp) = ui.allocate_exact_size(CELL, egui::Sense::click());
                            let p = ui.painter_at(rect);
                            if self.picked.contains(&k) {
                                p.rect_filled(rect, 3.0, ui.visuals().selection.bg_fill);
                            } else {
                                p.rect_filled(rect, 3.0, Color32::from_gray(30));
                            }
                            let pic = egui::Rect::from_min_size(rect.min, egui::vec2(CELL.x, CELL.y - 22.0));
                            self.thumb(&p, pic, k);
                            let colour = match self.review.status_of(k.0, k.1) {
                                Some(Status::Ok) => Color32::from_rgb(90, 200, 110),
                                Some(Status::Cut) => Color32::from_rgb(230, 150, 60),
                                Some(Status::Reject) => Color32::from_rgb(230, 80, 80),
                                None => Color32::from_gray(90),
                            };
                            p.circle_filled(rect.min + egui::vec2(9.0, 9.0), 4.0, colour);
                            let name = self.review.sets.get(k.0).and_then(|s| s.takes.get(k.1));
                            p.text(
                                egui::pos2(rect.left() + 3.0, rect.bottom() - 18.0),
                                egui::Align2::LEFT_TOP,
                                name.map_or("", |t| t.name.as_str()),
                                egui::FontId::proportional(10.0),
                                Color32::LIGHT_GRAY,
                            );
                            if resp.double_clicked() {
                                open = Some(k);
                            } else if resp.clicked() {
                                click = Some(k);
                            }
                        }
                    });
                }
            });
        if let Some(k) = click {
            self.pick(k, mods);
            self.select(k);
        }
        if let Some(k) = open {
            self.view = View::List;
            self.select(k);
        }
        let ctx = ui.ctx().clone();
        self.ask_thumbs(&on_screen, &ctx);
    }

    /// A click in the gallery: alone, Ctrl toggles, Shift takes the run from the last click.
    fn pick(&mut self, k: (usize, usize), mods: egui::Modifiers) {
        if mods.shift
            && let Some(from) = self.last_pick
        {
            let at = |x: (usize, usize)| self.queue.iter().position(|q| *q == x);
            if let (Some(a), Some(b)) = (at(from), at(k)) {
                let run: Vec<(usize, usize)> = self.queue[a.min(b)..=a.max(b)].to_vec();
                self.picked.extend(run);
            }
        } else if mods.command {
            if !self.picked.remove(&k) {
                self.picked.insert(k);
            }
        } else {
            self.picked.clear();
            self.picked.insert(k);
        }
        self.last_pick = Some(k);
    }

    /// Minutes per label and per usage label, the warning, and each pack's label.
    fn totals_panel(&mut self, ui: &mut egui::Ui) {
        let t = match &self.totals {
            Some(Ok(t)) => t.clone(),
            Some(Err(e)) => {
                ui.colored_label(Color32::LIGHT_RED, e);
                return;
            }
            None => return,
        };
        for w in &t.warnings {
            ui.colored_label(Color32::from_rgb(240, 170, 60), format!("Warning: {w}"));
        }
        ui.horizontal_wrapped(|ui| {
            if t.missing > 0 {
                ui.colored_label(
                    Color32::YELLOW,
                    format!(
                        "{} clips aren't here (give --library), so have no minutes",
                        t.missing
                    ),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (u, c) in [
                (Usage::ShareFreely, &t.share_freely),
                (Usage::TrainOnly, &t.train_only),
                (Usage::JustMe, &t.just_me),
            ] {
                ui.label(format!("{}: {} clips {:.1} min", u.title(), c.clips, c.minutes))
                    .on_hover_text(u.meaning());
            }
            if t.unmarked.clips > 0 {
                ui.label(
                    RichText::new(format!(
                        "({} not labelled yet, counted as Just me)",
                        t.unmarked.clips
                    ))
                    .weak(),
                );
            }
            ui.label(format!("{} rejected", t.rejected));
        });
        let mut mark = None;
        egui::CollapsingHeader::new(format!("{} labels, {} packs", t.labels.len(), t.packs.len()))
            .id_salt("totals-detail")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, c) in &t.labels {
                        ui.label(format!("{label}: {:.1} min", c.minutes));
                    }
                });
                for (pack, source) in &t.packs {
                    ui.horizontal(|ui| {
                        ui.label(pack);
                        for u in Usage::ALL {
                            if ui
                                .selectable_label(source == u.word(), u.title())
                                .on_hover_text(u.meaning())
                                .clicked()
                            {
                                mark = Some((pack.clone(), u.word()));
                            }
                        }
                        if source == "unmarked" {
                            ui.label(RichText::new("not labelled: counts as Just me").weak());
                        }
                    });
                }
            });
        if let Some((pack, word)) = mark {
            self.set_command("review.usage", json!({ "pack": pack, "usage": word }));
            self.refresh();
        }
    }

    /// The dropdown of training sets, with New, Duplicate, Copy labels from, public and undo.
    fn sets_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Training set");
            let open = self.review.training.active.as_ref().map(|a| a.name.clone());
            let mut pick = open.clone();
            egui::ComboBox::from_id_salt("training-set")
                .selected_text(open.clone().unwrap_or_else(|| "none (decisions file)".into()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, None, "none (decisions file)");
                    for n in &self.set_names {
                        ui.selectable_value(&mut pick, Some(n.clone()), n);
                    }
                });
            if pick != open
                && self
                    .set_command("review.set_open", json!({ "name": pick }))
                    .is_some()
            {
                self.reselect();
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.new_set)
                    .hint_text("name")
                    .desired_width(130.0),
            );
            if ui.button("New").clicked() {
                let name = self.new_set.clone();
                if self
                    .set_command("review.set_new", json!({ "name": name }))
                    .is_some()
                {
                    self.new_set.clear();
                    self.reselect();
                }
            }
            if ui
                .add_enabled(open.is_some(), egui::Button::new("Duplicate"))
                .clicked()
            {
                let name = self.new_set.clone();
                if self
                    .set_command("review.set_duplicate", json!({ "as": name }))
                    .is_some()
                {
                    self.new_set.clear();
                    self.reselect();
                }
            }
            let mut from = None;
            ui.add_enabled_ui(open.is_some(), |ui| {
                egui::ComboBox::from_id_salt("copy-labels")
                    .selected_text("Copy labels from")
                    .show_ui(ui, |ui| {
                        for n in self.set_names.iter().filter(|n| Some(*n) != open.as_ref()) {
                            if ui.selectable_label(false, n).clicked() {
                                from = Some(n.clone());
                            }
                        }
                    });
            });
            if let Some(from) = from
                && let Some(v) = self.set_command("review.set_copy_labels", json!({ "from": from }))
            {
                self.status = format!(
                    "labels from {from}: {} clips matched, {} changed, {} not in {from}",
                    v["matched"], v["changed"], v["not_in_source"]
                );
                self.reselect();
            }
            let public = self.review.training.active.as_ref().map(|a| a.public);
            if let Some(mut p) = public
                && ui.checkbox(&mut p, "public").changed()
            {
                self.set_command("review.set_public", json!({ "public": p }));
            }
            if ui
                .add_enabled(open.is_some(), egui::Button::new("Undo"))
                .clicked()
            {
                self.undo();
            }
        });
    }

    fn top(&mut self, ui: &mut egui::Ui) {
        self.sets_bar(ui);
        self.totals_panel(ui);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::List, "List");
            ui.selectable_value(&mut self.view, View::Groups, "Groups");
            ui.selectable_value(&mut self.view, View::Gallery, "Gallery");
            ui.selectable_value(&mut self.view, View::Styles, "Styles");
        });
        if self.view != View::Styles && self.filter.style.is_some() {
            self.close_style();
        }
        if self.view != View::Groups && self.filter.group.is_some() {
            self.close_group();
        }
        ui.horizontal_wrapped(|ui| {
            let before = self.filter.clone();
            egui::ComboBox::from_id_salt("show")
                .selected_text(match self.filter.show {
                    Show::Open => "To review",
                    Show::Flagged => "Flagged",
                    Show::Decided => "Decided",
                    Show::All => "All takes",
                    Show::Model => "Model fails",
                    Show::InSet => "In the set",
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
                    ui.selectable_value(&mut self.filter.show, Show::InSet, "In the set");
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
            let decided = match &self.review.training.active {
                Some(set) => format!("{} in {}", set.clips.len(), set.name),
                None => format!("{} in the decisions file", self.review.decisions.takes.len()),
            };
            ui.label(RichText::new(decided).weak());
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
                let d = self.review.status_of(s, t);
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
                take.frames as f64 / take.fps
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
        let in_set = self.review.training.active.is_some();
        if in_set {
            ui.label(badge(self.review.status_of(s, t)));
        }
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
        let (usage, chosen) = self.usages.of(&id);
        ui.horizontal_wrapped(|ui| {
            ui.label("Usage");
            self.usage_buttons(ui, vec![id.clone()], Some(usage));
        });
        ui.label(
            RichText::new(if chosen {
                usage.meaning().to_string()
            } else {
                format!("Not labelled yet. {}", usage.meaning())
            })
            .weak()
            .small(),
        );
        if in_set {
            ui.horizontal(|ui| {
                ui.label("Labels");
                ui.add(egui::TextEdit::singleline(&mut self.labels).hint_text("walk, tired…"));
                if ui.button("Save labels").clicked() {
                    let labels = self.labels_typed();
                    self.set_command("review.set_clip", json!({ "id": id, "labels": labels }));
                }
            });
        }
        if let Some(d) = d.as_ref().filter(|_| !in_set)
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
        let fps = self
            .review
            .sets
            .get(s)
            .and_then(|x| x.takes.get(t))
            .map_or(30.0, |x| x.fps);
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
            0 if self.frames_drawn > 5
                && ((self.unpacking.is_empty() && !self.thumbs.busy()) || self.frames_drawn > 3000) =>
            {
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
        if self.view != View::Gallery {
            egui::Panel::left("queue")
                .resizable(true)
                .default_size(430.0)
                .show(ui, |ui| {
                    if self.view == View::Groups && self.filter.group.is_none() {
                        self.groups_panel(ui);
                        return;
                    }
                    if self.view == View::Styles && self.filter.style.is_none() {
                        self.styles_panel(ui);
                        return;
                    }
                    if self.view == View::Styles {
                        self.style_header(ui);
                    }
                    if self.view == View::Groups && ui.button("Back to the groups (Esc)").clicked() {
                        self.close_group();
                    }
                    self.queue_panel(ui);
                });
        }
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
        if self.view == View::Gallery {
            egui::CentralPanel::default().show(ui, |ui| self.gallery(ui));
            self.screenshot(&ctx);
            return;
        }
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
