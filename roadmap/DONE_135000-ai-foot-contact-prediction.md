# Predict foot contacts in the fill

- **Status:** done (2026-10-31)
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

## Result
2026-10-30. The network has a contact head (`contact`, two logits a frame from the same normed rows
as the output); training adds a contact loss on the quality scan's planted test, a skate loss (the
fill's foot speed where the recording's is planted) and a floor loss (`--no-contacts` trains
without). The app: `Net::forward_contacts` (`crates/ac-ai/src/net.rs`) reads the head when the
weights have it; `Inbetweener::fill_contacts` returns the gap's contacts (none at `ai_force` 0);
`AiFiller` keeps them per span, and `Document::plant_steps` (`crates/ac-core/src/edits.rs`) holds
each run of two or more planted frames at the run's middle-frame ankle (leg IK), before the
still-in-both-keys planting; a run whose foot is more than 15 cm above the keys' floor is left.
Models without the head fill as before. Export writes the contact logits into the net golden and
`crates/ac-ai/tests/golden.rs` checks them (the quick pipeline passes with a contact-head model).
`crates/ac-core/tests/steps.rs` checks a planted foot stays within 1 cm and nothing else moves.

Still to do with the retrained model: the fixture that fills a real walk gap with a step inside
through `ai.fill` (it needs a model with the head), and the slide numbers.

## Closed (2026-10-31)
Closed by the owner. Built; the acceptance's foot-slide drop was not confirmed by this close.
