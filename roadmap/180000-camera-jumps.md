# The camera jumps while rotating

- **Status:** not started
- **Requires:** none
- **Source:** owner's bug report (2026-11-01); measured the same day by driving the app; the owner agreed the acceptance (2026-11-01)

## What
Orbit itself is sound: right-drags of 40 px and 60 px moved smoothly, with no jump when the drag
began (egui holds back the first few pixels). The camera only leaves the drag's path in these places
(`self.camera` is written nowhere else but zoom and pan):

1. **The number keys, with only the pointer over the view.** 1, 3, 7 and 0 (Ctrl for the opposites)
   set yaw, pitch and target at once, with no modifier or click (`shell/actions.rs:275`,
   `view_camera.rs:37`). A stray press looks exactly like jumping to a specific position, and any pan
   is lost because the target goes back to the body. The status bar then says `view: <name>`.
2. **Follow snaps the target.** With Follow on, the target is set to the root every frame
   (`view_camera.rs:90`), so any jump in the root moves the view at once: scrubbing, a loop
   restarting, or opening another clip. Measured: frame 0 to 30 of a 4 m move took the target from
   x = 0 to 3.48 in one frame, and back. Turning Follow on does it too. Starting a live take turns
   Follow on and moves the view; stopping a take recentres it (`panels/live.rs:110`, `:120`). Follow
   is saved in the preferences, so it can be on without having been turned on this session.
3. **Top and Bottom sit past the orbit's limit.** The views use pitch ±1.45 (`ac-render`
   `offscreen.rs:19`), orbit clamps to ±1.4 (`viewport.rs:140`). Measured: after Top, a drag that
   asked for more pitch dropped it from 1.45 to 1.40, a small snap the wrong way.

Not reproduced: a jump from a slow frame. The drag applies the pointer's whole movement since the
last frame without a cap, so a long stall could turn the view sharply, but scripted input can't show
it. Which of these the owner has been hitting is not known; if a jump remains after this item, the
status bar text and whether Follow is on will tell them apart.

## Decision
The number keys stay as bare keys over the view, as the agreed acceptance has them (owner, 2026-11-01).
They are the shortcuts documented for these views and can be rebound in Help > Keyboard shortcuts.
Changing to a smooth move (below) makes a stray press easy to read and hard to mistake for a bug.

## Build
- One pitch limit, ±1.45, shared by the views and the orbit.
- A camera change that isn't the person's drag eases over about 0.2 s: the views, Frame selection,
  and Follow when the root moves more than 0.5 m in one frame. Ordinary Follow stays locked to the
  root. A drag or wheel during the ease ends it.
- The preference for easing is not added; scripted and `--ui camera=` changes stay instant, so
  fixtures and pictures don't change.

## Acceptance
- `tools/drive_ui.py`: after Top, a drag keeps pitch continuous (no change bigger than the drag asks
  for); pressing 1 reaches the Front view within 0.3 s through in-between values.
- With Follow on, frame 0 to 30 of a 4 m move: no single window frame moves the target more than
  half the distance, and it arrives within 0.3 s; while playing normally the target matches the root.
- The camera keys' existing checks in `tools/drive_ui.py` still pass (waiting for the ease where
  they read the camera straight after a key).
- An MP4 of the Follow and view-key cases for the owner (`ac-review-video`).
- `cargo xtask check` passes.
