# Review Animation panel

- **Status:** built, waiting for the owner (2026-10-30)
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

## Built (2026-10-30)
A Review section in the inspector, also dockable (`crates/ac-app/src/app/panels/review.rs`). "Review
animation" in the File menu and the palette opens the same answer. It runs when asked, because it
tries fixes on a copy, and its answer is dropped when the animation changes. Findings mark the
timeline in the physics panel's colours.

`tools/drive_ui.py` passes (115 checks, including these): the fixture clip's slide shows, Go
to problem moves the playhead into the span, Fix lowers the total slide from 24.0 to 0.3 cm and says
so, and one Ctrl+Z restores the numbers and the keys. The script isn't in CI, as before.

Screenshots: `target/shots/2026-10-30/review/` (`review-problem.png`, `review-fixed.png`,
`review-clean.png`).

For the owner to judge: on `Standing/Idle` the Physics section lists "Feet in the ground 0-178" while
Review finds nothing. Both are right as written: Physics reports what its pass corrects in the keys,
and Review measures what plays. It may still read as a contradiction.
