# Review app: usage labels, style descriptions, preview by style

- **Status:** built, waiting for the owner
- **Source:** owner request (2026-11-01)
- **Requires:** 174000, 176000

## What
Three plain labels, for a pack or a single clip, replace mine and purchased in the app and the trainer: **Share freely** (the clip and models of it may leave), **Train only** (models of it, never the clip), **Just me** (neither). A clip with no label is Just me, except a free-licence set. A public set refuses to train on a Just me clip.

`training/review/styles.json` says what each style holds and leaves out. The Styles view lists them; choosing one plays its clips one after another. R throws a clip out and moves on, I, O and C cut frames, and Keep the rest keeps what is left under that label as one undo step. A clip's name only suggests it for a list: it gets the label when it is kept.

## Acceptance
The owner plays a style through, throws clips out, and agrees the labels and descriptions.
