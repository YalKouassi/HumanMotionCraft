# Undo and redo, for every command

- **Status:** done (2026-10-30)
- **Requires:** 115300
- **Source:** 115100

## What
`undo_and_redo_round_trip` covers two commands (`crates/ac-core/tests/commands.rs:117`), and
`edit-gesture-undo` covers gestures. Every command already has a fixture, so a generated test can
use them all: run each fixture, and around each undoable step, undo and check the document equals
the one before, then redo and check it equals the one after. Anything that fails is fixed with a
regression test.

## Acceptance
The new test runs every fixture's undoable steps, including `edit.batch` and gestures; any step it
leaves out is listed with the reason; `cargo xtask check` passes.

## Result
2026-10-30. `crates/ac-cli/tests/undo_sweep.rs`. No bug found: undo works from whole-document
snapshots, and every undoable step restored the document, and redo brought it back. It fails if a new
undoable command is neither swept nor listed. Left out, with reasons in the test: `clip.group` (it
moves a library file) and `layer.plan` (the same plan again). Steps inside a gesture aren't undone one
by one. `Document`'s comparison leaves out settings such as the AI sliders, so they aren't checked.
