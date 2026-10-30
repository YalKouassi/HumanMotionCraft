# Command line: render options for clean pictures and video

- **Status:** not started (acceptance proposed, waiting for the owner to agree)
- **Requires:** none
- **Source:** owner's question, 2026-10-30: can the CLI output images and video with or without the UI panels?

## What
`ac render` and `ac video` work with no window (run 2026-10-30, Walk clip: 33-frame MP4 in about
1.4 s), but every picture looks the same: the dark background and the floor grid, from a named
camera. Their options stop at frames, view and size (`crates/ac-cli/src/main.rs:222`, `:301`), and
`clip.video` takes only `path`, `from`, `to`, `fps`, `view`, `width`, `height` and `fixed`. So the
output can't be dropped onto another background or framed by hand.

Add, as parameters on the commands first and then as CLI flags, so MCP and the app's Export preview
get them too (everything is a command):

- **Hide the floor.** `--no-floor`. The floor pipeline is drawn at
  `crates/ac-render/src/renderer.rs:420`.
- **Background.** `--background <colour>`, or `transparent`. Transparent is PNG only: the H.264 MP4
  has no alpha, so asking for it there refuses and says so, as does GIF unless its one-bit alpha
  looks acceptable (decide when building, and show the owner).
- **Your own camera.** `--camera <azimuth,elevation,distance[,target]>` alongside the named views
  (`VIEWS`, `crates/ac-media/src/shots.rs:14`). The units are radians and metres. Values are
  input-derived, so they get finite checks and caps.

Defaults stay as they are, so existing renders don't change.

## Acceptance
Proposed; needs the owner's agreement before building:

- `ac render --script walk.json --frame 8 --background transparent --no-floor --out a.png` writes a
  PNG with the body opaque and every other pixel at alpha 0 (checked in a test).
- `ac video --script walk.json --no-floor --background #00ff00 --out a.mp4` writes a video of the
  body on flat green, with no grid.
- `--background transparent` with an `.mp4` refuses, and the message says PNG is the format to use.
- `--camera` with the values a named view uses gives the same picture as that view (pixel
  tolerance), and non-finite or out-of-range values refuse with a reason.
- `clip.video`, `render_preview` and `animation_preview` accept the same options, documented in
  `docs/commands.md`, with a fixture for each.
- With none of the options, the visual goldens pass with no bless.
- The owner reviews a still and an MP4 (`ac-review-video` skill).
