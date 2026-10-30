# Trustworthy validation and checkpoints

- **Status:** not started
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
