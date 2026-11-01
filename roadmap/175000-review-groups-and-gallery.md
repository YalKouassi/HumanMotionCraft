# Review app: group view and gallery

- **Status:** built, waiting for the owner
- **Source:** owner request (2026-11-01)
- **Requires:** 174000

## What
Rows of groups (pack, name, clip count, total minutes, thumbnail); keys keep or reject a whole group, and a group opens to override single clips. A gallery of thumbnails for mass selecting and labelling, staying smooth at 7,500 clips.

## Acceptance
A group decision is one undo step and a single clip can override it. Multi-select in the gallery labels or rejects all selected. Only on-screen thumbnails load. Screenshots for the owner.

## Result
List, Groups and Gallery views at the top of `ac-review`. A group is the takes of a pack that differ only by a trailing number (`crates/ac-review/src/groups.rs`); `review.groups`, `review.set_group`, and `review.undo` / Ctrl+Z take a group decision back as one step. A thumbnail is a line figure of the clip's middle frame made on a worker only while on screen (`crates/ac-review/src/thumbs.rs`), not a rendered picture, so no texture is kept per clip. Picking uses click, Ctrl-click and Shift-click. Screenshots: not taken, as this was written without a build; `ac-review --library <dir> --sets <dir> --set <name> --view groups|gallery --screenshot <png>` makes them, and the smoothness at 7,500 clips is still to be seen in the window.
