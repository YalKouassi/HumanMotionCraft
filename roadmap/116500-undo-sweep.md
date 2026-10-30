# Undo and redo, for every command

- **Status:** not started; the acceptance line needs the owner's agreement
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
