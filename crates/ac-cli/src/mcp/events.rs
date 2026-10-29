//! What happened, as numbered events an agent reads with a cursor (`ui_events`). In live mode the
//! app keeps them (control op `events`); headless, the server logs what its own tools ran, in the
//! same shape:
//!
//! | event | fields |
//! |---|---|
//! | `command` | `id`, `undoable`, `source` (`ui`, `agent`, `mcp`), `error` when refused |
//! | `undo` / `redo` | `label`: what it undid or redid |
//! | `frame` | `frame`: the playhead moved |
//! | `selection` | `bone`, `keys` (live only: what the person selected) |
//! | `reset` | the session started again |
//!
//! Every event carries `seq` (rising from 1); `since: n` returns those after `n`.

use std::collections::VecDeque;

use serde_json::{Value, json};

/// Events kept; older ones are dropped (and said to be).
pub const MAX_EVENTS: usize = 1000;
/// Most events one read returns.
pub const MAX_READ: usize = 1000;

#[derive(Debug, Default)]
pub struct Log {
    last: u64,
    ring: VecDeque<Value>,
}

impl Log {
    pub fn push(&mut self, kind: &str, data: Value) {
        self.last += 1;
        let mut e = json!({ "seq": self.last, "event": kind });
        if let (Some(m), Value::Object(d)) = (e.as_object_mut(), data) {
            m.extend(d);
        }
        self.ring.push_back(e);
        while self.ring.len() > MAX_EVENTS {
            self.ring.pop_front();
        }
    }

    /// The events after `cursor` (at most `limit`), the cursor to pass next, and how many were
    /// dropped before they could be read.
    pub fn since(&self, cursor: u64, limit: usize) -> Value {
        let events: Vec<&Value> = self
            .ring
            .iter()
            .filter(|e| e["seq"].as_u64().is_some_and(|s| s > cursor))
            .take(limit.clamp(1, MAX_READ))
            .collect();
        let oldest = self
            .ring
            .front()
            .and_then(|e| e["seq"].as_u64())
            .unwrap_or(self.last + 1);
        let next = events
            .last()
            .and_then(|e| e["seq"].as_u64())
            .unwrap_or(cursor.min(self.last));
        json!({
            "events": events,
            "cursor": next,
            "latest": self.last,
            "dropped": oldest.saturating_sub(cursor.saturating_add(1)),
        })
    }

    /// Log what a command run through this server did.
    pub fn command(&mut self, id: &str, undoable: bool, outcome: &Result<Value, super::Refusal>) {
        match outcome {
            Ok(v) => {
                self.push(
                    "command",
                    json!({ "id": id, "undoable": undoable, "source": "mcp" }),
                );
                match id {
                    "edit.undo" | "edit.redo" if v["changed"] == true => self.push(
                        if id == "edit.undo" { "undo" } else { "redo" },
                        json!({ "label": v["label"] }),
                    ),
                    "frame.set" => self.push("frame", json!({ "frame": v["frame"] })),
                    _ => {}
                }
            }
            Err(e) => self.push(
                "command",
                json!({ "id": id, "undoable": undoable, "source": "mcp", "error": super::backend::message(e) }),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reads_after_a_cursor_and_says_what_was_dropped() {
        let mut log = Log::default();
        for i in 0..(MAX_EVENTS + 5) {
            log.push("frame", json!({ "frame": i }));
        }
        let r = log.since(0, 10);
        assert_eq!(r["dropped"], 5);
        assert_eq!(r["events"][0]["seq"], 6);
        assert_eq!(r["cursor"], 15);
        let tail = log.since(r["latest"].as_u64().unwrap(), 10);
        assert_eq!(tail["events"].as_array().unwrap().len(), 0);
        assert_eq!(tail["cursor"], MAX_EVENTS as u64 + 5);
        // a cursor from the future (another session) reads nothing and stays sane
        assert_eq!(log.since(u64::MAX, 10)["dropped"], 0);
    }
}
