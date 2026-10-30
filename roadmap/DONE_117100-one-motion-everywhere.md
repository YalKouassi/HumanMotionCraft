# The same motion when editing, playing, exporting and reopening

- **Status:** done (2026-10-30)
- **Requires:** 115300
- **Source:** 115100

## What
The pass is shown at its nearest whole frame (`crates/ac-core/src/editor.rs:19-31`), and a clip over
`physics::MAX_FRAMES` plays as keyed. Nothing checks that the frame shown, a rendered frame and the
exported file all agree. Saving is checked by `open_save_reopen_is_the_same_clip` and
`recovery-restore`, on simple clips.

Test one busy clip (layers, part layers, links, surfaces, physics on, AI keys and markers) through
every route: shown, `clip.video` or `clip.sheet`, GLB export, save and reopen, and recovery.

## Acceptance
For that clip, joint positions on every frame agree within 0.1 mm across shown, rendered and exported
motion; after save and reopen, and after recovery, the document equals the original; a fixture or
test checks each route; differences found are fixed or listed with the reason.

## Result
2026-10-30. `crates/ac-io/tests/one_motion.rs`, on a walk with lock keys, a part layer, an additive
layer, a link layer, a box and a marker, with physics on.
- **Exported file:** every joint on every frame within 0.1 mm of what is shown.
- **Pictures and video:** drawn from `Document::shown_poses`, so they share that route. No separate
  pixel test.
- **Save and reopen, recovery:** a bug found and fixed. Saved numbers came back up to the last digit
  different, because `serde_json` parses floats inexactly by default. The workspace now uses its
  `float_roundtrip` feature, so a saved clip reads back exactly.
- **Left out on purpose:** which layer is being edited isn't saved; a clip opens on its base layer.
- **No AI keys in the clip:** this session has no model. The keys that `feet.lock` writes are tagged
  the same way.
