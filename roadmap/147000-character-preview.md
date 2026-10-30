# Preview your character while animating

- **Status:** not started; acceptance line proposed, waiting for the owner to agree
- **Requires:** 146000
- **Source:** 144000 (one rig, retarget on export)

## What
Show the target character in the viewport, driven live through its mapping, next to or over the
mannequin, with frames where the retarget struggles marked on the timeline (`TimelineMarks`):
sliding feet, missed contacts, bones it can't drive.

## Acceptance (proposed)
With a mapping loaded, the viewport shows both figures in step at 60 fps on the walk clip;
screenshots of side-by-side and overlaid views in `target/shots/<date>/character-preview/`.
