# Surface contacts: a hand or foot held on a wall for a set of frames

- **Status:** not started; acceptance agreed by the owner (2026-10-31), who is fine re-approving any physics golden it changes
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
