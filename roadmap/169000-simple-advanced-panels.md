# Advanced reopens what Simple closed

- **Status:** not started
- **Requires:** none
- **Source:** the 2026-10-31 review of 119500 (findings 4 and 5); the owner's decision

## What
Switching to Advanced changes only the label and the status, so the panels Simple closed stay closed.
`app.reset_layout` reopens everything while the header still says Simple. Both are in
`crates/ac-app/src/app/shell/layout.rs`.

## Acceptance
Simple then Advanced gives the panels back; Reset Layout leaves the mode on Advanced with the header saying so;
the choice survives a restart. Screenshots of each for the owner.
