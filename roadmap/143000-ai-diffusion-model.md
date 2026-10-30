# A diffusion in-betweener (v2)

- **Status:** not started; long-term
- **Requires:** 126000, 134000
- **Source:** 115000 (review 2 §6)

## What
The regression model averages ambiguous gaps (arm swings, weight shifts). A small diffusion model
(8–16 denoising steps) gives sharper motion and a seed for another take, as `text.generate` has.
It rewrites `model.py`, `train.py`, `infer.py` and `crates/ac-ai/src/net.rs`; keep today's model as
the fast default. Each step is a forward pass in plain Rust, so 125000's speed target applies.

## Acceptance
To be agreed with the owner once 126000 shows where the regression model falls short.
