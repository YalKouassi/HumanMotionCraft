# A guided path to a first animation

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 118900
- **Source:** 115100

## What
The pieces exist: the welcome page and the block-out wizard (`crates/ac-app/src/app/files/welcome.rs`,
`files/block.rs`) take someone from poses to a filled animation. The path then stops, with no lead
to previewing, reviewing or exporting.

Carry on from the wizard: choose poses, place keys, fill (AI or plain, as the wizard already offers,
and named as such), preview, review (118900), save or export. Each step offers the next action and
none blocks someone who wants to leave. No new engine commands should be needed. AI Fill stays the
model: this never routes it through the Walk loop.

## Acceptance
A scripted run through the window goes from the welcome page to an exported file through visible
buttons only; it is deterministic and in `drive_ui.py`; screenshots of each step for the owner, who
judges whether it is simple enough.
