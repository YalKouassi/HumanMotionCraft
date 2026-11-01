# Review app: usage labels, style descriptions, preview by style

- **Status:** built, waiting for the owner
- **Source:** owner request (2026-11-01)
- **Requires:** 174000, 176000

## What
Three plain labels, for a pack or a single clip, replace mine and purchased in the app and the trainer: **Share freely** (the clip and models of it may leave), **Train only** (models of it, never the clip), **Just me** (neither). A clip with no label is Just me, except a free-licence set. A public set refuses to train on a Just me clip.

`training/review/styles.json` says what each style holds and leaves out. The Styles view lists them; choosing one plays its clips one after another. R throws a clip out and moves on, I, O and C cut frames, and Keep the rest keeps what is left under that label as one undo step. A clip's name only suggests it for a list: it gets the label when it is kept.

## Acceptance
The owner plays a style through, throws clips out, and agrees the labels and descriptions.

## Decision (2026-11-01)
The owner: the base uses their own clips for walk, run, gesture, idle, jump, crouch and climb; zombie and acrobatic are trained add-ons. They preview each style, throw out bad clips, and send the clips whose names say nothing to a style (the `unsorted` list, keys 1 to 9). Sets live outside the repo. Nothing trains until the owner says so.

## Shoulders on ported clips (2026-11-01)
Sources are scaled by leg length and a clavicle aims at the source's shoulder position, so a source with other proportions dropped, narrowed or raised the rig's shoulders. `retarget::fit_shoulders` moves each arm so the shoulder sits where the rig's does, keeping shrugs and arm direction. The private library is re-ported once the owner approves the video.
