# Secondary motion

- **Status:** owner's decision first
- **Requires:** 117400
- **Source:** 115100

## What
Arm swing, shoulders, pelvis, spine counter-rotation, head stabilisation, hands, and follow-through
when movement stops. The network solves a group of bones, while fingers, toes, twist bones and the
head keep the keys' spline (`crates/ac-ai/src/lib.rs:7-8`), so a fill has no head stabilisation
and no hand follow-through.

## Question
Which route: (a) a separate, visible command that writes a layer the person can mute, or (b) more
bones in the model, which goes with a retrain and changes the network's size.

Recommendation: (a) first. It never overwrites manual keys, works on hand-keyed clips without AI,
and is deterministic. It is a separate feature from AI Fill and never runs inside it.

## Acceptance
To be agreed with the owner. Proposed for (a): one undo step; muting the layer gives back the clip
exactly; no key of the person's changes; side-by-side videos of a hand-keyed walk for the owner.
