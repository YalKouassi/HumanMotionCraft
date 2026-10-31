# Command line: render options for clean pictures and video

- **Status:** done (2026-10-31)
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
  has no alpha, so asking for it there refuses and says so, and GIF refuses the same way (the
  owner's decision, 2026-10-31).
- **Your own camera.** `--camera <azimuth,elevation,distance[,target]>` alongside the named views
  (`VIEWS`, `crates/ac-media/src/shots.rs:14`). The units are radians and metres. Values are
  input-derived, so they get finite checks and caps.

Defaults stay as they are, so existing renders don't change.

## Acceptance
Agreed by the owner 2026-10-31:

- `ac render --script walk.json --frame 8 --background transparent --no-floor --out a.png` writes a
  PNG with the body opaque and every other pixel at alpha 0 (checked in a test).
- `ac video --script walk.json --no-floor --background #00ff00 --out a.mp4` writes a video of the
  body on flat green, with no grid.
- `--background transparent` with an `.mp4` or a `.gif` refuses, and the message says PNG is the format to use.
- `--camera` with the values a named view uses gives the same picture as that view (pixel
  tolerance), and non-finite or out-of-range values refuse with a reason.
- `clip.video`, `render_preview` and `animation_preview` accept the same options, documented in
  `docs/commands.md`, with a fixture for each.
- With none of the options, the visual goldens pass with no bless.
- The owner reviews a still and an MP4 (`ac-review-video` skill).

## Result
Owner reviewed the still and the MP4s and is happy (2026-10-31). Built 2026-10-31. `cargo xtask check` passes: 81 fixtures, and the visual goldens
with no bless.

- **The options.** `floor`, `background`, `azimuth`, `elevation`, `distance` and `target` on
  `clip.video`, `render_preview`, `animation_preview` and `contact_sheet`; flags `--no-floor`,
  `--background` and `--camera` on `ac render` and `ac video`. `Offscreen::render` is the one place
  the look is applied, so no path can miss it.
- **Added beyond the list.** `clip.picture` (one PNG), because `clip.video` can't be transparent and a
  transparent picture should still be a command. `clip.sheet` takes the options too, and the Export
  preview dialog has Floor and Flat background, as What asks.
- **Tests.** `crates/ac-cli/tests/render_options.rs` covers each CLI acceptance line, reading a frame
  back out of the MP4. `picture_tools_take_the_look_options` (`mcp_ax.rs`) covers the two MCP tools.
  A fixture for each: `clip.video` and `clip.picture` are in `fixtures/export-preview.json`; fixtures
  can only run commands, so the MCP tools are held by that test instead of a fixture.
- **The running app.** Checked through `ac mcp --live`: `render_preview`, `contact_sheet` and
  `animation_preview` take the options, a picture after them is byte-identical to one before (the
  same renderer draws the library's thumbnails), and a transparent animation refuses. The dialog
  exported a green GIF with the floor off.

Review files, `target/shots/2026-10-31/render-options/`:

- `transparent-still.png`: the body and nothing else. An image viewer shows the clear part as its own
  background.
- `green-screen.mp4`: the body walking on flat green, no grid. Good is one even green to every edge.
- `own-camera.mp4`: `--camera 2.2,0.5,5.5`, from behind and above, with the floor as usual.
- `from-the-dialog-green.gif`, `dialog-flat.png`: the dialog's own export, and the dialog.
