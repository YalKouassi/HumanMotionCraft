//! The Community tab of the library panel: clips other people shared, to watch, install and rate,
//! and your own uploads; "Share to community…" on your own animations (`share.rs`).
//!
//! Every `community.*` command may wait on the network, so [`App::run`] hands it to a thread here
//! ([`App::community_deferred`]), which makes its result ([`ac_community::prepare`]); each frame
//! [`App::community_poll`] runs the commands whose results are ready, which return them at once.
//! A share or report the server refuses for want of the captcha opens the captcha page in the
//! browser (`captcha.rs`); once it hands back its token the command runs again.
//!
//! Rows play their preview strip while on screen (`strips.rs`): the frames the sharer's app drew,
//! the same kind of picture the library's popover plays. The pop-out button beside the tab opens
//! the community in a window of its own, with a gallery of large cards (`window.rs`).

mod captcha;
pub mod share;
mod strips;
mod tab;
pub mod window;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::time::Instant;

use serde_json::{Value, json};

use super::App;
use super::shell::Level;

/// Orders the tab offers: the command's word, and what the menu says.
pub const SORTS: &[(&str, &str)] = &[
    ("top", "Best rated"),
    ("new", "Newest"),
    ("downloads", "Most downloaded"),
];

/// A command running off the UI thread.
struct Job {
    id: String,
    params: Value,
    since: Instant,
    done: mpsc::Receiver<()>,
}

/// The tab's state.
pub struct State {
    pub search: String,
    pub sort: &'static str,
    /// Only clips with this tag.
    pub tag: Option<String>,
    /// The last list the server gave, and the query it was for.
    pub list: Vec<Value>,
    asked: Option<Value>,
    /// The list couldn't be fetched: why (the last list stays on screen).
    pub offline: Option<String>,
    /// My uploads, when that view is showing.
    pub mine: Option<Vec<Value>>,
    pub show_mine: bool,
    /// The selected clip (its popover shows).
    pub selected: Option<String>,
    pub report_reason: String,
    pub share: Option<share::Form>,
    pub strips: strips::Strips,
    /// In a window of its own, not the library's tab.
    pub floating: bool,
    /// How the window lays out the clips, and its cards' width (points).
    pub view: window::View,
    pub card: f32,
    jobs: Vec<Job>,
    captcha: Option<captcha::Wait>,
    /// The command the captcha was for, run again once it passes.
    after_captcha: Option<(String, Value)>,
    ctx: Option<egui::Context>,
}

impl Default for State {
    fn default() -> State {
        State {
            search: String::new(),
            sort: "top",
            tag: None,
            list: Vec::new(),
            asked: None,
            offline: None,
            mine: None,
            show_mine: false,
            selected: None,
            report_reason: String::new(),
            share: None,
            strips: strips::Strips::default(),
            floating: false,
            view: window::View::Gallery,
            card: window::CARD,
            jobs: Vec::new(),
            captcha: None,
            after_captcha: None,
            ctx: None,
        }
    }
}

impl State {
    /// Commands or strips still on their way (a screenshot waits for them).
    pub fn loading(&self) -> bool {
        !self.jobs.is_empty() || self.strips.loading()
    }

    /// A command of this name is still running.
    pub fn busy(&self, id: &str) -> bool {
        self.jobs.iter().any(|j| j.id == id)
    }

    /// The browse parameters the tab's search, tag and order ask for.
    fn query(&self) -> Value {
        let mut q = json!({ "sort": self.sort, "limit": 100 });
        if !self.search.trim().is_empty() {
            q["search"] = json!(self.search.trim());
        }
        if let Some(t) = &self.tag {
            q["tags"] = json!([t]);
        }
        q
    }
}

impl App {
    /// Whether `run` must make `id`'s result off the UI thread first; if so it is started.
    pub(in crate::app) fn community_deferred(&mut self, id: &str, params: &Value) -> bool {
        let Some(p) = params.as_object() else { return false };
        if !ac_community::waits(id) || ac_community::is_ready(id, p) {
            return false;
        }
        let st = &mut self.community;
        if st.jobs.iter().any(|j| j.id == id && j.params == *params) {
            return true;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ac_community::Ctx::of(&self.session.document);
        let (cmd, p) = (id.to_string(), p.clone());
        let wake = st.ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("community".into())
            .spawn(move || {
                if catch_unwind(AssertUnwindSafe(|| ac_community::prepare(&ctx, &cmd, &p))).is_err() {
                    ac_community::give_up(&cmd, &p, "the community command failed unexpectedly");
                }
                let _ = tx.send(());
                if let Some(c) = wake {
                    c.request_repaint();
                }
            });
        match spawned {
            Ok(_) => st.jobs.push(Job {
                id: id.to_string(),
                params: params.clone(),
                since: Instant::now(),
                done: rx,
            }),
            Err(e) => {
                self.status = format!("Community: can't start its thread: {e}");
                return false;
            }
        }
        true
    }

    /// Finish the commands whose results are made, and the captcha (every frame).
    pub(in crate::app) fn community_poll(&mut self, ctx: &egui::Context) {
        if self.community.ctx.is_none() {
            self.community.ctx = Some(ctx.clone());
        }
        let mut done = Vec::new();
        self.community.jobs.retain(|j| match j.done.try_recv() {
            Err(mpsc::TryRecvError::Empty) => true,
            _ => {
                done.push((j.id.clone(), j.params.clone()));
                false
            }
        });
        if !self.community.jobs.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        for (id, params) in done {
            let p = params.as_object().cloned().unwrap_or_default();
            if ac_community::take_captcha(&id, &p) {
                self.community_captcha(&id, params);
                continue;
            }
            let out = self.run(&id, params.clone());
            self.community_done(&id, &params, out);
        }
        self.captcha_poll(ctx);
        self.strips_poll(ctx);
    }

    /// What a finished command changes in the tab.
    fn community_done(&mut self, id: &str, params: &Value, out: Option<Value>) {
        let st = &mut self.community;
        match (id, out) {
            ("community.browse", Some(v)) => {
                st.list = v["clips"].as_array().cloned().unwrap_or_default();
                st.offline = None;
            }
            ("community.browse", None) => {
                st.offline = Some(format!(
                    "Can't reach the community{}",
                    if st.list.is_empty() {
                        ""
                    } else {
                        ": showing the last list"
                    }
                ));
            }
            ("community.install", Some(v)) => {
                let at = v["id"].as_str().unwrap_or_default().to_string();
                self.status = if v["already"].as_bool() == Some(true) {
                    format!("Already in your library as {at}")
                } else {
                    format!("Installed as {at}")
                };
                for n in v["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    self.shell.messages.push(Level::Warning, n.to_string());
                }
                self.refresh_library();
                self.community_browse();
            }
            ("community.rate", Some(v)) => {
                let cid = v["id"].as_str().unwrap_or_default();
                for c in st.list.iter_mut().filter(|c| c["id"].as_str() == Some(cid)) {
                    c["rating"] = v["rating"].clone();
                    c["ratings"] = v["ratings"].clone();
                }
                self.status = format!("Rated {} stars, thank you", params["stars"]);
            }
            ("community.report", Some(_)) => {
                st.report_reason.clear();
                self.status =
                    "Reported: thank you. Reports from several people hide a clip until someone looks".into();
            }
            ("community.verify", Some(_)) => {
                if let Some((again, p)) = st.after_captcha.take() {
                    self.run(&again, p);
                }
            }
            ("community.share", Some(v)) => {
                st.share = None;
                self.status = format!(
                    "Shared “{}”: it goes live once the checks pass (see My uploads)",
                    v["title"].as_str().unwrap_or_default()
                );
                self.community_mine();
            }
            ("community.share", None) => {
                if let Some(f) = &mut st.share {
                    f.sending = false;
                }
            }
            ("community.mine", Some(v)) => {
                st.mine = Some(v["clips"].as_array().cloned().unwrap_or_default());
            }
            ("community.remove", Some(_)) => {
                self.status = "Removed from the community".into();
                self.community_mine();
            }
            _ => {}
        }
    }

    /// Fetch the list for the tab's search, tag and order (unless that list is on screen).
    pub(in crate::app) fn community_browse(&mut self) {
        let q = self.community.query();
        self.community.asked = Some(q.clone());
        self.run("community.browse", q);
    }

    pub(in crate::app) fn community_mine(&mut self) {
        self.run("community.mine", json!({}));
    }

    /// The server wants the captcha before `id`: open its page, and run `id` again after.
    fn community_captcha(&mut self, id: &str, params: Value) {
        if self.community.captcha.is_some() {
            return;
        }
        let page = self
            .session
            .run("community.account", &json!({}))
            .ok()
            .and_then(|v| v["captcha"].as_str().map(str::to_string))
            .unwrap_or_default();
        match captcha::Wait::start(&page) {
            Ok(w) => {
                self.community.captcha = Some(w);
                self.community.after_captcha = Some((id.to_string(), params));
                self.status = "One quick check: tick the box in your browser (you only do this once)".into();
            }
            Err(e) => {
                let text = format!("Community: can't open the captcha page: {e}");
                self.shell.messages.push(Level::Error, text.clone());
                self.status = text;
            }
        }
    }

    fn captcha_poll(&mut self, ctx: &egui::Context) {
        let Some(w) = &self.community.captcha else { return };
        match w.poll() {
            None => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
            Some(Ok(token)) => {
                self.community.captcha = None;
                self.run("community.verify", json!({ "token": token }));
            }
            Some(Err(e)) => {
                self.community.captcha = None;
                self.community.after_captcha = None;
                if let Some(f) = &mut self.community.share {
                    f.sending = false;
                }
                self.status = format!("Community: {e}");
            }
        }
    }

    /// What the tab says about work in flight (empty: none).
    pub(in crate::app) fn community_working(&self) -> String {
        if self.community.captcha.is_some() {
            return "Waiting for the check in your browser…".into();
        }
        match self.community.jobs.iter().max_by_key(|j| j.since.elapsed()) {
            Some(j) if j.since.elapsed().as_millis() > 400 => match j.id.as_str() {
                "community.browse" => "Loading…".into(),
                "community.install" => "Installing…".into(),
                "community.share" => "Sharing…".into(),
                _ => "Working…".into(),
            },
            _ => String::new(),
        }
    }
}
