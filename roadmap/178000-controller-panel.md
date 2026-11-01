# Controller panel: see the inputs, and press them with the mouse

- **Status:** built, waiting for the owner
- **Requires:** none (158000, which it drives, is done)
- **Source:** owner request (2026-11-01)

## What
Live motion (158000) steers with W A S D or a controller, but nothing shows what the input is doing,
and trying a controller needs one plugged in. Add a Controller panel, toggled from the View menu
(`app.controller`, a checked row in `ACTIONS`, saved as a preference like the other panels).

- **A dropdown at the top:** Keyboard (W A S D), an Xbox layout, a PlayStation layout, and any
  controller actually plugged in, by the name it reports. A plugged-in controller shows the layout
  that suits it. Choosing a layout with nothing plugged in simulates it.
- **Keyboard:** W A S D and Shift drawn as keys. They light up when pressed on the keyboard, and
  pressing one with the mouse holds it down until the button is released.
- **Controller:** sticks, triggers, bumpers, face buttons, d-pad and start/select, drawn in code (no
  vendor art), showing real inputs live. With the mouse: press and drag a stick to tilt it, and it
  springs back to centre on release; click a button to press it; drag a trigger for a partial pull.
- **Steering:** what the panel presses goes down the same path as real input, into `live.input`, so a
  take can be steered with no controller. Mouse input and real input combine; the larger tilt wins.

Decided, for the owner to overrule:
- **Only the left stick and W A S D + Shift steer**, because those are all a take reads today
  (`crates/ac-app/src/app/panels/live.rs:178`). Every other control shows its state and does nothing
  yet, and the panel says so. Giving them jobs is later work.
- **Outside a take, W A S D keep their shortcuts** (W is the Move tool). The keys still light up, but
  aren't stolen. During a take they steer, as now.
- **A held mouse stick works as steering only during a take.** Outside one, it just moves on screen.
- **No new command.** `live.input` already takes the result; the panel is window state, like the
  keyboard and stick reading in `live.rs`. `panels/gamepad.rs` grows from the left stick to the whole
  controller and a list to choose from, and the picture and its hit testing go in a new file.
- **Rule 1:** the panel says when the chosen controller is unplugged, falls back to simulated, and
  says "controllers work on Windows only" elsewhere.

## Decision
The owner agreed the acceptance and allowed the Xbox and PlayStation names as plain input labels
(2026-11-01; recorded in `AGENTS.md`). The owner also asked for a made-up standard controller (two
sticks and four buttons) and a "no controller" choice, so the dropdown has both. No controller ignores
real sticks too.

## Acceptance
- With the panel on and no controller plugged in, in the release app: a take steered by holding the
  on-screen left stick with the mouse, and one by pressing the on-screen W and A. The body answers
  each, and stops when the mouse lets go. An MP4 shows the panel and the body together
  (`target/shots/<date>/controller-panel/`).
- Every drawn control on both layouts lights when pressed with the mouse, driven through `ui_input`.
  Screenshots of both layouts and the keyboard.
- Unit tests, without a window: stick drag to value (clamped to the circle, springs back on release,
  non-finite refused), mouse and real input combined, and each layout's hit areas.
- Unplugging the chosen controller mid-take stops the character, says so and doesn't crash.
- **The owner, with a real Xbox and a real PlayStation controller:** the dropdown names them, and every
  control lights the right one on screen. 158000 never read a real stick and an agent can't test this,
  so it stays open until then. Any controller the library can't read gets written here.
- `cargo xtask check` passes, and the owner plays it and approves.

## Result
**Built (2026-11-01):** `panels/controller.rs` (drawing, mouse, dropdown), `panels/gamepad.rs` (every
control, a list, choosing one), `app.controller` in the View menu, `--ui controller=none|standard|xbox|playstation|off`.
The panel opens as a dock tab, and Simple mode closes it. The dropdown choice isn't saved.

Proof in `target/shots/2026-11-01/controller-panel/`, release app, no controller plugged in:
- `take-mouse-stick.mp4`: on-screen left stick held from 1 s to 6 s. The body travels 18 hip heights
  and settles within about 2 s of letting go (the model's usual stop). `take-mouse-key.mp4`: on-screen W, 6 hip heights, then stops likewise.
- `layout-*.png`: 62 presses across the four choices (5 no controller, 11 standard, 23 Xbox, 23 PlayStation), through the control port: each changed its area when pressed and
  returned exactly on release, 0 failures. That run's log was overwritten by the later take runs, and `drive.py` no longer holds that check.
- The videos were made before one late fix (a tooltip covered the buttons while a stick was held), so they still show it. The fix passed the gate but wasn't driven again.
- Tests in `controller.rs`: stick drag clamped, hostile numbers, larger tilt wins, no controller ignores sticks, trigger, parts fit with no overlap, name to layout.

**Not done or not tested:**
- No real controller has been read: **the owner's check with an Xbox and a PlayStation pad is open**, and so is unplugging one mid-take. That path is written, not run.
- The mouse has one pointer, so it presses one on-screen key at a time: W and A together need the keyboard (or the stick, for a diagonal). A take was steered with W only; A lights up but wasn't steered with.
- Takes in the first drive runs were disturbed: the window was resized and a take started from outside the script, and once it closed. The Windows crash log wasn't checked (the query failed), so a crash isn't ruled out; four later runs stayed up. One of those shots showed the joints standing with the mesh flat on the floor, physics on, not seen again; reason unknown.
