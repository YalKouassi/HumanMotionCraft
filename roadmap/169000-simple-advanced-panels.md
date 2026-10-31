# Advanced reopens what Simple closed

- **Status:** built, waiting for the owner
- **Requires:** none
- **Source:** the 2026-10-31 review of 119500 (findings 4 and 5); the owner's decision

## What
Switching to Advanced changes only the label and the status, so the panels Simple closed stay closed.
`app.reset_layout` reopens everything while the header still says Simple. Both are in
`crates/ac-app/src/app/shell/layout.rs`.

## Acceptance
Simple then Advanced gives the panels back; Reset Layout leaves the mode on Advanced with the header saying so;
the choice survives a restart. Screenshots of each for the owner.

## Result
Built 2026-10-31; not yet compiled, run or looked at by this item's author (the integration worktree
builds it and takes the screenshots).

- Simple saves the panels it actually closed in the new `layout.simple_closed`; Advanced opens those
  again at their recorded places and clears the list. A panel that was not open stays closed, so
  Advanced on a fresh layout opens nothing and the status says so. A panel that cannot come back is
  named in the status.
- The list is saved with the mode and the dock, so Simple, restart, Advanced also gives the panels back.
- Reset Layout: the review's finding 5 did not match the code. `Layout::default()` already set
  Advanced, so the header already said so, and the default dock keeps the tool panels closed rather
  than reopening them. Reset now also clears the list and says it went back to advanced mode.
- New `--ui ui_mode=simple|advanced` and `--ui reset_layout=on` make the screenshots possible. Tests in
  `crates/ac-app/src/app/shell/tests.rs`; no command changed, so no fixture.
- Not kept: which tab was in front of a tab group. The last panel reopened is in front.
