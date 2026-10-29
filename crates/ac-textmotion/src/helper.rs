//! The helper process (`tools/textmotion/helper.py`, run by the install's Python): one JSON request
//! a line on its stdin; replies on its stdout carry the request's id, `{"id", "progress"}` while it
//! works and then `{"id", "result"}` or `{"id", "error"}`. What it prints besides goes to
//! `helper.log` in the install folder, which a failure names. It ends when its stdin closes (the
//! app quitting closes it too).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::engine::{Engine, Motion, Request, Status};
use crate::install::Install;

/// The protocol `helper.py` speaks.
pub const PROTOCOL: i64 = 1;
/// The longest a reply may take: the first load reads 16 GB of text encoder on the CPU.
const SLOW: Duration = Duration::from_secs(30 * 60);
/// `hello` imports the helper's own code only.
const HELLO: Duration = Duration::from_secs(120);

pub struct Helper {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next: u64,
    /// The process is (as far as we know) still up.
    up: bool,
    loaded: bool,
    model: String,
    log: PathBuf,
}

impl Helper {
    /// Start the helper and check it speaks [`PROTOCOL`].
    pub fn start(install: &Install) -> Result<Helper, String> {
        let log = install.root.join("helper.log");
        let err = std::fs::File::create(&log).map_err(|e| format!("can't write {}: {e}", log.display()))?;
        let mut cmd = Command::new(&install.python);
        cmd.arg("-I")
            .arg(&install.helper)
            .envs(&install.env)
            .current_dir(&install.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(err));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashes up
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("can't start the helper {}: {e}", install.python.display()))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("the helper's pipes did not open".into());
        };
        let (tx, lines) = mpsc::channel();
        std::thread::Builder::new()
            .name("textmotion-replies".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("can't start the reply reader: {e}"))?;
        let mut h = Helper {
            child,
            stdin,
            lines,
            next: 0,
            up: true,
            loaded: false,
            model: install.model.clone(),
            log,
        };
        let hello = h.call(json!({ "op": "hello" }), HELLO, &mut |_| {})?;
        let protocol = hello["protocol"].as_i64().unwrap_or(0);
        if protocol != PROTOCOL {
            return Err(ac_core::error::hinted(
                format!("the helper speaks protocol {protocol}; this app speaks {PROTOCOL}"),
                "run the setup from this app's source tree again",
            ));
        }
        if let Some(m) = hello["model"].as_str() {
            h.model = m.to_string();
        }
        h.loaded = hello["loaded"].as_bool().unwrap_or(false);
        Ok(h)
    }

    fn alive(&mut self) -> bool {
        self.up = self.up && matches!(self.child.try_wait(), Ok(None));
        self.up
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.up = false;
    }

    /// The end of the helper's log, for a failure.
    fn tail(&self) -> String {
        let text = std::fs::read_to_string(&self.log).unwrap_or_default();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let last = lines
            .get(lines.len().saturating_sub(3)..)
            .unwrap_or_default()
            .join(" | ");
        if last.is_empty() {
            format!("(see {})", self.log.display())
        } else {
            format!("({last}; see {})", self.log.display())
        }
    }

    /// Send one request and wait for its result.
    fn call(&mut self, mut req: Value, wait: Duration, say: &mut dyn FnMut(&str)) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        req["id"] = json!(id);
        let line = format!("{req}\n");
        if self
            .stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
            .is_err()
        {
            self.kill();
            return Err(format!("the helper has stopped {}", self.tail()));
        }
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let Ok(v) = serde_json::from_str::<Value>(&line) else {
                        continue; // not a reply: the helper sends replies only, so this is noise
                    };
                    if v["id"].as_u64() != Some(id) {
                        continue;
                    }
                    if let Some(p) = v["progress"].as_str() {
                        say(p);
                    } else if let Some(e) = v["error"].as_str() {
                        return Err(format!("the helper refused: {e}"));
                    } else {
                        return Ok(v["result"].clone());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.kill();
                    return Err(format!(
                        "the helper gave no answer in {} minutes and was stopped {}",
                        wait.as_secs() / 60,
                        self.tail()
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.kill();
                    return Err(format!("the helper has stopped {}", self.tail()));
                }
            }
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.kill();
    }
}

fn floats(v: &Value) -> Option<Vec<f64>> {
    v.as_array()?.iter().map(Value::as_f64).collect()
}

fn quat(v: &Value) -> Option<[f64; 4]> {
    floats(v)?.try_into().ok()
}

fn vec3(v: &Value) -> Option<[f64; 3]> {
    floats(v)?.try_into().ok()
}

/// A generate result as a [`Motion`].
pub fn motion(v: &Value) -> Result<Motion, String> {
    let bad = || {
        "the helper's result is not rotations [frame][bone][x, y, z, w] and roots [frame][x, y, z]"
            .to_string()
    };
    let rotations = v["rotations"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|f| {
            f.as_array()
                .and_then(|b| b.iter().map(quat).collect::<Option<Vec<_>>>())
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)?;
    let roots = v["roots"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(vec3)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)?;
    let notes = v["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n.as_str().map(String::from))
        .collect();
    Ok(Motion {
        rotations,
        roots,
        notes,
    })
}

impl Engine for Helper {
    fn status(&self) -> Status {
        Status {
            running: self.up,
            loaded: self.loaded,
            model: self.model.clone(),
        }
    }

    fn load(&mut self, say: &mut dyn FnMut(&str)) -> Result<(), String> {
        let r = self.call(json!({ "op": "load" }), SLOW, say);
        self.loaded = r.is_ok() && self.alive();
        r.map(|_| ())
    }

    fn generate(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Motion, String> {
        let mut body = serde_json::to_value(req).map_err(|e| e.to_string())?;
        body["op"] = json!("generate");
        let r = self.call(body, SLOW, say);
        self.loaded = self.alive() && (self.loaded || r.is_ok());
        motion(&r?)
    }
}
