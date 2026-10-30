# Root path as an input

- **Status:** not started
- **Requires:** 130000
- **Source:** 115000 (review 2 §3)

## What
`trajectory` blends the root onto the keys' path after the network runs
(`crates/ac-ai/src/lib.rs:407-422`), so the legs and lean don't know the path. Feed a few root
positions across the gap as a given channel (the root position given apart from the root
rotation), dropped in training often enough that a fill without a path still works. Keep the
post-blend: it is what guarantees the path.

## Acceptance
With `trajectory` 1, foot slide in 126000 drops against today's post-blend alone.
