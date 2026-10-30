# Command line: video and clean pictures of the app window

- **Status:** not started (acceptance proposed, waiting for the owner to agree)
- **Requires:** none
- **Source:** owner's question, 2026-10-30: can the CLI output images and video with or without the UI panels?

## What
Today (all run 2026-10-30, Walk clip, release builds):

| Output | Without UI | With UI panels |
|---|---|---|
| Image | `ac render` | `humanmotioncraft --screenshot` |
| Several frames | `ac render --frames`, `--strip` | one shot per run |
| Video | `ac video` (MP4 or GIF) | **nothing** |

This item fills the right-hand column and makes the window's output controllable. `ac render` and
`ac video` are the other column; their options are 160000.

**Video of the window.** Add `--record <mp4|gif>` to the app, with `--from`, `--to` and `--fps`.
Step through the frames, wait for each one to settle, grab the window and write it through the
encoder `ac-media` already has (`Encoder`, as `clip.video` uses). Stepping, not screen recording,
so no frame can be dropped and a slow machine makes the same file. The grab and settle logic to
start from is `--screenshot` (`crates/ac-app/src/app.rs:511`). Quit when done, and exit non-zero if
a frame or the encoder failed (Rule 1). Today `--screenshot` only logs a failed write
(`app.rs:525`) and still exits 0; fix that in the same work.

**Window size.** The window is fixed at 1440x900 (`crates/ac-app/src/main.rs:104`), so a shot can't
be any other size. Add `--size <w>x<h>`. MP4 needs even sizes, as `ac video` already handles
(`crates/ac-cli/src/main.rs:330`).

**Hiding the rest of the window.** `--ui library=off --ui inspector=off --ui timeline=off` hides
those panels, and the menu bar, tool row and status bar stay. `--ui overlay=off` removes the physics
marks, and `--ui handles=body` still leaves the joint dots (tried 2026-10-30). Add `--ui` names for
the menu bar, tool row, status bar and joint dots, following `--ui` (`crates/ac-app/src/main.rs:9`
lists them).

**Not in this item.** Running the window with no display: it needs a real window. `ac render` and
`ac video` already cover machines with no display.

## Acceptance
Proposed; needs the owner's agreement before building:

- `humanmotioncraft --open Locomotion/Walk --record walk.mp4` writes an MP4 of the whole window with
  one frame per clip frame (33 for Walk at the clip's fps, checked with `ffprobe`), then closes and
  exits 0.
- The same with `--ui library=off --ui inspector=off --ui timeline=off` shows only the viewport and
  whatever chrome is left on.
- `--size 1920x1080` gives a picture and a video of exactly that size, and an odd size in an MP4 is
  rounded down to even.
- With the new `--ui` names, a shot shows the viewport alone: no menu bar, tool row, status bar or
  joint dots.
- A failed write (for example an unwritable path, or no ffmpeg for MP4) says why and exits non-zero,
  for both `--record` and `--screenshot`.
- A run without these flags is unchanged: the visual goldens pass with no bless.
- The owner reviews the MP4s (`ac-review-video` skill).
