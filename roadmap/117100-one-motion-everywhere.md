# The same motion when editing, playing, exporting and reopening

- **Status:** not started; the acceptance line needs the owner's agreement
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
