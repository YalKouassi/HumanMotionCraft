//! The helper process (`tools/textmotion/helper.py`): a JSON-lines process
//! ([`ac_helper::process::Process`]) speaking this crate's ops, `hello`, `load` and `generate`.

use std::time::Duration;

use ac_helper::process::Process;
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
    process: Process,
    loaded: bool,
    model: String,
}

impl Helper {
    /// Start the helper and check it speaks [`PROTOCOL`].
    pub fn start(install: &Install) -> Result<Helper, String> {
        let (process, hello) =
            Process::start_checked(install, "textmotion-replies", "hello", PROTOCOL, HELLO)?;
        Ok(Helper {
            process,
            loaded: hello["loaded"].as_bool().unwrap_or(false),
            model: hello["model"]
                .as_str()
                .map_or_else(|| install.model.clone(), String::from),
        })
    }
}

/// A generate result as a [`Motion`].
pub fn motion(v: &Value) -> Result<Motion, String> {
    let (rotations, roots, notes) = ac_helper::frames::read(v)?;
    Ok(Motion {
        rotations,
        roots,
        notes,
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

    fn load(&mut self, say: &mut dyn FnMut(&str)) -> Result<(), String> {
        let r = self.process.call(json!({ "op": "load" }), SLOW, say);
        self.loaded = r.is_ok() && self.process.alive();
        r.map(|_| ())
    }

    fn generate(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Motion, String> {
        let mut body = serde_json::to_value(req).map_err(|e| e.to_string())?;
        body["op"] = json!("generate");
        let r = self.process.call(body, SLOW, say);
        self.loaded = self.process.alive() && (self.loaded || r.is_ok());
        motion(&r?)
    }
}
