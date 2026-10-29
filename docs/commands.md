# Commands

Generated from the command registry by `ac commands --md`: do not edit by hand. A test
(`crates/ac-cli/tests/commands_doc.rs`) fails when it is stale. `ac commands <id>` prints
one command as JSON.

Every command runs through the session, from the UI, a script, a fixture, the CLI or MCP.
An undoable command makes one undo step, named by its label unless the command names it
for each run. A command that writes files writes outside the document: undo does not
take the files back. Kind "JSON" is free-form: its description gives the shape.

## ai

### `ai.accept`: Keep AI fill

undoable, the undo step is named for each run, icon `ai`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | your key before the gap |
| `to` | integer | yes |  | your key after the gap |
| `frames` | integer | yes |  | frames filled |
| `model` | text | yes |  | the model label, or plain interpolation |
| `mode` | one of ai \| plain | yes |  | the model or plain interpolation |
| `root_heads` | list of [x, y, z] | yes |  | the root head on every frame of the span (metres) |
| `notes` | list of text | yes |  | impossible poses the fill made and bent back to the possible limit, or left (empty: none) |

### `ai.clear`: Clear AI keys

undoable, icon `clear`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes | 0 to 10000000 | key before the span |
| `to` | integer | yes | 0 to 10000000 | key after the span |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `cleared` | integer | yes |  | AI keys removed |

### `ai.discard`: Discard AI preview

not undoable, icon `ai`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `discarded` | bool | yes |  | there was a preview |

### `ai.fill`: AI fill

undoable, the undo step is named for each run, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | your key before the gap (default: around the playhead) |
| `to` | integer |  | 0 to 10000000 | your key after the gap |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | your key before the gap |
| `to` | integer | yes |  | your key after the gap |
| `frames` | integer | yes |  | frames filled |
| `model` | text | yes |  | the model label |
| `notes` | list of text | yes |  | impossible poses the fill made and bent back to the possible limit, or left (empty: none) |

### `ai.fill_all`: AI fill all gaps

undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `refill` | bool |  |  | also fill again the gaps the model filled before (default: only empty gaps) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `filled` | JSON | yes |  | the gaps filled: [[from, to]] |
| `skipped` | JSON | yes |  | the gaps left: [{from, to, reason}] |
| `model` | text | yes |  | the model label |
| `notes` | list of text | yes |  | impossible poses the fills made and bent back to the possible limit, or left (each starts with its gap, from–to; empty: none) |

### `ai.gaps`: AI gaps

not undoable, icon `ai`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `gaps` | JSON | yes |  | every gap between your keys: [{from, to, frames, filled, refused}] (refused: why the model can't fill it, null: it can) |
| `at` | JSON | yes |  | the gap the playhead is in (null: none) |
| `max_gap` | integer | yes |  | most frames the model fills in one gap (null: no model loaded) |

### `ai.info`: AI model

not undoable.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `model` | JSON | yes |  | the model card (null: none loaded) |
| `settings` | JSON | yes |  | the fill settings |

### `ai.keep`: Keep AI keys

undoable, icon `keep`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes | 0 to 10000000 | first frame |
| `to` | integer | yes | 0 to 10000000 | last frame |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `kept` | integer | yes |  | AI keys made yours |

### `ai.preview`: Preview AI fill

not undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | your key before the gap (default: around the playhead) |
| `to` | integer |  | 0 to 10000000 | your key after the gap |
| `mode` | text |  |  | "ai" (the model, default) or "plain" (plain interpolation) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | your key before the gap |
| `to` | integer | yes |  | your key after the gap |
| `frames` | integer | yes |  | frames filled |
| `model` | text | yes |  | the model label, or plain interpolation |
| `mode` | one of ai \| plain | yes |  | the model or plain interpolation |
| `root_heads` | list of [x, y, z] | yes |  | the root head on every frame of the span (metres) |
| `notes` | list of text | yes |  | impossible poses the fill made and bent back to the possible limit, or left (empty: none) |

### `ai.settings`: AI fill settings

not undoable.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `style` | text |  |  | a style the model knows ("any": none) |
| `style_force` | number |  | 0 to 3 | style guidance: 0 none, 1 the style, more is stronger |
| `ai_force` | number |  | 0 to 1.5 | how much of the network's change to keep (0: plain interpolation) |
| `trajectory` | number |  | 0 to 1 | pull the root onto the keys' own path |
| `head` | number |  | 0 to 1 | keep the neck and head on the keys' spline |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `style` | text | yes |  | the style |
| `style_force` | number | yes |  | style guidance |
| `ai_force` | number | yes |  | how much of the network's change is kept |
| `trajectory` | number | yes |  | pull onto the keys' path |
| `head` | number | yes |  | neck and head on the keys' spline |

### `ai.toggle`: Switch AI / plain

undoable, shortcut `Shift+A`, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | your key before the gap (default: around the playhead) |
| `to` | integer |  | 0 to 10000000 | your key after the gap |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | your key before the gap |
| `to` | integer | yes |  | your key after the gap |
| `mode` | one of ai \| plain | yes |  | what the span is now |
| `cleared` | integer |  |  | plain: AI keys removed |
| `model` | text |  |  | ai: the model label |
| `notes` | list of text |  |  | ai: impossible poses the fill made and bent back, or left |

## anim

### `anim.block`: Block out animation

undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `poses` | list of text | yes |  | library poses to key, in order |
| `frames` | list of integers | yes |  | the frame of each pose, rising |
| `fill` | one of ai \| plain \| none |  |  | between the poses: the model (default), plain interpolation, or the keys' own |
| `style` | text |  |  | the AI style for the fills (default: the current setting) |
| `physics` | bool |  |  | turn the physics pass on or off |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `keys` | integer | yes |  | poses keyed |
| `fills` | JSON | yes |  | [{from, to, model}] |
| `physics` | bool | yes |  | the physics pass plays |
| `frames` | integer | yes |  | the animation length |

### `anim.steps`: Footsteps

undoable, icon `foot`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | your key the walk starts on (with `to`; default: the keys around the playhead) |
| `to` | integer |  | 0 to 10000000 | your key the walk ends on |
| `steps` | integer |  | 2 to 64 | footfalls, the last one bringing the feet together on `to` (default: between a step every 0.55 m and every 15 frames) |
| `lead` | one of L \| R |  |  | the foot that steps first (default R) |
| `fill` | one of cycle \| ai \| plain \| none |  |  | how it walks: the built-in Walk loop with planted feet (default, a key every 2 frames), or only step keys filled by the model, plain interpolation, or the keys' own curve |
| `style` | text |  |  | the AI style for the fills (default walk) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the walk's first key |
| `to` | integer | yes |  | its last key |
| `keys` | list of integers | yes |  | the keys set, in order |
| `contacts` | list of integers | yes |  | the footfalls between the two keys (a foot lands on each) |
| `steps` | integer | yes |  | footfalls |
| `stride` | number | yes |  | metres from one footfall to the next |
| `lead` | text | yes |  | the foot that steps first |
| `fills` | JSON | yes |  | [{from, to, model}] |
| `notes` | list of text | yes |  | what the user should know |

## assets

### `assets.reload`: Reload assets

not undoable, icon `restore`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `what` | list of text |  |  | any of limits, library, samples (default: all of them) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `reloaded` | list of text | yes |  | what was reloaded |
| `limits` | JSON |  |  | {bones}: joints with limits |
| `library` | JSON |  |  | {animations, poses, part_sets} |
| `samples` | JSON |  |  | {samples, recipes} |

## bone

### `bone.world`: Bone world transform

not undoable, icon `bone`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | the bone |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `position` | [x, y, z] | yes |  | head position in metres |
| `rotation` | [x, y, z, w] | yes |  | world rotation |

## clip

### `clip.contacts`: Foot contacts

not undoable, icon `foot`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `L` | list of bool | yes |  | per frame: the left foot is planted |
| `R` | list of bool | yes |  | per frame: the right foot is planted |

### `clip.critique`: Review animation

not undoable, icon `review`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | first frame (default 0) |
| `to` | integer |  | 0 to 10000000 | last frame (default: the last) |
| `motion` | one of shown \| keys |  |  | what plays (the physics pass when it is on; default) or the keys alone |
| `limit` | integer |  | 1 to 200 | most findings (default 20) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `findings` | JSON | yes |  | [{kind, severity: high\|medium\|low, score, frames: [a, b], message, command, params, batch: [{id, params}]}] worst first |
| `count` | integer | yes |  | findings returned |
| `metrics` | JSON | yes |  | the measurements (clip.metrics) |

### `clip.curves`: Joint curves

not undoable, icon `graph`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | a bone |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | JSON | yes |  | per frame, the curve values |
| `keys` | JSON | yes |  | [{frame, values}] |

### `clip.delete`: Delete animation

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `delete`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `closed` | text |  |  | the open animation was this one and is closed |

### `clip.duplicate`: Duplicate animation

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `duplicate`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text |  |  | a library id, <group>/<name> |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the copy |

### `clip.export`: Export .glb

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), shortcut `Ctrl+E`, icon `export`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the .glb to write |
| `ids` | list of text |  |  | library animations to export (default: the open one) |
| `profile` | one of full \| game \| body |  |  | which joints to keep |
| `bones` | list of text |  |  | exactly these joints instead of a profile |
| `drop` | list of text |  |  | joints to leave out as well |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bytes` | integer | yes |  | file size |
| `joints` | integer | yes |  | joints exported |
| `animations` | integer | yes |  | animations in the file |
| `frames` | list of integers | yes |  | frames per animation |
| `path` | text | yes |  | the file written |

### `clip.export_batch`: Export each to .glb

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `export`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `ids` | list of text | yes |  | the library animations to export, a file each |
| `folder` | text | yes |  | the folder the files go in (made if missing) |
| `profile` | one of full \| game \| body |  |  | which joints to keep |
| `bones` | list of text |  |  | exactly these joints instead of a profile |
| `drop` | list of text |  |  | joints to leave out as well |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `folder` | text | yes |  | the folder the files went in |
| `files` | JSON | yes |  | [{id, file, path, joints, frames}] written |
| `failed` | JSON | yes |  | [{id, error}] not written |

### `clip.fix_impossible`: Fix impossible poses

undoable, icon `graph`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | first frame (default 0) |
| `to` | integer |  | 0 to 10000000 | last frame (default: the last) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `keys` | integer | yes |  | keys of the edited layer between from and to that were checked |
| `fixed` | JSON | yes |  | [{frame, breaches: [{joint, component, value_deg, lo, hi, past_deg}]}]: keys bent back to the possible limit (the values before) |
| `added` | JSON | yes |  | [frame]: frames between keys where the curve swung past a limit, given a key with the pose bent back |
| `left` | integer | yes |  | frames still impossible after (a joint another layer sets); notes say which |
| `notes` | list of text | yes |  | what was left, and why |

### `clip.group`: Move animation to group

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `group`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |
| `group` | text |  |  | a group (folder) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `clip.import`: Import animation file

undoable, icon `import`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | a file path |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `length` | integer | yes |  | frames |

### `clip.loop`: Loop

undoable, icon `loop`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `on` | bool | yes |  | loop the animation |

### `clip.metrics`: Motion metrics

not undoable, icon `graph`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | first frame (default 0) |
| `to` | integer |  | 0 to 10000000 | last frame (default: the last) |
| `motion` | one of shown \| keys |  |  | what plays (the physics pass when it is on; default) or the keys alone |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | first frame measured |
| `to` | integer | yes |  | last frame measured |
| `frames` | integer | yes |  | frames measured |
| `motion` | one of shown \| keys | yes |  | the motion measured |
| `sliding` | JSON | yes |  | [{foot: L\|R, from, to, slide_cm}] per contact span: how far a planted foot slid |
| `sliding_max_cm` | number | yes |  | the worst slide |
| `penetration` | JSON | yes |  | {frames, spans: [[a, b]], max_cm, worst_frame, joint}: below the ground |
| `balance` | JSON | yes |  | {frames, spans, max_cm, worst_frame, moving_frames}: still frames with the centre of mass outside the feet |
| `limits` | JSON | yes |  | [{bone, frames, spans, max_deg, worst_frame}] past their joint limits, worst first (null: no limits loaded) |
| `keys` | JSON | yes |  | [{frame, dv, joint}]: at each key, the largest velocity change (m/s) and its joint |
| `pops` | integer | yes |  | keys whose velocity change is over 1 m/s |
| `notes` | list of text | yes |  | what was not checked, and why |

### `clip.move`: Move animation

undoable, icon `move`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `by` | [x, y, z] |  |  | slide, metres (-Y is the way the rig faces at rest) |
| `turn` | number |  | -360 to 360 | turn about the vertical, degrees (counter-clockwise from above) |
| `pivot` | [x, y, z] |  |  | the turn's centre, metres (z unused; default: the root on the frame shown) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `keys` | integer |  |  | keys moved |
| `notes` | list of text |  |  | what stayed where it was |

### `clip.new`: New animation

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), shortcut `Ctrl+N`, icon `new`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text |  |  | a name |
| `group` | text |  |  | a group (folder) |
| `fps` | number |  | 1 to 240 | frames per second |
| `pose` | text |  |  | start from this pose (library id) |
| `link` | text |  |  | play this animation underneath (library id) |
| `parts` | list of text |  |  | body parts (empty: the whole body) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | its library id (null: no library) |

### `clip.open`: Open animation

undoable, icon `open`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |
| `length` | integer | yes |  | frames |

### `clip.possible`: Impossible poses

not undoable, icon `graph`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | first frame (default 0) |
| `to` | integer |  | 0 to 10000000 | last frame (default: the last) |
| `motion` | one of shown \| keys |  |  | what plays (the physics pass when it is on; default) or the keys alone |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | first frame measured |
| `to` | integer | yes |  | last frame measured |
| `frames` | integer | yes |  | frames measured |
| `motion` | one of shown \| keys | yes |  | the motion measured |
| `level` | one of typical \| unusual \| impossible | yes |  | the worst over the frames: typical (inside the clinical limits), unusual (past them but realistic: athletic) or impossible |
| `message` | text | yes |  | what the level means |
| `impossible` | JSON | yes |  | {frames, spans: [[a, b]], max_deg, worst_frame}: frames with a joint past its possible limit |
| `breaches` | JSON | yes |  | per joint angle past its possible limit, worst first: [{joint, component, value_deg, lo, hi, past_deg, frames, spans, worst_frame}] (value and past at the worst frame) |
| `unusual` | JSON | yes |  | {frames, spans, max_deg, worst_frame}: frames past a clinical limit (any range pair and side) |
| `possible_note` | text | yes |  | where the possible limits come from |
| `notes` | list of text | yes |  | what was not checked, and why |

### `clip.range`: Joint range of motion

not undoable, icon `graph`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `pair` | one of hip_knee \| ankle_knee \| hip_swing \| shoulder_swing \| trunk_bend \| neck_bend |  |  | coupled joint pair (default: hip_knee) |
| `side` | one of L \| R |  |  | left or right side on sided pairs (default: L) |
| `from` | integer |  | 0 to 10000000 | first frame (default 0) |
| `to` | integer |  | 0 to 10000000 | last frame (default: the last) |
| `motion` | one of shown \| keys |  |  | what plays (the physics pass when it is on; default) or the keys alone |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `pair` | text | yes |  | the pair id |
| `label` | text | yes |  | the pair's readable name |
| `sided` | bool | yes |  | whether the pair has left and right sides |
| `side` | text | yes |  | "L" or "R" (null on trunk_bend and neck_bend) |
| `note` | text | yes |  | anatomical coupling note |
| `limits_note` | text | yes |  | which source each clinical limit comes from, and what joining between them is ours |
| `from` | integer | yes |  | first frame measured |
| `to` | integer | yes |  | last frame measured |
| `frames` | integer | yes |  | frames measured |
| `motion` | one of shown \| keys | yes |  | the motion measured |
| `x` | JSON | yes |  | {joint, component, label, hard: [lo, hi] or null} of the horizontal angle (degrees) |
| `y` | JSON | yes |  | {joint, component, label, hard: null} of the vertical angle |
| `hard` | JSON | yes |  | [{x, lo, hi}] clinical hard limit band (degrees) |
| `typical` | JSON | yes |  | [{x, lo, hi, count}] 1st-99th percentile mocap envelope (degrees) |
| `playhead` | JSON | yes |  | {frame, x, y, hard_lo, hard_hi, typical_lo, typical_hi, closeness, past_hard_deg, past_typical_deg} of the pose shown |
| `path` | JSON | yes |  | per frame: [{frame, x, y, hard_lo, hard_hi, typical_lo, typical_hi, closeness, typical_closeness, past_hard_deg, past_typical_deg}] |
| `max_closeness` | number | yes |  | highest closeness to the hard limit across the measured frames (1.0 = on the edge, > 1.0 = past it) |
| `outside_hard` | JSON | yes |  | {frames, spans: [[a, b]], max_deg, worst_frame} past the clinical hard limit |
| `outside_typical` | JSON | yes |  | {frames, spans: [[a, b]], max_deg, worst_frame} outside the mocap typical band |
| `summary` | JSON | yes |  | every pair and side: [{pair, label, side, max_closeness, peak_frame, outside_hard_frames, outside_hard_spans, max_past_hard_deg, worst_frame}] |
| `sources` | JSON | yes |  | [{id, title, url, note}] clinical and mocap reference sources |
| `notes` | list of text | yes |  | what was not checked, and why |

### `clip.rename`: Rename animation

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `rename`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `to` | text | yes |  | the new name |
| `id` | text |  |  | a library animation to rename (default: the open one) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `clip.sample`: Sample clip

not undoable.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | number |  | 0 to 10000000 | first frame |
| `to` | number |  | 0 to 10000000 | last frame |
| `step` | number |  | 0.01 to 10000 | frames between samples |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | list of number | yes |  | the frames sampled |
| `rotations` | JSON | yes |  | per frame, one [x, y, z, w] per bone |
| `roots` | list of [x, y, z] | yes |  | per frame, the root offset (hip heights) |

### `clip.save`: Save

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), shortcut `Ctrl+S`, icon `save`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `clip.save_as`: Save as

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `save-as`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text |  |  | a name |
| `group` | text |  |  | a group (folder) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `clip.sheet`: Export contact sheet

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `sheet`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the .png to write |
| `frames` | list of integers |  |  | the frames to show (1 to 64) |
| `count` | integer |  | 1 to 64 | else this many, spread over the animation (default 6) |
| `view` | one of front \| back \| left \| right \| side \| top \| three-quarter |  |  | the camera: front, back, left, right, side (default), top or three-quarter |
| `width` | integer |  | 16 to 2048 | a cell's pixels across (default 320) |
| `height` | integer |  | 16 to 2048 | a cell's pixels down (default 400) |
| `cols` | integer |  | 1 to 64 | cells to a row (default: one row) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file written |
| `frames` | list of number | yes |  | the frames shown |
| `width` | integer | yes |  | the sheet's pixels across |
| `height` | integer | yes |  | the sheet's pixels down |
| `note` | text |  |  | how the pictures were made, when it matters (the software adapter) |

### `clip.video`: Export preview video

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `video`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the .mp4 (needs ffmpeg) or .gif to write |
| `from` | number |  | 0 to 1000000 | the first frame (default 0) |
| `to` | number |  | 0 to 1000000 | the last frame (default the animation's last) |
| `fps` | number |  | 1 to 120 | frames per second (default the animation's) |
| `view` | one of front \| back \| left \| right \| side \| top \| three-quarter |  |  | the camera: front, back, left, right, side (default), top or three-quarter |
| `width` | integer |  | 16 to 4096 | pixels across |
| `height` | integer |  | 16 to 4096 | pixels down |
| `fixed` | bool |  |  | one camera that sees the whole shot, so travel shows (default: it follows the body) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file written |
| `format` | one of mp4 \| gif | yes |  | the file's format |
| `frames` | integer | yes |  | frames written |
| `fps` | number | yes |  | frames per second |
| `width` | integer | yes |  | pixels across |
| `height` | integer | yes |  | pixels down |
| `from` | number | yes |  | the first frame |
| `to` | number | yes |  | the last frame |
| `note` | text |  |  | how the pictures were made, when it matters (the software adapter) |

### `clip.write`: Write animation file

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `export`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | a file path |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file written |

## edit

### `edit.batch`: Run commands

undoable, the undo step is named for each run, icon `batch`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `commands` | JSON | yes |  | [{id, params}, ...] run in order (at most 1000; not edit.* or session.*) |
| `atomic` | bool |  |  | true (default): a refusal puts everything back; false: keep the steps before it |
| `label` | text |  |  | the undo step's name |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `results` | JSON | yes |  | each step's result, in order |
| `done` | integer | yes |  | steps that ran |
| `label` | text | yes |  | the undo step's name |
| `failed` | JSON |  |  | atomic false only: {step, id, error} of the step that was refused |

### `edit.begin`: Begin gesture

not undoable, the undo step is named for each run.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `label` | text |  |  | the undo step's name |

### `edit.checkpoint`: Name this point

not undoable, icon `flag`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text | yes |  | the checkpoint's name (an existing one moves here) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text | yes |  | the checkpoint |
| `index` | integer | yes |  | its history index |

### `edit.dry_run`: Try a command

not undoable, icon `preview`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the command to try |
| `params` | JSON |  |  | its parameters (an object) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the command tried |
| `result` | JSON | yes |  | what it would return |
| `changed` | JSON | yes |  | what it would change (the change summary) |
| `undo_label` | text | yes |  | the undo step it would make (null: none) |

### `edit.end`: End gesture

not undoable.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `changed` | bool | yes |  | whether the gesture made an undo step |

### `edit.history`: Undo history

not undoable, icon `history`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `steps` | JSON | yes |  | [{index, label, command, done}] oldest first; done false: undone (redo brings it back) |
| `current` | integer | yes |  | the index of the document shown (0: before the first step) |
| `first` | integer | yes |  | the oldest index still reachable |
| `checkpoints` | JSON | yes |  | [{name, index}] |
| `gesture` | text |  |  | the open gesture's label |

### `edit.redo`: Redo

not undoable, shortcut `Ctrl+Y`, icon `redo`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `changed` | bool | yes |  | whether a step was undone or redone |
| `note` | text |  |  | why nothing changed |
| `command` | text |  |  | the command that made the step |
| `label` | text |  |  | the step's name |

### `edit.revert_to`: Go back to

not undoable, icon `history`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text |  |  | a checkpoint |
| `index` | integer |  | 0 to 1000000000 | or a history index (edit.history) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the index before |
| `to` | integer | yes |  | the index now |
| `undone` | integer | yes |  | steps undone |
| `redone` | integer | yes |  | steps redone |

### `edit.undo`: Undo

not undoable, shortcut `Ctrl+Z`, icon `undo`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `changed` | bool | yes |  | whether a step was undone or redone |
| `note` | text |  |  | why nothing changed |
| `command` | text |  |  | the command that made the step |
| `label` | text |  |  | the step's name |

## editor

### `editor.state`: Editor state

not undoable.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number | yes |  | the playhead |
| `length` | integer | yes |  | frames |
| `fps` | number | yes |  | frames per second |
| `loop` | bool | yes |  | the animation loops |
| `name` | text | yes |  | its name |
| `group` | text | yes |  | its group |
| `id` | text | yes |  | its library id (null: not saved) |
| `unsaved` | bool | yes |  | changes the library lacks |
| `keys` | JSON | yes |  | [{frame, interp, source, model, part?, rows?}] of the edited layer |
| `layers` | JSON | yes |  | [{name, keys, muted, weight, link}] |
| `edit_layer` | integer | yes |  | the layer keys go on |
| `locked` | list of text | yes |  | locked parts |
| `pins` | list of text | yes |  | pinned bones |
| `bands` | JSON | yes |  | AI bands [{from, to, model}] |
| `clipboard` | text | yes |  | the parts copied (null: nothing) |
| `part_presets` | JSON | yes |  | the body part presets |
| `part_rows` | JSON | yes |  | the dope sheet rows |
| `physics` | bool | yes |  | the physics pass plays |
| `preview` | JSON | yes |  | {from, to, model, plain, live} of the AI preview (null: none) |
| `markers` | JSON | yes |  | timeline markers [{frame, name}] by frame |

## feet

### `feet.lock`: Lock feet

undoable, icon `foot`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames changed |

### `feet.plant`: Plant foot

undoable, icon `foot`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `foot` | one of L \| R | yes |  | which foot |
| `from` | integer | yes | 0 to 10000000 | first frame |
| `to` | integer | yes | 0 to 10000000 | last frame |
| `on` | bool |  |  | plant (default) or take the plant's in-between keys out |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames changed (planted or freed) |
| `notes` | list of text | yes |  | what stays as it was |

## frame

### `frame.set`: Go to frame

not undoable, icon `playhead`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number | yes | 0 to 10000000 | a frame (fractional while scrubbing) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number | yes |  | the playhead |

## key

### `key.channel`: Set key channel

undoable, icon `curve`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | a bone |
| `frame` | integer | yes | 0 to 10000000 | a key's frame |
| `to` | integer |  | 0 to 10000000 | move the joint's key to this frame |
| `axis` | integer | yes | 0 to 5 | 0-2: angle about X/Y/Z (degrees); 3-5: hips position |
| `value` | number | yes | -1000000 to 1000000 | the new value |

### `key.copy`: Copy pose to frame

undoable, icon `copy-key`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes | 0 to 10000000 | the frame to copy |
| `to` | integer | yes | 0 to 10000000 | the frame to key |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `notes` | list of text | yes |  | what else changed (AI spans refilled) |

### `key.delete`: Delete key

undoable, shortcut `Shift+K`, icon `unkey`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `parts` | list of text |  |  | body parts (empty: the whole body) |

### `key.hold`: Hold parts

undoable, icon `hold`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes | 0 to 10000000 | first frame |
| `to` | integer | yes | 0 to 10000000 | last frame |
| `parts` | list of text |  |  | body parts (empty: the whole body) |

### `key.interp`: Set interpolation

undoable, icon `curve`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `interp` | one of step \| linear \| smooth \| ease_in \| ease_out \| ease | yes |  | interpolation into the next key |

### `key.move`: Move key

undoable, the undo step is named for each run, icon `move-key`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes | 0 to 10000000 | the key's frame |
| `to` | integer | yes | 0 to 10000000 | its new frame |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `notes` | list of text | yes |  | what else changed (AI spans refilled, keys merged) |

### `key.move_parts`: Move part keys

undoable, icon `move-key`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes | 0 to 10000000 | the key's frame |
| `to` | integer | yes | 0 to 10000000 | where those parts' keys go |
| `parts` | list of text | yes |  | the body parts to move |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bones` | integer |  |  | bones moved |

### `key.remove_parts`: Remove parts from key

undoable, icon `unkey`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes | 0 to 10000000 | a key's frame |
| `parts` | list of text |  |  | body parts (empty: the whole body) |

### `key.set`: Set key

undoable, shortcut `K`, icon `key`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `interp` | one of step \| linear \| smooth \| ease_in \| ease_out \| ease |  |  | interpolation into the next key |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes |  | the frame keyed |
| `notes` | list of text |  |  | spans next to the key that were left as they were |

## keys

### `keys.copy`: Copy keys

not undoable, icon `copy-key`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | list of integers | yes |  | the keys' frames |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `copied` | integer |  |  | keys copied |

### `keys.delete`: Delete keys

undoable, icon `unkey`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | list of integers | yes |  | the keys' frames |

### `keys.paste`: Paste keys

undoable, icon `paste`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `at` | integer |  | 0 to 10000000 | where the first key goes (default: the playhead) |
| `mirror` | bool |  |  | swap left and right |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | list of integers |  |  | frames pasted |
| `replaced` | list of integers |  |  | frames whose keys were replaced |

### `keys.retime`: Retime keys

undoable, icon `retime`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `mode` | one of insert \| delete \| scale | yes |  | insert or delete frames at `at` (everything after slides), or scale keys about `pivot` |
| `at` | integer |  | 0 to 10000000 | insert/delete: the first frame inserted or deleted |
| `count` | integer |  | 1 to 1000000 | insert/delete: how many frames |
| `frames` | list of integers |  |  | scale: the keys to scale (default: every key of the edited layer) |
| `pivot` | number |  | 0 to 10000000 | scale: the frame that stays (default: the first key) |
| `factor` | number |  | 0.01 to 100 | scale: 2 doubles the spacing, 0.5 halves it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `keys` | list of integers |  |  | the edited layer's key frames after |
| `moved` | integer |  |  | keys that changed frame |
| `removed` | list of text |  |  | keys deleted with their frames (layer: frame) |
| `notes` | list of text |  |  | fills to redo, markers removed |

## layer

### `layer.add`: Add layer

undoable, icon `layer-add`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `source` | one of keys \| link |  |  | a layer of keys, or another animation |
| `link` | text |  |  | the animation a link layer plays (library id) |
| `name` | text |  |  | the layer's name |
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `mode` | one of override \| additive |  |  | replace what is below, or add to it |
| `offset` | number |  | -100000 to 100000 | frames a link starts later |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.bake`: Bake layers down

undoable, icon `bake`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |
| `baked` | integer | yes |  | keys written |

### `layer.delete`: Delete layer

undoable, icon `delete`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.edit`: Edit layer

undoable, icon `layer`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.move`: Move layer

undoable, icon `layer`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base) |
| `to` | integer | yes |  | its new place |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.plan`: Plan travel again

undoable, icon `travel`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.set`: Layer settings

undoable, icon `layer`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base) |
| `name` | text |  |  | the layer's name |
| `mode` | one of override \| additive |  |  | replace what is below, or add to it |
| `weight` | number |  | 0 to 1 | how much it shows, 0..1 |
| `offset` | number |  | -100000 to 100000 | frames a link starts later |
| `speed` | number |  | 0.01 to 100 | playback speed of a link |
| `loop` | bool |  |  | a link repeats |
| `muted` | bool |  |  | off |
| `gait` | one of auto \| walk \| run \| crouch |  |  | which Locomotion clips to walk with |
| `fade` | integer |  | 0 to 600 | frames the layer fades in and out over |
| `start` | number |  | -1000000 to 1000000 | first frame of its range |
| `end` | number |  | -1000000 to 1000000 | last frame of its range |
| `whole` | bool |  |  | true: no range, the whole animation |
| `waypoints` | list of [x, y] |  |  | a travel's path, [[x, y], ...] metres |
| `moves` | {name: [dx, dy]} |  |  | footsteps moved by hand, {"L0": [dx, dy]} |
| `parts` | list of text |  |  | body parts (empty: the whole body) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.solo`: Solo layer

undoable, icon `layer`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the layer (0: the base alone) |
| `on` | bool |  |  | true (default): mute every other layer above the base; false: unmute them all |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `layer.travel`: Travel between keys

undoable, icon `travel`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 1000000 | first frame (with `to`; default: the keys around the playhead) |
| `to` | integer |  | 0 to 1000000 | last frame |
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `fade` | integer |  | 0 to 600 | frames the layer fades in and out over |
| `gait` | one of auto \| walk \| run \| crouch |  |  | which Locomotion clips to walk with |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |
| `i` | integer | yes |  | the travel layer |

### `layer.travel_preview`: Preview travel

not undoable, icon `travel`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 1000000 | first frame (with `to`; default: the keys around the playhead) |
| `to` | integer |  | 0 to 1000000 | last frame |
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `fade` | integer |  | 0 to 600 | frames the layer fades in and out over |
| `gait` | one of auto \| walk \| run \| crouch |  |  | which Locomotion clips to walk with |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the travel's first frame |
| `to` | integer | yes |  | its last frame |
| `gait` | text | yes |  | the gait it walks with |
| `path` | list of [x, y, z] | yes |  | where the hips go, one [x, y, z] a frame (metres) |
| `steps` | JSON | yes |  | its footsteps, as the travel layer keeps them |

## layers

### `layers.list`: Layers

not undoable, icon `layers`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

## library

### `library.add_file`: Import file

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `import`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | one animation (*.clip.json) or pose (*.pose.json) file |
| `group` | text |  |  | the group it goes in (default Imported) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `kind` | one of animation \| pose | yes |  | what the file was |
| `id` | text | yes |  | its library id |
| `already` | bool | yes |  | the same item was in the library already (nothing written) |

### `library.import`: Import library

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `import`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | text | yes |  | a folder of *.clip.json and *.pose.json files (subfolders are groups) |
| `group` | text |  |  | the group for files directly in the folder (default: the folder's name) |
| `overwrite` | bool |  |  | replace items already in the library (your changes to them are lost); built-ins are always kept |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `group` | text | yes |  | the group they went into |
| `animations` | JSON | yes |  | {copied, kept} |
| `poses` | JSON | yes |  | {copied, kept} |
| `skipped` | JSON | yes |  | [{file, error}] |
| `cut` | integer |  |  | files left out past the limit |

### `library.install`: Install built-ins

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `restore`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | text | yes |  | the built-in library folder |
| `overwrite` | bool |  |  | replace your edited copies too |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `copied` | integer | yes |  | files copied |

### `library.list`: Library

not undoable, icon `library`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `animations` | JSON | yes |  | [{id, name, group, current, builtin, edited, ...}] |
| `poses` | JSON | yes |  | [{id, name, group, part, builtin, edited, ...}] |
| `groups` | list of text | yes |  | every group |
| `custom_parts` | JSON | yes |  | your part sets |

### `library.samples`: Import samples

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `import`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `overwrite` | bool |  |  | replace samples already in the library (your changes to them are lost) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `copied` | list of text | yes |  | samples copied |
| `kept` | list of text | yes |  | samples already there, kept |
| `group` | text | yes |  | their group |

## marker

### `marker.add`: Add marker

undoable, icon `marker`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes | 0 to 10000000 | the marker's frame |
| `name` | text |  |  | the marker's name (one line) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `markers` | JSON | yes |  | every marker after the change: [{frame, name}] by frame |

### `marker.delete`: Delete marker

undoable, icon `marker`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes | 0 to 10000000 | the marker's frame |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `markers` | JSON | yes |  | every marker after the change: [{frame, name}] by frame |

### `marker.move`: Move marker

undoable, icon `marker`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes | 0 to 10000000 | the marker's frame |
| `to` | integer | yes | 0 to 10000000 | its new frame |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `markers` | JSON | yes |  | every marker after the change: [{frame, name}] by frame |

### `marker.rename`: Rename marker

undoable, icon `marker`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes | 0 to 10000000 | the marker's frame |
| `name` | text | yes |  | the marker's name (one line) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `markers` | JSON | yes |  | every marker after the change: [{frame, name}] by frame |

## onion

### `onion.frames`: Onion-skin frames

not undoable, icon `onion`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number |  | 0 to 10000000 | around this frame (default: the playhead) |
| `count` | integer |  | 1 to 5 | ghosts on each side (default 2) |
| `step` | integer |  | 1 to 1000 | frames between ghosts (default 2) |
| `mode` | one of frames \| keys |  |  | every `step` frames, or the nearest keys |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `before` | list of integers | yes |  | frames before, nearest first |
| `after` | list of integers | yes |  | frames after, nearest first |
| `note` | text |  |  | why there is nothing to ghost |

## parts

### `parts.delete`: Delete part set

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `delete`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text | yes |  | a name |

### `parts.lock`: Lock parts

undoable, icon `lock`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `parts` | list of text |  |  | body parts (empty: the whole body) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `locked` | list of text | yes |  | the locked parts |

### `parts.save`: Save part set

undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `parts`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text | yes |  | a name |
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `joint` | bone |  |  | a joint with everything below it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text | yes |  | the part set |
| `joints` | integer | yes |  | joints in it |

## physics

### `physics.overlay`: Physics overlay

not undoable, icon `physics`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number |  | 0 to 100000 | the frame to look at (default: the current one) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes |  | the whole frame shown |
| `physics` | bool | yes |  | the pass plays |
| `com` | [x, y, z] | yes |  | centre of mass (metres) |
| `support` | list of [x, y, z] | yes |  | the support outline on the ground (null: off the ground) |
| `inside` | bool | yes |  | the centre of mass is over the support (null: off the ground) |
| `contacts` | JSON | yes |  | [{point, pinned, foot}] |
| `pinned` | integer | yes |  | contacts pinned |
| `flight` | JSON | yes |  | {from, to, arc} (null: on the ground) |
| `keyed_moved` | number | yes |  | metres the pass moved the root head |

### `physics.set`: Physics pass

undoable, icon `physics`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `on` | bool | yes |  | play and export through the physics pass |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `physics.solve`: Physics pass result

not undoable, icon `physics`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number |  | 0 to 100000 | the frame whose root to report |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames solved |
| `flights` | JSON | yes |  | [[take-off, landing]] |
| `lifted` | integer | yes |  | frames lifted out of the ground |
| `slid` | integer | yes |  | frames the fulcrum rule moved |
| `balanced` | list of integers | yes |  | frames balanced |
| `lifted_frames` | list of integers | yes |  | the frames lifted out of the ground |
| `slid_frames` | list of integers | yes |  | the frames where a sliding foot was held |
| `problems` | JSON | yes |  | runs of frames the pass fixed, to jump to: [{kind, from, to, label}] (kind: penetration, sliding or unbalanced) |
| `keyed` | [x, y, z] |  |  | with `frame`: the keyed root offset |
| `root` | [x, y, z] |  |  | with `frame`: the solved root offset |
| `keyed_balance` | JSON |  |  | with `frame`: {com, inside} keyed |
| `balance` | JSON |  |  | with `frame`: {com, inside} solved |

## pin

### `pin.set`: Pin joint

undoable, icon `pin`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | a bone |
| `on` | bool |  |  | pin (true) or unpin |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `pins` | list of text | yes |  | the pinned bones |

## pins

### `pins.clear`: Clear pins

undoable, icon `unpin`.

No inputs.

## pose

### `pose.apply`: Apply pose

undoable, icon `pose`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `key` | bool |  |  | key it (default true) |

### `pose.copy`: Copy pose

not undoable, shortcut `Ctrl+C`, icon `copy`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `parts` | list of text |  |  | body parts (empty: the whole body) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `copied` | text | yes |  | the parts copied |

### `pose.delete`: Delete pose

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `delete`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |

### `pose.drag`: Drag joint

undoable, interactive tool, icon `drag`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | a bone |
| `to` | [x, y, z] |  |  | where the bone's head goes (metres) |
| `by` | [x, y, z] |  |  | or: an offset from where it is now (metres) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `position` | [x, y, z] | yes |  | the bone's head after the drag (metres) |

### `pose.get`: Get pose

not undoable, icon `pose`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone |  |  | just this bone: its local rotation |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `rotations` | list of [x, y, z, w] |  |  | one [x, y, z, w] per bone |
| `root` | [x, y, z] | yes |  | root offset in hip heights |
| `bone` | bone |  |  | with `bone`: the bone asked for |
| `rotation` | [x, y, z, w] |  |  | with `bone`: its local rotation |

### `pose.group`: Move pose to group

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `group`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |
| `group` | text |  |  | a group (folder) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `pose.mirror`: Mirror pose

undoable, icon `mirror`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `mode` | one of flip \| l2r \| r2l |  |  | flip, or copy one side onto the other |

### `pose.move`: Move body on this frame

undoable, icon `move`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `by` | [x, y, z] |  |  | slide, metres (-Y is the way the rig faces at rest) |
| `turn` | number |  | -360 to 360 | turn about the vertical, degrees (counter-clockwise from above) |
| `pivot` | [x, y, z] |  |  | the turn's centre, metres (z unused; default: the root on the frame shown) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer |  |  | the frame moved |
| `keyed` | bool |  |  | its key moved too (false: no key there, the move is an edit not keyed yet) |
| `notes` | list of text |  |  | what the user should know |

### `pose.paste`: Paste pose

undoable, shortcut `Ctrl+V`, icon `paste`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `parts` | list of text |  |  | body parts (empty: the whole body) |
| `mirror` | bool |  |  | paste mirrored |

### `pose.query`: Measure pose

not undoable, icon `ruler`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number |  | 0 to 10000000 | a frame of the motion shown (default: the pose shown now) |
| `bones` | list of text |  |  | bones to report |
| `pairs` | JSON |  |  | [[bone, bone] or [bone, "floor"], ...]: distances |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | number | yes |  | the frame asked for (null: the pose shown) |
| `bones` | JSON | yes |  | [{bone, position, rotation, local, height, past_limits_deg}] (metres; world and local [x, y, z, w]) |
| `pairs` | JSON | yes |  | [{a, b, distance, delta}] (metres) |
| `limbs` | JSON | yes |  | [{limb, distance, length, reach, straight, bend_deg}] per arm and leg |
| `outside_limits` | JSON | yes |  | [{bone, past_deg}] worst first (null: no limits loaded) |
| `notes` | list of text | yes |  | what was not checked |

### `pose.reach`: Reach (full body)

undoable, interactive tool, icon `reach`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | a bone |
| `to` | [x, y, z] |  |  | where the bone's head goes (metres) |
| `by` | [x, y, z] |  |  | or: an offset from where it is now (metres) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `position` | [x, y, z] | yes |  | the bone's head after the solve (metres) |
| `error` | number | yes |  | metres left to the target |
| `reached` | bool | yes |  | within 0.1 mm |

### `pose.rename`: Rename pose

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `rename`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |
| `to` | text | yes |  | the new name |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `pose.reset`: Reset pose

undoable, icon `reset`.

No inputs.

### `pose.rotate`: Rotate joint

undoable, interactive tool, icon `rotate`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | a bone |
| `axis` | [x, y, z] | yes |  | world axis |
| `angle` | number | yes | -100 to 100 | radians |

### `pose.save`: Save pose

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `save-pose`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text | yes |  | a name |
| `group` | text |  |  | a group (folder) |
| `parts` | list of text |  |  | body parts (empty: the whole body) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id |

### `pose.set_bone`: Set bone rotation

undoable, icon `rotate`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | the bone to turn |
| `rotation` | [x, y, z, w] | yes |  | [x, y, z, w] relative to the parent |

### `pose.set_root`: Set root offset

undoable, icon `move`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `root` | [x, y, z] | yes |  | offset in hip heights |

### `pose.spread_bend`: Spread bend into the back

undoable, icon `spine`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `degrees` | number | yes |  | the lean moved into the back |

## recovery

### `recovery.clear`: Discard unsaved work

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `delete`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `file` | text |  |  | a recovery file's name, as recovery.list gives it |
| `all` | bool |  |  | every recovery file (default: the open animation's) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `removed` | list of text | yes |  | the files removed |

### `recovery.list`: Unsaved work

not undoable, icon `restore`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `items` | JSON | yes |  | [{file, id, name, keys, library, offer, error?}] (library: missing, same, older or newer) |
| `offered` | integer | yes |  | files worth restoring |

### `recovery.restore`: Restore unsaved work

undoable, icon `restore`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `file` | text | yes |  | a recovery file's name, as recovery.list gives it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id it keeps (null: opened as a new animation) |
| `unsaved` | bool | yes |  | changes the library lacks |
| `length` | integer | yes |  | frames |

### `recovery.write`: Keep unsaved work

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `save`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `file` | text | yes |  | the recovery file's name |
| `id` | text | yes |  | the animation's library id (null: never saved) |

## rig

### `rig.info`: Rig info

not undoable, icon `rig`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bones` | integer | yes |  | number of bones |
| `names` | list of text | yes |  | bone names, parents first |
| `hip_height` | number | yes |  | metres |

## session

### `session.info`: Session info

not undoable, icon `info`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `product` | text | yes |  | the product's name |
| `version` | text | yes |  | its version |
| `seed` | integer | yes |  | the random seed (null: nothing draws random numbers) |
| `deterministic` | bool | yes |  | same commands, same results (byte for byte) |
| `rig_hash` | text | yes |  | FNV-1a of the rig (names, parents, heads) |
| `limits_hash` | text | yes |  | FNV-1a of the joint limits (null: none loaded) |
| `model` | JSON | yes |  | {tag, hash} of the in-betweener (null: none loaded; hash null: it gives none) |
| `commands` | integer | yes |  | commands registered |
| `history` | integer | yes |  | the history index shown |
| `check_outputs` | bool | yes |  | results are checked against their declared outputs |

### `session.reset`: Reset session

not undoable, icon `reset`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `reset` | bool | yes |  | a fresh document; undo history cleared |
| `dropped` | integer | yes |  | undo and redo steps cleared |

## surface

### `surface.add`: Add box

undoable, icon `box`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `name` | text |  |  | the box's name |
| `width` | number |  | 0.05 to 100 | metres across the body |
| `depth` | number |  | 0.05 to 100 | metres along the facing |
| `top` | number |  | 0 to 100 | height of its top (m) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `surface.delete`: Delete box

undoable, icon `delete`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the box |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

### `surface.set`: Box settings

undoable, icon `box`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `i` | integer | yes |  | the box |
| `name` | text |  |  | the box's name |
| `top` | number |  | 0 to 100 | height of its top (m) |
| `move` | [x, y, z] |  |  | nudge [dx, dy, 0] metres |
| `size` | [x, y, z] |  |  | [width, depth, 0] metres around its centre |
| `centre` | [x, y, z] |  |  | put its middle at [x, y, 0] metres |
| `turn` | number |  | -360 to 360 | how far it is turned about its centre (degrees) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `layers` | JSON | yes |  | [{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}] |
| `edit` | integer | yes |  | the layer keys go on |
| `surfaces` | JSON | yes |  | [{name, min, max, top}] |
| `physics` | bool | yes |  | the physics pass plays |

## text

### `text.fill`: Fill gap from text

undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `prompt` | text | yes |  | what the body does, in plain words ("A person waves hello with their right hand.") |
| `from` | integer |  | 0 to 10000000 | your key before the gap (default: around the playhead) |
| `to` | integer |  | 0 to 10000000 | your key after the gap |
| `seed` | integer |  | 0 to 1000000000 | another seed makes another take of the same prompt (default 1) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the motion's first frame (a key of yours) |
| `to` | integer | yes |  | its last frame (a key of yours) |
| `frames` | integer | yes |  | AI keys written between them |
| `model` | text | yes |  | the keys' model tag: "text" and the prompt |
| `notes` | list of text | yes |  | what to know about the result |

### `text.generate`: Generate motion from text

undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `prompt` | text | yes |  | what the body does, in plain words ("A person waves hello with their right hand.") |
| `seconds` | number |  | 0.1 to 9.9 | how long (default 4) |
| `seed` | integer |  | 0 to 1000000000 | another seed makes another take of the same prompt (default 1) |
| `at` | integer |  | 0 to 10000000 | the frame it starts on (default the playhead); it starts from the pose there when the animation has keys |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the motion's first frame (a key of yours) |
| `to` | integer | yes |  | its last frame (a key of yours) |
| `frames` | integer | yes |  | AI keys written between them |
| `model` | text | yes |  | the keys' model tag: "text" and the prompt |
| `notes` | list of text | yes |  | what to know about the result |

### `text.helper`: Motion from text: helper

not undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `action` | one of status \| load \| stop |  |  | status (default) says what is installed and running; load starts the helper and loads the model (a minute or more); stop ends it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `installed` | bool | yes |  | the helper is installed |
| `dir` | text | yes |  | where it is (or would be) installed |
| `running` | bool | yes |  | its process is up |
| `loaded` | bool | yes |  | the model is in memory |
| `busy` | bool | yes |  | it is making motion now |
| `model` | text | yes |  | the model installed |
| `note` | text |  |  | what is missing and how to install it |

### `text.install`: Motion from text: install

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `action` | one of check \| start \| stop |  |  | check (default) says what the install needs and what it is doing; start runs the setup (about 22 GB downloaded, an hour or more; the window stays usable); stop ends it, and start carries on from there |
| `repair` | bool |  |  | start even when it is installed: the setup updates or repairs it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `state` | one of not installed \| installing \| stopping \| installed \| failed \| stopped | yes |  | what the install is doing |
| `dir` | text | yes |  | where it is (or would be) installed |
| `log` | text |  |  | the setup's log |
| `step` | integer |  |  | the setup's step working now (or where it ended), of steps |
| `steps` | integer |  |  | how many steps the setup has |
| `label` | text |  |  | what that step does |
| `last` | text |  |  | the last line the setup printed (a download's progress) |
| `seconds` | integer |  |  | how long the setup started here has run |
| `error` | text |  |  | why it failed |
| `requirements` | JSON |  |  | when it is not installed or running: what it needs, a list of {name, ok, required, note} |
| `ready` | bool |  |  | everything required is there: start can run |

## trail

### `trail.points`: Motion trail

not undoable, icon `trail`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bone` | bone | yes |  | the joint to trace |
| `from` | integer |  | 0 to 10000000 | first frame (default 0) |
| `to` | integer |  | 0 to 10000000 | last frame (default: the clip's last) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | list of integers | yes |  | every frame from `from` to `to` |
| `points` | list of [x, y, z] | yes |  | one [x, y, z] a frame: the joint's head (metres) |
| `keys` | list of integers | yes |  | frames in the range with a key that sets the joint |
