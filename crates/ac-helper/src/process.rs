//! A helper process (`helper.py`, run by the install's Python): one JSON request a line on its
//! stdin; replies on its stdout carry the request's id, `{"id", "progress"}` while it works and
//! then `{"id", "result"}` or `{"id", "error"}`. What it prints besides goes to `helper.log` in
//! the install folder, which a failure names. It ends when its stdin closes (the app quitting
//! closes it too).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::install::Install;

pub struct Process {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next: u64,
    /// The process is (as far as we know) still up.
    up: bool,
    log: PathBuf,
}

/// No console flashes up on Windows.
pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

impl Process {
    /// Start the helper, with its replies read by a thread named `reader`.
    pub fn start(install: &Install, reader: &str) -> Result<Process, String> {
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
        let mut child = quiet(&mut cmd)
            .spawn()
            .map_err(|e| format!("can't start the helper {}: {e}", install.python.display()))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("the helper's pipes did not open".into());
        };
        let (tx, lines) = mpsc::channel();
        std::thread::Builder::new()
            .name(reader.into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("can't start the reply reader: {e}"))?;
        Ok(Process {
            child,
            stdin,
            lines,
            next: 0,
            up: true,
            log,
        })
    }

    /// Start the helper and check the reply to `hello_op` says it speaks `protocol`; returns the
    /// process and that reply.
    pub fn start_checked(
        install: &Install,
        reader: &str,
        hello_op: &str,
        protocol: i64,
        wait: Duration,
    ) -> Result<(Process, Value), String> {
        let mut p = Process::start(install, reader)?;
        let hello = p.call(json!({ "op": hello_op }), wait, &mut |_| {})?;
        let spoken = hello["protocol"].as_i64().unwrap_or(0);
        if spoken != protocol {
            return Err(ac_core::error::hinted(
                format!("the helper speaks protocol {spoken}; this app speaks {protocol}"),
                "run the setup from this app's source tree again",
            ));
        }
        Ok((p, hello))
    }

    /// Whether the process is still up (asked of it now).
    pub fn alive(&mut self) -> bool {
        self.up = self.up && matches!(self.child.try_wait(), Ok(None));
        self.up
    }

    /// The process is up, as far as the last call or check said.
    pub fn up(&self) -> bool {
        self.up
    }

    pub fn kill(&mut self) {
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

    /// Send one request (its `id` is set here) and wait up to `wait` for its result, handing each
    /// progress line to `say`.
    pub fn call(
        &mut self,
        mut req: Value,
        wait: Duration,
        say: &mut dyn FnMut(&str),
    ) -> Result<Value, String> {
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

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}
