//! The window's side of the control port (`control.rs`): requests answered between frames, and
//! what happens in the window (commands run by the person or an agent, undo and redo, the playhead,
//! the selection) published as events.

use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use ac_render::glam::Vec3;
use serde_json::{Value, json};

use super::{App, png_bytes};

/// What the window showed at the end of the last frame, so only changes are published.
#[derive(Default)]
pub struct Watch {
    frame: Option<f64>,
    bone: Option<Option<usize>>,
    keys: Option<BTreeSet<i64>>,
}

/// Commands that only read: an agent asks these all the time, so they are not published.
fn is_query(id: &str) -> bool {
    const VERBS: &[&str] = &[
        "state", "get", "world", "info", "list", "sample", "curves", "contacts", "phases", "overlay",
        "solve", "metrics", "critique", "query", "history",
    ];
    id.rsplit('.').next().is_some_and(|v| VERBS.contains(&v))
}

impl App {
    /// Publish what a command did (`source`: ui or agent). Queries, the playhead (a `frame` event
    /// says it) and the steps inside a gesture (its end says it, with the undo step's name) are
    /// left out, so the log stays readable.
    pub(super) fn announce(&self, id: &str, source: &str, outcome: &Result<Value, String>) {
        let Some(events) = &self.events else { return };
        if is_query(id)
            || id == "frame.set"
            || (self.session.in_gesture() && id != "edit.end")
            || id == "edit.begin"
        {
            return;
        }
        let undoable = self.session.spec(id).is_some_and(|c| c.undoable);
        match outcome {
            Ok(v) => {
                let mut e = json!({ "id": id, "undoable": undoable, "source": source });
                if id == "edit.end" {
                    if v["changed"] != true {
                        return; // a gesture that changed nothing made no undo step
                    }
                    e["label"] = json!(self.session.undo_label());
                }
                events.publish("command", e);
                if matches!(id, "edit.undo" | "edit.redo") && v["changed"] == true {
                    let kind = if id == "edit.undo" { "undo" } else { "redo" };
                    events.publish(kind, json!({ "label": v["label"], "source": source }));
                }
            }
            Err(e) => events.publish(
                "command",
                json!({ "id": id, "undoable": undoable, "source": source, "error": e }),
            ),
        }
    }

    /// End of a frame: publish the playhead and the selection if they changed (while playing, the
    /// playhead once a whole frame).
    pub(super) fn publish_changes(&mut self) {
        let Some(events) = &self.events else { return };
        let frame = self.session.document.frame;
        let shown = |f: f64| if self.playing { f.floor() } else { f };
        if let Some(old) = self.watch.frame
            && shown(old) != shown(frame)
        {
            events.publish("frame", json!({ "frame": frame, "playing": self.playing }));
        }
        self.watch.frame = Some(frame);
        let keys = &self.timeline.selected;
        let changed = self.watch.bone.is_some_and(|b| b != self.selected)
            || self.watch.keys.as_ref().is_some_and(|k| k != keys);
        if changed {
            let bone = self
                .selected
                .and_then(|i| self.session.document.skeleton.names.get(i));
            events.publish("selection", json!({ "bone": bone, "keys": keys }));
        }
        self.watch.bone = Some(self.selected);
        if self.watch.keys.as_ref() != Some(keys) {
            self.watch.keys = Some(keys.clone());
        }
    }

    /// Answer the control port's requests (between frames, in order).
    pub(super) fn serve_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.control else { return };
        let requests: Vec<crate::control::Request> = rx.try_iter().collect();
        for req in requests {
            let reply = match req.msg["op"].as_str().unwrap_or("run") {
                "run" => {
                    let id = req.msg["command"].as_str().unwrap_or_default().to_string();
                    let params = req.msg.get("params").cloned().unwrap_or(json!({}));
                    // the refusal stays structured for the reply (`detail`: kind, hint, suggestions)
                    let (out, detail) = match self.session.run(&id, &params) {
                        Ok(v) => (Ok(v), Value::Null),
                        Err(e) => (Err(e.to_string()), e.to_json()),
                    };
                    self.announce(&id, "agent", &out);
                    match out {
                        Ok(v) => {
                            // only changes are shown: an agent's queries would drown them
                            if self.session.spec(&id).is_some_and(|c| c.undoable) {
                                self.status = format!("agent: {id}");
                            }
                            if id.starts_with("library.")
                                || id.starts_with("clip.")
                                || id.starts_with("pose.")
                                || id.starts_with("session.")
                            {
                                self.refresh_library();
                            }
                            json!({ "ok": v })
                        }
                        Err(e) => {
                            self.status = format!("agent: {id}: {e}");
                            json!({ "error": e, "detail": detail })
                        }
                    }
                }
                "render" => match self.picture(&req.msg) {
                    Ok(png) => json!({ "ok": { "png": ac_render::offscreen::base64(&png) } }),
                    Err(e) => json!({ "error": e }),
                },
                "animation" => match self.animation(&req.msg) {
                    Ok(v) => json!({ "ok": v }),
                    Err(e) => json!({ "error": e }),
                },
                "input" => match self.queue_input(&req.msg, req.reply.clone()) {
                    Ok(()) => {
                        ctx.request_repaint();
                        continue; // answered once the events are played
                    }
                    Err(e) => json!({ "error": e }),
                },
                "layout" => json!({ "ok": self.layout }),
                "click" => match self.click_request(&req.msg, req.reply.clone(), ctx) {
                    Ok(()) => {
                        ctx.request_repaint();
                        continue; // answered once the click is played
                    }
                    Err(e) => json!({ "error": e }),
                },
                "ui" => {
                    let (k, v) = (
                        req.msg["name"].as_str().unwrap_or_default(),
                        req.msg["value"].as_str().unwrap_or_default(),
                    );
                    match self.set_ui(k, v) {
                        Ok(()) => json!({ "ok": {} }),
                        Err(e) => json!({ "error": format!("ui {k}={v}: {e}") }),
                    }
                }
                "sheet" => match self.sheet(&req.msg) {
                    Ok(v) => json!({ "ok": v }),
                    Err(e) => json!({ "error": e }),
                },
                "screenshot" => {
                    let reply = req.reply.clone();
                    let pending = self.shots_pending.clone();
                    pending.fetch_add(1, Ordering::SeqCst);
                    ctx.request_screenshot(move |img| {
                        pending.fetch_sub(1, Ordering::SeqCst);
                        let v = match png_bytes(&img) {
                            Ok(png) => json!({ "ok": { "png": ac_render::offscreen::base64(&png) } }),
                            Err(e) => json!({ "error": e }),
                        };
                        // the connection waiting for it may have timed out and gone
                        let _ = reply.send(v);
                    });
                    ctx.request_repaint();
                    continue;
                }
                other => json!({ "error": format!(
                    "unknown op {other:?} (run, render, sheet, animation, ui, input, click, layout, screenshot, events, subscribe, unsubscribe)"
                ) }),
            };
            // the connection waiting for it may have timed out and gone
            let _ = req.reply.send(reply);
        }
    }

    /// A headless picture of the pose shown (`view`, `frame`, `width`, `height`), for agents.
    fn picture(&mut self, msg: &Value) -> Result<Vec<u8>, String> {
        if let Some(f) = msg["frame"].as_f64() {
            self.session
                .run("frame.set", &json!({ "frame": f }))
                .map_err(|e| e.to_string())?;
        }
        self.ensure_offscreen()?;
        let off = self.offscreen.as_mut().ok_or("no renderer")?;
        let doc = &self.session.document;
        let (w, h) = (
            msg["width"].as_u64().unwrap_or(800).clamp(16, 4096) as u32,
            msg["height"].as_u64().unwrap_or(600).clamp(16, 4096) as u32,
        );
        let (rots, pos) = doc
            .skeleton
            .fk(&doc.pose.rotations, doc.pose.root)
            .map_err(|e| e.to_string())?;
        let root = pos.first().copied().unwrap_or_default();
        let camera = ac_render::offscreen::view(
            msg["view"].as_str().unwrap_or("three-quarter"),
            Vec3::new(root[0] as f32, root[1] as f32, 0.9),
        )?;
        let rgba = off.render(w, h, &camera, (&rots, &pos), &doc.skeleton.heads)?;
        ac_render::offscreen::png(w, h, &rgba)
    }

    /// A contact sheet of the open animation for the control port (`ac mcp --live`): the same
    /// tool as headless, from this window's document (left unchanged).
    fn sheet(&mut self, msg: &Value) -> Result<Value, String> {
        let doc = &self.session.document;
        let frames = doc.sheet_frames(msg)?;
        let posed = doc
            .shown_poses(&frames)?
            .iter()
            .map(|p| doc.skeleton.fk(&p.rotations, p.root).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()?;
        let cell = (
            msg["width"].as_u64().unwrap_or(320).clamp(16, 2048) as u32,
            msg["height"].as_u64().unwrap_or(400).clamp(16, 2048) as u32,
            msg["cols"].as_u64().unwrap_or(frames.len() as u64).clamp(1, 64) as u32,
        );
        self.ensure_offscreen()?;
        let doc = &self.session.document;
        let off = self.offscreen.as_mut().ok_or("no renderer")?;
        let png = off.sheet(
            msg["view"].as_str().unwrap_or("side"),
            cell,
            &posed,
            &doc.skeleton.heads,
            true,
        )?;
        Ok(json!({ "png": ac_render::offscreen::base64(&png), "frames": frames }))
    }

    /// The motion at `times` as an animated GIF and/or an MP4 (`ac mcp --live`'s
    /// `animation_preview`, which works out the plan); the document is left unchanged.
    fn animation(&mut self, msg: &Value) -> Result<Value, String> {
        let list = msg["times"].as_array().map_or(&[][..], Vec::as_slice);
        let times: Vec<f64> = list.iter().filter_map(Value::as_f64).collect();
        if times.is_empty()
            || times.len() != list.len()
            || times.len() > ac_render::anim::MAX_FRAMES
            || times.iter().any(|t| !(0.0..=1e7).contains(t))
        {
            return Err(format!(
                "times: 1 to {} frame numbers, 0 or more",
                ac_render::anim::MAX_FRAMES
            ));
        }
        let size = (
            msg["width"].as_u64().unwrap_or(320).clamp(16, 1024) as u32,
            msg["height"].as_u64().unwrap_or(240).clamp(16, 1024) as u32,
        );
        let mp4 = match msg["mp4"].as_str() {
            Some(p) => {
                let path = std::path::PathBuf::from(p);
                if !path.is_absolute() || path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("mp4")) {
                    return Err(format!("mp4 {p:?}: an absolute path ending in .mp4"));
                }
                Some(path)
            }
            None => None,
        };
        let doc = &self.session.document;
        let posed = doc
            .shown_poses(&times)?
            .iter()
            .map(|p| doc.skeleton.fk(&p.rotations, p.root).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()?;
        self.ensure_offscreen()?;
        let doc = &self.session.document;
        let off = self.offscreen.as_mut().ok_or("no renderer")?;
        let view = msg["view"].as_str().unwrap_or("side");
        let fixed = msg["fixed"].as_bool().unwrap_or(true);
        let frames = off.frames(view, size, &posed, &doc.skeleton.heads, fixed)?;
        let mut out = json!({ "frames": frames.len() });
        if msg["gif"].as_bool().unwrap_or(true) {
            let delay = msg["delay"].as_u64().unwrap_or(4).clamp(2, 6000) as u16;
            let gif = ac_render::anim::gif(size.0, size.1, &frames, delay)?;
            out["gif"] = json!(ac_render::offscreen::base64(&gif));
        }
        if let Some(path) = mp4 {
            let fps = msg["fps"].as_f64().unwrap_or(30.0);
            ac_render::anim::mp4(&path, size.0, size.1, fps, &frames)?;
            out["mp4"] = json!(path.to_string_lossy());
        }
        Ok(out)
    }
}
