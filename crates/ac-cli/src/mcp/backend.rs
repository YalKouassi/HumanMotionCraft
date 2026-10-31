//! Where MCP's tools run: a session of its own (headless, `ac mcp`) or the running app's control
//! port (`ac mcp --live <port>`, the app started with `--control <port>`).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use ac_core::Session;
use serde_json::{Value, json};

use crate::render::Pictures;

/// Seconds one request to the app may take unless `AC_CONTROL_TIMEOUT` says otherwise. The app's
/// control port reads the same variable, and every request carries the client's own `timeout`, so
/// the two always agree.
pub const DEFAULT_TIMEOUT_S: u64 = 120;
/// The longest timeout accepted.
pub const MAX_TIMEOUT_S: u64 = 3600;
/// How often a long request reports that it is still running.
pub const TICK: Duration = Duration::from_secs(1);

/// `AC_CONTROL_TIMEOUT` (seconds, 1..=3600), else [`DEFAULT_TIMEOUT_S`].
pub fn control_timeout() -> u64 {
    std::env::var("AC_CONTROL_TIMEOUT")
        .ok()
        .and_then(|t| t.trim().parse::<u64>().ok())
        .map_or(DEFAULT_TIMEOUT_S, |t| t.clamp(1, MAX_TIMEOUT_S))
}

/// Hears how many seconds a long request has run so far, about once a second.
pub type Tick<'a> = &'a mut dyn FnMut(f64);

/// An animation preview's pictures.
#[derive(Debug, Default)]
pub struct Animation {
    pub gif: Option<Vec<u8>>,
    /// The MP4 written, when one was asked for.
    pub mp4: Option<String>,
    pub frames: usize,
}

/// A refusal as structured JSON: `kind` and `message` always, and `command`, `hint`,
/// `suggestions` and the failed `step` when the session gave them (`CoreError::to_json`).
pub type Refusal = Value;

/// A failure that is not the session's (no app, a broken connection) as a [`Refusal`].
pub fn refusal(message: impl Into<String>) -> Refusal {
    json!({ "kind": "error", "message": message.into() })
}

/// A refusal's whole message.
pub fn message(r: &Refusal) -> String {
    match r["message"].as_str() {
        Some(m) => m.to_string(),
        None => r.to_string(),
    }
}

/// Where commands run.
pub trait Backend {
    /// Run a command; `tick` (for a request that may take long) hears how long it has run. A
    /// refusal comes back structured.
    fn exec(&mut self, id: &str, params: &Value, tick: Option<Tick>) -> Result<Value, Refusal>;
    /// [`Backend::exec`] with the refusal as its message.
    fn run(&mut self, id: &str, params: &Value) -> Result<Value, String> {
        self.exec(id, params, None).map_err(|r| message(&r))
    }
    /// PNG of the pose from a named view (`view`, `frame`, `width`, `height`).
    fn preview(&mut self, args: &Value) -> Result<Vec<u8>, String>;
    /// PNG of the app window.
    fn window(&mut self) -> Result<Vec<u8>, String>;
    /// A contact sheet PNG (`frames` or `count`, `view`, `width`, `height`, `cols`) and the frames
    /// on it.
    fn sheet(&mut self, args: &Value) -> Result<(Vec<u8>, Vec<f64>), String>;
    /// A request only the running app answers (`input`, `layout`, `ui`, `events`).
    fn window_op(&mut self, msg: Value) -> Result<Value, String>;
    /// The motion as pictures: `{times, view, width, height, fixed, delay, gif, mp4, fps}`
    /// (checked by `preview::plan`).
    fn animation(&mut self, plan: &Value, tick: Tick) -> Result<Animation, String>;
    /// The adapter pictures come from, once one is chosen.
    fn adapter(&self) -> Option<String>;
}

/// A contact sheet's cell size and columns from tool arguments (cells 320 x 400, one row).
pub fn sheet_cell(args: &Value, n: usize) -> (u32, u32, u32) {
    let w = args["width"].as_u64().unwrap_or(320).clamp(16, 2048) as u32;
    let h = args["height"].as_u64().unwrap_or(400).clamp(16, 2048) as u32;
    let cols = args["cols"].as_u64().unwrap_or(n as u64).clamp(1, 64) as u32;
    (w, h, cols)
}

/// Run `f` on a thread of its own and tick while it runs. A panic in `f` is an error, not a crash.
fn watched<T: Send>(tick: Tick, f: impl FnOnce() -> Result<T, String> + Send) -> Result<T, String> {
    std::thread::scope(|scope| {
        let (tx, rx) = channel();
        let worker = scope.spawn(move || {
            // the receiver is gone only when this function has already returned
            let _ = tx.send(f());
        });
        let start = Instant::now();
        loop {
            match rx.recv_timeout(TICK) {
                Ok(out) => {
                    let _ = worker.join();
                    return out;
                }
                Err(RecvTimeoutError::Timeout) => tick(start.elapsed().as_secs_f64()),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(match worker.join() {
                        Err(p) => format!(
                            "the request failed unexpectedly: {}",
                            p.downcast_ref::<&str>()
                                .map(|s| (*s).to_string())
                                .or_else(|| p.downcast_ref::<String>().cloned())
                                .unwrap_or_else(|| "unknown panic".into())
                        ),
                        Ok(()) => "the request stopped without an answer".into(),
                    });
                }
            }
        }
    })
}

/// A GIF and/or MP4 of RGBA frames, as `plan` asks.
pub fn encode(plan: &Value, frames: &[Vec<u8>], (w, h): (u32, u32)) -> Result<Animation, String> {
    let mut out = Animation {
        frames: frames.len(),
        ..Animation::default()
    };
    if plan["gif"].as_bool().unwrap_or(true) {
        let delay = plan["delay"].as_u64().unwrap_or(4).clamp(2, 6000) as u16;
        out.gif = Some(ac_render::anim::gif(w, h, frames, delay)?);
    }
    if let Some(path) = plan["mp4"].as_str() {
        let fps = plan["fps"].as_f64().unwrap_or(30.0);
        ac_render::anim::mp4(std::path::Path::new(path), w, h, fps, frames)?;
        out.mp4 = Some(path.to_string());
    }
    Ok(out)
}

/// A session of its own.
pub struct Headless {
    pub(super) session: Session,
    assets: PathBuf,
    pictures: Option<Pictures>,
}

impl Headless {
    pub fn new(session: Session, assets: PathBuf) -> Headless {
        Headless {
            session,
            assets,
            pictures: None,
        }
    }

    fn pictures(&mut self) -> Result<&mut Pictures, String> {
        if self.pictures.is_none() {
            self.pictures = Some(Pictures::new(
                &self.assets,
                &self.session.document.skeleton.names,
            )?);
        }
        self.pictures.as_mut().ok_or_else(|| "no renderer".into())
    }
}

impl Backend for Headless {
    fn exec(&mut self, id: &str, params: &Value, tick: Option<Tick>) -> Result<Value, Refusal> {
        let Some(tick) = tick else {
            return self.session.run(id, params).map_err(|e| e.to_json());
        };
        let session = &mut self.session;
        match watched(tick, move || Ok(session.run(id, params))) {
            Ok(out) => out.map_err(|e| e.to_json()),
            Err(e) => Err(refusal(e)),
        }
    }

    fn preview(&mut self, args: &Value) -> Result<Vec<u8>, String> {
        if let Some(f) = args["frame"].as_f64() {
            self.session
                .run("frame.set", &json!({ "frame": f }))
                .map_err(|e| e.to_string())?;
        }
        let w = args["width"].as_u64().unwrap_or(800).clamp(16, 4096) as u32;
        let h = args["height"].as_u64().unwrap_or(600).clamp(16, 4096) as u32;
        let view = args["view"].as_str().unwrap_or("three-quarter").to_string();
        let look = ac_media::look_of(args)?;
        self.pictures()?;
        let pictures = self.pictures.as_mut().ok_or("no renderer")?;
        pictures.set_look(look);
        pictures.png(&self.session.document, &view, w, h)
    }

    fn sheet(&mut self, args: &Value) -> Result<(Vec<u8>, Vec<f64>), String> {
        let frames = self.session.document.sheet_frames(args)?;
        let look = ac_media::look_of(args)?;
        self.pictures()?;
        let pictures = self.pictures.as_mut().ok_or("no renderer")?;
        pictures.set_look(look);
        let view = args["view"].as_str().unwrap_or("side");
        let png = crate::render::sheet(
            pictures,
            &self.session.document,
            view,
            sheet_cell(args, frames.len()),
            &frames,
        )?;
        Ok((png, frames))
    }

    fn window_op(&mut self, msg: Value) -> Result<Value, String> {
        Err(format!(
            "{} needs the running app: start it with --control <port> and run `ac mcp --live <port>`",
            msg["op"].as_str().unwrap_or("this")
        ))
    }

    fn window(&mut self) -> Result<Vec<u8>, String> {
        Err(
            "this server is headless: start the app with --control <port> and run `ac mcp --live <port>`"
                .into(),
        )
    }

    fn animation(&mut self, plan: &Value, tick: Tick) -> Result<Animation, String> {
        let times: Vec<f64> = plan["times"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect();
        let size = (
            plan["width"].as_u64().unwrap_or(320) as u32,
            plan["height"].as_u64().unwrap_or(240) as u32,
        );
        let view = plan["view"].as_str().unwrap_or("side").to_string();
        let fixed = plan["fixed"].as_bool().unwrap_or(true);
        let look = ac_media::look_of(plan)?;
        self.pictures()?;
        let pictures = self.pictures.as_mut().ok_or("no renderer")?;
        pictures.set_look(look);
        let doc = &self.session.document;
        watched(tick, move || {
            let frames = pictures.frames(doc, &times, &view, size, fixed)?;
            encode(plan, &frames, size)
        })
    }

    fn adapter(&self) -> Option<String> {
        self.pictures.as_ref().map(|p| p.adapter().to_string())
    }
}

/// The running app's control port: one JSON request a line, one reply a line
/// (`{op: run, command, params}` / `{op: render, view, frame, width, height}` / `{op: screenshot}`
/// -> `{ok: value}` or `{error: text}`, with `detail` the structured refusal when the app sends it;
/// pictures come back as `{ok: {png: base64}}`). Every
/// request carries an `id` (echoed in its reply, so a late reply to an earlier request is never
/// taken for this one's) and the client's `timeout`; with `progress: true` the app sends
/// `{id, progress}` lines while it works.
/// Connects on first use and again after the app restarts, so the server can start before the app.
pub struct Live {
    port: u16,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    timeout: u64,
    next_id: u64,
}

impl Live {
    /// A server for the app on `port`; nothing is opened until a tool needs the app.
    pub fn new(port: u16) -> Live {
        Live {
            port,
            conn: None,
            timeout: control_timeout(),
            next_id: 0,
        }
    }

    /// Connect now (an error when nothing listens on the port).
    pub fn connect(port: u16) -> Result<Live, String> {
        let mut live = Live::new(port);
        live.open()?;
        Ok(live)
    }

    /// Seconds each request may take (1..=3600); the app is told with every request.
    pub fn with_timeout(mut self, seconds: u64) -> Live {
        self.timeout = seconds.clamp(1, MAX_TIMEOUT_S);
        self
    }

    pub fn timeout(&self) -> u64 {
        self.timeout
    }

    fn open(&mut self) -> Result<(), String> {
        let port = self.port;
        let s = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| format!("no app on port {port} (start it with --control {port}): {e}"))?;
        // the app answers or says it is still working well within this
        s.set_read_timeout(Some(Duration::from_secs(self.timeout + 10)))
            .map_err(|e| format!("control port {port}: {e}"))?;
        self.conn = Some((BufReader::new(s.try_clone().map_err(|e| e.to_string())?), s));
        Ok(())
    }

    /// One request and its reply on the open connection. `Err(None)`: the connection is dead
    /// before the app heard anything (the app that held it closed), and a new one can carry it
    /// again.
    fn exchange(&mut self, msg: &Value, tick: &mut dyn FnMut(f64)) -> Result<Value, Option<String>> {
        let id = msg["id"].clone();
        let timeout = self.timeout;
        let Some((reader, writer)) = self.conn.as_mut() else {
            return Err(None);
        };
        if writeln!(writer, "{msg}").and_then(|()| writer.flush()).is_err() {
            return Err(None);
        }
        let start = Instant::now();
        let mut heard = false;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) if heard => {
                    return Err(Some("the app closed the connection while working on it".into()));
                }
                Ok(0) => return Err(None),
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Err(Some(format!(
                        "the app did not answer in {timeout} s (AC_CONTROL_TIMEOUT sets it): {e}"
                    )));
                }
                Err(_) if heard => {
                    return Err(Some("the connection to the app broke while it worked".into()));
                }
                Err(_) => return Err(None),
            }
            let v: Value =
                serde_json::from_str(&line).map_err(|e| Some(format!("bad reply from the app: {e}")))?;
            // a late reply to an earlier request that timed out
            if v.get("id").is_some_and(|got| *got != id) {
                continue;
            }
            heard = true;
            if v.get("progress").is_some() && v.get("ok").is_none() && v.get("error").is_none() {
                tick(start.elapsed().as_secs_f64());
                if start.elapsed() > Duration::from_secs(timeout + 5) {
                    return Err(Some(format!("the app did not finish in {timeout} s")));
                }
                continue;
            }
            return Ok(v);
        }
    }

    fn send(&mut self, msg: &Value, tick: Option<Tick>) -> Result<Value, String> {
        self.send_detailed(msg, tick).map_err(|r| message(&r))
    }

    /// One request; a refusal is the app's `detail` (`CoreError::to_json`) when it sent one.
    fn send_detailed(&mut self, msg: &Value, tick: Option<Tick>) -> Result<Value, Refusal> {
        let mut msg = match msg {
            Value::Object(_) => msg.clone(),
            other => return Err(refusal(format!("a control request is an object, not {other}"))),
        };
        self.next_id += 1;
        msg["id"] = json!(self.next_id);
        msg["timeout"] = json!(self.timeout);
        if tick.is_some() {
            msg["progress"] = json!(true);
        }
        if self.conn.is_none() {
            self.open().map_err(refusal)?;
        }
        let mut quiet = |_: f64| {};
        let tick: &mut dyn FnMut(f64) = match tick {
            Some(t) => t,
            None => &mut quiet,
        };
        let v = match self.exchange(&msg, &mut *tick) {
            Ok(v) => v,
            Err(None) => {
                // the app closed or restarted since the last request: reconnect and send it again
                self.open().map_err(refusal)?;
                self.exchange(&msg, tick).map_err(|e| {
                    self.conn = None;
                    refusal(e.unwrap_or_else(|| "the app closed the connection".into()))
                })?
            }
            Err(Some(e)) => {
                self.conn = None;
                return Err(refusal(e));
            }
        };
        match (v.get("error"), v.get("detail")) {
            (Some(_), Some(d)) if d["message"].is_string() => Err(d.clone()),
            (Some(e), _) => Err(refusal(e.as_str().unwrap_or("error"))),
            (None, _) => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
        }
    }

    pub fn request(&mut self, msg: &Value) -> Result<Value, String> {
        self.send(msg, None)
    }

    /// A request that may take long: the app reports while it works and `tick` hears it.
    pub fn request_watched(&mut self, msg: &Value, tick: Tick) -> Result<Value, String> {
        self.send(msg, Some(tick))
    }

    fn picture(&mut self, msg: &Value) -> Result<Vec<u8>, String> {
        let v = self.request(msg)?;
        crate::render::unbase64(v["png"].as_str().ok_or("the app sent no picture")?)
    }
}

impl Backend for Live {
    fn exec(&mut self, id: &str, params: &Value, tick: Option<Tick>) -> Result<Value, Refusal> {
        self.send_detailed(&json!({ "op": "run", "command": id, "params": params }), tick)
    }

    fn preview(&mut self, args: &Value) -> Result<Vec<u8>, String> {
        let mut m = args.clone();
        m["op"] = json!("render");
        self.picture(&m)
    }

    fn window(&mut self) -> Result<Vec<u8>, String> {
        self.picture(&json!({ "op": "screenshot" }))
    }

    fn window_op(&mut self, msg: Value) -> Result<Value, String> {
        self.request(&msg)
    }

    fn sheet(&mut self, args: &Value) -> Result<(Vec<u8>, Vec<f64>), String> {
        let mut m = args.clone();
        m["op"] = json!("sheet");
        let v = self.request(&m)?;
        let png = crate::render::unbase64(v["png"].as_str().ok_or("the app sent no picture")?)?;
        let frames = v["frames"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect();
        Ok((png, frames))
    }

    fn animation(&mut self, plan: &Value, tick: Tick) -> Result<Animation, String> {
        let mut m = plan.clone();
        m["op"] = json!("animation");
        let v = self.request_watched(&m, tick)?;
        Ok(Animation {
            gif: match v["gif"].as_str() {
                Some(b) => Some(crate::render::unbase64(b)?),
                None => None,
            },
            mp4: v["mp4"].as_str().map(str::to_string),
            frames: v["frames"].as_u64().unwrap_or(0) as usize,
        })
    }

    fn adapter(&self) -> Option<String> {
        None
    }
}
