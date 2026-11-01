# Review app: training sets

- **Status:** built, waiting for the owner
- **Source:** owner request (2026-11-01)
- **Requires:** none

## What
A training set is a named, versioned list of clips and labels. A dropdown at the top of `ac-review` picks it, with New, Duplicate and Copy labels from X. The app also reads a `.clip.json` library (`--library`). A set holding private clips is saved outside the repo; the app refuses to write one inside a git checkout.

## Acceptance
Create, duplicate and copy labels through the window and through `review.*` commands, with tests. A set with a library clip refuses a path inside a repo.

## Result
`<folder>/<name>.set.json` (`crates/ac-review/src/training_set.rs`); `--sets` picks the folder (default `training-sets` beside the app's library), `--library` adds a `.clip.json` library whose groups read as sets named `library:<group>`, which also play in the viewer. Commands: `review.sets`, `set_new`, `set_duplicate`, `set_open`, `set_public`, `set_copy_labels`, `set_clip`, `set_remove`, `undo`. Decisions made in writing: with a set open K C R and Backspace change that set, with none the decisions file as before; a library clip is refused by the decisions file; `set_version` goes up with each saved change; copied labels replace those of clips in both sets and add no clips; evaluation-only clips are left out of the library and said. Tests: `crates/ac-review/tests/sets.rs`, in the crate's `training_set.rs`. Written without a build: run them at integration.
