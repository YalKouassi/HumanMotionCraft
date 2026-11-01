# A body file and its check

- **Status:** not started; later (owner, 2026-11-01)
- **Requires:** 189000
- **Source:** owner's request (2026-11-01): lay the foundation for swapping in other meshes on the same rig

## What
Define what a body is, in one place, before anything swaps one in. The creator will live in another repo
(189000), so this is a published contract as well as code here.

A body is a `.glb` skinned to the rig's 104 joints, with a small block of details in the file: name, where
it came from, its licence, height, hip height and which version of the rig it was made for.
`crates/ac-render/src/mesh.rs` already reads skinned meshes with several parts, and
`crates/ac-io/src/export.rs` already takes joints from inverse bind matrices.

What another repo gets:

- **A small crate, `ac-body`:** the rig's joint list, reading and writing the details, and the check, with no
  GPU or app dependencies. Another repo can take it by git revision, as we take craft-libs. This repo's code is
  GPL-3.0-or-later, so an app linking it must be under a compatible licence.
- **A written format,** `docs/body-file.md`, so the file can be understood without the crate.
- **A rig version.** If the rig ever changes, an older body is reported as made for another version, not
  misread.

`assets/rig/mannequin.glb` stays byte for byte as it is and becomes the first body. Its details sit in a
small list beside it.

`body.check` lists every problem by name and never crashes. It refuses a body when:

- a joint is missing or extra, or a parent differs from the rig's;
- a rest direction is too far from the rig's, for example a T-pose on our A-pose. Start at 10 degrees and
  tune it on real bodies;
- it isn't in metres, isn't Z up or doesn't face -Y once read;
- a vertex's weights don't add up to 1, or it has no weights;
- height or limb ratios fall outside the allowed range;
- it has no licence;
- it is made for another rig version;
- it is too large (`MAX_VERTICES`, a hostile file is refused, not allocated).

The allowed range is a data table, set to adult humans first. Small bodies may come later (189000), and adding
them should change that table and its tests, not the code. The check can only judge joints and proportions. It
can't prove that a surface looks human.

## Acceptance
- The mannequin passes, and its file is unchanged.
- Fixtures made by a test, one fault each, are refused with the right named reason: missing bone, wrong
  parent, T-pose, centimetres, Y up, facing +Y, bad weights, no licence, wrong rig version, too large.
- A fixture with longer limbs and the same rest directions passes.
- `body.check` is a command with JSON output, and a body's details survive saving and loading byte for byte.
- A throwaway project outside this workspace uses `ac-body` by git revision to write a body that passes the
  check here.
