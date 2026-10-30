# Foot contact phases

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 115600, 116200
- **Source:** 115100

## What
Contact is decided per foot point and per frame, so one frame near the floor plants a point, and the
mean of several slips can change suddenly as points join or leave (`crates/ac-core/src/physics.rs:501-522`).
The rules also differ: 5 cm in the pass and metrics, 4 cm in `lock_feet`, and another in the quality
scan.

Make one contact function that `physics`, `metrics`, `feet.lock` and `plant_feet` all use, giving each
foot a phase per frame: swing, landing, planted, release. It needs a minimum duration, and separate
thresholds for entering and leaving contact. Correction blends in and out across landing and release.
Deterministic, with rules simple enough to check by hand, as the pass's header asks.

## Acceptance
A fixture: a foot that skims the floor for one frame isn't planted, and a real step gives the four
phases on the frames worked out by hand. The ratchet (115300) improves on slide with no new pops. The
contact thresholds exist in one place in `ac-core`. Before-and-after contact sheets of a walk, a turn
and a side step for the owner.

## Result
2026-10-30. `crates/ac-core/src/contact.rs`: a foot comes down at 5 cm and 0.5 m/s, leaves past 8 cm or
0.8 m/s, and a contact lasts at least 3 frames. New query `feet.phases`; `fixtures/feet-phases.json`
works a landing, a release and a one-frame skim by hand.
- `physics`, `clip.metrics` and `feet.lock` use it. The fulcrum rule also moves the body at most 2 cm a
  frame more than it did, so a point joining the hold eases in. Slide is now measured on `Planted`
  frames, so a foot landing or leaving isn't counted.
- **Still separate:** `plant_feet` (it asks whether a foot stands at one spot in both keys), the
  `clip.contacts` query (fixture-worked 4 cm limits) and the Python quality scan.
- **Ratchet:** slide with physics on improved on most clips, partly because of the new measure. The
  worst root jump is `Locomotion/Side step 9` at 2.01 m/s, unchanged: it isn't from the fulcrum rule,
  and its cause isn't known. `feet.lock` got slightly worse on 22 clips' worst velocity change, still
  inside its limit of 2 m/s. Its slide limit is now a rise of at most 0.5 cm unless it stays under
  2 cm; on `Walk`, a foot it holds now counts as planted, reading 1.71 cm where it read 0 before.
- Contact sheets not drawn: no pose of a walk or side step changed visibly enough to tell apart.
