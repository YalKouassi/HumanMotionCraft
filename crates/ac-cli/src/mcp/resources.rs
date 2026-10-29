//! MCP resources: read-only views an agent can load into context without a tool call.
//!
//! | uri | what |
//! |---|---|
//! | `ac://state` | the editor state (`editor.state`) |
//! | `ac://skeleton` | bone names, parents, rest heads, joint limits, body parts, conventions |
//! | `ac://library` | the library's animations, poses, groups and custom parts (`library.list`) |
//! | `ac://commands/{id}` | one command: JSON Schema of inputs and outputs, an example call |

use serde_json::{Map, Value, json};

use super::Server;

pub const STATE: &str = "ac://state";
pub const SKELETON: &str = "ac://skeleton";
pub const LIBRARY: &str = "ac://library";
pub const COMMANDS: &str = "ac://commands/";

/// Rig space as every command uses it.
pub const CONVENTIONS: &str = "metres and radians; Z up; the body faces -Y (its left is +X); root offsets \
    are in hip heights (rig.info hip_height), so 1.0 lifts the root one hip height; the standing pose \
    (pose.reset) has the root offset `standing_root`; frames are whole numbers at the clip's fps";

impl Server {
    pub(super) fn resource_list(&self) -> Value {
        json!({ "resources": [
            { "uri": STATE, "name": "editor state", "mimeType": "application/json",
              "description": "The open animation: keys, layers, playhead, pins, locks, AI bands, physics." },
            { "uri": SKELETON, "name": "skeleton", "mimeType": "application/json",
              "description": "Bone names (parents first), rest heads, joint limits (rotation vectors, radians), body parts and their bones, export profiles, and the rig's conventions." },
            { "uri": LIBRARY, "name": "library", "mimeType": "application/json",
              "description": "The library: animations, poses (with the part each keys), groups, custom parts." },
        ]})
    }

    pub(super) fn resource_templates(&self) -> Value {
        json!({ "resourceTemplates": [
            { "uriTemplate": format!("{COMMANDS}{{id}}"), "name": "command", "mimeType": "application/json",
              "description": "One command: input and output JSON Schema and an example call (the same as describe_command)." },
        ]})
    }

    /// The resource's JSON, or why there is none.
    pub(super) fn resource(&mut self, uri: &str) -> Result<Value, String> {
        match uri {
            STATE => self.backend.run("editor.state", &json!({})),
            LIBRARY => self.backend.run("library.list", &json!({})),
            SKELETON => Ok(self.skeleton()),
            _ => match uri.strip_prefix(COMMANDS) {
                Some(id) => self.describe_command(id),
                None => Err(format!(
                    "no resource {uri:?}; there are {STATE}, {SKELETON}, {LIBRARY} and {COMMANDS}{{id}}"
                )),
            },
        }
    }

    /// `resources/read`'s result.
    pub(super) fn resource_read(&mut self, uri: &str) -> Result<Value, String> {
        let v = self.resource(uri)?;
        let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        Ok(json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }))
    }

    fn skeleton(&mut self) -> Value {
        let sk = self.skeleton.clone();
        let limits = self.limits.clone();
        let bones: Vec<Value> = sk
            .names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let parent = sk.parents.get(i).copied().flatten().and_then(|p| sk.names.get(p));
                let lim = limits
                    .as_ref()
                    .and_then(|l| Some(json!({ "lo": l.lo.get(i)?, "hi": l.hi.get(i)? })));
                json!({ "name": name, "parent": parent, "head": sk.heads.get(i), "limits": lim })
            })
            .collect();
        // custom parts live in the library (and in the open document): ask the session
        let (custom, custom_note) = match self.backend.run("library.list", &json!({})) {
            Ok(v) => (v["custom_parts"].clone(), None),
            Err(e) => (json!({}), Some(format!("custom parts unavailable: {e}"))),
        };
        let custom_map: ac_core::parts::Custom = serde_json::from_value(custom.clone()).unwrap_or_default();
        let mut parts = Map::new();
        for name in ac_core::parts::PRESETS
            .iter()
            .map(|s| (*s).to_string())
            .chain(custom_map.keys().cloned())
        {
            let bones: Vec<&String> = ac_core::parts::bones(&sk, &custom_map, &name)
                .unwrap_or_default()
                .iter()
                .filter_map(|&i| sk.names.get(i))
                .collect();
            parts.insert(name, json!(bones));
        }
        json!({
            "conventions": CONVENTIONS,
            "hip_height": sk.hip_height,
            "standing_root": ac_core::document::standing(&sk).root,
            "bones": bones,
            "limits_space": if limits.is_some() { json!("rotation_vector_radians, per axis [x, y, z] relative to the parent") } else { json!(null) },
            "parts": parts,
            "part_rows": ac_core::parts::ROWS,
            "custom_parts": custom,
            "custom_parts_note": custom_note,
            "export_profiles": sk.profiles,
        })
    }
}
