# Autofill review: recheck of two outside reviews

- **Status:** done (rechecked against the code on 2026-10-29, after commits `ee47a2c`, `bd70d5b`
  and `ec854d3`)
- **Requires:** none

Two outside reviews proposed ways to improve AI fill. Every claim was checked against the code.
This file records each verdict and where the work now lives. Everything accepted has its own file.

## What changed since the first check
- `training/ac_train/train.py`, `model.py`, `infer.py` and `crates/ac-ai` are **unchanged**, so
  every training and inference finding below still stands.
- **New post-processes after each fill** (`crates/ac-core/src/ai.rs:236-238`): `ease_borders`
  (`crates/ac-core/src/ease.rs`: a 4-frame Hermite through a key shared by two fills),
  `plant_feet` (leg IK now keeps the pose's own knee bend), and `keep_possible`
  (`crates/ac-core/src/possible.rs:222`: impossible joints bent back, with a note).
- **New data**: ACCAD and AddBiomechanics are registered, retargeted and scanned, but not trained.
  The review now rejects 353 duplicate CMU takes and cuts or rejects takes with impossible poses.
- **Correction to the first check:** it said neighbouring fills meet at a shared key without a seam.
  That was wrong. `ease.rs` exists because they kink there: each fill "heads for the next key
  whatever speed the motion came in with". The ease hides the kink after the fact. The cause is
  that the model ignores the incoming velocity, and fixing it in training is 129000.

## Review 1 ("train on the inputs autofill actually receives")

| Claim | Verdict | Where now |
|---|---|---|
| Training context is clean mocap; the app's context is the clip's spline (`crates/ac-ai/src/lib.rs:301`) | Correct, and worse than stated: before the first key the context is frame 0 repeated (`lib.rs:296-297`), a still context training never shows | 129000 |
| Gaps 1–4 never trained (`train.py:290` samples 5..45; `lib.rs:273` accepts 1..45) | Correct | 128000 |
| Evaluation is only mean joint error at 5/15/30 (`train.py:204`) | Correct; the shipped card has no 45 either | 126000 |
| Add contact, continuity, rotation metrics, broken down by gap, type and style | Agreed | 126000 |
| Context after key B | Agreed, later: it changes the Rust input contract | 140000 |
| Rotation, temporal and contact losses, without over-smoothing | Agreed | 132000, 135000 |
| Style consistency across gap lengths and part fills | Agreed, as metrics | 126000 |
| Validation split by subject/session; no train-as-val fallback for real runs (`train.py:262-265`) | Correct. Duplicate CMU takes are now rejected, which helps, but the split is still per take (`train.py:81`) | 131000 |
| Save the best checkpoint | Small gain: the learning rate decays to zero (`train.py:289`), so the last model is usually near the best. Cheap, so folded in | 131000 |
| Visual regression set of hard fills across model versions | Agreed | 126000 |
| Keep the Rust/PyTorch parity goldens | Agreed; they exist | 127000 widens them |
| Presets (Preserve my motion, Natural, Follow style) | Fine only as AI slider presets, never a hidden walk loop. The owner decides | 139000 |
| Show why a fill looks wrong; Fill All report | Partly built: `ai.fill_all` returns filled, skipped and notes | 138000 |
| Don't make the network bigger first | Agreed | 143000 is last |

## Review 2 ("contacts, seams, masks, diffusion")

| Claim | Verdict | Where now |
|---|---|---|
| §1 Foot-contact flags as input | Wrong as stated: inside the gap contacts are the unknown, and `clip.contacts` there comes from the spline. The model should predict them and be trained with a skating loss. `plant_feet` already plants feet that are still in both keys | 135000 |
| §2 Hermite root base; fade the correction to zero at the keys | Rejected: zero correction at the keys pins the fill to slerp's constant speed, which is the jump against a moving context. `ease.rs` now eases shared keys properly (matching both sides' velocity) | rejected; 129000 for the cause |
| §3 Trajectory as an input | Good idea; keep the post-process as the guarantee | 141000 |
| §4 Random per-bone given masks | The mismatch is real and worse than stated: the app passes the *complement* of a layer's parts (`crates/ac-core/src/ai.rs:160`), training gives one group (`train.py:166-177`). Train on the app's masks, not random ones. Pins and `parts.lock` don't use the mask | 130000 |
| §5 Fill any subset of frames; one pass per clip | One pass per clip is impossible (56-frame window, `model.py:36`, `lib.rs:106`). Several keys in one window is a good later step | 142000 |
| §6 Diffusion v2 | Agreed as a long-term v2 | 143000 |
| §7 Velocity, end-effector, jerk losses | Agreed | 132000 |
| §8 Gate tumbling; context noise; speed warp | Agreed | 133000 |
| §8 Shift the window ±1 frame | No-op: windows already start at any valid frame | rejected |
| §9 Gap curriculum | Fine to try; it does not replace training 1–4 in the final mix | 128000 |
| §10 Add LaFAN1 | Rejected: non-commercial, removed on 2026-10-29 (112000) | rejected |
| §10 Hard validation slice | Agreed | 126000 |
| §10 GPU memory "3.4 GB, duplicated" | Wrong: validation takes are disjoint (`train.py:84`); all rig data is a few hundred MB on disk and the GPU holds 19 bones | rejected |
| §11 Foot slide, seam velocity, root error metrics | Agreed | 126000 |
| §12 Batched inference, N× faster | Rejected: gaps depend on each other in order, and CPU batching is not N× | rejected; speed is 125000 |
| §13 More goldens | Agreed | 127000 |
| §14 Flag takes the model fails on | Agreed: send them to the review queue, never auto-reject | 137000 |

## New from the recent changes
- The model makes impossible poses that `keep_possible` must bend back. Count them in the
  evaluation and train against them: 136000.
- ACCAD and AddBiomechanics wait for a retrain: 134000. The next `train` run reads them by
  default (they are `commercial=True`).
