# FBX import and retarget onto the rig

- **Status:** not started
- **Requires:** none
- **Source:** owner request (2026-10-30)

## What
Bring a humanoid FBX animation onto the rig through a command, so it works from the CLI and MCP as
well as the app. Today `clip.import` reads `.clip.json` only (`crates/ac-io/src/commands.rs:509`),
and the retarget exists only in Python (`training/ac_train/retarget.py`), which can't run in the app.

- **Reader:** our own Rust reader for binary FBX (7.x, compressed arrays included). Input is hostile:
  caps, finite checks, and a refusal that says what is wrong, never a panic. The file's
  `UpAxis`, `FrontAxis` and `UnitScaleFactor` are read, not assumed.
- **Retarget:** port `retarget.py` to Rust, pinned to the Python results by goldens. The source's world
  positions give the landmarks, so it doesn't matter which bone carries the motion.
- **Joint map:** guessed from bone names, correctable by parameter. A missing landmark is a
  refusal that names it.
- **Direction:** the rig is Z up and faces -Y.
- **Root:** the rig's root is the hips, stored as an offset in hip heights. As in `retarget.py:222`, the
  hip joints land on the source's, because the rig's root sits about 11.5 cm above them. The floor
  moves to z = 0.
- **Commands:** extend `clip.import`, plus a batch command that writes a folder of `.clip.json`.
- **Training:** a set reader for those `.clip.json` files, so there is one importer and no second
  FBX reader in Python. Training ignores in-place clips: they teach the model that feet slide.

Licences: test files are FBX we generate ourselves or animations the owner owns. A purchased pack
may forbid training, whatever the tool can read; store packs stay out of `training/` unless their
publisher has consented in writing.

Fingers, face and props have no landmark: they stay at rest, and the result says so (Rule 1).
Related: 146000 is the other direction (export onto a character) and can reuse the name guessing.

## Acceptance
- A fixture imports a generated FBX through `clip.import` and through `ac`, and an MCP test does the
  same.
- Direction: a clip walking forward travels -Y; a clip stepping left moves to the rig's left; the left
  and right feet are not swapped; a Y-up and a Z-up file of the same motion give the same result.
- Root: the rig's hip joints stay within 1 cm of the source's (scaled) on every frame, and the feet
  rest on z = 0 where the source's rest on its floor.
- The Rust retarget matches the Python goldens within 1e-4 on a fixed input.
- Corrupt, truncated and non-humanoid files are refused with a reason, with a regression test for each.
- `training/` can build a set from a folder of imported clips, and the scan (`ac_train.quality`) runs on it.
- The owner approves a contact sheet of one of their own FBX files beside the source.
