# Testing strategy

| Layer | Proves | Where |
|---|---|---|
| **Parity (goldens)** | the port does what the earlier Python prototype (cf-animate) did | `tests/fixtures/golden/*.json`, written by that prototype (not public) and checked in; read by each crate's `tests/` |
| **Pose fill** | `posefill.*` without the install, with a fake engine (chaining, the context and targets, every refused gap, bad helper output) and the fixtures' stand-in | `crates/ac-posefill/tests`, `fixtures/pose-fill.json` |
| **Live motion** | `live.*` without the install: the take as a state machine (the stick's smoothing, late replies, the cap, a helper dying with the recording kept) and a scripted take clipped in one undo step. How it feels is only tested by a person playing it | `crates/ac-posefill/src/live.rs`, `crates/ac-posefill/tests/live.rs`, `fixtures/live-motion.json` |
| **Motion from text** | `text.*` without the install, with a stand-in engine, and the setup's progress | `crates/ac-textmotion/tests`, `fixtures/text-motion.json`, `fixtures/text-install.json` |
| **Command extremes** | every command survives every input: each parameter at its limits and just past them, 0, -1, NaN, ±inf, 1e300, wrong type, missing, unknown keys. A refused command says why and changes nothing | one generated test over the registry (`every_command_survives_its_extremes`) |
| **Fixtures** | each behaviour, with numbers worked out by hand | `fixtures/*.json`, run by `ac fixtures` (and `every_fixture_passes`); each names `fixtures/schema.json` (`every_fixture_names_the_schema`), which checks a step's params against its command's inputs in an editor |
| **Bug fixtures** | a reproduced bug stays fixed | `fixtures/bug-<slug>.json`: what went wrong in the description, numbers worked out by hand. Only bugs that were reproduced go in |
| **Review of the built-ins** | `clip.critique` agrees with `clip.possible` on every built-in, the clean clips get nothing medium or high, and every fix it offers helps its own finding and worsens no other when really run | `the_review_of_every_builtin_can_be_trusted` |
| **Quality ratchet** | no built-in animation's motion gets worse: slide, penetration, balance, pops, the worst velocity change, and the root's speed above the keyed speed | `builtin_motion_quality_only_improves` against `tests/quality/builtins.json`. A change either way fails; after looking, bless with `AC_BLESS_QUALITY=1` and say so in the commit |
| **Declared outputs** | every result keeps its command's declared `outputs` (keys, kinds, forms, required ones) | `Session::check_outputs`, on in debug builds and the fixture runner, so every fixture and test checks it; `no_undoable_command_declares_a_session_key` (`changed` is the session's) |
| **Generated files** | `docs/commands.md` and `fixtures/schema.json` match the registry | `the_command_reference_is_up_to_date`, `the_fixture_schema_is_up_to_date`: rewrite with `ac commands --md` / `ac fixtures --schema` |
| **Agent session tools** | batches (atomic and not), dry runs, history and checkpoints, change summaries, reset and info | `crates/ac-core/tests/session_tools.rs`, `fixtures/edit-batch-history.json` |
| **MCP** | every tool reaches the commands: schemas by form, search, describe, batches through `edit.batch`, `dry_run`, structured refusals, resources, prompts, previews, `expect` and the fixtures it saves, reset, events, live ids, progress and timeouts | `crates/ac-cli/tests/mcp.rs`, `crates/ac-cli/tests/mcp_ax.rs` (live mode against a stand-in control port) |
| **Encoders** | GIF bytes are deterministic and decode back; a frame at a time gives the same file | `ac_render::anim` tests; `ac_media::encode` tests |
| **Entry points** | every toolbar button, menu item, shortcut and tool runs a registered command; every command is in a menu and the palette | `every_ui_entry_runs_a_command`, `every_command_is_in_a_menu_and_the_palette` (`crates/ac-app/src/app/shell/tests.rs`) |
| **Live gestures** | the window's pointer and key gestures do what they say | `python tools/drive_ui.py` (needs a window, so not in nextest) |
| **Repo guards** | no other product's name, every asset credited, every path `AGENTS.md` cites exists | `cargo xtask names`, `assets`, `docs` (all in `cargo xtask check`) |
| **Training** | quaternions, retargeting, the export round trip against the `ac-ai` golden | `training/tests/` (pytest; CI runs it with ruff) |
| **Determinism** | same inputs, same bytes | run twice, compare (solver, AI fill, export) |
| **Images** | what a person sees is right | `--screenshot` / MCP `render_preview`, `ui_screenshot` / `ac fixtures --shots` (gallery); approved by the owner |
| **Visual goldens** | a picture the owner approved stays as it was | every fixture view at 320x240 on the software adapter (WARP) vs `tests/visual/`; a pixel differs past 24 levels in a channel, a view fails past 0.2% of pixels (`visual` test, `ac fixtures --golden-check`) |
| **Unit tests** | the maths inside a crate | `#[cfg(test)]` |

## Every command is tested several ways
1. At least one fixture uses it, with its effect checked.
2. At least one `errors` entry: a realistic mistake, refused with a message, nothing changed.
3. The extremes test covers every input it declares.

`every_command_has_a_fixture` checks 1 and 2 for every registered command. A new command's
declared `outputs` are checked by every fixture that runs it, so declare what it really returns
(shared shapes go in the crate's `returns.rs`).

## Tools
- `cargo xtask check`: the whole gate, the same as CI. Run it before every commit.
- `ac fixtures --only <name>` (a misspelt name gets "did you mean"), `--print`, `--bless`,
  `--timings`, `--watch`, `--shots <dir>` (a gallery with golden, actual and diff);
  `AC_LOG=debug` logs every command with its time.
- `ac new-command <area.verb>` writes a command stub, its registration and a fixture skeleton.

## Goldens
- Each golden case carries its `tolerance`. Analytic code (FK, quaternions, two-bone IK, clip
  sampling) matches to 1e-7 or tighter. The full-body optimiser matches to 1e-3 and must also meet
  its goals.
- The parity goldens (`tests/fixtures/golden/`) come from the earlier Python prototype and are
  never regenerated from HumanMotionCraft's own output. The `ac-ai` goldens are rewritten only by
  `python -m ac_train.export`, alongside the weights they pin. A changed golden is called out in
  the commit.

## What a test is not
- Not "the image is not blank" alone.
- Not a copy of the program's current output, unless pinned and said.
