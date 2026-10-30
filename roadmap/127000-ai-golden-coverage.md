# AI golden coverage

- **Status:** not started
- **Requires:** 103000
- **Source:** 115000 (review 2 §13)

## What
`crates/ac-ai/tests/golden/` has one network case and one fill case. Add fill goldens from
`training/ac_train/infer.py` for:
- gaps of 1 and 45 (the ends of the range `crates/ac-ai/src/lib.rs:273` accepts);
- a part layer (a `given` list, as `crates/ac-core/src/ai.rs:160` builds it);
- `trajectory` 0.5 and `head` 0.5;
- `ai_force` 0 (the plain interpolation must come back exactly);
- a fill at the clip start (the context repeats frame 0).

## Acceptance
`python -m ac_train.export` writes the new goldens; `cargo nextest run -p ac-ai` checks them
within today's tolerances (network 1e-4, fills 0.04 degrees).
