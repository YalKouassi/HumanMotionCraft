//! MCP over stdio (JSON-RPC 2.0, one message per line). Every tool reaches the same commands the
//! app and the CLI run; nothing here has command logic of its own:
//!
//! | tool | does |
//! |---|---|
//! | `command_list` | find commands (`query` ranks ids, labels and docs; `area`; `schema`) |
//! | `describe_command` | one command's input/output JSON Schema and an example call |
//! | `command_run` / `command_batch` | run one / several commands |
//! | `editor_state` | the open animation, keys, layers, playhead |
//! | `render_preview` / `contact_sheet` | PNGs of the pose / several frames from one camera |
//! | `animation_preview` | the clip as an animated GIF (and MP4) |
//! | `expect` | fixture-style checks now; `save_as` writes them and the steps as a fixture |
//! | `session_reset` | start again |
//! | `ui_events` | what happened since a cursor |
//! | `ui_layout` / `ui_input` / `ui_click` / `ui_set` / `ui_screenshot` | the app window (live only) |
//!
//! Also resources (`ac://state`, `ac://skeleton`, `ac://library`, `ac://commands/{id}`) and prompts
//! (ready-made workflows). Long calls send `notifications/progress` when the call carries a
//! `progressToken`. Two backends: a session of its own (headless, `ac mcp`), or the running app
//! (`ac mcp --live <port>`, the app started with `--control <port>`): then every command edits what
//! the person is looking at and shares their undo history.

mod backend;
mod events;
mod expect;
mod preview;
mod prompts;
mod resources;
mod schema;
mod tools;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ac_core::{CommandSpec, Session, Skeleton};
use serde_json::{Value, json};

pub use backend::{
    Animation, Backend, DEFAULT_TIMEOUT_S, Headless, Live, Refusal, control_timeout, refusal, sheet_cell,
};
pub use expect::is_query;
pub use schema::{input_schema, kind_schema, output_schema};

const PROTOCOL: &str = "2025-06-18";
const INSTRUCTIONS: &str = concat!(
    ac_core::product_name!(),
    " animation editor (headless). Every edit is a command: command_list {query} finds ids, describe_command {id} gives a command's JSON Schema and an example, command_run {id, params} runs one (undoable ones go on the undo stack: edit.undo / edit.redo), command_batch runs several. Rig space: metres, Z up, the body faces -Y; root offsets are in hip heights (resource ac://skeleton). Look before and after an edit with render_preview (views: front, back, left, right, side, top, three-quarter), contact_sheet and animation_preview; check numbers with expect (save_as keeps them as a fixture). prompts/list has ready-made workflows."
);
const LIVE_INSTRUCTIONS: &str = concat!(
    ac_core::product_name!(),
    ", the running app (the window the person sees). Every edit is a command: command_list {query} finds ids, describe_command {id} gives a command's JSON Schema and an example, command_run {id, params} runs one in the app (it shares the person's undo history: edit.undo / edit.redo), command_batch runs several. Rig space: metres, Z up, the body faces -Y; root offsets are in hip heights (resource ac://skeleton). Look before and after an edit: render_preview draws the pose from a named view (front, back, left, right, side, top, three-quarter), animation_preview plays it, ui_screenshot shows the whole window, ui_events says what the person did. prompts/list has ready-made workflows."
);

/// Steps kept for `expect`'s `save_as`.
const MAX_STEPS: usize = 10_000;

pub struct Server {
    backend: Box<dyn Backend>,
    /// The command registry (the same code in every front end).
    specs: Vec<&'static CommandSpec>,
    skeleton: Arc<Skeleton>,
    limits: Option<Arc<ac_core::fullbody::Limits>>,
    assets: PathBuf,
    live: bool,
    /// The commands this server ran that changed something, as fixture steps.
    steps: Vec<Value>,
    /// Headless: what this server's tools did (live, the app keeps its own).
    events: events::Log,
    fixtures: Option<Vec<(String, Value)>>,
    model: Option<Value>,
    previews: u32,
    /// Where notifications go (stdout while serving).
    notify: Box<dyn FnMut(&Value)>,
}

/// What the server reads from a session up front: the commands, the rig and its limits.
type Registry = (
    Vec<&'static CommandSpec>,
    Arc<Skeleton>,
    Option<Arc<ac_core::fullbody::Limits>>,
);

fn registry(s: &Session) -> Registry {
    (
        s.commands().to_vec(),
        s.document.skeleton.clone(),
        s.document.limits.clone(),
    )
}

/// `command_list`'s entries for a session's registry.
pub fn describe(s: &Session) -> Vec<Value> {
    s.commands().iter().map(|c| schema::entry(c, false)).collect()
}

pub(crate) fn text(v: &Value, is_error: bool) -> Value {
    let t = serde_json::to_string_pretty(v).unwrap_or_default();
    json!({ "content": [{ "type": "text", "text": t }], "isError": is_error })
}

pub(crate) fn fail(message: impl Into<String>) -> Value {
    text(&json!({ "error": message.into() }), true)
}

/// A refused command as a tool error: the text carries `error` (the whole message) beside the
/// refusal's fields (kind, command, hint, suggestions, step), and `structuredContent` the refusal
/// itself, so an agent reads the fix without parsing prose.
pub(crate) fn refused(r: &Refusal) -> Value {
    let mut body = json!({ "error": backend::message(r) });
    if let (Some(b), Some(fields)) = (body.as_object_mut(), r.as_object()) {
        for (k, v) in fields {
            if k != "message" {
                b.insert(k.clone(), v.clone());
            }
        }
    }
    let mut out = text(&body, true);
    out["structuredContent"] = r.clone();
    out
}

fn image(png: &[u8], caption: String) -> Value {
    json!({ "content": [
        { "type": "image", "data": crate::render::base64(png), "mimeType": "image/png" },
        { "type": "text", "text": caption },
    ]})
}

/// `{tmp}` and `{assets}` in a fixture's texts, made real for an example call.
fn fill(v: &Value, tmp: &str, assets: &str) -> Value {
    match v {
        Value::String(s) => Value::String(s.replace("{tmp}", tmp).replace("{assets}", assets)),
        Value::Array(a) => Value::Array(a.iter().map(|x| fill(x, tmp, assets)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), fill(x, tmp, assets))).collect()),
        other => other.clone(),
    }
}

/// FNV-1a over the default model's files (names and bytes, sorted by name): a fingerprint that
/// changes when the weights do.
fn model_fingerprint(assets: &Path) -> Value {
    let models = assets.join("models");
    let manifest: Value = match std::fs::read_to_string(models.join("MANIFEST.json")) {
        Ok(t) => serde_json::from_str(&t).unwrap_or(Value::Null),
        Err(e) => return json!({ "error": format!("no model manifest: {e}") }),
    };
    let list = manifest["models"].as_array().map_or(&[][..], Vec::as_slice);
    let Some(m) = list
        .iter()
        .find(|m| m["default"] == true)
        .or_else(|| list.first())
    else {
        return json!({ "error": "the model manifest lists no model" });
    };
    let dir = models.join(m["path"].as_str().unwrap_or_default());
    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(d) => d.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect(),
        Err(e) => return json!({ "tag": m["tag"], "error": format!("{}: {e}", dir.display()) }),
    };
    files.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for f in &files {
        eat(f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
            .as_bytes());
        match std::fs::read(f) {
            Ok(b) => eat(&b),
            Err(e) => return json!({ "tag": m["tag"], "error": format!("{}: {e}", f.display()) }),
        }
    }
    json!({ "tag": m["tag"], "hash": format!("fnv1a64:{h:016x}"), "files": files.len() })
}

impl Server {
    fn with(backend: Box<dyn Backend>, registry: Registry, assets: PathBuf, live: bool) -> Server {
        Server {
            backend,
            specs: registry.0,
            skeleton: registry.1,
            limits: registry.2,
            assets,
            live,
            steps: Vec::new(),
            events: events::Log::default(),
            fixtures: None,
            model: None,
            previews: 0,
            notify: Box::new(|_| {}),
        }
    }

    /// A headless server on its own session.
    pub fn new(session: Session, assets: PathBuf) -> Server {
        let registry = registry(&session);
        Server::with(
            Box::new(backend::Headless::new(session, assets.clone())),
            registry,
            assets,
            false,
        )
    }

    /// A server driving the running app; `registry` only lists the commands (and the rig).
    pub fn live(backend: Live, registry: &Session) -> Server {
        let assets = registry.document.assets.clone().unwrap_or_default();
        Server::with(Box::new(backend), self::registry(registry), assets, true)
    }

    /// Where notifications (progress) go; `serve` sends them to stdout.
    pub fn set_notifier(&mut self, f: Box<dyn FnMut(&Value)>) {
        self.notify = f;
    }

    fn spec(&self, id: &str) -> Option<&'static CommandSpec> {
        self.specs.iter().copied().find(|c| c.id == id)
    }

    /// Run a command for a tool: with progress when the call asked for it, logged as a step (if
    /// it can change anything) and, headless, as events. A refusal comes back structured.
    fn run(&mut self, id: &str, params: &Value, token: Option<&Value>) -> Result<Value, Refusal> {
        let out = match token {
            Some(t) => {
                let notify = &mut self.notify;
                let mut tick = |secs: f64| {
                    notify(&json!({ "jsonrpc": "2.0", "method": "notifications/progress",
                        "params": { "progressToken": t, "progress": secs.floor(), "message": format!("{id}: {secs:.0} s") } }));
                };
                self.backend.exec(id, params, Some(&mut tick))
            }
            None => self.backend.exec(id, params, None),
        };
        let spec = self.spec(id);
        if !self.live {
            self.events.command(id, spec.is_some_and(|c| c.undoable), &out);
        }
        if out.is_ok() && spec.is_some_and(|c| !is_query(c)) && self.steps.len() < MAX_STEPS {
            self.steps.push(json!({ "command": id, "params": params }));
        }
        out
    }

    fn fixtures(&mut self) -> &[(String, Value)] {
        if self.fixtures.is_none() {
            let dir = self.assets.join("../fixtures");
            let loaded = crate::load_fixtures(&dir).unwrap_or_default();
            self.fixtures = Some(
                loaded
                    .into_iter()
                    .map(|(_, f)| (f["name"].as_str().unwrap_or_default().to_string(), f))
                    .collect(),
            );
        }
        self.fixtures.as_deref().unwrap_or_default()
    }

    /// `describe_command` / `ac://commands/{id}`.
    fn describe_command(&mut self, id: &str) -> Result<Value, String> {
        let Some(c) = self.spec(id) else {
            let near = schema::suggest(&self.specs, id);
            return Err(if near.is_empty() {
                format!("unknown command {id:?} (command_list lists them)")
            } else {
                format!("unknown command {id:?}; did you mean {}?", near.join(", "))
            });
        };
        let tmp = std::env::temp_dir()
            .join(format!("{}-mcp-scratch", ac_core::product::ID))
            .to_string_lossy()
            .replace('\\', "/");
        let assets = self.assets.to_string_lossy().replace('\\', "/");
        let names = self.skeleton.names.clone();
        let fixtures = self.fixtures();
        let example = schema::example(c, fixtures, &names, &|v| fill(v, &tmp, &assets));
        Ok(schema::describe(c, &names, example))
    }

    /// What makes this server's results repeatable, and what can vary.
    fn determinism(&mut self) -> Value {
        if self.model.is_none() {
            self.model = Some(model_fingerprint(&self.assets));
        }
        // the session's own account: rig, limits and model hashes, the command count, whether
        // results are checked against their outputs (live: the app's session)
        let session = match self.backend.exec("session.info", &json!({}), None) {
            Ok(v) => v,
            Err(r) => json!({ "error": backend::message(&r) }),
        };
        json!({
            "session": session,
            "version": env!("CARGO_PKG_VERSION"),
            "backend": if self.live { "live" } else { "headless" },
            "core": "f64 maths, no clocks, no unseeded randomness: the same commands from the same start give byte-identical documents (session_reset starts again)",
            "ai_model": self.model,
            "ai_model_note": if self.live { "the fingerprint of the model in this server's assets; the app loads its own (ai.info names it)" } else { "the model this session loaded" },
            "render_adapter": self.backend.adapter(),
            "render_note": "pictures depend a little on the GPU adapter (null: not chosen until the first picture); the visual goldens use the software adapter",
        })
    }

    fn call(&mut self, name: &str, args: &Value, token: Option<&Value>) -> Value {
        let empty = json!({});
        match name {
            "command_list" => {
                let area = args["area"].as_str().unwrap_or("");
                let schema = args["schema"].as_bool().unwrap_or(false);
                let specs: Vec<&'static CommandSpec> = self
                    .specs
                    .iter()
                    .copied()
                    .filter(|c| c.id.starts_with(area))
                    .collect();
                let mut list = match args["query"].as_str().filter(|q| !q.trim().is_empty()) {
                    Some(q) => schema::search(&specs, q, schema),
                    None => specs.iter().map(|c| schema::entry(c, schema)).collect(),
                };
                if let Some(n) = args["limit"].as_u64() {
                    list.truncate(n.clamp(1, 500) as usize);
                }
                text(&json!(list), false)
            }
            "describe_command" => match self.describe_command(args["id"].as_str().unwrap_or_default()) {
                Ok(v) => text(&v, false),
                Err(e) => fail(e),
            },
            "command_run" => {
                let id = args["id"].as_str().unwrap_or_default();
                let params = args.get("params").unwrap_or(&empty);
                let out = if args["dry_run"] == true {
                    // the session tries it on a copy: result, change summary and undo label
                    self.run("edit.dry_run", &json!({ "id": id, "params": params }), token)
                } else {
                    self.run(id, params, token)
                };
                match out {
                    Ok(v) => text(&v, false),
                    Err(r) => refused(&r),
                }
            }
            "command_batch" => self.command_batch(args, token),
            "editor_state" => match self.backend.run("editor.state", &empty) {
                Ok(v) => text(&v, false),
                Err(e) => fail(e),
            },
            "render_preview" => match self.backend.preview(args) {
                Ok(png) => image(
                    &png,
                    format!("{} view", args["view"].as_str().unwrap_or("three-quarter")),
                ),
                Err(e) => fail(e),
            },
            "contact_sheet" => match self.backend.sheet(args) {
                Ok((png, frames)) => image(&png, format!("frames {frames:?}, left to right, top to bottom")),
                Err(e) => fail(e),
            },
            "animation_preview" => self.animation_preview(args, token),
            "expect" => self.expect(args),
            "session_reset" => match self.backend.exec("session.reset", &empty, None) {
                // headless or live, the session's own command: a fresh document, no undo history,
                // the same library
                Ok(v) => {
                    self.steps.clear();
                    self.events.push("reset", json!({}));
                    text(&json!({ "result": v, "determinism": self.determinism() }), false)
                }
                Err(r) => refused(&r),
            },
            "ui_events" => {
                let since = args["since"].as_u64().unwrap_or(0);
                let limit = args["limit"]
                    .as_u64()
                    .unwrap_or(100)
                    .clamp(1, events::MAX_READ as u64);
                if self.live {
                    match self
                        .backend
                        .window_op(json!({ "op": "events", "since": since, "limit": limit }))
                    {
                        Ok(v) => text(&v, false),
                        Err(e) => fail(e),
                    }
                } else {
                    let mut v = self.events.since(since, limit as usize);
                    v["note"] =
                        json!("headless: the commands this server ran (no window, so no selection events)");
                    text(&v, false)
                }
            }
            "ui_layout" | "ui_input" | "ui_click" | "ui_set" => {
                let msg = match name {
                    "ui_layout" => json!({ "op": "layout" }),
                    "ui_input" => json!({ "op": "input", "events": args["events"] }),
                    "ui_click" => {
                        let mut m = json!({ "op": "click", "widget": args["id"] });
                        for k in ["button", "modifiers"] {
                            if let Some(v) = args.get(k) {
                                m[k] = v.clone();
                            }
                        }
                        m
                    }
                    _ => json!({ "op": "ui", "name": args["name"], "value": args["value"] }),
                };
                match self.backend.window_op(msg) {
                    Ok(v) => text(&v, false),
                    Err(e) => fail(e),
                }
            }
            "ui_screenshot" => match self.backend.window() {
                Ok(png) => image(&png, "the app window".into()),
                Err(e) => fail(e),
            },
            other => fail(format!("unknown tool {other:?}")),
        }
    }

    /// `command_batch`: the steps run as one `edit.batch` (one undo step, named by `label`).
    /// Atomic (the default): a refusal puts everything back and the error names the failed step;
    /// `atomic: false` keeps the steps before it and says which failed.
    fn command_batch(&mut self, args: &Value, token: Option<&Value>) -> Value {
        let Some(list) = args["commands"].as_array() else {
            return fail("`commands` must be a list of {id, params}");
        };
        // MCP callers write `id`; fixtures write `command`: both reach edit.batch as written
        let mut params = json!({ "commands": list });
        for key in ["atomic", "label"] {
            if let Some(v) = args.get(key).filter(|v| !v.is_null()) {
                params[key] = v.clone();
            }
        }
        match self.run("edit.batch", &params, token) {
            Ok(v) => {
                let partial = v.get("failed").is_some_and(|f| !f.is_null());
                text(&v, partial)
            }
            Err(r) => refused(&r),
        }
    }

    fn animation_preview(&mut self, args: &Value, token: Option<&Value>) -> Value {
        let state = match self.backend.run("editor.state", &json!({})) {
            Ok(v) => v,
            Err(e) => return fail(e),
        };
        let (plan, notes) = match preview::plan(args, &state) {
            Ok(p) => p,
            Err(e) => return fail(e),
        };
        let notify = &mut self.notify;
        let mut tick = |secs: f64| {
            if let Some(t) = token {
                notify(&json!({ "jsonrpc": "2.0", "method": "notifications/progress",
                    "params": { "progressToken": t, "progress": secs.floor(), "message": format!("rendering: {secs:.0} s") } }));
            }
        };
        let anim = match self.backend.animation(&plan, &mut tick) {
            Ok(a) => a,
            Err(e) => return fail(e),
        };
        self.previews += 1;
        preview::result(args, &plan, notes, anim, self.previews).unwrap_or_else(fail)
    }

    /// Answer one JSON-RPC message (`None` for notifications).
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned()?;
        let method = msg["method"].as_str().unwrap_or_default();
        let params = &msg["params"];
        let error = |code: i64, message: String| {
            Some(json!({ "jsonrpc": "2.0", "id": id.clone(), "error": { "code": code, "message": message } }))
        };
        let result = match method {
            "initialize" => json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
                "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
                "serverInfo": { "name": ac_core::product::ID, "version": env!("CARGO_PKG_VERSION") },
                "instructions": if self.live { LIVE_INSTRUCTIONS } else { INSTRUCTIONS },
                "determinism": self.determinism(),
            }),
            "ping" => json!({}),
            "tools/list" => json!({ "tools": tools::list() }),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or_default().to_string();
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let token = params["_meta"].get("progressToken").cloned();
                self.call(&name, &args, token.as_ref())
            }
            "resources/list" => self.resource_list(),
            "resources/templates/list" => self.resource_templates(),
            "resources/read" => match self.resource_read(params["uri"].as_str().unwrap_or_default()) {
                Ok(v) => v,
                Err(e) => return error(-32002, e),
            },
            "prompts/list" => prompts::list(),
            "prompts/get" => {
                match prompts::get(params["name"].as_str().unwrap_or_default(), &params["arguments"]) {
                    Ok(v) => v,
                    Err(e) => return error(-32602, e),
                }
            }
            other => return error(-32601, format!("unknown method {other:?}")),
        };
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    /// Serve stdin/stdout until stdin closes.
    pub fn serve(&mut self) -> Result<(), String> {
        self.notify = Box::new(|v| {
            let mut out = std::io::stdout().lock();
            // a broken stdout also fails the reply that follows, which ends the server
            let _ = writeln!(out, "{v}").and_then(|()| out.flush());
        });
        let stdin = std::io::stdin();
        let mut out = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => self.handle(&msg),
                Err(e) => Some(
                    json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } }),
                ),
            };
            if let Some(r) = reply {
                writeln!(out, "{r}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

/// `--timeout <s>`: seconds one request to the app may take, 1 to 3600.
pub fn parse_timeout(text: &str) -> Result<u64, String> {
    match text.trim().parse::<u64>() {
        Ok(s) if (1..=backend::MAX_TIMEOUT_S).contains(&s) => Ok(s),
        _ => Err(format!(
            "--timeout {text:?}: seconds from 1 to {}",
            backend::MAX_TIMEOUT_S
        )),
    }
}

/// `ac mcp --live <port> [--timeout <s>]`: MCP on stdio, every tool forwarded to the running app.
/// Starts whether or not the app is up yet; each tool call reaches it when it is. `timeout`: the
/// seconds one request may take (default `AC_CONTROL_TIMEOUT`, else 120); every request tells the
/// app, so both sides wait as long.
pub fn serve_live(assets: &std::path::Path, port: u16, timeout: Option<u64>) -> Result<(), String> {
    let mut live = Live::new(port);
    if let Some(t) = timeout {
        live = live.with_timeout(t);
    }
    let registry = crate::session(
        assets,
        &std::env::temp_dir().join(format!("{}-live-registry", ac_core::product::ID)),
    )?;
    Server::live(live, &registry).serve()
}
