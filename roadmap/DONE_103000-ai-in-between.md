# AI in-between

- **Status:** done (built by 2026-10-29)
- **Requires:** 101000, 110000

## What works
`ac-ai`: our own `parts` network run in plain Rust (f32 transformer, no ML runtime), trained by
`training/` on the free-licence sets only: CMU, 100STYLE, our Kimodo-generated motion and the CC0
loops (2.2 cm mean joint error over 30-frame gaps on their held-out takes). 33 AI styles and 14
motion types. Canonical frame, 6D rotations, style guidance, `ai_force`, trajectory, head, given
bones for part layers. `ai.fill`, `ai.fill_all`, `ai.preview/accept/discard`, `ai.gaps`,
`ai.toggle`; edits refill the AI spans they touch; a fill is eased through a key it shares with
another fill (`crates/ac-core/src/ease.rs`), and its feet are planted (a bent knee keeps its own
direction); a pose past what a body can do is bent back to the possible limit and the fill says
so. Matches PyTorch: network within 1e-4, fills within 0.04 degrees.

## What comes next
The autofill work is in the files from 126000 on. Start with 126000 (the evaluation suite).
