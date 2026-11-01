# Clashes in the training data and the base retrain

- **Status:** not started; training needs the owner's word
- **Requires:** 183000, 184000
- **Source:** limb clash analysis (2026-11-01); owner asked for it to be filed

## What
The model learns clashes from its data.

Frames per 1,000 with an overlap of 4 cm or more / 8 cm or more:

| Set | 4 cm | 8 cm |
|---|---|---|
| CMU | 89 | 12.9 |
| 100STYLE | 46 | 3.1 |
| AddBiomechanics | 46 | 1.0 |
| ACCAD | 15 | 4.4 |
| Natural | 62 | 6.3 |

- 159 of 3,501 takes have over 5% of frames at 8 cm or more, 130 of them CMU.
- Hand into thigh is about 73% of natural clashes at 4 cm or more, and the only clash in 42% of them.
- Wrists, `upperarm02`, `lowerarm02`, fingers, head and `neck02` sit at rest in all five recording
  sets, which is the likely cause of hand into thigh. Not fixed by any retarget so far.
- Rig proportions may add to it: shoulder to leg .407 against 100STYLE's .437, about 1.5 cm less room.
  Likely, not proven.

Held-out recordings, windows with a clash the fill made, 4 cm or more: model 2.9%, interpolation 2.0%.
Gaps 31-45: 6.2% against 3.9%. The model invents arm-against-arm clashes (8.7 per 1,000 against the
data's 3.1).

Parts:

1. Review the flagged takes (the review app, 174000).
2. Repair as a separate step after retarget. A prototype took natural's 4 cm rate from 57.5 to 2.5 per
   1,000 and its 8 cm rate from 5.6 to 0.1, with a mean move of 4.7 mm, 84 mm at the 99th percentile,
   jerk +8% and foot speed unchanged. Not reviewed by eye.
3. A clash loss and truth-free posed-key windows in the base retrain, with 186000's routes as a
   possible teacher. A loss alone can't choose a side of the body, because averaging a left route and a
   right route goes through it, and recording windows gave 136000's limit loss nothing to act on. Loss
   cost: about 5% of a training step.

Order: 134500 records the owner's decision (2026-10-31) that the base retrain comes after 154000, and
add-ons are tied to the base's weights. So this belongs in that one retrain, not a separate one.

## Acceptance
- A repaired copy of the data is produced by a repeatable step. The originals are untouched, and the
  owner has seen a reel of the most-changed takes.
- After the retrain, on the 126000 held-out windows: fill-made clashes of 4 cm or more at or below
  interpolation's 2.0%, and joint error and foot slide within 134500's lines.
- On the 183000 whole posed keys: fill-made clashes of 4 cm or more at or below interpolation's 19%.
- The owner judges side-by-side videos before any export.
