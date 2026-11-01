# AI Fill: arms take a route around the body

- **Status:** not started; prototype measured, with known bugs (below)
- **Requires:** 183000, 184000, 185000
- **Source:** owner's question (2026-11-01): when a fill moves the hand through the torso, what movement
  should the arm take? Hands behind the back to hands in front, without clashing, inside realistic
  ranges.

## What
A fill goes more or less straight from key to key, so a hand behind the back goes through the hips.
Smoothing can't fix that. Something has to choose a way round.

Measured on whole hand-made poses through the real `ai.fill`, gap 30, physics off, 200 windows: 28% of
fills make a new clash of 4 cm or more (interpolation 19%), and 17.5% one of 8 cm or more
(interpolation 7.5%).

The method, run for each arm only when its straight path clashes, so every other fill stays the same:

1. Describe the arm with shoulder flexion, abduction, rotation and real elbow bend.
2. Block what is impossible: the study limits as hard walls, and any arm position overlapping the
   body. A key that already overlaps a pair may stay that deep near that key, but no deeper.
3. Prefer what people do: common recorded poses and clearance from the body cost less. Keep the
   preference light, because a stronger one sends the arm on long detours.
4. Search the grid with A*, checking each step of the found path and searching again if one clashes.
   One neighbour on the grid can move the hand 10 cm, so checking only the grid points misses clashes.
   No randomness.
5. Relax each path point only while it stays clear, then time it with `10t^3 - 15t^4 + 6t^5`.
6. Land exactly on the keys: blend the leftover bone turns, turn the forearm about the elbow so the
   wrist stays on the path, and carry the hand in the chest's frame.
7. Plan the arm with the deeper clash first, and the other arm around it.

The route replaces the arm bones of the fill. The model keeps the legs and torso. It isn't the Walk
loop or a preset.

## Measured (2026-11-01)
Prototype: `route2.py` in the gitignored `target/analysis/2026-11-01/limb-clash/`, reel in
`target/shots/2026-11-01/limb-clash/route-reel.mp4`. The owner hasn't reviewed it. Deepest new arm
overlap, today's fill against the route:

| Move | Fill | Route | Peak hand speed |
|---|---|---|---|
| At ease to Arms crossed, 1 s | 12.4 cm | 6.8 cm | 0.45 to 1.66 m/s |
| At ease to Arms crossed, 1.5 s | 12.7 cm | 6.7 cm | 0.30 to 1.12 m/s |
| Arms crossed to At ease, 1 s | 12.4 cm | 5.1 cm | 0.41 to 1.68 m/s |
| At ease to Chin on hand, 1 s | 12.8 cm | 3.4 cm | 0.85 to 2.04 m/s |
| At ease to Thinking, 1 s | 12.5 cm | 2.8 cm | 0.69 to 2.25 m/s |
| At ease to Boxing guard, 1 s | 10.0 cm | 2.9 cm | 1.06 to 2.61 m/s |
| At ease to Relaxed, 0.5 s | 9.4 cm | 1.4 cm | 0.75 to 1.84 m/s |
| Hands on hips to At ease, 1 s | 4.5 cm | 2.0 cm | 0.16 to 0.72 m/s |

The 37 of those 200 windows whose fill made an arm clash:

- 4 cm or more: 32 fills today, 10 with the route.
- Median overlap 6.7 to 2.1 cm.
- Median jerk 15 to 76 m/s^3.
- Median peak hand speed 1.12 to 1.79 m/s.

The app's range check found no impossible frames on the 8 moves. It can't see elbow or shoulder
rotation yet (185000).

Three route poses as extra keys, with the shipped model filling between them, lost most of the gain
(Thinking 9.2 cm, Boxing guard 8.9 cm, Chin on hand 8.7 cm). Apply the route directly.

## Known bugs to solve
- **Worse on 4 of the 37:** Side sit to Hugging knees 7.0 to 7.6 cm, Hugging knees to T-pose 7.6 to 8.8
  cm, Hugging knees to Cheering 7.6 to 8.6 cm, At attention to Push-up 2.5 to 5.4 cm. The check samples
  the body at only 5 moments, so moving legs need obstacles that move through time.
- **Too jerky:** about 5 times the fill's jerk. Part of that is a longer path in the same time, and
  part is corners left in tight spaces.
- **Hands too fast on short gaps:** up to 2.61 m/s on the 8 moves, 4.53 m/s on the 37. When a gap is
  too short for a believable speed, say so (Rule 1). Don't rush it silently. Low crouch to Triangle
  pose peaks at 14.7 m/s in today's fill and 14.4 with the route, so that one comes from the fill, not
  the route, and needs its own look.
- **Keys that already overlap:** Arms crossed sits 5.7 cm in the belly and the route keeps about 7 cm
  on the next capsule across. The relief near a key is per pair, so it should cover the body part.
- **Arm against arm:** Arms crossed to At ease rose from 3.7 to 5.1 cm. The second arm sees the first
  at the 5 moments only.
- **Keys off the arm grid:** the search joins each key to its nearest clear grid point without
  checking that step.
- **Reach:** no moving the shoulder or leaning the torso to make room. Python took up to about 10
  seconds an arm; Rust speed isn't measured.

## Acceptance
On the same 200 windows, physics off:

- Of the 37 fills above, those with a new arm overlap of 4 cm or more drop from 32 to 10 or fewer, and
  none gets worse by more than 0.5 cm.
- No impossible frames with 185000's ranges.
- Median jerk at most 3 times today's fill.
- A gap too short for hand speed under 2.5 m/s is reported, not hidden.
- Fills whose straight path is clear stay identical, bit for bit, and the result is deterministic.
- The owner approves a reel by eye. A zero clash count alone doesn't pass: an earlier solver cleared
  it by flinging the arms.
