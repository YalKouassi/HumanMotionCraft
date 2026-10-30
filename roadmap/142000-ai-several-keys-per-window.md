# Several keys in one window

- **Status:** not started
- **Requires:** 129000
- **Source:** 115000 (review 2 §5, corrected)

## What
Train with a random set of known frames inside the window, not only "context + key B", so short
neighbouring gaps can fill in one pass and agree at their shared keys. One pass per clip is not
possible: the window is 56 frames (`training/ac_train/model.py:36`). Time-to-go becomes time to the
next and from the last known frame.

## Acceptance
Two neighbouring gaps that fit one window fill in one pass with no kink at the shared key without
`ease.rs`.
