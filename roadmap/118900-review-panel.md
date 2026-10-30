# Review Animation panel

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 118600
- **Source:** 115100

## What
"Review animation" is in the File menu and the palette, and its answer goes to the status line
(`crates/ac-app/src/app/shell/actions.rs:453-455`). Give it a visible button and an inspector panel
(`crates/ac-app/src/app/panels/`), acting through commands:
- findings, worst first: what is wrong, where, why and what the fix changes;
- each marks the timeline (`TimelineMarks`) and has "Go to problem";
- "Fix" runs its batch as one undo step and shows the measure before and after;
- it says plainly when nothing was found, and what it didn't check (`notes`).

Share marks with the Physics and Range panels rather than a third look, and with 138000, whose fill
diagnostics are the same idea for a fill.

## Acceptance
Through `drive_ui.py` or `--control`: a clip with a known slide shows its finding, Go to problem moves
the playhead into the span, Fix lowers the number shown and one Ctrl+Z restores the clip;
screenshots of a clean clip and a problem clip for the owner.
