# Lock feet makes motion worse

- **Status:** done (2026-10-30)
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

## Result
2026-10-30. The main cause wasn't the missing blend. The lock forced the ankle to the mean place in
three directions, so when a heel rose it pulled the leg straight, and the knee popped on release.
Blending alone left the worst rise at 6 m/s. `lock_moves` now holds the ball of the foot, across the
floor only, and eases over 4 frames either side; a one-frame run is no contact.

| On the keys of the 55 built-ins | Before | After |
|---|---|---|
| Worst slide raised on | 9 clips | 0 |
| Worst slide lowered on | 31 | 46 |
| Largest rise in the worst velocity change at a key | 8.2 m/s | 1.72 |

The limit is 2 m/s, and `builtin_motion_quality_only_improves` asserts both lines on every clip. The
worst velocity change still rises on 22 clips, and pop counts rise from 618 to 1320. Pops are counted
at keys and the lock adds keys, so that count isn't comparable; 118600 changes how pops are measured.
It still writes a key on each frame it changes.
