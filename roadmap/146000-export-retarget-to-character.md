# Export onto your own character (retarget)

- **Status:** not started; acceptance line proposed, waiting for the owner to agree
- **Requires:** 144000, 122500
- **Source:** 144000 (one rig, retarget on export)

## What
- Import a character's skeleton (.glb) as a target; guess the bone mapping from names, let the
  person correct it, align the rest poses (A-pose, T-pose, bone roll); save it as a mapping file.
- `clip.export` / `clip.export_batch` take the mapping as `target`.
- Keep contacts: scale the root path by the hip-height ratio, leg IK on planted frames (the
  contacts the app already computes), optional hand targets.
- Extra spine bones are shared out from ours; bones we can't drive stay at rest, and the result
  lists them.
- Commands first: e.g. `retarget.load`, `retarget.map`, `retarget.check`.

## Acceptance (proposed)
A test character with different proportions, an A-pose and other bone names exports a walk whose
planted feet move under 1 cm and stay above the floor; a fixture checks the guessed mapping and
the list of undriven bones; the mapping file round-trips byte for byte.
