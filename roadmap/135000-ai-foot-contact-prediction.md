# Predict foot contacts in the fill

- **Status:** not started
- **Requires:** 126000, 132000
- **Source:** 115000 (review 2 §1, corrected; review 1 contact loss)

## What
After a fill, `plant_feet` (`crates/ac-core/src/ai.rs:237`) plants only feet that are still and low
in *both* keys. A step inside the gap still slides. Contacts can't be an input inside the gap:
they are the unknown there, and the clip's contacts in the gap come from the spline. Instead:
- the network also outputs a left and right contact per frame (truth from the same height and speed
  test the quality scan uses);
- a skating loss on frames the truth marks planted, and a floor loss;
- `plant_feet` uses the predicted contacts for steps inside the gap.

This changes the network's output size, so `crates/ac-ai/src/net.rs`, the card check in
`crates/ac-ai/src/lib.rs:70`, `infer.py` and the goldens change together.

## Acceptance
Foot slide in 126000 drops on gaps with a step inside; a fixture fills a walk gap with one step
inside and checks the planted foot stays within 1 cm.
