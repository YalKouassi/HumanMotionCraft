# Command line: video and clean pictures of the app window

- **Status:** done (2026-10-31)
- **Requires:** 160000
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
Agreed by the owner 2026-10-31:

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

## Result
Owner reviewed the MP4s and is happy (2026-10-31). Built 2026-10-31. It requires 160000, which is built and waiting for the owner too.

- **Recording.** `--record <mp4|gif>` with `--from`, `--to` and `--fps` (`crates/ac-app/src/app/record.rs`).
  Walk gives 33 frames by `ffprobe`, then the window closes and exits 0. It takes about 3 s.
- **Size.** `--size <w>x<h>` is in the display's pixels, whatever its scale: 1920x1080 came out exactly
  that, and 1281x721 as an MP4 came out 1280x720. A GIF or a PNG keeps an odd size. A window that won't
  take the size fails and says so.
- **Hiding.** `--ui menu=off`, `tools=off`, `status=off` and `joints=off`. With the menu, tool row and
  status bar all off, the viewport runs to the window's edges. Keep `overlay=off` too, or the physics
  marks stay.
- **Failures.** A write that fails says why and exits 1, for `--record` and `--screenshot`; so does no
  ffmpeg. Beyond the list: with `--record`, an `--open` or `--script` that fails also stops the run, so
  it can't record the wrong animation (`--screenshot` still carries on and shows it in the status bar).
- **Tests.** `crates/ac-app/tests/record.rs` opens real windows, so its tests are `#[ignore]` and run by
  hand: `cargo test -p ac-app --test record -- --ignored --test-threads=1`. All 3 pass here. `cargo xtask
  check` runs only the argument and cropping unit tests, so **no automatic check opens a window.**
- **Not proved.** Only run at 100% display scale, on this machine. The scale handling hasn't been seen
  at 125% or 150%. A minimised window isn't drawn, so a recording would wait until it is restored.

Review files, `target/shots/2026-10-31/window-capture/`:

- `whole-window.mp4`: the whole window. Good is the playhead and the `Frame n` readout going up one at
  a time, with the body moving in step.
- `no-panels.mp4`: library, inspector and timeline off; the menu, tool row and status bar stay.
- `viewport-alone.mp4`, `viewport-alone.png`: nothing but the 3D view, 1280x720.
- `1920x1080.mp4`: the size option.
