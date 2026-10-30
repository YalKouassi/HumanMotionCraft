---
name: ac-feature
description: Use for ANY feature, fix or refactor in HumanMotionCraft (any crate): a command, tool, panel, interpolation, layer type, solver step, importer/exporter or roadmap row. Gives the command-first checklist, the extension-point rule, the parity (golden) and fixture loop, and the image-confirmation step. Load it before writing code in this repo.
---

# Building a feature

The aim: a feature *adds* a command (and files) through an existing extension point, is tested
headless through that command, matches the checked-in parity goldens where the earlier Python
prototype (cf-animate) already did it, and is
confirmed by a person from images.

## 1. Orient
- Read the item's file in `roadmap/` (the `roadmap` skill has the naming rules) and its acceptance line. No acceptance line: propose
  one and wait for the owner to agree.

## 2. Design check (before any code)
Write down in your reply:
- **The command(s)**: id (`area.verb`), typed `inputs` with ranges, `outputs`, `undoable`, `tool`.
  The UI and MCP reach the feature only through these.
- **The owning crate** and the layer rule it obeys (`AGENTS.md`, "Where things are").
- **The extension point** you use, the files you add, and the existing files you edit. Three or
  more edits beyond registration: add the extension point first, in its own commit.
- **Parity**: is it covered by a parity golden from the earlier prototype
  (`tests/fixtures/golden/*.json`)? Then name the file that proves it. The prototype is not
  public, so no new parity goldens can be made: without one, prove the behaviour with hand-worked
  fixture numbers.
- **Saved data**: new file fields are `#[serde(default)]`; the format version moves only with a
  migration.
- **Rule 1**: every fallback or partial result is announced.

## 3. Fixture first
Write `fixtures/<feature>.json` (format in `docs/llm-guidelines.md`): a script of commands, numbers
worked out by hand, and `errors` (bad input that must be refused without changing the document).

## 4. Implement
- Never crash: no unwrap/expect/panic/indexing on input; every number is hostile; `Result` errors.
- One job per file; aim for under 700 lines. Deterministic: no clocks, no unseeded randomness.
- Use a `craft-*` crate when one covers the job.

## 5. Test
- `cargo test -p <crate>` while working (parity tests read the goldens).
- Before committing: `cargo xtask check` (fmt, clippy, nextest, fixtures, name/asset/doc guards;
  the same as CI).
- A failing rule or golden means the design or the port is wrong. Never loosen a tolerance,
  weaken a check or delete a fixture to pass.

## 6. Look, then ask
- For anything visible, run the app with `--screenshot` (or MCP `render_preview` / `ui_screenshot`) and read the PNG
  yourself first. Proof shots go to `target/shots/<date>/<topic>/` in this checkout (gitignored).
- For anything that moves, the owner gets MP4s, not stills: the `ac-review-video` skill.
- Ask for confirmation in the format from `docs/llm-guidelines.md`. Mark the row `[~]` until the
  owner says yes.

## 7. Commit
The feature, its fixture, any golden it added and doc updates, together.
