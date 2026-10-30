# AI golden coverage

- **Status:** built, waiting for the owner (2026-10-30)
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

## Result
Built 2026-10-30. `crates/ac-ai/tests/golden/parts-fill.json` now holds ten fills of
`Locomotion/Walk` (a 33-frame CC0 loop) instead of three; `parts-net.json` and the three old cases
are byte-identical to before. Exported from the shipped checkpoint
(`training/data/models/parts-lowweights/best.pt`, as `assets/models/MANIFEST.json` records) with
the new `python -m ac_train.export parts-lowweights --checkpoint best --as parts`: `--as` ships a
training run under the app's tag (card, folder, golden names), and the re-exported
`assets/models/parts/weights.{bin,json}` came out byte-identical, which confirms the checkpoint.

The cases added (a, b: the keys; every one passes `cargo nextest run -p ac-ai` at the existing
tolerances, 2e-3 rad a bone and 1e-4 hip heights for the root), with the measured worst errors:

| Case | a-b | Worst bone (rad) | Worst root (hip heights) |
|---|---|---|---|
| gap 1 | 10-12 | 6.40e-4 | 2.78e-8 |
| gap 45 (more than one lap of the loop) | 4-50 | 7.64e-4 | 3.83e-6 |
| part layer: every solved bone but the left arm given (16 names, as `AiFiller::fill` builds them) | 6-22 | 8.07e-4 | 1.08e-7 |
| trajectory 0.5 | 14-36 | 7.27e-4 | 2.62e-7 |
| head 0.5 | 16-40 | 7.62e-4 | 1.25e-6 |
| ai_force 0 | 20-39 | 7.37e-4 | 8.36e-8 |
| fill at the clip start (the context repeats frame 0) | 0-9 | 6.51e-4 | 2.16e-7 |

The three kept cases: 6.82e-4, 6.80e-4, 6.88e-4 rad. The ~7e-4 rad floor is the same with
`ai_force` 0, so it is the reference's float32 6D/quaternion round trip (its `mat_to_quat` takes
square roots of near-zero sums in float32), not the network. So "the plain interpolation must come
back exactly" is checked against the interpolation worked out in the test itself, not the
reference: with `ai_force` 0 every solved bone is the slerp between the keys, the root their lerp
and the other bones the keys' spline, within 1e-5 rad and 1e-6 hip heights (measured 7.9e-7 rad).

Decisions made without asking: the Walk loop stays the golden clip for every case (one clip a
file; the 45-frame gap wraps through the loop, which both sides sample the same way); the Rust
test reads the new `head` and `given` fields (missing ones mean 0 and none, so other exports still
load); the training tag `parts-lowweights` was not renamed. Nothing was trained.

Found on the way: `training/tests/test_export.py::test_committed_model_matches_the_golden` had
failed since the model gained its contact and path heads (it built the network without them, so
`load_state_dict` refused `contact.weight`); it now builds the heads the card names, feeds the
golden's path and checks the contact logits too (`uv run --with pytest python -m pytest
tests/test_export.py`: 2 passed).
