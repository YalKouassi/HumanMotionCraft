//! The control port (`--control <port>`): lets an agent drive the window the person sees.
//!
//! One JSON request a line on `127.0.0.1:<port>`, one reply a line:
//!
//! | request | reply `ok` |
//! |---|---|
//! | `{op: "run", command, params}` | the command's result (it runs in the app's session: same undo) |
//! | `{op: "render", view, frame?, width?, height?}` | `{png}`: the pose from a named view (base64) |
//! | `{op: "screenshot"}` | `{png}`: the whole window (base64) |
//! | `{op: "sheet", frames? or count?, view?, width?, height?, cols?}` | `{png, frames}`: a contact sheet of the animation |
//! | `{op: "animation", times, view?, width?, height?, fixed?, delay?, gif?, mp4?, fps?}` | `{gif?, mp4?, frames}`: the motion as an animated GIF (base64) and/or an MP4 written to `mp4` |
//! | `{op: "ui", name, value}` | `{}`: set the window's own state, as `--ui name=value` (main.rs) |
//! | `{op: "input", events: [{kind, x, y, button?, dx?, dy?, key?, modifiers?}]}` | `{layout, status, frame, keys, selected_keys}` once the window settles: pointer and key input, one event a frame (app/input.rs) |
//! | `{op: "click", widget, button?, modifiers?}` | as `input`, plus `clicked`: press a widget by its stable id (`app/ui_click.rs`); a menu bar item replies `{clicked, invoked, status}` |
//! | `{op: "layout"}` | `widgets` (every interactive widget: `{id, label, rect, enabled}`, `app/ui_ids.rs`) and where things are on screen: viewport rect, joints, timeline `frame_x`, `ruler_y`, `keys_y`; the viewport's `tool`, `selection`, `gizmo` (centre, ring radii, a grab point per axis ring), `onion` frames, `trail`, `camera`, `picker` (joints and buttons) |
//! | `{op: "events", since?, limit?}` | `{events, cursor, latest, dropped}`: what happened after the cursor `since` |
//! | `{op: "subscribe", events?: [kinds]}` | `{subscribed, cursor}`, then every new event as its own line `{event, seq, ...}` on this connection |
//! | `{op: "unsubscribe"}` | `{}`: no more event lines |
//!
//! Every request may carry:
//! - `id`: echoed in its reply (and its progress lines), so a client can tell replies apart;
//! - `timeout`: seconds to wait for the window (1..=3600; default `AC_CONTROL_TIMEOUT`, else 120,
//!   the same default `ac mcp --live` uses);
//! - `progress: true`: while the window works on it, a line `{id, progress: {op, command, elapsed}}`
//!   comes every second (long ops: `ai.fill`, `physics.solve`, renders).
//!
//! Events (`seq` rising from 1): `command` (`id`, `undoable`, `source`: ui or agent, `error` when
//! refused), `undo` / `redo` (`label`), `frame` (`frame`), `selection` (`bone`, `keys`).
//!
//! A failure replies `{error: text}`; a refused `run` adds `detail` (the core error as JSON: kind,
//! message, hint, suggestions). Requests are handled on the UI thread between frames, in
//! order; `ac mcp --live <port>` turns this into MCP tools.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Seconds a request waits for the window unless it or `AC_CONTROL_TIMEOUT` says otherwise.
pub const DEFAULT_TIMEOUT_S: u64 = 120;
pub const MAX_TIMEOUT_S: u64 = 3600;
/// Events kept for `events` reads.
pub const MAX_EVENTS: usize = 1000;
/// Most events one `events` read returns.
const MAX_READ: u64 = 1000;
/// How often a request that asked for progress hears that the window is still on it.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

/// A request from the port and where its reply goes.
pub struct Request {
    pub msg: Value,
    pub reply: Sender<Value>,
}

/// The request's own timeout, else `AC_CONTROL_TIMEOUT`, else 120 s.
fn timeout_of(msg: &Value) -> Duration {
    let env = || {
        std::env::var("AC_CONTROL_TIMEOUT")
            .ok()
            .and_then(|t| t.trim().parse::<u64>().ok())
    };
    let s = msg["timeout"]
        .as_u64()
        .or_else(env)
        .unwrap_or(DEFAULT_TIMEOUT_S)
        .clamp(1, MAX_TIMEOUT_S);
    Duration::from_secs(s)
}

#[derive(Default)]
struct Hub {
    last: u64,
    ring: VecDeque<Value>,
    /// Subscribers: the kinds they want (empty: all) and where events go.
    subs: Vec<(Vec<String>, Sender<Value>)>,
}

/// What happened in the window, numbered, for `events` reads and `subscribe` streams. Cloned
/// handles share one log.
#[derive(Clone, Default)]
pub struct Events(Arc<Mutex<Hub>>);

impl Events {
    fn hub(&self) -> std::sync::MutexGuard<'_, Hub> {
        // a panic while holding the lock leaves plain data behind: keep using it
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Log an event and send it to every subscriber that wants its kind.
    pub fn publish(&self, kind: &str, data: Value) {
        let mut hub = self.hub();
        hub.last += 1;
        let mut e = json!({ "seq": hub.last, "event": kind });
        if let (Some(m), Value::Object(d)) = (e.as_object_mut(), data) {
            m.extend(d);
        }
        hub.subs.retain(|(kinds, tx)| {
            (!kinds.is_empty() && !kinds.iter().any(|k| k == kind)) || tx.send(e.clone()).is_ok()
        });
        hub.ring.push_back(e);
        while hub.ring.len() > MAX_EVENTS {
            hub.ring.pop_front();
        }
    }

    /// The events after `cursor` (at most `limit`), the cursor to pass next, and how many were
    /// dropped before they could be read.
    pub fn since(&self, cursor: u64, limit: u64) -> Value {
        let hub = self.hub();
        let events: Vec<&Value> = hub
            .ring
            .iter()
            .filter(|e| e["seq"].as_u64().is_some_and(|s| s > cursor))
            .take(limit.clamp(1, MAX_READ) as usize)
            .collect();
        let oldest = hub
            .ring
            .front()
            .and_then(|e| e["seq"].as_u64())
            .unwrap_or(hub.last + 1);
        let next = events
            .last()
            .and_then(|e| e["seq"].as_u64())
            .unwrap_or(cursor.min(hub.last));
        json!({
            "events": events,
            "cursor": next,
            "latest": hub.last,
            "dropped": oldest.saturating_sub(cursor.saturating_add(1)),
        })
    }

    fn subscribe(&self, kinds: Vec<String>) -> (Receiver<Value>, u64) {
        let (tx, rx) = channel();
        let mut hub = self.hub();
        hub.subs.push((kinds, tx));
        (rx, hub.last)
    }
}

/// Listen on `port` (localhost only); requests arrive on the returned channel, and the window is
/// woken for each. The window publishes what happens to the returned [`Events`].
pub fn listen(port: u16, ctx: egui::Context) -> Result<(Receiver<Request>, Events), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("control port {port}: {e}"))?;
    let events = Events::default();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
    Ok((accept(listener, wake, events.clone())?, events))
}

/// Serve every connection to `listener` on a thread of its own.
fn accept(
    listener: TcpListener,
    wake: Arc<dyn Fn() + Send + Sync>,
    events: Events,
) -> Result<Receiver<Request>, String> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("control".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let (tx, wake, events) = (tx.clone(), wake.clone(), events.clone());
                // a connection that can't get a thread is closed, and its client sees that
                let _ = std::thread::Builder::new()
                    .name("control-conn".into())
                    .spawn(move || serve(stream, &tx, &*wake, &events));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(rx)
}

/// One line to the client; false when the connection is gone.
fn send(out: &Mutex<TcpStream>, v: &Value) -> bool {
    let mut s = out.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    writeln!(s, "{v}").and_then(|()| s.flush()).is_ok()
}

/// Wait for the window's reply to `msg`, sending progress lines when it asked for them.
fn wait(rx: &Receiver<Value>, msg: &Value, out: &Mutex<TcpStream>) -> Value {
    let limit = timeout_of(msg);
    let start = Instant::now();
    let progress = msg["progress"] == true;
    loop {
        let left = limit.saturating_sub(start.elapsed());
        let slice = if progress { left.min(PROGRESS_EVERY) } else { left };
        match rx.recv_timeout(slice) {
            Ok(v) => return v,
            Err(RecvTimeoutError::Disconnected) => {
                return json!({ "error": "the window closed before it answered" });
            }
            Err(RecvTimeoutError::Timeout) if start.elapsed() >= limit => {
                return json!({ "error": format!(
                    "the app did not answer in {} s (it may still run the request; the request's `timeout` or AC_CONTROL_TIMEOUT sets the wait)",
                    limit.as_secs()
                ) });
            }
            Err(RecvTimeoutError::Timeout) => {
                let line = json!({ "id": msg.get("id"), "progress": {
                    "op": msg["op"].as_str().unwrap_or("run"), "command": msg.get("command"),
                    "elapsed": start.elapsed().as_secs(), "timeout": limit.as_secs(),
                } });
                if !send(out, &line) {
                    return json!({ "error": "the client left" });
                }
            }
        }
    }
}

fn serve(stream: TcpStream, tx: &Sender<Request>, wake: &dyn Fn(), events: &Events) {
    let Ok(read) = stream.try_clone() else { return };
    let out = Arc::new(Mutex::new(stream));
    let mut stop: Option<Arc<AtomicBool>> = None;
    for line in BufReader::new(read).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg = match serde_json::from_str::<Value>(&line) {
            Ok(m) if m.is_object() => m,
            Ok(other) => {
                if !send(
                    &out,
                    &json!({ "error": format!("a request is a JSON object, not {other}") }),
                ) {
                    break;
                }
                continue;
            }
            Err(e) => {
                if !send(&out, &json!({ "error": format!("not JSON: {e}") })) {
                    break;
                }
                continue;
            }
        };
        let mut reply = match msg["op"].as_str().unwrap_or("run") {
            "events" => json!({ "ok": events.since(
                msg["since"].as_u64().unwrap_or(0),
                msg["limit"].as_u64().unwrap_or(100),
            ) }),
            "subscribe" => {
                let kinds: Vec<String> = msg["events"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect();
                if let Some(old) = stop.take() {
                    old.store(true, Ordering::SeqCst);
                }
                let (rx, cursor) = events.subscribe(kinds.clone());
                let flag = Arc::new(AtomicBool::new(false));
                stop = Some(flag.clone());
                let out = out.clone();
                let started = std::thread::Builder::new()
                    .name("control-events".into())
                    .spawn(move || {
                        for e in rx {
                            if flag.load(Ordering::SeqCst) || !send(&out, &e) {
                                break;
                            }
                        }
                    });
                match started {
                    Ok(_) => {
                        json!({ "ok": { "subscribed": if kinds.is_empty() { json!("all") } else { json!(kinds) }, "cursor": cursor } })
                    }
                    Err(e) => json!({ "error": format!("no thread for the event stream: {e}") }),
                }
            }
            "unsubscribe" => {
                if let Some(old) = stop.take() {
                    old.store(true, Ordering::SeqCst);
                }
                json!({ "ok": {} })
            }
            _ => {
                let (rtx, rrx) = channel();
                if tx
                    .send(Request {
                        msg: msg.clone(),
                        reply: rtx,
                    })
                    .is_err()
                {
                    break; // the window closed
                }
                wake();
                wait(&rrx, &msg, &out)
            }
        };
        if let (Some(id), Some(m)) = (msg.get("id"), reply.as_object_mut()) {
            m.insert("id".into(), id.clone());
        }
        if !send(&out, &reply) {
            break;
        }
    }
    if let Some(s) = stop {
        s.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// A port with a stand-in window: `run` answers after `delay`, echoing the command.
    fn port(delay: Duration) -> (u16, Events) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let events = Events::default();
        let rx = accept(listener, Arc::new(|| {}), events.clone()).unwrap();
        let ev = events.clone();
        std::thread::spawn(move || {
            for req in rx {
                std::thread::sleep(delay);
                ev.publish("command", json!({ "id": req.msg["command"], "source": "agent" }));
                let _ = req.reply.send(json!({ "ok": { "ran": req.msg["command"] } }));
            }
        });
        (port, events)
    }

    fn client(port: u16) -> (BufReader<TcpStream>, TcpStream) {
        let s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        (BufReader::new(s.try_clone().unwrap()), s)
    }

    fn line(r: &mut BufReader<TcpStream>) -> Value {
        let mut l = String::new();
        r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }

    #[test]
    fn replies_echo_the_id_and_bad_lines_say_why() {
        let (port, _) = port(Duration::ZERO);
        let (mut r, mut w) = client(port);
        writeln!(
            w,
            "{}",
            json!({ "id": "a7", "op": "run", "command": "editor.state" })
        )
        .unwrap();
        let v = line(&mut r);
        assert_eq!(v["id"], "a7");
        assert_eq!(v["ok"]["ran"], "editor.state");
        writeln!(w, "[1, 2]").unwrap();
        assert!(line(&mut r)["error"].as_str().unwrap().contains("object"));
        writeln!(w, "{{nope").unwrap();
        assert!(line(&mut r)["error"].as_str().unwrap().contains("not JSON"));
    }

    #[test]
    fn slow_requests_report_progress_and_time_out_as_asked() {
        let (port, _) = port(Duration::from_millis(2600));
        let (mut r, mut w) = client(port);
        writeln!(
            w,
            "{}",
            json!({ "id": 1, "op": "run", "command": "ai.fill", "progress": true })
        )
        .unwrap();
        let p = line(&mut r);
        assert_eq!(p["id"], 1);
        assert_eq!(p["progress"]["command"], "ai.fill");
        assert_eq!(line(&mut r)["progress"]["elapsed"], 2);
        assert_eq!(line(&mut r)["ok"]["ran"], "ai.fill");
        // a 1 s timeout on a 2.6 s request: an error naming the wait
        writeln!(
            w,
            "{}",
            json!({ "id": 2, "op": "run", "command": "x", "timeout": 1 })
        )
        .unwrap();
        let v = line(&mut r);
        assert_eq!(v["id"], 2);
        assert!(v["error"].as_str().unwrap().contains("1 s"), "{v}");
    }

    #[test]
    fn events_are_read_by_cursor_and_streamed_to_subscribers() {
        let (port, events) = port(Duration::ZERO);
        events.publish("frame", json!({ "frame": 3 }));
        let (mut sr, mut sw) = client(port);
        writeln!(
            sw,
            "{}",
            json!({ "id": 1, "op": "subscribe", "events": ["command"] })
        )
        .unwrap();
        let sub = line(&mut sr);
        assert_eq!(sub["ok"]["cursor"], 1);
        let (mut r, mut w) = client(port);
        writeln!(w, "{}", json!({ "op": "run", "command": "key.set" })).unwrap();
        line(&mut r);
        events.publish("frame", json!({ "frame": 4 }));
        writeln!(w, "{}", json!({ "op": "run", "command": "key.delete" })).unwrap();
        line(&mut r);
        // the subscriber hears only commands, in order
        let a = line(&mut sr);
        assert_eq!(
            (a["event"].as_str(), a["id"].as_str()),
            (Some("command"), Some("key.set"))
        );
        assert_eq!(line(&mut sr)["id"], "key.delete");
        // a cursor read sees everything after it
        writeln!(w, "{}", json!({ "op": "events", "since": 1 })).unwrap();
        let v = line(&mut r);
        let kinds: Vec<&str> = v["ok"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["event"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["command", "frame", "command"]);
        assert_eq!(v["ok"]["cursor"], 4);
        assert_eq!(v["ok"]["dropped"], 0);
        writeln!(sw, "{}", json!({ "op": "unsubscribe", "id": 9 })).unwrap();
        assert_eq!(line(&mut sr)["id"], 9);
    }

    #[test]
    fn the_log_keeps_the_newest_and_says_what_it_dropped() {
        let e = Events::default();
        for i in 0..(MAX_EVENTS + 3) {
            e.publish("frame", json!({ "frame": i }));
        }
        let v = e.since(0, 5);
        assert_eq!(v["dropped"], 3);
        assert_eq!(v["events"][0]["seq"], 4);
        assert_eq!(e.since(u64::MAX, 5)["events"], json!([]));
    }
}
