//! The tools `tools/list` offers. Each reaches commands through the backend; none has command
//! logic of its own.

use serde_json::{Value, json};

pub fn list() -> Value {
    json!([
        { "name": "command_list", "description": "Find commands. `query` searches ids, labels and input docs (best first, with where each matched), e.g. \"foot slide\" or \"undo\"; `area` keeps ids starting with it (e.g. \"layer\"); `schema: true` adds each command's input and output JSON Schema. Without either, every command: id, label, inputs, outputs, undoable.",
          "inputSchema": { "type": "object", "properties": {
              "query": { "type": "string" }, "area": { "type": "string" },
              "schema": { "type": "boolean" }, "limit": { "type": "integer", "minimum": 1, "maximum": 500 } } } },
        { "name": "describe_command", "description": "One command in full: the JSON Schema of its inputs (types, ranges, enums, defaults, required) and outputs, and an example call taken from a fixture that uses it. Read it before calling a command for the first time.",
          "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] } },
        { "name": "command_run", "description": "Run one command by id with its parameters object; describe_command {id} gives the parameters' JSON Schema. A refusal says why and leaves the document as it was: the error carries `kind`, `command`, and when there are any a `hint` (how to get past it) and `suggestions` (the ids meant), also as structuredContent. `dry_run: true` tries it on a copy (edit.dry_run): what it would return, what it would change and the undo step it would make, with nothing changed (commands that write files are refused). Long commands (ai.fill, physics.solve) report progress when the call carries a progressToken.",
          "inputSchema": { "type": "object", "properties": { "id": { "type": "string" }, "params": { "type": "object" },
              "dry_run": { "type": "boolean", "description": "try it without keeping it (default false)" } }, "required": ["id"] } },
        { "name": "command_batch", "description": "Run several commands in order as one edit (edit.batch): one undo step, named by `label`. Atomic (the default): the first refusal puts the document back as it was and the error names the failed `step` (from 0), its `id` and its own error. `atomic: false` keeps the steps before the refusal and returns their results with `failed` (an error result). Each result carries `changed`, the change summary.",
          "inputSchema": { "type": "object", "properties": {
              "commands": { "type": "array", "maxItems": 1000, "items": { "type": "object", "properties": {
                  "id": { "type": "string" }, "params": { "type": "object" } }, "required": ["id"] } },
              "atomic": { "type": "boolean", "description": "a refusal puts everything back (default true)" },
              "label": { "type": "string", "description": "the undo step's name (default: the command's label, or \"N commands\")" } },
            "required": ["commands"] } },
        { "name": "editor_state", "description": "The open animation: keys, layers, playhead, pins, locks (also the resource ac://state).",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "render_preview", "description": "A PNG of the pose shown (optionally at `frame`) from a named view: front, back, left, right, side, top, three-quarter.",
          "inputSchema": { "type": "object", "properties": {
              "view": { "type": "string", "enum": ["front", "back", "left", "right", "side", "top", "three-quarter"] },
              "frame": { "type": "number", "minimum": 0 },
              "width": { "type": "integer", "minimum": 16, "maximum": 4096 }, "height": { "type": "integer", "minimum": 16, "maximum": 4096 },
              "floor": { "type": "boolean", "description": "draw the floor and its grid (default true)" },
              "background": { "type": "string", "description": "#rrggbb, or transparent (default the viewport's colour)" },
              "azimuth": { "type": "number", "minimum": -6.3, "maximum": 6.3, "description": "your own camera, with elevation and distance: radians about the vertical, 0 at the front of the body" },
              "elevation": { "type": "number", "minimum": -1.55, "maximum": 1.55, "description": "radians above the floor" },
              "distance": { "type": "number", "minimum": 0.1, "maximum": 150, "description": "metres from what it looks at" },
              "target": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": "the point your own camera looks at (default where the view would look)" } } } },
        { "name": "contact_sheet", "description": "One PNG of several frames of the animation as it plays (physics included), side by side from one camera that sees them all, so heights and travel compare. `frames` lists them, else `count` (default 6) spread over the clip. Cells `width` x `height` (default 320 x 400), `cols` per row (default: one row). View default: side.",
          "inputSchema": { "type": "object", "properties": {
              "frames": { "type": "array", "items": { "type": "number" } }, "count": { "type": "integer" },
              "view": { "type": "string" }, "width": { "type": "integer" }, "height": { "type": "integer" },
              "cols": { "type": "integer" },
              "floor": { "type": "boolean", "description": "draw the floor and its grid (default true)" },
              "background": { "type": "string", "description": "#rrggbb, or transparent (default the viewport's colour)" },
              "azimuth": { "type": "number", "minimum": -6.3, "maximum": 6.3, "description": "your own camera, with elevation and distance: radians about the vertical, 0 at the front of the body" },
              "elevation": { "type": "number", "minimum": -1.55, "maximum": 1.55, "description": "radians above the floor" },
              "distance": { "type": "number", "minimum": 0.1, "maximum": 150, "description": "metres from what it looks at" },
              "target": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": "the point your own camera looks at (default where the view would look)" } } } },
        { "name": "animation_preview", "description": "The animation as it plays (physics included) as an animated GIF, in real time: inline when under `inline_max_kb` (default 1024), else written to a file (`out`, or a temp file) and the path returned. `from`/`to` frames (default the whole clip), `fps` pictures a second (default the clip's, at most 30), `view` (default side), `width` x `height` (default 320 x 240, at most 1024), `fixed` camera that sees every frame (default true; false follows the root), `max_frames` (default 240; a longer range is thinned and the result says so). `mp4`: an absolute .mp4 path also gets an H.264 video (ffmpeg; even sizes). Reports progress when the call carries a progressToken.",
          "inputSchema": { "type": "object", "properties": {
              "from": { "type": "number", "minimum": 0 }, "to": { "type": "number", "minimum": 0 },
              "fps": { "type": "number", "minimum": 1, "maximum": 60 },
              "view": { "type": "string", "enum": ["front", "back", "left", "right", "side", "top", "three-quarter"] },
              "width": { "type": "integer", "minimum": 16, "maximum": 1024 }, "height": { "type": "integer", "minimum": 16, "maximum": 1024 },
              "fixed": { "type": "boolean" }, "max_frames": { "type": "integer", "minimum": 1, "maximum": 600 },
              "gif": { "type": "boolean" }, "inline_max_kb": { "type": "integer", "minimum": 0, "maximum": 8192 },
              "out": { "type": "string", "description": "absolute .gif path to write the GIF to" },
              "mp4": { "type": "string", "description": "absolute .mp4 path" },
              "floor": { "type": "boolean", "description": "draw the floor and its grid (default true)" },
              "background": { "type": "string", "description": "#rrggbb (default the viewport's colour); GIF and MP4 can't be transparent" },
              "azimuth": { "type": "number", "minimum": -6.3, "maximum": 6.3, "description": "your own camera, with elevation and distance: radians about the vertical, 0 at the front of the body" },
              "elevation": { "type": "number", "minimum": -1.55, "maximum": 1.55, "description": "radians above the floor" },
              "distance": { "type": "number", "minimum": 0.1, "maximum": 150, "description": "metres from what it looks at" },
              "target": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": "the point your own camera looks at (default where the view would look)" } } } },
        { "name": "expect", "description": "Check the session as it is now, as a fixture does (numbers within `tolerance`, objects key by key, {\"$contains\": [...]} for lists). Each check runs a query (`command`, default editor.state, with `params`; or `bone` for bone.world) and compares `value` at `path` (\"keys.0.frame\" or \"/keys/0/frame\") or the whole result against `expect`. Returns each check's verdict and the actual value when it fails. `save_as`: an absolute path for a new fixture file holding the commands this server ran so far plus these checks (never into a repository's root folder; never overwrites).",
          "inputSchema": { "type": "object", "properties": {
              "checks": { "type": "array", "items": { "type": "object", "properties": {
                  "command": { "type": "string" }, "params": { "type": "object" }, "bone": { "type": "string" },
                  "path": { "type": "string" }, "value": {}, "expect": {}, "tolerance": { "type": "number", "minimum": 0 } } } },
              "tolerance": { "type": "number", "minimum": 0 },
              "save_as": { "type": "string" }, "name": { "type": "string" }, "feature": { "type": "string" },
              "description": { "type": "string" } }, "required": ["checks"] } },
        { "name": "session_reset", "description": "Start again: headless, a fresh session (the default document, empty undo history, the same library); live, the app's session.reset command. Clears the steps expect would save.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "ui_events", "description": "What happened since `since` (a cursor; 0 for everything kept): commands run (by the person or an agent), undo and redo, the playhead moving, the selection changing (live). Returns the events, the `cursor` to pass next time, and how many were `dropped` before they could be read.",
          "inputSchema": { "type": "object", "properties": {
              "since": { "type": "integer", "minimum": 0 }, "limit": { "type": "integer", "minimum": 1, "maximum": 1000 } } } },
        { "name": "ui_layout", "description": "Where things are in the app window (points): the viewport, each body joint on screen, the timeline (frame x = origin + (frame - start) * points_per_frame, keys_y, ruler_y, part rows), the selected keys, and every interactive widget with a stable id, label, rect and enabled state (`widgets`; ids like `menu.File.Save as`, `toolbar.tool.rotate`, `panel.layers.solo.0`) (live mode only).",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "ui_input", "description": "Use the app window as a person does: pointer and key events played one a frame through the UI's own input (live mode only). Events: {kind: move|down|up|click|scroll|key, x, y, button: primary|secondary|middle, dx, dy, key: egui key name, modifiers: [shift, alt, ctrl]}. A drag is a down, some moves and an up. Returns the layout, frame, key count and selected keys after the window settles.",
          "inputSchema": { "type": "object", "properties": { "events": { "type": "array", "items": { "type": "object" } } }, "required": ["events"] } },
        { "name": "ui_click", "description": "Click a widget by its stable id from ui_layout's `widgets` (live mode only), as a person would. Menu-bar items run directly; an item inside a closed popup needs its opener clicked first. A greyed-out widget or an unknown id is refused (with the nearest ids). Returns what ui_input returns, plus `clicked`.",
          "inputSchema": { "type": "object", "properties": {
              "id": { "type": "string" }, "button": { "type": "string", "enum": ["primary", "secondary", "middle"] },
              "modifiers": { "type": "array", "items": { "type": "string", "enum": ["shift", "alt", "ctrl"] } } }, "required": ["id"] } },
        { "name": "ui_set", "description": "Set the window's own state (live mode only), the same names as the app's `--ui name=value`: e.g. tab=animations|builtin|poses, search=<text>, dope=on|off, overlay=on|off|perf, ghost=on|off, tool=drag|move|rotate|reach, camera=<view>, zoom=<metres>, onion, trails, picker, gizmo, dialog=export|preview|block|welcome, window=palette|settings|history|shortcuts, theme=dark|light, joint=<bone>, curves=on|off, time=frames|seconds|timecode. An unknown name is refused with the full list.",
          "inputSchema": { "type": "object", "properties": { "name": { "type": "string" }, "value": { "type": "string" } }, "required": ["name", "value"] } },
        { "name": "ui_screenshot", "description": "The app window as the person sees it (live mode only).",
          "inputSchema": { "type": "object", "properties": {} } }
    ])
}
