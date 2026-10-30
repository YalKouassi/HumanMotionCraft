# Physics: lying bodies sit up

- **Status:** not started; the acceptance line needs the owner's agreement
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
