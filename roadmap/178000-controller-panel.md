# Controller panel: see the inputs, and press them with the mouse

- **Status:** owner's decision first (the acceptance below, and the naming question)
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

## Question
`AGENTS.md` keeps other products' names out of code and docs (`cargo xtask names`), with one exception:
the export presets. "Xbox" and "PlayStation" need a second exception limited to this panel's labels.
Recommendation: allow it, for the same reason as the presets (people need to recognise theirs), with
the buttons' letters and shapes drawn by us. The alternative is neutral labels: "A B X Y layout" and
"Shapes layout". The gate changes only on the owner's word.

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
