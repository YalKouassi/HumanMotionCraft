# Reports in windows, so text doesn't move the panel

- **Status:** built, waiting for the owner (2026-10-31)
- **Requires:** none
- **Source:** owner, 2026-10-31, on the AI in-between panel's fill report: "This is all SLOP, i don't want text to move ui elements around, they should be in the messages log ... OR a button to open a floating panel, this is the same for the red/yellow warnings on the physics area"

## What
The fill report and the physics problem list grew inside their panels and pushed everything below
them down. Both go behind a button that opens a floating window.

## Acceptance
Taken from the owner's request:

- **AI in-between:** a Fill report button beside Fill all gaps, always there, off until a fill has
  reported, with the count of things to look at. The window holds the report, and a click on a line
  still goes to its frame.
- **Physics:** a Problems (n) button, off when there are none. The window holds the list, and a click
  still goes to the frame.
- Filling doesn't move any widget below. Checked in the running app by comparing widget positions
  before and after Fill all gaps.
- The panel's other changing text doesn't move anything either: the gap line is one line with the rest
  in its hover, the reason Preview and Fill are off is in the hover, "too long for the model" is in Fill
  all's hover, the working/last-preview row keeps its height, and failures go to Messages.
- `cargo xtask check` passes. The owner approves it from the screenshots.

## Result
Built 2026-10-31. A floating button won over the messages log because the lines are clicked to go to a
frame, and the log doesn't do that. The timeline marks are unchanged.

In the running app on Walk 12, Fill all gaps took it from 46 to 90 keys and raised 50 flags: none of the
20 AI, text, physics and pose fill widgets on screen moved. Both windows opened from their buttons, and
a click on a physics problem went to its frame (a click on a fill report line wasn't tried).

The Off note under the Physics switch moved into the switch's hover too.

**Not changed:** the Range panel, the install panels and the Layers note still put some text inline, and
"The animation changed since the preview." still appears with the preview's own controls. The report
itself is still long (50 flags on this clip): it moved, it wasn't shortened.

Review: `target/shots/2026-10-31/panel-reports/ai-before-fill.png`, `ai-after-fill.png`,
`fill-report-window.png`, `physics-problems-window.png`.
