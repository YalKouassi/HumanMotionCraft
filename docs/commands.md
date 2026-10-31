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
| `anchors` | one of auto \| none |  |  | a gap over the model's limit: "auto" (default) places anchor poses from the library inside it and fills each span between them; "none" refuses it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | your key before the gap |
| `to` | integer | yes |  | your key after the gap |
| `frames` | integer | yes |  | frames filled |
| `model` | text | yes |  | the model label |
| `notes` | list of text | yes |  | impossible poses the fill made and bent back to the possible limit, or left, and library animations left out of the anchor poses (empty: none) |
| `anchors` | JSON | yes |  | a gap over the model's limit: the anchor poses placed inside it, [{frame, id, source_frame}] (the library animation and its frame the pose is); empty: the model took the gap in one |
| `anchor_segment` | JSON | yes |  | the library segment the anchors came from: {id, from, to, speed, match_deg} (its frames matched to the two keys, library frames per animation frame, and the mean angle between its end poses and the keys); null: the model took the gap in one |
| `diagnostics` | JSON | yes |  | measured on the keys the fill wrote: {from, to, sliding: [{foot, from, to, slide_cm}] per contact span, sliding_max_cm, path: {max_m, frame} (the root from the keys' own path), seams: [{frame, dv (m/s), joint, dw (rad/s), bone}] at the two keys and at each anchor pose between them (null: no frame beyond it), bent: [{joint, component, past_deg, worst_frame, spans}] the joints bent back inside the possible limits, flags: [{kind: sliding\|path\|pop\|spin\|bent, from, to, text}] what is past a line, notes: what could not be measured} |

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
| `anchors` | JSON | yes |  | the anchor poses placed in gaps over the model's limit: [{from, to, frame, id, source_frame}] (empty: none needed) |
| `passes` | JSON | yes |  | the gaps filled several at once, with a model trained with several keys in a window (ai.info's multi_key): one list of keys per pass, [[from, key, ..., to]] (empty: every gap on its own) |
| `diagnostics` | JSON | yes |  | one per gap filled, in order, as ai.fill's: [{from, to, sliding, sliding_max_cm, path, seams, bent, flags, notes}] |

### `ai.gaps`: AI gaps

not undoable, icon `ai`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `gaps` | JSON | yes |  | every gap between your keys: [{from, to, frames, filled, refused, anchors}] (refused: why the model can't fill it, null: it can; anchors: over the model's limit, how many anchor poses from the library split it, null: none needed) |
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
| `preset` | text |  |  | a named preset: "preserve" \| "natural" \| "follow_style" |
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
| `anchors` | JSON |  |  | ai: the anchor poses placed in a gap over the model's limit, as ai.fill's (empty: none) |
| `diagnostics` | JSON |  |  | ai: why the fill might look wrong, measured, as ai.fill's |

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
| `findings` | JSON | yes |  | [{kind, severity: high\|medium\|low, score, frames: [a, b], message, command, params, batch: [{id, params}]}] worst first; a batch is offered only if, tried on a copy, it helped (none: empty, and command is null) |
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
| `root` | one of hips \| ground \| in_place |  |  | root motion bone: hips (pelvis only), ground (bone on the floor), in_place (ground bone with movement removed) |
| `target` | text |  |  | export onto your own character: its .glb (the mapping is guessed) or a mapping file saved by retarget.save; the file is written with the animation on the character's own nodes |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `bytes` | integer | yes |  | file size |
| `joints` | integer | yes |  | joints exported (with a target: nodes driven) |
| `animations` | integer | yes |  | animations in the file |
| `frames` | list of integers | yes |  | frames per animation |
| `path` | text | yes |  | the file written |
| `target` | JSON |  |  | with a target character: {character, driven, undriven: [node], scale, feet: [{clip, L: {planted_frames, slide_cm, travel_cm, lowest_cm, unreached_frames, holds}, R}], problems: [[{from, to, kind, label}]], notes} |

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
| `root` | one of hips \| ground \| in_place |  |  | root motion bone: hips (pelvis only), ground (bone on the floor), in_place (ground bone with movement removed) |
| `target` | text |  |  | export onto your own character: its .glb (the mapping is guessed) or a mapping file saved by retarget.save; the file is written with the animation on the character's own nodes |

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
| `path` | text | yes |  | an animation file: .clip.json, or a humanoid .fbx (retargeted onto the rig) |
| `joints` | JSON |  |  | an .fbx: landmark -> joint name, correcting the guess ({"l_knee": "LeftLeg"}); the landmarks are hips, spine, neck, head and per side shoulder, elbow, wrist, hip, knee, ankle, toe |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `length` | integer | yes |  | frames |
| `fps` | number |  |  | an .fbx: the file's frame rate |
| `frames` | integer |  |  | an .fbx: frames read (setup frames dropped) |
| `joints` | JSON |  |  | an .fbx: {landmark: the file's joint that stood for it} |
| `given` | list of text |  |  | an .fbx: the landmarks the joints parameter set |
| `unused` | list of text |  |  | an .fbx: the file's joints with no landmark (fingers, face, props, ends) |
| `at_rest` | list of text |  |  | an .fbx: rig bone families left at rest ("finger: 30 of 30 at rest") |
| `scale` | number |  |  | an .fbx: the factor that made the source's legs the rig's |
| `floor` | number |  |  | an .fbx: how far the floor was moved to z = 0 (metres; 0: left as it was) |
| `hips` | JSON |  |  | an .fbx: {mid, left, right}: how far the rig's hip joints land from the source's on the worst frame (metres) |
| `notes` | list of text |  |  | an .fbx: what was guessed, dropped or left at rest |
| `source` | JSON |  |  | an .fbx: {version, up_axis, front_axis, unit_scale, fps_from, take, joints, frames, dropped} |

### `clip.import_batch`: Import a folder of FBX

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `import`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | text | yes |  | a folder of .fbx files (subfolders too) |
| `to` | text | yes |  | the folder the .clip.json files go in (made if missing) |
| `joints` | JSON |  |  | landmark -> joint name, correcting the guess for every file ({"l_knee": "LeftLeg"}) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `folder` | text | yes |  | the folder the clips went in |
| `files` | JSON | yes |  | [{file, path, frames, fps, notes}] written |
| `failed` | JSON | yes |  | [{file, error}] not imported |
| `cut` | text |  |  | set when more files were found than one batch takes |

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
| `capture` | JSON | yes |  | {frames, spans, max_cm, worst_frame}: a moving body's capture point outside the feet it has or is about to have |
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

### `clip.picture`: Export picture

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `sheet`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the .png to write |
| `frame` | number |  | 0 to 1000000 | the frame as it plays (default the pose on show) |
| `view` | one of front \| back \| left \| right \| side \| top \| three-quarter |  |  | the camera: front, back, left, right, side, top or three-quarter (default) |
| `width` | integer |  | 16 to 4096 | pixels across |
| `height` | integer |  | 16 to 4096 | pixels down |
| `floor` | bool |  |  | draw the floor and its grid (default true) |
| `background` | text |  |  | #rrggbb, or transparent (default the viewport's colour) |
| `azimuth` | number |  | -6.3 to 6.3 | your own camera instead of the view's angles, with elevation and distance: radians about the vertical, 0 at the front of the body |
| `elevation` | number |  | -1.55 to 1.55 | your own camera: radians above the floor |
| `distance` | number |  | 0.1 to 150 | your own camera: metres from what it looks at |
| `target` | [x, y, z] |  |  | your own camera: the point it looks at (default where the view would look) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file written |
| `width` | integer | yes |  | pixels across |
| `height` | integer | yes |  | pixels down |
| `note` | text |  |  | how the pictures were made, when it matters (the software adapter) |

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
| `floor` | bool |  |  | draw the floor and its grid (default true) |
| `background` | text |  |  | #rrggbb, or transparent (default the viewport's colour) |
| `azimuth` | number |  | -6.3 to 6.3 | your own camera instead of the view's angles, with elevation and distance: radians about the vertical, 0 at the front of the body |
| `elevation` | number |  | -1.55 to 1.55 | your own camera: radians above the floor |
| `distance` | number |  | 0.1 to 150 | your own camera: metres from what it looks at |
| `target` | [x, y, z] |  |  | your own camera: the point it looks at (default where the view would look) |

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
| `floor` | bool |  |  | draw the floor and its grid (default true) |
| `background` | text |  |  | the colour behind the body as #rrggbb (default the viewport's); a video can't be transparent |
| `azimuth` | number |  | -6.3 to 6.3 | your own camera instead of the view's angles, with elevation and distance: radians about the vertical, 0 at the front of the body |
| `elevation` | number |  | -1.55 to 1.55 | your own camera: radians above the floor |
| `distance` | number |  | 0.1 to 150 | your own camera: metres from what it looks at |
| `target` | [x, y, z] |  |  | your own camera: the point it looks at (default where the view would look) |

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

## community

### `community.account`: Community account

not undoable, icon `user`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `signed_in` | bool | yes |  | an account exists on this computer |
| `verified` | bool | yes |  | it passed the captcha: may share and report |
| `name` | text | yes |  | the name shown on its clips |
| `captcha` | text | yes |  | the captcha page |

### `community.browse`: Browse the community

not undoable, icon `globe`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `search` | text |  |  | words that must all be in the title, description or tags |
| `tags` | list of text |  |  | tags the clip must all have |
| `sort` | one of top \| new \| downloads |  |  | top (best rated, the default), new, or most downloaded |
| `page` | integer |  | 0 to 10000 | which page, from 0 |
| `limit` | integer |  | 1 to 100 | clips a page (default 50) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `clips` | JSON | yes |  | {id, title, description, author, tags, licence, credit, fps, frames, seconds, rating, ratings, downloads, published, preview, installed, official} |
| `count` | integer | yes |  | how many on this page |

### `community.fetch`: Fetch community clip

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `download`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the community clip's id |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the clip |
| `path` | text | yes |  | the unpacked clip file, to play (not in the library) |
| `clip` | JSON | yes |  | {id, title, description, author, tags, licence, credit, fps, frames, seconds, rating, ratings, downloads, published, preview, installed, official} |

### `community.install`: Install community clip

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `download`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the community clip's id |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the library id it was installed as (Community/<title>) |
| `clip` | text | yes |  | the community clip |
| `already` | bool | yes |  | it was installed before: nothing changed |
| `notes` | list of text | yes |  | what didn't go to plan (empty: nothing) |

### `community.mine`: My community uploads

not undoable, icon `user`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `clips` | JSON | yes |  | [{id, title, status (checking, published, failed, hidden, removed), reason, created}], newest first |

### `community.rate`: Rate community clip

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `star`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the community clip's id |
| `stars` | integer | yes | 1 to 5 | 1 to 5 |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the clip |
| `rating` | number | yes |  | its average now, to two decimals |
| `ratings` | integer | yes |  | how many have rated it |

### `community.remove`: Remove my community clip

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `trash`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the community clip's id |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the clip |
| `status` | text | yes |  | removed |

### `community.report`: Report community clip

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `flag`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the community clip's id |
| `reason` | text |  |  | what's wrong with it (500 characters at most) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the clip |
| `reported` | bool | yes |  | true |

### `community.share`: Share to community

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `upload`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | one of your animations in the library |
| `title` | text |  |  | 80 characters at most (default: the animation's name) |
| `description` | text |  |  | 2000 characters at most |
| `tags` | list of text |  |  | 10 at most: letters, digits and hyphens, 24 each |
| `licence` | one of CC0-1.0 \| CC-BY-4.0 | yes |  | CC0-1.0 (anyone, any use) or CC-BY-4.0 (with credit) |
| `credit` | text |  |  | who or what to credit (200 characters at most) |
| `name` | text |  |  | the name shown as the author (40 at most) |
| `preview` | text |  |  | its frames side by side as one PNG strip (1 MB at most), as the Library draws thumbnails |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `clip` | text | yes |  | its community id |
| `status` | text | yes |  | checking: it goes live once the checks pass |
| `title` | text | yes |  | the title it was shared with |
| `tags` | list of text | yes |  | the tags, cleaned |

### `community.verify`: Pass the community captcha

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `check`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `token` | text | yes |  | the token the captcha page gave |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `verified` | bool | yes |  | true |

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
| `project` | text | yes |  | the project file it is (null: a library animation) |
| `unsaved` | bool | yes |  | changes the library (or the project file) lacks |
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

### `feet.phases`: Foot phases

not undoable, icon `graph`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `motion` | one of shown \| keys |  |  | what plays (the physics pass when it is on; default) or the keys alone |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `L` | list of text | yes |  | the left foot on each frame: swing, landing, planted or release |
| `R` | list of text | yes |  | the right foot, the same |

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

### `feet.roll`: Roll feet

undoable, icon `foot`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames changed |
| `short` | integer | yes |  | frames a leg still can't reach at the most a foot rolls |

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

## hips

### `hips.transfer`: Shift weight over the feet

undoable, icon `foot`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames changed |
| `left_cm` | number | yes |  | the worst gap left, to the side of the feet |

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

### `library.favourite`: Star in library

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `star`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | a library id, <group>/<name> |
| `kind` | one of animations \| poses |  |  | animations (default) or poses |
| `on` | bool |  |  | star it (default true) or take the star off |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `id` | text | yes |  | the item |
| `favourite` | bool | yes |  | whether it is starred now |

### `library.find`: Find in library

not undoable, icon `search`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `kind` | one of animations \| poses |  |  | animations (default) or poses |
| `query` | text |  |  | words that must all be in the name, group or part (any case) |
| `source` | one of all \| mine \| builtin |  |  | all (default), your own, or the built-ins |
| `group` | text |  |  | only this group |
| `length` | one of short \| medium \| long |  |  | animations only: short (under 2 s), medium (2 to 6 s) or long (over 6 s) |
| `loop` | bool |  |  | animations only: true for loops only, false for the others |
| `edited` | bool |  |  | true: only built-ins changed in your library |
| `part` | text |  |  | poses only: this body part (Whole body, Left arm, ...) |
| `favourites` | bool |  |  | true: only starred items |
| `sort` | one of name \| length \| modified |  |  | name (by group, the default), length (shortest first) or modified (newest first) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `items` | JSON | yes |  | the items found, as library.list lists them, in order |
| `count` | integer | yes |  | how many |
| `groups` | JSON | yes |  | [{group, count}] of the items found, in order |

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
| `animations` | JSON | yes |  | [{id, name, group, current, builtin, edited, favourite, frames, fps, seconds, loop, modified, ...}] |
| `poses` | JSON | yes |  | [{id, name, group, part, builtin, edited, favourite, modified, ...}] |
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

### `library.sync`: Update built-ins

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `restore`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | text | yes |  | the built-in library folder |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `animations` | JSON | yes |  | {added, updated, retired: [id], moved: [id], kept: [{id, why}]}: new built-ins copied in, unchanged copies replaced by a fixed one, ones no longer shipped moved to <library>/.retired (or, when they moved to the community, kept in place as community installs), and ones you changed left as they are |
| `poses` | JSON | yes |  | the same for poses |
| `changed` | integer | yes |  | items added, updated, retired or moved |

## live

### `live.clip`: Live motion: keep a range

undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | the recording's first frame to keep (default 0) |
| `to` | integer |  | 0 to 10000000 | its last frame to keep (default the last) |
| `at` | integer |  | 0 to 10000000 | the animation's frame it starts on (default the playhead) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the animation's first frame written |
| `to` | integer | yes |  | its last |
| `frames` | integer | yes |  | AI keys written between them |
| `model` | text | yes |  | the keys' model tag |
| `notes` | list of text | yes |  | what to know |

### `live.input`: Live motion: steer

not undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `heading` | number |  | -3600 to 3600 | degrees: 0 the rig's forward (-Y), positive to its left |
| `speed` | number |  | 0 to 100 | m/s, 0 to stop (held to the targets' top speed) |
| `style` | text |  |  | a target of the set to aim at instead of blending by speed |
| `frames` | integer |  | 0 to 600 | frames at 30 fps to move on (default 1) |
| `wait` | bool |  |  | wait for each stretch (default); false lets the model work on a thread while the last stretch plays on |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `state` | one of live \| full | yes |  | full: the recording reached its cap and the character stands |
| `frames` | integer | yes |  | frames recorded so far |
| `calls` | integer | yes |  | stretches asked of the model so far |
| `speed` | number | yes |  | the stick smoothed, m/s |
| `target` | text | yes |  | what the last stretch aimed at |
| `why` | text | yes |  | why the last stretch was asked for |
| `root` | [x, y, z] | yes |  | the root offset on show |
| `notes` | list of text | yes |  | what to know |

### `live.show`: Live motion: look at a recorded frame

not undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes | 0 to 10000000 | the recording's frame to show |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frame` | integer | yes |  | the frame on show |
| `root` | [x, y, z] | yes |  | its root offset |

### `live.start`: Live motion: start a take

not undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `max_frames` | integer |  | 2 to 18000 | the most frames to record (default ten minutes at 30 fps) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `session` | integer | yes |  | the take's number |
| `targets` | list of text | yes |  | the target poses of the set |
| `max_speed` | number | yes |  | the set's top speed, m/s |
| `max_frames` | integer | yes |  | the most frames it records |

### `live.stop`: Live motion: stop the take

not undoable, icon `ai`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames recorded; they stay for live.clip |
| `calls` | integer | yes |  | stretches asked of the model |
| `held` | integer | yes |  | frames shown with no motion ready: the last pose held |
| `late` | integer | yes |  | frames skipped because a stretch came late |
| `round_trip_ms` | number | yes |  | mean time a stretch took, helper round trip |
| `round_trip_max_ms` | number | yes |  | the longest |
| `model_ms` | number | yes |  | mean of the model's own time |

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

## posefill

### `posefill.fill`: Fill gap from poses

undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer |  | 0 to 10000000 | your key before the gap (default: around the playhead) |
| `to` | integer |  | 0 to 10000000 | your key after the gap; keys of yours between from and to are gone through, one call a gap |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `from` | integer | yes |  | the fill's first frame (a key of yours) |
| `to` | integer | yes |  | its last frame (a key of yours) |
| `frames` | integer | yes |  | AI keys written between them |
| `model` | text | yes |  | the keys' model tag |
| `calls` | integer | yes |  | model calls made: one a gap between keys of yours |
| `seconds` | number | yes |  | the model's own time over those calls, as it reports it |
| `notes` | list of text | yes |  | what to know about the result |

### `posefill.helper`: Pose fill: helper

not undoable, icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `action` | one of status \| load \| stop |  |  | status (default) says what is installed and running; load starts the helper and loads the model (a few minutes the first time, while it builds its plan for this GPU); stop ends it |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `installed` | bool | yes |  | the helper is installed |
| `dir` | text | yes |  | where it is (or would be) installed |
| `running` | bool | yes |  | its process is up |
| `loaded` | bool | yes |  | the model is in memory |
| `busy` | bool | yes |  | it is making a fill now |
| `model` | text | yes |  | the model installed |
| `plan` | text |  |  | whether the model's plan for this GPU is built (the first load builds it, a few minutes) |
| `gpu` | text |  |  | the GPU the helper runs on |
| `note` | text |  |  | what is missing and how to install it |

### `posefill.install`: Pose fill: install

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `ai`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `action` | one of check \| start \| stop |  |  | check (default) says what the install needs and what it is doing; start runs the setup (about 4 GB downloaded, a few minutes; the window stays usable); stop ends it, and start carries on from there |
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
| `licences` | JSON |  |  | what installing accepts, a list of {part, licence, url} |
| `left_out` | text |  |  | what the install never takes from the engine's source |

## project

### `project.info`: Project file info

not undoable, icon `info`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | a project file (.project.json) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file read |
| `version` | integer | yes |  | the version it was written with |
| `current` | bool | yes |  | whether that is the version this build writes |
| `name` | text | yes |  | the animation's name |
| `group` | text | yes |  | its group |
| `fps` | number | yes |  | frames per second |
| `frames` | integer | yes |  | its length |
| `layers` | integer | yes |  | layers, the base included |
| `boxes` | integer | yes |  | scene boxes |
| `links` | list of text | yes |  | the linked clips it carries, by library id |
| `clips` | integer | yes |  | clips in the file |
| `bytes` | integer | yes |  | its size |
| `hash` | text | yes |  | FNV-1a of its bytes |

### `project.open`: Open project

undoable, shortcut `Ctrl+Shift+O`, icon `open`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | a project file (.project.json) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file opened |
| `name` | text | yes |  | the animation's name |
| `length` | integer | yes |  | frames |
| `frame` | number | yes |  | the playhead, as saved |
| `links` | list of text | yes |  | the linked clips it carries, by library id |
| `layers` | integer | yes |  | layers, the base included |
| `boxes` | integer | yes |  | scene boxes |
| `version` | integer | yes |  | the version the file was written with |
| `migrated` | bool | yes |  | an older version, brought up to date on opening |
| `notes` | list of text | yes |  | what the open session can't honour (an unknown AI style) |

### `project.save`: Save project

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), shortcut `Ctrl+Shift+S`, icon `save`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text |  |  | where to write it (default: the file the project was opened from or saved to) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file written |
| `bytes` | integer | yes |  | its size |
| `hash` | text | yes |  | FNV-1a of its bytes (the same document gives the same hash) |
| `links` | list of text | yes |  | the linked clips embedded, by library id |
| `clips` | integer | yes |  | clips in the file (the animation and its links) |

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

## retarget

### `retarget.check`: Check character retarget

not undoable, icon `graph`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `frames` | integer | yes |  | frames driven |
| `scale` | number | yes |  | the character's legs over the rig's |
| `driven` | integer | yes |  | nodes the rig drives |
| `undriven` | list of text | yes |  | nodes under the hips nothing drives |
| `feet` | JSON | yes |  | {L, R: {planted_frames, spans: [{from, to, slide_cm}], slide_cm, travel_cm, lowest_cm, unreached_frames, unreached_cm, holds}}: how far a planted foot drifts from where it came down (slide), its per-frame travel added up (travel), the lowest it got above the character's floor, frames its leg could not reach, and whether it holds (drift under 1 cm, never under the floor, always reached) |
| `problems` | JSON | yes |  | [{from, to, kind: foot-slide \| below-floor \| unreached \| undriven, label}] |
| `notes` | list of text | yes |  | the plan's notes |

### `retarget.load`: Load character

undoable, icon `import`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | a character .glb (the mapping is guessed from its node names) or a saved mapping file (.retarget.json) |
| `joints` | JSON |  |  | landmark -> node name, correcting the guess ({"l_knee": "L_Calf"}; "none" unmaps one) |
| `hands` | bool |  |  | solve the arms onto the rig's wrists too (default: legs only) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `character` | text | yes |  | the character file, as given |
| `nodes` | integer | yes |  | nodes in the file |
| `skin_joints` | integer | yes |  | joints of its skin (0: no skin) |
| `mesh` | bool | yes |  | a mesh is skinned to it |
| `joints` | JSON | yes |  | {landmark: the node that stands for it} |
| `given` | list of text | yes |  | the landmarks the joints parameter set |
| `hands` | bool | yes |  | the arms are solved onto the wrists |
| `missing` | list of text | yes |  | required landmarks with no node yet |
| `ready` | bool | yes |  | the mapping is complete and the character can be checked and exported |
| `driven` | integer | yes |  | nodes the rig drives |
| `undriven` | list of text | yes |  | nodes under the hips nothing drives (they keep their rest) |
| `outside` | list of text | yes |  | nodes not under the hips (left as they are) |
| `scale` | number |  |  | the character's legs over the rig's (null: not ready) |
| `floor` | number |  |  | the character's floor height (null: not ready) |
| `source` | JSON |  |  | {nodes, skin_joints, meshes, animations} of the file |
| `notes` | list of text | yes |  | what was guessed, left out or is missing |

### `retarget.map`: Map character joints

undoable, icon `edit`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `joints` | JSON |  |  | landmark -> node name, correcting the guess ({"l_knee": "L_Calf"}; "none" unmaps one) |
| `hands` | bool |  |  | solve the arms onto the rig's wrists too (default: legs only) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `character` | text | yes |  | the character file, as given |
| `nodes` | integer | yes |  | nodes in the file |
| `skin_joints` | integer | yes |  | joints of its skin (0: no skin) |
| `mesh` | bool | yes |  | a mesh is skinned to it |
| `joints` | JSON | yes |  | {landmark: the node that stands for it} |
| `given` | list of text | yes |  | the landmarks the joints parameter set |
| `hands` | bool | yes |  | the arms are solved onto the wrists |
| `missing` | list of text | yes |  | required landmarks with no node yet |
| `ready` | bool | yes |  | the mapping is complete and the character can be checked and exported |
| `driven` | integer | yes |  | nodes the rig drives |
| `undriven` | list of text | yes |  | nodes under the hips nothing drives (they keep their rest) |
| `outside` | list of text | yes |  | nodes not under the hips (left as they are) |
| `scale` | number |  |  | the character's legs over the rig's (null: not ready) |
| `floor` | number |  |  | the character's floor height (null: not ready) |
| `source` | JSON |  |  | {nodes, skin_joints, meshes, animations} of the file |
| `notes` | list of text | yes |  | what was guessed, left out or is missing |

### `retarget.save`: Save character mapping

not undoable, writes files (undo and a batch rollback leave them; a dry run refuses it), icon `save`.

Inputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the mapping file to write (.retarget.json) |

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `path` | text | yes |  | the file written |
| `bytes` | integer | yes |  | its size |
| `hash` | text | yes |  | a hash of its bytes (the same mapping gives the same file) |
| `character` | text | yes |  | the character file it names |

### `retarget.unload`: Unload character

undoable, icon `close`.

No inputs.

Outputs:

| Name | Kind | Required | Range | What |
|---|---|---|---|---|
| `unloaded` | bool | yes |  | the character is out of the document |
| `character` | text | yes |  | the character file that was loaded |

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
