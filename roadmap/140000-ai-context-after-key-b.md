# Context after key B

- **Status:** not started
- **Requires:** 134000
- **Source:** 115000 (review 1 §2A)

## What
The window is context frames ending on key A, the gap and key B (`training/ac_train/model.py:7`);
nothing after B, so the model knows B's pose but not how the motion leaves it. Give a few frames
after B, falling back to one side at the clip's end. Try it at the same total window (56 frames,
`model.py:36`) by trading left context for right.

This changes the input contract: `crates/ac-ai/src/lib.rs` (window and masks), `infer.py`, the card
and the goldens change together.

## Acceptance
Seam velocity into B improves in 126000, joint error is no worse, and the parity goldens pass.
