# Physics: lying bodies sit up

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 115300
- **Source:** 102000's known issue; 115100 (measured)

## What
102000 records that the pass tilts a body lying on the floor up by about 40 degrees, so floor and
fall clips look as if they sit up. It is still open, and physics is on by default. On
`On the floor/On the ground 10` at frame 76, the pass moves the root about 0.56 m from the keys
(22 frames rebalanced, 88 slid, 67 lifted) and raises penetration from 1.14 to 4.97 cm, the only
built-in it makes worse.

Foot points are all the pass knows of the ground, so it treats a lying body as standing. It needs
to recognise a body supported by something other than its feet, and leave it alone.

## Acceptance
On the `On the floor` and `Falls` built-ins, physics on no longer lifts the hips or tilts the trunk
past a stated tolerance from the keys, and never raises penetration; standing and walking clips are
unchanged; contact sheets, physics on and off, for the owner.

## Result
2026-10-30. `Body::rests`: a trunk, knee, elbow or wrist joint within 15 cm of the ground means the
feet aren't the only support. Such a frame counts as on the ground, so it is never a flight, and
neither the fulcrum rule nor balance touches it. `clip.metrics` skips its balance too. A bending
person whose hand comes within 15 cm of the floor also gets no correction on those frames.

`On the floor/On the ground 10`: penetration with physics 4.97 cm to 0 (keys 1.14), root jump 3.35 to
0.5 m/s. Its slide rises from 7.16 to 11.32 cm (keys 16.52), because the pass no longer drags the body
to hold feet that aren't carrying it. The ratchet flagged that, and it is blessed as meant.
`crates/ac-core/tests/physics.rs` checks a lying body stays as keyed. Sheets with physics on and off
now match: `target/shots/2026-10-30/physics/floor-physics-{on,off}.png`.
