# Lock feet makes motion worse

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 115300
- **Source:** 115100 (measured)

## What
`feet.lock` is what `clip.critique` suggests for every sliding finding, and it often does harm. On
the keys of the 55 built-ins it raises the largest velocity change at a key on 43 clips and lowers it
on none (`Locomotion/Small steps 4`: 1.5 to 9.7 m/s). It lowers the worst slide on 31 and raises it
on 9. It doubles the keys, 2374 to 4723.

Likely causes, in `lock_feet` (`crates/ac-core/src/edits.rs:630`): the ankle snaps to the contact's
mean position on its first frame and off it after the last, with no blend; it decides contact at
4 cm where `clip.metrics` and the physics pass use 5 cm; and it holds the ankle, not the foot points
the metric measures. The key doubling may also inflate the pop count, since pops are measured at
keys, so check that with the key frames held fixed.

## Acceptance
On every built-in, `feet.lock` never raises the worst slide, and raises the largest velocity change
at a key by no more than a stated limit; a new `fixtures/bug-feet-lock-pop.json` checks a clip with a
step; the existing lock fixtures pass.
