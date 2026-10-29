//! MCP prompts: ready-made workflows in the conventions the fixtures use (Z up, the body faces -Y,
//! root offsets in hip heights). Each is a user message with the steps as tool calls; the
//! arguments fill in numbers and names.

use serde_json::{Value, json};

struct Arg {
    name: &'static str,
    description: &'static str,
    default: &'static str,
}

struct Prompt {
    name: &'static str,
    description: &'static str,
    args: &'static [Arg],
    /// `{name}` is replaced by the argument (or its default).
    text: &'static str,
}

const RIG: &str = "Rig space: metres and radians, Z up, the body faces -Y (its left is +X). Root offsets are in \
hip heights: read `hip_height` and `standing_root` from the resource ac://skeleton (the default rig: hip height \
0.8698308 m, standing root offset [0, 0, 0.9450101]). Moving the body d metres forward is a root offset y of \
-d / hip_height. Every edit is a command (command_run {id, params}); undoable ones go on the undo stack \
(edit.undo). describe_command {id} gives a command's schema and an example; command_list {query} finds one.";

const PROMPTS: &[Prompt] = &[
    Prompt {
        name: "block_walk",
        description: "Block a walk from library poses: stride keys moving forward along -Y, filled, then checked from the side.",
        args: &[
            Arg {
                name: "steps",
                description: "how many steps",
                default: "4",
            },
            Arg {
                name: "stride",
                description: "metres per step",
                default: "0.7",
            },
            Arg {
                name: "frames_per_step",
                description: "frames between contacts",
                default: "15",
            },
        ],
        text: "Block a walk of {steps} steps, {stride} m each, one step every {frames_per_step} frames.\n\n{rig}\n\n\
1. Start clean: command_run clip.new {\"name\": \"Walk block\"} (headless: session_reset first if this session has history). \
Read ac://skeleton for hip_height and standing_root, and ac://library for poses: \"Standing/Mid-step\" is a stride, \
\"Standing/Relaxed\" a stand.\n\
2. For step i = 0 .. {steps}: frame.set {\"frame\": i * {frames_per_step}}; pose.apply {\"id\": \"Standing/Mid-step\", \"key\": false} \
(the pose lands where the body stands); on odd steps pose.mirror {\"mode\": \"flip\"} so the other foot leads; read the root \
with pose.get {} and set pose.set_root {\"root\": [x, -(i * {stride}) / hip_height, z]} keeping its x and z; then key.set {}. \
Batch a step's commands with command_batch.\n\
3. Fill between the contacts: ai.fill {\"from\": a, \"to\": b} for each pair of keys (or ai.preview to look before keeping). \
physics.set {\"on\": true} lets the physics pass plant the feet and keep the balance.\n\
4. Look: contact_sheet {\"view\": \"side\", \"count\": 8} and animation_preview {\"view\": \"side\"}. The body should travel along -Y \
(to the left in the side view) by about {steps} x {stride} m, feet alternating.\n\
5. Check numbers, not only pictures: expect {\"checks\": [{\"bone\": \"root\", \"path\": \"position.1\", ...}]} at the last \
key (frame.set first), within 0.01 m of -{steps} x {stride} plus the rest y (-0.0108229 on the default rig). \
expect's save_as keeps the steps and checks as a fixture.",
    },
    Prompt {
        name: "fix_foot_sliding",
        description: "Find frames where a planted foot slides and fix them: measure, plant, re-check.",
        args: &[Arg {
            name: "tolerance",
            description: "metres a planted foot may drift",
            default: "0.01",
        }],
        text: "Find and fix foot sliding in the open animation (a planted foot may drift at most {tolerance} m).\n\n{rig}\n\n\
1. Measure: clip.metrics {} gives `sliding`, one entry per contact span ({foot, from, to, slide_cm}), and \
`sliding_max_cm`; spans over {tolerance} m (x 100 in cm) need fixing. clip.critique {} ranks the same findings, \
worst first, each with a suggested `command`/`params` and a `batch` that fixes it. Both measure the motion shown \
(the physics pass included); `motion: \"keys\"` measures the keys alone.\n\
2. Look: contact_sheet {\"view\": \"top\", \"count\": 8} shows the feet's paths from above; animation_preview \
{\"view\": \"side\"} shows the slide in motion.\n\
3. Fix, smallest change first. Try a finding's fix without keeping it: command_run {id, params, dry_run: true} says \
what it would change. Then: a finding's `batch` through command_batch {commands, label: \"Fix foot sliding\"} (one \
undo step; a refusal puts everything back); feet.plant {\"foot\": \"L\", \"from\": a, \"to\": b} plants one foot over one \
span; feet.lock {} pins the feet on all their contact frames; physics.set {\"on\": true} lets the pass hold planted feet \
(physics.solve {} lists the frames it corrected in `slid_frames` and `problems`).\n\
4. Re-measure with clip.metrics {}: every span's slide_cm at most {tolerance} x 100. Check that the body still travels \
as before (bone.world root on the first and last frames) and that nothing else moved: clip.critique {} finds no new \
problem, editor.state's keys and bands. Undo (edit.undo) any step that made it worse.",
    },
    Prompt {
        name: "review_contact_sheet",
        description: "Review the open animation from contact sheets and an animated preview, and list what to fix.",
        args: &[Arg {
            name: "count",
            description: "frames on each sheet",
            default: "8",
        }],
        text: "Review the open animation and report what a person would want fixed.\n\n{rig}\n\n\
1. Read ac://state: length, fps, keys (yours vs AI), AI bands, layers, physics on or off.\n\
2. contact_sheet {\"view\": \"side\", \"count\": {count}} then {\"view\": \"front\", \"count\": {count}}: one camera sees \
every frame, so heights and travel compare. In the side view the body faces left (-Y). Add {\"view\": \"top\"} when it \
turns or travels. contact_sheet {\"frames\": [...]} looks closer at a stretch.\n\
3. animation_preview {\"view\": \"three-quarter\"} for timing: pops, holds, hitches.\n\
4. Look for: feet sliding or sinking below the floor, knees bending backwards, the body leaning past its feet \
(physics.overlay {\"frame\": f} shows the support area and centre of mass), arms passing through the body, sudden \
turns between keys, AI bands that drift from the keys around them.\n\
5. For each problem name the frames, the bone or part, what is wrong and the command that would fix it (key.set, \
pose.reach, ai.fill, physics.set, ...). Change nothing unless asked; if asked, make one fix at a time and look again.",
    },
    Prompt {
        name: "export_game_engine",
        description: "Export animations as .glb for a game engine (Y up), with the game joint profile, and check the file.",
        args: &[
            Arg {
                name: "path",
                description: "the .glb to write (absolute path)",
                default: "",
            },
            Arg {
                name: "profile",
                description: "joints to keep: full, game or body",
                default: "game",
            },
        ],
        text: "Export for a game engine to {path} with the {profile} joint profile.\n\n{rig}\n\nThe export maps rig axes \
(x, y, z) to glTF's (x, z, -y): Y up, the body facing +Z (glTF's forward). Do not convert anything yourself.\n\n\
1. With `ids` the export reads the saved library animations: if editor.state says `unsaved`, clip.save {} first.\n\
2. Pick what to export: ac://library lists animation ids (\"Group/Name\"); with no ids the open one is exported.\n\
3. command_run clip.export {\"path\": \"{path}\", \"ids\": [...], \"profile\": \"{profile}\"}. ac://skeleton's \
export_profiles lists each profile's joints (the game profile keeps 68). bones/drop pick joints by name instead.\n\
4. Check the result it returns: `joints` and `animations` as expected. A path not ending in .glb, an unknown \
profile or a joint the rig lacks is refused with the reason; nothing is written then.\n\
5. If the engine wants root motion on the hips only, or in place, say so to the person: this export keeps the root's \
travel as keyed.",
    },
];

/// `prompts/list`.
pub fn list() -> Value {
    json!({ "prompts": PROMPTS.iter().map(|p| json!({
        "name": p.name,
        "description": p.description,
        "arguments": p.args.iter().map(|a| json!({
            "name": a.name,
            "description": if a.default.is_empty() { a.description.to_string() } else { format!("{} (default {})", a.description, a.default) },
            "required": a.default.is_empty(),
        })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>() })
}

/// `prompts/get`: the workflow with its arguments filled in.
pub fn get(name: &str, args: &Value) -> Result<Value, String> {
    let p = PROMPTS.iter().find(|p| p.name == name).ok_or_else(|| {
        let names: Vec<&str> = PROMPTS.iter().map(|p| p.name).collect();
        format!("no prompt {name:?}; there are {}", names.join(", "))
    })?;
    let mut text = p.text.replace("{rig}", RIG);
    for a in p.args {
        let v = match &args[a.name] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Null if !a.default.is_empty() => a.default.to_string(),
            Value::Null => return Err(format!("prompt {name} needs the argument {:?}", a.name)),
            other => other.to_string(),
        };
        if v.len() > 1024 {
            return Err(format!("argument {:?} is longer than 1024 bytes", a.name));
        }
        text = text.replace(&format!("{{{}}}", a.name), &v);
    }
    Ok(json!({
        "description": p.description,
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
    }))
}
