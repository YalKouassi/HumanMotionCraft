# Project files

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 101000, 104000
- **Source:** old `ROADMAP.md` row 4

## What
Save and open several clips, the scene boxes and the layers as one versioned project file.

## Decision (2026-10-30, the agent's call; the owner may overrule)
A project is **one self-contained JSON file** that holds the whole working document, so it can be
sent to someone or kept outside the library:

- `format: "<product id>.project"` (named through `product::format` / `is_format`; a former id
  still reads), `version: 1`. The file name ending the dialogs use is `.project.json`; any name
  opens, the `format` field decides.
- `clip`: the open animation, as a clip file object (keys, every layer, the scene boxes
  (`clip.surfaces`: they were already document data, saved in the clip file, so no new data was
  needed), the physics switch, markers).
- `links`: every clip the link layers reach, by library id, each embedded as a clip file object
  (the file opens without the library; the embedded clips play, not the library's; adding a link
  layer to a project document keeps the embedded clips and only fetches what they lack).
- `edit`: the layer keys go on; `frame`: the playhead; `pins` and `locked`: the solver's pins
  and the locked parts; `ai`: the AI fill settings (`ai.settings`).
- Not in it: the library, the preferences, derived data (poses, meshes, the physics cache). No
  `origin` field either: the clip's own `name` and `group` say where it came from.

Commands (ac-io, `crates/ac-io/src/project.rs`): `project.open {path}` (one undo step, replaces
the document; the document then belongs to the project: `clip_id` is cleared and Save goes to
the file), `project.save {path?}` (`.writes()`; `path` defaults to the file the document came
from or was saved to; with neither it refuses and says to give one), `project.info {path}`
(reads without opening: version, counts, size, FNV hash of the bytes). Extension point first:
`Document::project` (`ac_core::ProjectFile`: the path and what the file holds), so `unsaved`
compares a project document with its file; `editor.state.project`.

Migration: version 0 was the format's first cut (links as a `[{id, clip}]` list, the playhead as
`playhead`, no `ai` block); `project.open` migrates it to version 1 and says so (`migrated`,
`version`). `fixtures/data/project-v0.project.json` is a hand-written version-0 file (identity
rotations, so every number is worked by hand); `project-future.project.json` (version 99) and
`project-corrupt.project.json` are refused. A future version is refused with "written by a newer
version", never guessed at. A file for another rig is refused too.

Window: File > Open project… (Ctrl+Shift+O: a file dialog, then `project.open`; asks first when
the open animation has unsaved changes), Save project (Ctrl+Shift+S: `project.save` to the
project's file, or the dialog when there is none yet), Save project as… (an app action), Project
file info… (the command's form); Ctrl+S and the timeline's Save button save a project document
to its file; a dropped `.project.json` opens; the window title reads `<project> – <product>`
while a project is open; the dialogs start beside the open project, else where one was last
(`files.project_dir` in the preferences).

## Acceptance
Save, close and open gives a byte-identical document; an older version loads through a migration
with a fixture.

## Result
- `fixtures/project-files.json`: an animation with two keys, a link layer on Locomotion/Walk, a
  box, AI settings, a lock, a pin and the playhead at 4; `project.save`, `session.reset`,
  `project.open`, then the roots, box, link (not missing without the library), settings, lock,
  pin and playhead read back; the second save and `project.info` give the same hash as the first
  (`c8591468c957e18c`, pinned: byte-identical). The version-0 file migrates (frame 7, roots
  interpolated to `[0.5, 0, 1]` at frame 10, its Ledge box and embedded link). Undo takes the
  open back off. Refused with the document unchanged: a missing file, a rig file, a corrupt
  file, a version-99 file, a save with no path and no project.
- `crates/ac-io/tests/project.rs` (4 tests): save → reset → open → save compares the files'
  bytes directly; the version-0 migration and a re-save as version 1; the refusals and their
  reasons; the migration's duplicate-id check.
- `cargo xtask check` passed (see the commit).
- Screenshots in `target/shots/2026-10-30/project-files/`: `project-open.png` (the project open:
  the box, the Walk layer, the lock and pin, frame 4, Saved) and `file-menu.png` (the File menu
  with the project entries). Driving the window over the control port: the title read
  `jump – HumanMotionCraft`, a key made it unsaved, File > Save project wrote the file
  ("saved the project jump (2 clips, 288 KB)") and it read Saved again.
- Not done: no "recent projects" list (the library's Recent menu is for library ids); the
  recovery file of a project document is kept under "untitled" (it is offered back as a new
  animation, not as the project). Both are small follow-ups if the owner wants them.

## Review (2026-10-31)
Reviewed `crates/ac-io/src/project.rs`, `crates/ac-io/tests/project.rs`, `fixtures/project-files.json` and
`fixtures/data/project-v0.project.json` against the What, Acceptance and Result (commit 6bf26f4).
- **Fixed (low, Rule 1):** `migrate` (`crates/ac-io/src/project.rs:192`) moved `playhead` onto `frame` even
  when a version-0 file already had a `frame`, overwriting it silently. Refused now ("both playhead and
  frame"). Test `migrate_refuses_a_version_0_file_that_names_the_playhead_twice`.
- **Test gap, closed:** `a_version_0_file_migrates` checked the frame and the link but not the lock, the
  pins, the edit layer or the AI defaults the v0 file carries; all asserted now.
- **Checked and fine:** hostile numbers (`number()` demands finite and bounded: frame 0..=MAX_FRAME, pins
  ±1e4, AI settings in range; `edit` must name a layer; pin bones and locked parts are checked against the
  rig in `project.open`; linked clips must share the clip's bones, at most 256; the file size is capped by
  `MAX_FILE_BYTES`); version 99 is refused before the migration (`load_text`, `:215`); a non-object, a clip
  file and a rig file are named for what they are; save, open, save is byte-identical by the test's bytes
  and the fixture's pinned hash; the migration covers the three version-0 differences exactly (links list
  to map, duplicate ids refused; playhead to frame; no `ai` to defaults).
- **Not fixed, noted (low):** a version-1 `links` object with one id twice is merged silently by the JSON
  parser (last wins), as duplicate keys in a clip file are; only a hand-edited file has them. `"version":
  1.5` is refused as "no version", a misleading wording. `project.save` drops the links the clip no longer
  reaches without counting them in the result (that drop is the point of the save).
- The Result's claims hold: the 4 tests (5 now), the fixture, the hash pinned and marked pinned.
