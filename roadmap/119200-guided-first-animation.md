# A guided path to a first animation

- **Status:** built, waiting for the owner (2026-10-30)
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

## Built (2026-10-30)
After the block-out wizard, a small window offers the next step: play, review, then save or export
(`crates/ac-app/src/app/files/guide.rs`). It never blocks, it can be closed, and "Don't show this
again" is remembered in the preferences. It names how the gaps were filled (AI, plain or none) and
adds no engine commands.

`tools/drive_ui.py` runs welcome page to `Blocked out.glb` through visible buttons. Scaffolding, not
buttons: `export_dir` points the Export dialog at a scratch folder, and the welcome opens through `--ui
dialog=welcome`, as it does for a person only on first run.

- **Reliability:** the full script reached 122 checks with 0 failed on 4 runs in a row, but one run
  before them reported 1 failed, and its output was cut off, so the check is unknown. So
  "deterministic" isn't proven, and the script isn't in CI.
- **Screenshots, for the owner to judge if it is simple enough:**
  `target/shots/2026-10-30/guide/guide-1-welcome.png` to `guide-6-exported.png`.
- **Seen, not fixed:** the library's Recent card for a new animation still read "1 frames" after the
  block-out made 31.
