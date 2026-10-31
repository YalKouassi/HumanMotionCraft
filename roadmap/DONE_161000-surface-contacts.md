# Surface contacts: a hand or foot held on a wall for a set of frames

- **Status:** done (2026-10-31)
- **Requires:** none
- **Source:** owner's question about wall flips (2026-10-30)

## What
A run into a wall that kicks off into a backflip can't work today, even with a perfect model. The
physics pass only knows the floor and the tops of boxes, so a foot on a wall counts as being in the
air. If the last floor step to the landing is under `MAX_AIR`, the hips become one parabola between
them and the kick disappears. This is separate from the AI: the model is not told about contacts or
walls, and this item doesn't change that.

What exists (checked 2026-10-30):
- `feet.plant` holds one foot through a frame range at its mean position, with leg IK and tagged keys
  (`crates/ac-core/src/keyops.rs:172`). Feet only.
- `pin.set` holds a world position while dragging a pose (`crates/ac-core/src/editor.rs:379`). It has no
  frames and doesn't reach playback, fills or physics.
- `surface.add` makes a box, but height comes from x and y alone (`crates/ac-core/src/physics.rs:135`),
  so its side faces don't exist for physics. Rule 1 lifts the whole frame to the top of a box when a foot
  point crosses its footprint (`physics.rs:670`).
- `on[f]` decides where flight starts and ends, and is only true near the ground or when the body
  rests on it (`physics.rs:703`). A hand near a box top should already count, so a low vault may split.

To build:
1. **`feet.plant` gains hands.** It holds a hand as well as a foot through a frame range (arm IK), at
   any height. Calls with `foot: L | R` keep working.
2. **The contact is recorded on the document:** bone, frame range and position, saved with it and
   undoable, so physics can read it. An AI fill or a key move doesn't lose it.
3. **Physics counts it.** A frame with a contact is `on`, so flight splits into run-up and kick-off to
   landing with no new flight rule. The held point stays where it was on those frames: the fulcrum,
   balance and flight rules don't move it.
4. **Boxes stop lifting through their sides.** A foot point that enters a footprint from the side,
   below the top, isn't lifted. A foot that comes down on the top is still supported, as now.

Left out: telling the model about contacts, walls that aren't boxes, moving surfaces, sliding along a
wall, and a wall-run style or add-on (see 154000).

## Acceptance
- `fixtures/surface-contacts.json`: a foot and a hand held on a box's side for a frame range, one undo
  step each; the contact survives save and reload, an AI fill and a key move; turning it off removes it.
- Tests in `ac-core` for physics with a contact: the held point doesn't move on those frames, flight
  splits at the contact (take-off is the contact's last frame), and the same clip without the contact
  still gets one parabola.
- Tests for boxes: a toe crossing a tall box's side causes no lift; landing on a box's top lifts as
  before. Existing physics goldens are unchanged, or any change is listed under `## Result` and approved.
- A hand-keyed wall flip (run, foot on a wall, backflip, landing) with physics on, in an MP4 for the
  owner (`ac-review-video`), next to the same keys with physics off.
- `cargo xtask check` passes, and the owner approves the video.

## Result
Owner reviewed the wall flip and is happy (2026-10-31). Built 2026-10-31. **No physics golden changed**, so there is none to re-approve.

- **`feet.plant` takes `hand: L | R`** as well as `foot`, at any height, with arm IK. It takes one or the
  other and refuses both or neither. `on: false` removes the hold.
- **The hold is on the document:** a `contacts` list on the clip (bone, from, to, at), saved in the clip
  file and listed by `layers.list`. The `clip.contacts` command is a different thing, the foot strip's
  planted spans, and is unchanged. Older files have none. Insert and delete frames move it as they move markers.
- **Physics counts it** (`physics::solve_held`; `solve` is the same with no holds). In the hand-keyed
  flip below, with the foot held on 19–23, the flight is 23–42: take-off is the contact's last frame.
  The same keys not held give one parabola, 17–42, straight over the kick.
- **The point doesn't move.** The flip's ankle is at the same place on frames 19, 21 and 23, to 15
  decimal places. The limb bends to reach it, and if it can't, the body moves until it does.
- **Boxes.** A foot point coming in through a side, below the top, isn't lifted until it leaves the
  footprint; one coming down on the top is lifted as before.
- **Beyond the list.** A joint more than `RESTING` below a box's top no longer counts as resting on it.
  Without this, a hand inside a tall box's footprint stopped the fulcrum rule and split flights.
- **Tests.** `crates/ac-core/tests/contacts.rs` (8) and `fixtures/surface-contacts.json`. `cargo xtask
  check` passes: 82 fixtures, and the visual goldens with no bless.

Things that behave differently, or that this doesn't do:

- **A floor plant now records a hold too**, as item 2 asks, so with physics on the planted foot is kept
  exactly at the plant's point. Plants made before this have no record and act as they did.
- **A hold is a fixed point in the world.** Moving the whole animation, or mirroring it, doesn't move
  the hold, so the body goes back to the old point. Take the hold out and plant it again.
- **A key move doesn't shift a hold's frames.** It keeps its range, and physics still holds the point.
- **No button for hands yet.** Hands are through the command and MCP; the timeline's plant is still
  feet. The What didn't ask for a button.
- **`feet.plant` with `on: false` still removes both feet's in-between keys**, as it always has, while
  taking out only the named limb's hold.
- **Other readers of the floor height aren't changed.** The contact phases, the metrics and the
  viewport's overlay still treat everything in a footprint as over its top.
- **`ac video` doesn't draw scene boxes**, so these videos come from the window (`--record`, 159000).

Review files, `target/shots/2026-10-31/surface-contacts/`, all the app window at 960x540 a side, from
the left with no joint dots:

- `wall-flip.mp4`: `KEYS, PHYSICS OFF` beside `PHYSICS ON, FOOT HELD`, the same keys. The foot is on the
  wall from 0:00.63 to 0:00.77. This is the one to approve.
- `wall-flip-quarter-speed.mp4`: the same, slowed four times.
- `held-or-not.mp4`: `PHYSICS ON, NOT HELD` beside `PHYSICS ON, FOOT HELD`.

The flip is keyed by a script, to test the physics. It isn't good animation, and its foot doesn't lie
flat on the wall. Keys for the run-up put the feet under the floor on some frames, which physics lifts
(`Feet in the ground` in the app); that is the keying too.
