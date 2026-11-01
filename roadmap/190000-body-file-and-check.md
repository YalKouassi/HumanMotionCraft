# A body file and its check

- **Status:** not started; later (owner, 2026-11-01)
- **Requires:** 189000
- **Source:** owner's request (2026-11-01): lay the foundation for swapping in other meshes on the same rig

## What
Define what a body is, in one place, before anything swaps one in.

A body is a `.glb` skinned to the rig's 104 joints, with a small block of details in the file: name, where
it came from, its licence, height and hip height. `crates/ac-render/src/mesh.rs` already reads skinned
meshes with several parts, and `crates/ac-io/src/export.rs` already takes joints from inverse bind
matrices, so this is mostly the rules and the check. The file format goes through `product::is_format`.

`assets/rig/mannequin.glb` stays byte for byte as it is and becomes the first body. Its details sit in a
small list beside it.

`body.check` lists every problem by name and never crashes. It refuses a body when:

- a joint is missing or extra, or a parent differs from the rig's;
- a rest direction is too far from the rig's, for example a T-pose on our A-pose. Start at 10 degrees and
  tune it on real bodies;
- it isn't in metres, isn't Z up or doesn't face -Y once read;
- a vertex's weights don't add up to 1, or it has no weights;
- height or limb ratios fall outside the human range agreed in 189000;
- it has no licence;
- it is too large (`MAX_VERTICES`, a hostile file is refused, not allocated).

The check can only judge joints and proportions. It can't prove that a surface looks human.

## Acceptance
- The mannequin passes, and its file is unchanged.
- Fixtures made by a test, one fault each, are refused with the right named reason: missing bone, wrong
  parent, T-pose, centimetres, Y up, facing +Y, bad weights, no licence, too large.
- A fixture with longer limbs and the same rest directions passes if 189000 allows its own lengths.
- `body.check` is a command with JSON output, and a body's details survive saving and loading byte for byte.
