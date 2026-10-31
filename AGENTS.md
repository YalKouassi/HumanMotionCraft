# HumanMotionCraft: instructions for agents

HumanMotionCraft is a standalone, pure-Rust keyframe animation application: a posable rig, a
timeline with layers and curves, IK, a physics-assisted solver and learned in-betweening, with a
native `wgpu` + `egui` UI. It succeeds cf-animate, an earlier Python prototype (not public); that
prototype's reference results are checked in as the parity goldens in `tests/fixtures/golden`, and
comments saying "a port of cf-animate's ..." name where code came from. `CLAUDE.md` points here:
this file is the single source of agent rules.

## Start every session here
1. Read the roadmap: `ls roadmap` lists `!README.md` (the naming convention) and then one file per
   piece of work, the not-done ones first in pickup order (`DONE_` files are finished, `OBSOLETE_`
   files were dropped). Load the `roadmap` skill
   (`.claude/skills/roadmap/SKILL.md`) before adding, reordering or finishing an item.
2. **For any feature, fix or refactor, load the `ac-feature` skill**
   (`.claude/skills/ac-feature/SKILL.md`).
3. The feature loop and fixture format: `docs/llm-guidelines.md`. What the tests are:
   `docs/testing.md`.
4. Don't stop to ask unless the decision is the owner's (product scope, licences, branding,
   releases, anything destructive).

## Where things are

| Folder | What | Rule |
|---|---|---|
| `crates/ac-core` | rig, quaternions, FK, IK, limits, clip/layer/curve model, the physics pass (`ac_core::physics`), the retarget of landmark positions onto the rig (`ac_core::retarget`, a port of `training/ac_train/retarget.py` pinned by `crates/ac-core/tests/golden/retarget-walk.json`), the joint-name guess it needs (`ac_core::jointmap`, shared with the export onto a character), a character of the person's own as the export's target (`ac_core::character`: its skeleton, the mapping, the plan, the drive with leg IK, the check), **the command registry and session (undo)** | no UI, GPU, file dialogs or threads; wasm-safe |
| `crates/ac-ai` | in-betweener: the network in plain Rust (weights from `training/`: `python -m ac_train.export`) and the fill pipeline | depends on `ac-core` only; goldens in `crates/ac-ai/tests/golden` |
| `crates/ac-io` | GLB export, clip and pose files, the library, recovery, binary FBX import (`ac_io::fbx`: our own reader, `clip.import` of an `.fbx`, `clip.import_batch`; test files from `training/ac_train/fbxwrite.py`), the export onto a character of the person's own (`ac_io::character`: our own glTF skeleton reader, the `retarget.*` commands, the mapping file, `clip.export {target}`; the test character from `training/ac_train/glbwrite.py`), and the one session setup every front end uses (`ac_io::setup::SessionBuilder`) | depends on `ac-core` only (plus `miniz_oxide` for FBX's zlib arrays); never reads or copies another product's FBX code |
| `crates/ac-render` | `wgpu` renderer: skinned mesh (the mannequin, and mesh slots for a character of the person's own), joints, grid, onion skin, gizmos, offscreen pictures; the one GIF/MP4 encoder (`ac_render::anim`: `GifWriter`, `Mp4Writer`) | no egui |
| `crates/ac-media` | pictures and video of the animation as it plays: `clip.video` (MP4 through ffmpeg, or GIF), `clip.sheet` (contact sheet PNG); `Shots` renders headless, `Encoder` writes the file with `ac_render::anim` | depends on `ac-core` and `ac-render`; registered through `SessionBuilder::commands` by the CLI and the app |
| `crates/ac-cli` | headless CLI `ac`: commands, scripts, fixtures, renders, video, scaffolding; MCP over stdio in `crates/ac-cli/src/mcp/` (generated from the command registry) | no command logic of its own |
| `crates/ac-app` | the binary (`eframe`, wgpu backend); the `egui` panels (viewport, timeline, library, inspector) are in `crates/ac-app/src/app/` | thin: reads session state, runs commands |
| `crates/ac-review` | the training-data review app `ac-review` (dev only, never shipped): plays each take of `training/data/rig/*.npz` with the scan's flags (`ac_train.quality`), and keeps the keep/cut/reject decisions, tags and notes in `training/review/decisions.json`; headless `ac-review run review.list|take|decide|clear|summary` for agents | writes only the decisions file and its own unpack cache (`training/data/rig/.review-cache/`) |
| `xtask/` | repo tasks: `cargo xtask check` (the whole pre-commit gate), `names`, `assets`, `docs` | dev only; never a dependency |
| `.github/workflows/ci.yml` | CI: the same gate on Windows | |
| `.github/workflows/release.yml` | the release: a `v<version>` tag builds the Windows portable zip (the app, `ac.exe`, `assets/`), checks it against Craft Unofficial Launcher's contract (`craft-app.json`) and publishes it as the latest release; a manual run only builds and checks | releases are the owner's call; data releases are prereleases |
| `assets/` | the baked rig, limits, mannequin, built-in library, models (`MANIFEST.json`) | every file has a row in `ATTRIBUTION.md` |
| `tests/fixtures/golden` | the earlier prototype's results on fixed inputs (parity) | checked in; the prototype is not public, so they can't be regenerated: never edit them or rewrite them from this repo's own output |
| `fixtures/` | feature fixtures (JSON scripts of commands with hand-worked expectations) | never deleted to pass a check |
| `crates/ac-helper` | what the optional Python helpers share: the install folder and `install.json` (`ac_helper::install`), the setup run from the app with its `== [n/N]` progress lines, Stop and resume (`ac_helper::setup`), `uv` fetching, the JSON-lines helper process (`ac_helper::process`), the one engine behind a lock with its cache of results (`ac_helper::slot`), and the frame-rate fit and reply reading (`ac_helper::frames`) | depends on `ac-core` only; threads and child processes (so not in `ac-core`); no UI; each helper crate keeps its own protocol, `Engine` seam and commands |
| `crates/ac-textmotion` | motion from text: `text.install`, `text.helper`, `text.generate`, `text.fill`, through the optional helper process that `tools/textmotion/` installs; `Engine` is the seam (a fake in the tests) | depends on `ac-core` only; registered through `SessionBuilder::commands`; without the install `text.generate` and `text.fill` refuse and say how to install it |
| `crates/ac-posefill` | pose fill: `posefill.install`, `posefill.helper`, `posefill.fill` (the gap between two keys of yours filled by the pose-to-pose model from the poses around it; a longer stretch chains through the keys inside it), and live motion control: `live.start`, `live.input`, `live.stop`, `live.show`, `live.clip` (`live` is the pure take: stick smoothing, targets, the recording; `live_commands` the commands, asking the helper off the UI thread), through the optional helper process that `tools/posefill/` installs; `Engine` is the seam (a fake in the tests, `stand_in` in fixture runs); its keys carry the tag `ac_core::edits::POSE_FILL` | depends on `ac-core` and `ac-helper`; registered through `SessionBuilder::commands`; without the install every `posefill.*` command but `helper` and `install` refuses and says how to install it |
| `crates/ac-community` | the community library: `community.browse`, `.fetch`, `.install`, `.rate`, `.report`, `.verify`, `.share`, `.mine`, `.remove`, `.account`, through the server in `services/community/`; `Backend` is the seam (`Http` the live server, `Fake` the fixtures') | depends on `ac-core` and `ac-io`; registered through `SessionBuilder::commands`; with no backend connected (tests) every command refuses; fixture runs and `--screenshot` use the fake, never the network |
| `training/` | Python: dataset retargeting, training, weight export and library clip cutting (`training/README.md`); data under `training/data/` is gitignored | the only Python in the repo besides `tools/drive_ui.py`, `tools/textmotion/` and `tools/posefill/`; never a runtime dependency |
| `tools/textmotion/` | Python: the optional text-to-motion install (`setup_textmotion.py`), the app's helper process (`helper.py`, with `engine.py`, JSON lines on stdin/stdout) and training-clip generation (`generate.py`) | installed outside the repo; the app runs without it |
| `tools/posefill/` | Python: the optional pose-to-pose install (`setup_posefill.py`: the engine, its model and TensorRT into the person's data folder, NVIDIA GPU only, the evaluation-only sample clips left out) and the app's helper process (`helper.py`, `engine.py`, `live.py`, JSON lines on stdin/stdout); `live_drive.py` is live motion control's scripted stick test beside the app's own `live.*` (roadmap 158000) | installed outside the repo; the app runs without it; packaging (124000) ships it with `tools/textmotion/` |
| `services/community/` | the community library's server: Supabase migrations and Edge Functions (SQL, TypeScript), clip files on Cloudflare R2 (`services/community/README.md`); secrets in its gitignored `.env.local` | never part of the app's build; the app reaches it over HTTPS only |
| `vendor/craft-libs` (optional, gitignored; not in a clone) | a local read-only copy of [craft-libs](https://github.com/storytold/craft-libs) for reading its source | depend on craft-libs by pinned git `rev`, never by path |

Layering: `ac-core` → (`ac-ai`, `ac-io`) → (`ac-cli`, `ac-app`); `ac-render` stands on its own
(`wgpu`, no egui) and is used by `ac-media`, `ac-cli` and `ac-app`; `ac-media` stands on `ac-core`
and `ac-render` and is used by `ac-cli` and `ac-app`; `ac-helper` stands on `ac-core` and is
used by the helper crates; `ac-textmotion` and `ac-posefill` stand on `ac-core` and `ac-helper` and are
used by `ac-cli` and `ac-app`; `ac-community` stands on `ac-core` and `ac-io` and is used by `ac-cli` and `ac-app`, which don't depend on each other. Only
`ac-app` and the separate dev app `ac-review` depend on egui, eframe or winit (the owner
chose a separate review exe, 2026-10-28, so the shipped app has no training buttons). The solver,
the UI panels and MCP live in `ac_core::physics`, `crates/ac-app/src/app/` and
`crates/ac-cli/src/mcp/`, not in crates of their own.

## Everything is a command
User-visible behaviour is a `CommandSpec` in `ac-core` (or the crate that owns it): our own dotted
lower-case id (`pose.set_bone`, `key.set`, `pose.drag`), label, icon name, shortcut, typed `inputs`
and `outputs`, `tool`, `undoable`, and a JSON `run`. The toolbar, menus, shortcuts, interactive
tools, scripts, the CLI, fixtures and MCP all reach the same commands through the session. That is
what makes every feature testable headless and drivable by an agent.
- **Programmatic calls never open dialogs.** A command takes its file paths and choices as
  parameters; only a UI click starts an interactive tool, which then runs commands.
- **The UI is thin.** Panels read session state and act through `session.run(id, params)`.
- **New things are new files behind an extension point** (a command, a layer source, an
  interpolation, an exporter). If a feature needs edits in 3+ existing files beyond one
  registration line, add the extension point first as its own behaviour-preserving commit.
- **The document is data; poses and meshes are derived.** Keys, layers and settings are stored;
  sampled poses, world matrices and skinned meshes are recomputed, never saved.
- **A command declares what it returns** (`outputs`; shared shapes in each crate's `returns.rs`).
  The session checks every result against them in debug builds and the fixture runner
  (`Session::check_outputs`), so an undeclared key is a failing test. Never declare `changed` on
  an undoable command: the session adds its change summary under that name. A command that writes
  files outside the document says `.writes()` (a dry run refuses it, a batch rollback names it);
  `.undo_label(f)` names its undo step for each run.
- **The session's own tools**: `edit.batch` (several commands, one undo step, atomic by default),
  `edit.dry_run` (try on a copy), `edit.history` / `edit.checkpoint` / `edit.revert_to`,
  `session.reset` / `session.info`. A refusal is a `CoreError` (`to_json()`: kind, hint,
  suggestions, the failed step); "did you mean" comes from `ac_core::suggest`, the one helper.

### Extension points
Add to these instead of editing the code around them:

| To add | Where |
|---|---|
| a command | a `CommandSpec` in its area's `SPECS` (`crates/ac-core/src/commands/`, or the crate that owns it); `ac new-command <area.verb>` writes the stub, the fixture skeleton and the registration |
| a crate's commands | its `SPECS`, registered through `ac_io::setup::SessionBuilder::commands` (as `ac-media` is) |
| an app action (window only: tools, panels, playback) | a row in `ACTIONS` and an arm in `App::act` (`crates/ac-app/src/app/shell/actions.rs`); the keymap, menus, palette and the shortcut list pick it up |
| a menu | commands land in menus by their id's area (`crates/ac-app/src/app/shell/menus.rs`, `AREAS`); a new area goes in Edit > More until it is placed there |
| a saved preference | its own section struct (`#[serde(default)]`) and one field on `Prefs` (`crates/ac-app/src/app/prefs.rs`); never rename or retype a stored key |
| a mark on the timeline (a problem, a span to jump to) | `TimelineMarks`: `self.timeline.marks.set("<source>", marks)` (`crates/ac-app/src/app/timeline/marks.rs`) |
| an inspector panel | a file in `crates/ac-app/src/app/panels/` with its state on `Panels` |
| a file feature (import, export dialog, recovery, hot reload) | `crates/ac-app/src/app/files/`, acting through commands; the window calls its four hooks |
| another skinned mesh in the viewport (a character beside the mannequin) | `Renderer::set_mesh(device, slot, &mesh)` and a `Body { mesh: slot, .. }` a frame (`crates/ac-render/src/renderer.rs`; slot 0 is the mannequin); the character preview does this in `crates/ac-app/src/app/character.rs` |

## Rule 1: never hide a problem
A failure, fallback or missing piece is announced, never silent. Errors say what failed and why.
No `let _ =` on a `Result` that matters.
- **A bug or bad data our code made** (a file we wrote that won't parse, a broken invariant) is an
  error: the command fails, and the user sees it.
- **Something the user hasn't set up or chose** (no model installed, a missing library item) is a
  notification: the app carries on with a stated fallback and says what is missing and how to fix
  it, in the UI and in the command's result, never only in a log.

## Never crash
A crash loses someone's animation. **This outranks features.**
- No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!` in non-test code;
  no `unsafe` (`unsafe_code = "forbid"`). Every production crate root carries
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.
- Errors are `Result<T, E>` through the crate's error type and `?`. An unfinished feature returns
  "not supported yet: …".
- **Input-derived numbers are hostile** (command params, project files, MCP input): `get()` not
  `[i]`, finite checks, caps (`MAX_*` constants).
- `Session::run` runs every command under `catch_unwind`; a failed or panicking command restores
  the document and reports an error. Every crash fix lands with a regression test.

## Determinism
Same inputs → byte-identical outputs in `ac-core` (the solver included), `ac-io` and `ac-ai`. No wall-clock
time and no unseeded randomness in them. Maths is `f64` in the core (the GPU gets `f32`).

## Non-negotiables
- **Our own work and clean names.** Code, models and help text are our own. Never copy another
  app's code, weights, icons, help text or files. Other products' names don't appear in code, docs
  or assets (`cargo xtask names` checks; credits for assets we use live in
  `ATTRIBUTION.md`/`NOTICE`).
- **Our own name lives in one place.** Code never spells the product name: it uses
  `ac_core::product::NAME` / `ID` (or `product_name!()` / `product_id!()` inside `concat!`), and
  file formats go through `product::is_format`. `docs/naming.md` lists what can't read the
  constant and the rename steps.
- **Licences.** Every asset and every model has a row in `ATTRIBUTION.md` with its source and
  licence. A model ships only when every dataset it was trained on allows it
  (`assets/models/MANIFEST.json`). Never read, copy or link GPL/LGPL/AGPL code. The repo
  supplies free-licence training sets only, and the shipped model and clips come from them alone
  (owner's decision, 2026-10-29). Never add a non-commercial set to the `REGISTRY`, the docs or a
  release; one a user adds is theirs and stays in their gitignored `training/data/`.
- **Assets.** No vendor icons, images or fonts. Icons are drawn in code or come from craft-libs.
- **craft-libs first.** Settings, atomic saves, keymaps, menus, quit handling, UI widgets, docking,
  i18n: use the `craft-*` crate when one covers it, and keep app policy in the app.
- **Rust only at runtime.** Python lives in `training/` (and `tools/drive_ui.py`). The exceptions are optional helpers the app starts only when asked and never needs: motion from text runs in a Python helper process installed by `tools/textmotion/setup_textmotion.py` (owner's decision, 2026-10-28), and pose fill in one installed by `tools/posefill/setup_posefill.py` (owner's decision, 2026-10-30; NVIDIA GPU only). The community library's server (`services/community/`) is SQL and TypeScript running on Supabase, not in the app (owner's decision, 2026-10-30).
- **Units:** metres and radians internally; Z up, the rig faces -Y; glTF export maps to Y up.
- Never weaken a gate, add an allow-list entry or delete a fixture or golden to get past a check;
  fix the design. Changing a rule is the owner's decision.

## Test what you changed
| When | Run |
|---|---|
| While working | `cargo nextest run -p <crate>` (or `cargo test`); `ac fixtures --only <name>` (`--shots <dir>` renders the views and writes `index.html`, a gallery with each description); `ac render --script <f> --view <v> --out <png>` (`--frames 0,8,15 --strip`: one picture, one camera for all); `ac video --script <f> [--fixed] --out <mp4>`; the app with `--script <f> --screenshot <png>` for UI work |
| A fixture fails | `ac fixtures --only <name> --print` prints every checked step's whole result as JSON lines; `--bless` writes the actual values into the `expect`s it already names. Bless only after checking them against the description's hand-worked numbers |
| A picture changed | `cargo nextest run -p ac-cli --test visual` checks every fixture view against `tests/visual/` (software adapter, pixel tolerance) and leaves `target/visual-diff/<fixture>-<view>-{actual,diff}.png`. Look at them; if the change is meant, bless with `AC_BLESS_VISUAL=1` (or `ac fixtures --golden-check --bless-visual`) and say so in the commit |
| Starting a command | `ac new-command <area.verb>` writes the stub, a fixture skeleton and the registration; `ac commands <id>` prints a command's definition; `ac commands --md` regenerates `docs/commands.md` (a test fails when it is stale) |
| Fixtures, faster | `ac fixtures --only <name> --watch` runs again on every save; `--timings` lists fixtures and steps by time; `AC_LOG=debug` logs every command with its time |
| Before every commit | `cargo xtask check` (run it, and nothing else, as the gate): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` (visual goldens included; `cargo test --workspace` when nextest is missing), `ac fixtures`, and the name, asset and doc guards (`cargo xtask names` / `assets` / `docs`). CI (`.github/workflows/ci.yml`) runs the same |

`--jsonl` (before the subcommand) prints one JSON value a line, for `jq`. Without `--library`,
`ac` works on a scratch copy of the built-in library (said on stderr) and deletes it after, so a
script never touches the app's library. Nothing may be written to the repo root
(`repo_root` test); write under `{tmp}` in fixtures and under `target/` (shots in `target/shots/`)
otherwise.

Never run two cargo commands at once. Builds go to the checkout's own `target/`.

## Looking at the app
For UI work, look at the result: run the app with `--screenshot PATH` (or the MCP `render_preview`
/ `ui_screenshot` tools) and read the PNG. Review and proof images go under
`target/shots/<date>/<topic>/` in this checkout: gitignored, so they stay on your machine (and
`cargo clean` removes them). Never write them outside the repo. For anything that moves, the owner
reviews MP4s in the same folder, not stills: see the `ac-review-video` skill.

## Roadmap
Every piece of work is a file in `roadmap/`, named `roadmap/<number>-<slug>.md` (six digits, spaced by 1000,
the midpoint to insert between two) and renamed `roadmap/DONE_<number>-<slug>.md` when finished, or
`roadmap/OBSOLETE_<number>-<slug>.md` when dropped (by the owner); no subfolders, and prerequisites are named by number (`Requires:`). The `roadmap` skill has the full
rules. Set an item's status to "built, waiting for the owner" when it is built and its images are
ready, and add the `DONE_` prefix only after the owner confirms.
Commit after every feature arc that builds.

## MCP

`ac mcp` serves MCP over stdio (`crates/ac-cli/src/mcp/`). Tools: `command_list {query?, area?,
schema?}` (ranked search over ids, labels and docs), `describe_command {id}` (input and output JSON
Schema, following each parameter's form, the undo label, whether it writes files, and an example
from a fixture), `command_run {id, params, dry_run?}` (`dry_run: true` runs `edit.dry_run`: what it
would return and change, nothing kept), `command_batch {commands, atomic?, label?}` (one
`edit.batch`: one undo step; atomic by default), `editor_state`, `render_preview`, `contact_sheet`
(several frames from one camera, physics included), `animation_preview` (GIF inline or a file;
`mp4`), `expect` (fixture checks now; `save_as` writes a new fixture, never in a repository root),
`session_reset` (the `session.reset` command), `ui_events {since}`, and in live mode
`ui_screenshot`, `ui_layout`, `ui_input`, `ui_click`, `ui_set`. A refusal is an error result whose
`structuredContent` is `CoreError::to_json()` (kind, command, hint, suggestions, the failed step).
Resources: `ac://state`, `ac://skeleton`, `ac://library`, `ac://commands/{id}`. Prompts:
`block_walk`, `fix_foot_sliding` (`clip.metrics` / `clip.critique`), `review_contact_sheet`,
`export_game_engine`. `initialize` returns a `determinism` block (with `session.info`'s rig, limits
and model hashes). It opens the app's library (`%APPDATA%/HumanMotionCraft/library`) unless
`--library` is given. Register it with a client, e.g.
`claude mcp add humanmotioncraft -- <repo>/target/release/ac.exe --assets <repo>/assets mcp`.
It runs its own headless session.

**Live:** start the app with `--control <port>` (e.g. `humanmotioncraft --control 7811`, or `cargo run --release -p ac-app -- --control 7811`) and serve
`ac mcp --live 7811 [--timeout <s>]`: the same tools then run in the window the person sees (same
undo), and `ui_screenshot` returns the whole window. `ui_layout` says where things are (joints, the
timeline's frame-to-x mapping, key row, and every widget's stable id, label, rect and enabled
state), `ui_click {id}` clicks a widget by that id, `ui_input` plays pointer and key events one a frame
(move, down, up, click, scroll, key; modifiers) and replies once the window settles, and `ui_set`
sets window state (the same names as `--ui name=value`, listed in `crates/ac-app/src/main.rs`). The control port speaks JSON lines on localhost
(`{op: run|render|sheet|screenshot|ui|input|click|layout|animation|events|subscribe|unsubscribe}`, see
`crates/ac-app/src/control.rs`): a request's `id` is echoed in its reply, `timeout` (default 120 s,
`AC_CONTROL_TIMEOUT`; `--timeout` on the client) bounds it, and `progress: true` gets progress
lines while it works. A refused `run` replies `{error}`, with `detail` (the structured refusal,
`CoreError::to_json()`) when the app sends it; `ac mcp --live` passes `detail` on.

`python tools/drive_ui.py [--port 7812] [--shots <dir>]` starts the release app on a scratch
library, drives the timeline and viewport gestures through `input` (box-select, key drags and
their undo labels, a refused move, Delete, Ctrl+Z, snapping with and without Alt, wheel zoom,
right-click pins, the Reach and Move tools), checks each, and closes it. Run it after touching
`app/timeline.rs`, `app/viewport.rs` or `app/input.rs`.
