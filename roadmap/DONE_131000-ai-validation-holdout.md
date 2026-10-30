# Trustworthy validation and checkpoints

- **Status:** done (2026-10-30)
- **Requires:** none
- **Source:** 115000 (review 1, "make the training pipeline more reliable")

## What
- Hold out by subject or session where the set says (CMU subject, ACCAD figure, AddBiomechanics
  subject), not per take (`training/ac_train/train.py:81`), so near-identical takes can't sit on
  both sides. Keep the split stable when sets or decisions change.
- A real run refuses to validate on training takes; the fallback (`train.py:262-265`) stays for
  `--steps` quick checks only, and the card says so.
- Keep the best checkpoint by a composite of 126000's metrics as well as the last one. The gain is
  small (the learning rate decays to zero, `train.py:289`), but it is cheap.

## Acceptance
The card lists the held-out subjects; a pytest shows the split is the same when a set is added;
a quick run's card is marked as validated on training takes.

## Result
2026-10-30. `training/ac_train/split.py`: a take's group is its CMU subject, ACCAD performer
(female1, male1, male2; the five takes without one are their own groups), AddBiomechanics study and
subject, or generated prompt (every seed together); 100STYLE (one performer) and the built-in loops
group by take. A group is held out when a CRC-32 of `<set>/<group>` falls under 5 %, so the split
depends on names alone. Held out today: CMU subjects 36, 63, 80, 84, 113, 117, 142 (187 takes);
AddBiomechanics tiziana2019 subjects 2 and 6, uhlrich2023 subjects 5 and 9 (113 takes); 42
100STYLE takes; 2 generated prompts.

ACCAD first grouped by performer, and none of its three was under 5 %, so none of its takes were
validated. It now groups by **recording session** (performer and session letter: `male2_e`), which
still keeps a session's near-identical takes on one side: held out are `male2_e` (38 takes) and
`male2_a` (15). The 126000 numbers above predate this and have no ACCAD in them.

The card lists them (`holdout`) and says what it validated on (`validated_on`). A real run with
nothing held out stops; a `--steps` run falls back to its training takes and says so (checked with
`python -m ac_train.pipeline --quick`). `tests/test_split.py` shows the split is the same when a set
is added or a take dropped. Every `--eval-every` steps (1000) a quick evaluation scores the model
and the best is saved as `best.pt` beside `model.pt`; both are evaluated in full at the end
(`evaluation`, `evaluation_best`); `ac_train.export --checkpoint best` ships the best.
