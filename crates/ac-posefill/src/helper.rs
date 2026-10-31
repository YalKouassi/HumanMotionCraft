//! The helper process (`tools/posefill/helper.py`): a JSON-lines process
//! ([`ac_helper::process::Process`]) speaking this crate's ops, `info`, `load` and `fill`.

use std::time::Duration;

use ac_helper::process::Process;
use serde_json::{Value, json};

use crate::engine::{Engine, Fill, Info, LiveBegun, LiveStart, Request, Status};
use crate::install::Install;
use crate::live::{StepAsk, Stretch};

/// The protocol `helper.py` speaks.
pub const PROTOCOL: i64 = 1;
/// The longest a reply may take: the first load builds the TensorRT plan for this GPU.
const SLOW: Duration = Duration::from_secs(30 * 60);
/// `info` imports the helper's own code only.
const HELLO: Duration = Duration::from_secs(120);
/// A live stretch takes tens of milliseconds; this long means it has stuck.
const STEP: Duration = Duration::from_secs(30);

pub struct Helper {
    process: Process,
    loaded: bool,
    model: String,
    info: Info,
    live: bool,
}

fn info_of(v: &Value, into: &mut Info) {
    if let Some(p) = v["plan"].as_str() {
        into.plan = p.to_string();
    }
    if let Some(g) = v["gpu"].as_str() {
        into.gpu = g.to_string();
    }
}

impl Helper {
    /// Start the helper and check it speaks [`PROTOCOL`].
    pub fn start(install: &Install) -> Result<Helper, String> {
        let (process, hello) = Process::start_checked(install, "posefill-replies", "info", PROTOCOL, HELLO)?;
        let mut info = Info::default();
        info_of(&hello, &mut info);
        Ok(Helper {
            process,
            loaded: hello["loaded"].as_bool().unwrap_or(false),
            model: hello["model"]
                .as_str()
                .map_or_else(|| install.model.clone(), String::from),
            live: hello["ops"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|o| o == "live_start")),
            info,
        })
    }

    fn check_live(&self) -> Result<(), String> {
        if self.live {
            return Ok(());
        }
        Err(ac_core::error::hinted(
            "the installed helper is older than live motion",
            "update it: posefill.install with action start and repair",
        ))
    }
}

/// A live step's result as a [`Stretch`].
pub fn stretch(v: &Value) -> Result<Stretch, String> {
    let (rotations, roots, notes) = ac_helper::frames::read(v)?;
    Ok(Stretch {
        rotations,
        roots,
        transition: v["transition"].as_u64().map_or(0, |t| t as usize),
        target: v["target"].as_str().unwrap_or_default().to_string(),
        seconds: v["seconds"].as_f64().unwrap_or(0.0),
        notes,
    })
}

/// A fill result as a [`Fill`].
pub fn fill(v: &Value) -> Result<Fill, String> {
    let (rotations, roots, notes) = ac_helper::frames::read(v)?;
    Ok(Fill {
        rotations,
        roots,
        notes,
        seconds: v["seconds"].as_f64().unwrap_or(0.0),
    })
}

impl Engine for Helper {
    fn status(&self) -> Status {
        Status {
            running: self.process.up(),
            loaded: self.loaded,
            model: self.model.clone(),
        }
    }

    fn info(&self) -> Info {
        self.info.clone()
    }

    fn load(&mut self, say: &mut dyn FnMut(&str)) -> Result<(), String> {
        let r = self.process.call(json!({ "op": "load" }), SLOW, say);
        self.loaded = r.is_ok() && self.process.alive();
        if let Ok(v) = &r {
            info_of(v, &mut self.info);
            self.info.plan = "built".into();
        }
        r.map(|_| ())
    }

    fn fill(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Fill, String> {
        let mut body = serde_json::to_value(req).map_err(|e| e.to_string())?;
        body["op"] = json!("fill");
        let r = self.process.call(body, SLOW, say);
        self.loaded = self.process.alive() && (self.loaded || r.is_ok());
        if r.is_ok() {
            self.info.plan = "built".into();
        }
        fill(&r?)
    }

    fn live_start(&mut self, req: &LiveStart, say: &mut dyn FnMut(&str)) -> Result<LiveBegun, String> {
        self.check_live()?;
        let mut body = serde_json::to_value(req).map_err(|e| e.to_string())?;
        body["op"] = json!("live_start");
        let v = self.process.call(body, SLOW, say)?;
        Ok(LiveBegun {
            session: v["session"].as_i64().ok_or("the helper named no live session")?,
            targets: v["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(String::from))
                .collect(),
            max_speed: v["max_speed"]
                .as_f64()
                .filter(|s| s.is_finite() && *s > 0.0)
                .ok_or("the target set has no top speed")?,
        })
    }

    fn live_step(&mut self, ask: &StepAsk, say: &mut dyn FnMut(&str)) -> Result<Stretch, String> {
        let mut body = serde_json::to_value(ask).map_err(|e| e.to_string())?;
        body["op"] = json!("live_step");
        stretch(&self.process.call(body, STEP, say)?)
    }
}
