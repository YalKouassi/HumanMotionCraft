//! The Models tab (roadmap 196000): start and stop a training run, see every model, choose the main
//! one, compare two, and build the set made of other sets. Whatever changes something goes through
//! [`ac_review::review::Review::run`], as headless does.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use ac_review::models::{self, ModelList, Places};
use ac_review::train_run::{self, JobStatus, Kind};
use egui::{Color32, RichText};
use serde_json::{Value, json};

use super::App;

/// The sets the base model is made from, ticked until they are changed.
const BASE_SETS: [&str; 7] = ["walk", "run", "idle", "jump", "crouch", "gesture", "climb"];
/// The set the page starts on when there is one, and what Combine sets makes.
const DEFAULT_SET: &str = "base";
/// How long a run lasts unless told otherwise, in minutes (the shipped model's run was 180).
const DEFAULT_MINUTES: f64 = 180.0;
const GOOD: Color32 = Color32::from_rgb(90, 200, 110);
const BAD: Color32 = Color32::from_rgb(230, 80, 80);

pub(super) struct Page {
    list: Option<ModelList>,
    job: JobStatus,
    poll: Option<Receiver<JobStatus>>,
    polled: Option<Instant>,
    set: String,
    name: String,
    /// The name was typed, so a change of set leaves it.
    name_typed: bool,
    name_problem: Option<String>,
    minutes: f64,
    steps: String,
    compare_a: String,
    compare_b: String,
    compared: Option<Value>,
    combine_name: String,
    combine_free: bool,
    /// The sets the person changed from the ticks they start with.
    combine_ticks: Vec<(String, bool)>,
}

impl Default for Page {
    fn default() -> Page {
        Page {
            list: None,
            job: JobStatus::default(),
            poll: None,
            polled: None,
            set: String::new(),
            name: String::new(),
            name_typed: false,
            name_problem: None,
            minutes: DEFAULT_MINUTES,
            steps: String::new(),
            compare_a: String::new(),
            compare_b: String::new(),
            compared: None,
            combine_name: DEFAULT_SET.into(),
            combine_free: true,
            combine_ticks: Vec::new(),
        }
    }
}

/// What a click on the page asked for; done after drawing it.
enum Do {
    Check,
    Start,
    Stop { force: bool },
    Main(Option<String>),
    Compare,
    Combine,
    Refresh,
}

fn number(v: Option<f64>, digits: usize) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.digits$}"))
}

/// The job's state in a line.
fn job_line(s: &JobStatus) -> String {
    let Some(job) = &s.job else {
        return "Nothing is running.".to_string();
    };
    if !s.running {
        return s.ended.clone().unwrap_or_else(|| "It has ended.".to_string());
    }
    match job.kind {
        Kind::Check => format!(
            "Checking {}: this reads every clip, so it can take some minutes.",
            job.tag
        ),
        Kind::Main => format!("Exporting {} for the app.", job.tag),
        Kind::Train => match s.step {
            None => format!(
                "Training {}: reading the clips, the first step comes after that.",
                job.tag
            ),
            Some(step) => format!(
                "Training {}: step {step}, {} of {} minutes, about {} left, score {}, best so far {}.",
                job.tag,
                number(s.minutes, 1),
                number(job.minutes, 0),
                number(s.minutes_left, 0),
                number(s.score, 3),
                number(s.best_score, 3),
            ),
        },
    }
}

impl App {
    fn models_places(&self) -> &Places {
        &self.review.places
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.models.poll {
            match rx.try_recv() {
                Ok(s) => {
                    let ended = self.models.job.running && !s.running;
                    self.models.job = s;
                    self.models.poll = None;
                    if ended {
                        self.load_models();
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.models.poll = None,
            }
        }
        let every = if self.models.job.running { 2 } else { 5 };
        let due = self
            .models
            .polled
            .is_none_or(|t| t.elapsed() > Duration::from_secs(every));
        if self.models.poll.is_none() && due {
            let (tx, rx) = channel();
            let places = self.models_places().clone();
            std::thread::spawn(move || {
                let _ = tx.send(train_run::status(&places));
            });
            self.models.poll = Some(rx);
            self.models.polled = Some(Instant::now());
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn load_models(&mut self) {
        let list = models::list(self.models_places());
        let tags: Vec<&String> = list.rows.iter().filter(|r| r.usable).map(|r| &r.tag).collect();
        for side in [&mut self.models.compare_a, &mut self.models.compare_b] {
            if !tags.contains(&&*side) {
                side.clear();
            }
        }
        self.models.list = Some(list);
        self.check_name();
    }

    fn check_name(&mut self) {
        let places = &self.review.places;
        self.models.name_problem = if self.models.name.is_empty() {
            Some("Give the model a name.".to_string())
        } else {
            models::name_problem(&places.models, &self.models.name)
        };
    }

    /// A new name for the set, unless one was typed.
    fn suggest_name(&mut self) {
        if !self.models.name_typed && !self.models.set.is_empty() {
            self.models.name = models::suggest_name(&self.review.places.models, &self.models.set);
        }
        self.check_name();
    }

    fn combine_sets(&self) -> Vec<String> {
        self.models
            .combine_ticks
            .iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// The sets to tick from: those that exist, the base's ticked; what was changed is kept.
    fn update_ticks(&mut self) {
        let was = std::mem::take(&mut self.models.combine_ticks);
        let target = &self.models.combine_name;
        self.models.combine_ticks = self
            .set_names
            .iter()
            .filter(|n| *n != target)
            .map(|n| {
                let on = was
                    .iter()
                    .find(|(m, _)| m == n)
                    .map_or(BASE_SETS.contains(&n.as_str()), |(_, on)| *on);
                (n.clone(), on)
            })
            .collect();
    }

    fn do_click(&mut self, todo: Do) {
        self.status.clear();
        match todo {
            Do::Check => {
                let set = self.models.set.clone();
                if self
                    .set_command("review.train_check", json!({ "set": set }))
                    .is_some()
                {
                    self.models.polled = None;
                }
            }
            Do::Start => {
                let steps: Option<u64> = match self.models.steps.trim() {
                    "" => None,
                    t => match t.parse() {
                        Ok(n) => Some(n),
                        Err(_) => {
                            self.status = format!("steps {t:?}: a whole number, or nothing");
                            return;
                        }
                    },
                };
                let p = json!({
                    "set": self.models.set, "tag": self.models.name,
                    "minutes": self.models.minutes, "steps": steps,
                });
                if self.set_command("review.train_start", p).is_some() {
                    self.models.polled = None;
                    self.models.name_typed = false;
                    self.suggest_name();
                }
            }
            Do::Stop { force } => {
                if let Some(v) = self.set_command("review.train_stop", json!({ "force": force })) {
                    self.status = v["said"].as_str().unwrap_or_default().to_string();
                    self.models.polled = None;
                }
            }
            Do::Main(tag) => {
                if let Some(v) = self.set_command("review.model_main", json!({ "tag": tag })) {
                    if v["exporting"].as_bool() == Some(true) {
                        self.models.polled = None;
                    }
                    self.load_models();
                }
            }
            Do::Compare => {
                let p = json!({ "a": self.models.compare_a, "b": self.models.compare_b });
                self.models.compared = self.set_command("review.models_compare", p);
            }
            Do::Combine => {
                let p = json!({
                    "name": self.models.combine_name,
                    "from": self.combine_sets(),
                    "free": self.models.combine_free,
                });
                if let Some(v) = self.set_command("review.set_combine", p) {
                    self.status = format!(
                        "{}: {} clips kept, {} rejected ({} free clips listed), version {}",
                        v["name"].as_str().unwrap_or_default(),
                        v["keep"],
                        v["reject"],
                        v["free_clips"],
                        v["set_version"],
                    );
                }
            }
            Do::Refresh => self.load_models(),
        }
    }

    pub(super) fn models_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll_job(&ctx);
        if self.models.list.is_none() {
            self.load_models();
        }
        if self.models.set.is_empty()
            && let Some(first) = self.set_names.first().cloned()
        {
            // the base model's set, else the one being gone through
            let open = self.review.training.active.as_ref().map(|a| a.name.clone());
            self.models.set = if self.set_names.iter().any(|n| n == DEFAULT_SET) {
                DEFAULT_SET.to_string()
            } else {
                open.unwrap_or(first)
            };
            self.suggest_name();
        }
        self.update_ticks();
        let mut todo = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if !self.status.is_empty() {
                ui.colored_label(Color32::YELLOW, &self.status);
            }
            self.train_section(ui, &mut todo);
            ui.separator();
            self.models_section(ui, &mut todo);
            ui.separator();
            self.compare_section(ui, &mut todo);
            ui.separator();
            self.combine_section(ui, &mut todo);
        });
        for t in todo {
            self.do_click(t);
        }
    }

    fn train_section(&mut self, ui: &mut egui::Ui, todo: &mut Vec<Do>) {
        ui.heading("Train");
        let busy = self.models.job.running;
        ui.horizontal_wrapped(|ui| {
            ui.label("Set");
            let before = self.models.set.clone();
            egui::ComboBox::from_id_salt("train-set")
                .selected_text(if before.is_empty() {
                    "none"
                } else {
                    before.as_str()
                })
                .show_ui(ui, |ui| {
                    for n in &self.set_names {
                        ui.selectable_value(&mut self.models.set, n.clone(), n);
                    }
                });
            if self.models.set != before {
                self.suggest_name();
            }
            ui.label("Name");
            let typed = ui.add(egui::TextEdit::singleline(&mut self.models.name).desired_width(220.0));
            if typed.changed() {
                self.models.name_typed = true;
                self.check_name();
            }
            ui.label("Minutes");
            ui.add(egui::DragValue::new(&mut self.models.minutes).range(1.0..=models::MAX_MINUTES));
            ui.label("Stop after steps");
            ui.add(
                egui::TextEdit::singleline(&mut self.models.steps)
                    .hint_text("no limit")
                    .desired_width(70.0),
            );
        });
        if let Some(why) = &self.models.name_problem {
            ui.colored_label(BAD, why);
        }
        if self.models.set.starts_with("addon") {
            ui.colored_label(
                Color32::YELLOW,
                "This would train a whole new model from these clips alone: starting an add-on from the base isn't built yet (roadmap 154000).",
            );
        }
        ui.horizontal_wrapped(|ui| {
            let has_set = !self.models.set.is_empty();
            if ui
                .add_enabled(!busy && has_set, egui::Button::new("Check first"))
                .on_hover_text(
                    "The trainer's dry run: clips, minutes and what it leaves out. Nothing is trained.",
                )
                .clicked()
            {
                todo.push(Do::Check);
            }
            let can_start = !busy && has_set && self.models.name_problem.is_none();
            if ui.add_enabled(can_start, egui::Button::new("Start")).clicked() {
                todo.push(Do::Start);
            }
            if ui
                .add_enabled(busy, egui::Button::new("Stop"))
                .on_hover_text("A run finishes its step, evaluates and saves what it has.")
                .clicked()
            {
                todo.push(Do::Stop { force: false });
            }
            if ui
                .add_enabled(busy, egui::Button::new("Force stop"))
                .on_hover_text("Ends the process now. Only the last best checkpoint is left.")
                .clicked()
            {
                todo.push(Do::Stop { force: true });
            }
        });
        let s = &self.models.job;
        let colour = match (&s.ended, s.ended_well) {
            (Some(_), true) => GOOD,
            (Some(_), false) => BAD,
            (None, _) => ui.visuals().text_color(),
        };
        ui.colored_label(colour, job_line(s));
        if let Some(p) = &s.problem {
            ui.colored_label(BAD, p);
        }
        if s.job.is_some() {
            ui.push_id("job-log", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .auto_shrink([false, true])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for l in &s.tail {
                            // a size, as .small() would take away the fixed width the tables need
                            ui.label(RichText::new(l).monospace().size(11.0));
                        }
                    });
            });
        }
    }

    fn models_section(&mut self, ui: &mut egui::Ui, todo: &mut Vec<Do>) {
        ui.horizontal(|ui| {
            ui.heading("Every model");
            if ui.button("Refresh").clicked() {
                todo.push(Do::Refresh);
            }
        });
        let Some(list) = &self.models.list else { return };
        let busy = self.models.job.running;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("The app opens with {}.", list.opens_with));
            if ui
                .add_enabled(!list.main_is_shipped, egui::Button::new("Use the shipped model"))
                .clicked()
            {
                todo.push(Do::Main(None));
            }
        });
        if let Some(why) = &list.main_problem {
            ui.colored_label(BAD, format!("The main model can't be used: {why}"));
        }
        ui.label(format!(
            "What everyone gets: {}. Changing that is a release, not done here.",
            list.everyone_gets
        ));
        if list.rows.is_empty() {
            ui.label("No models yet.");
            return;
        }
        egui::Grid::new("models")
            .striped(true)
            .num_columns(10)
            .show(ui, |ui| {
                for head in ["Name", "Day", "Set", "Minutes", "Steps", "Styles", "Score", "Best", "State", ""] {
                    let label = ui.label(RichText::new(head).strong());
                    if head == "Score" || head == "Best" {
                        label.on_hover_text(
                            "Lower is better. Measured on the clips each run held back, so runs of different sets don't compare exactly.",
                        );
                    }
                }
                ui.end_row();
                for r in &list.rows {
                    ui.label(RichText::new(&r.tag).monospace());
                    ui.label(&r.date);
                    ui.label(&r.set);
                    ui.label(number(r.minutes, 0));
                    ui.label(r.steps.map_or("-".to_string(), |n| n.to_string()));
                    ui.label(r.styles.map_or("-".to_string(), |n| n.to_string()));
                    ui.label(number(r.score, 3));
                    ui.label(number(r.best_score, 3));
                    match &r.problem {
                        Some(p) => {
                            ui.colored_label(BAD, "card won't read").on_hover_text(p);
                        }
                        None => {
                            ui.label(&r.state);
                        }
                    }
                    ui.horizontal(|ui| {
                        if r.is_main {
                            ui.colored_label(GOOD, "Main");
                        } else if ui
                            .add_enabled(r.usable && !busy, egui::Button::new("Set as main"))
                            .clicked()
                        {
                            todo.push(Do::Main(Some(r.tag.clone())));
                        }
                        if r.shipped {
                            ui.label("Shipped");
                        }
                    });
                    ui.end_row();
                }
            });
    }

    fn compare_section(&mut self, ui: &mut egui::Ui, todo: &mut Vec<Do>) {
        ui.heading("Compare");
        let Some(list) = &self.models.list else { return };
        let tags: Vec<String> = list
            .rows
            .iter()
            .filter(|r| r.usable)
            .map(|r| r.tag.clone())
            .collect();
        let (a0, b0) = (self.models.compare_a.clone(), self.models.compare_b.clone());
        ui.horizontal_wrapped(|ui| {
            for (id, pick) in [
                ("compare-a", &mut self.models.compare_a),
                ("compare-b", &mut self.models.compare_b),
            ] {
                let shown = if pick.is_empty() {
                    "pick a model".to_string()
                } else {
                    pick.clone()
                };
                egui::ComboBox::from_id_salt(id)
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for t in &tags {
                            ui.selectable_value(pick, t.clone(), t);
                        }
                    });
            }
        });
        let both = !self.models.compare_a.is_empty() && !self.models.compare_b.is_empty();
        if both && (self.models.compare_a != a0 || self.models.compare_b != b0) {
            todo.push(Do::Compare);
        }
        let Some(c) = &self.models.compared else { return };
        for n in c["notes"].as_array().into_iter().flatten() {
            ui.colored_label(Color32::YELLOW, n.as_str().unwrap_or_default());
        }
        let (a, b) = (
            c["a"]["tag"].as_str().unwrap_or("a"),
            c["b"]["tag"].as_str().unwrap_or("b"),
        );
        egui::Grid::new("compared")
            .striped(true)
            .num_columns(5)
            .show(ui, |ui| {
                let plain_a = format!("plain blend, {a}");
                let plain_b = format!("plain blend, {b}");
                for head in ["Gap, frames", a, b, plain_a.as_str(), plain_b.as_str()] {
                    ui.label(RichText::new(head).strong());
                }
                ui.end_row();
                let mm = |v: &Value| v.as_f64().map_or("-".to_string(), |x| format!("{x:.1} mm"));
                for row in c["gaps"].as_array().into_iter().flatten() {
                    ui.label(row["frames"].as_str().unwrap_or_default());
                    ui.label(mm(&row["a_mm"]));
                    ui.label(mm(&row["b_mm"]));
                    ui.label(mm(&row["a_plain_mm"]));
                    ui.label(mm(&row["b_plain_mm"]));
                    ui.end_row();
                }
                let score = |v: &Value| v.as_f64().map_or("-".to_string(), |x| format!("{x:.3}"));
                ui.label("Score");
                ui.label(score(&c["a"]["score"]));
                ui.label(score(&c["b"]["score"]));
                ui.end_row();
                ui.label("Best");
                ui.label(score(&c["a"]["best_score"]));
                ui.label(score(&c["b"]["best_score"]));
                ui.end_row();
            });
        ui.label("Average joint error on what each run held back, lower is better. The numbers don't replace watching them.");
    }

    fn combine_section(&mut self, ui: &mut egui::Ui, todo: &mut Vec<Do>) {
        ui.heading("Combine sets");
        ui.label("One set made of others, for one model. A clip rejected in any of them stays out, and it is rebuilt when a run of it starts, so later rejects count.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut self.models.combine_name).desired_width(140.0));
            for (name, on) in &mut self.models.combine_ticks {
                ui.checkbox(on, name.as_str());
            }
            ui.checkbox(&mut self.models.combine_free, "the free clips");
        });
        if ui
            .add_enabled(
                !self.combine_sets().is_empty(),
                egui::Button::new("Build the set"),
            )
            .clicked()
        {
            todo.push(Do::Combine);
        }
    }
}
