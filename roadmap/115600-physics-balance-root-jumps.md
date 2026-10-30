# Physics: balance makes the body jump

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 115300
- **Source:** 115100 (measured)

## What
With physics on, `Locomotion/Walk` jumps about 14 cm in one frame (root speed 4.1 m/s, keys at most
0.1 m/s), and `Locomotion/Run` about 65 cm. Of 39 such jumps on 9 built-ins, 22 are where balance
turns on or off.

The cause on the Walk: `still` tests the keyed centre of mass against `STILL_SPEED`
(`crates/ac-core/src/physics.rs:477-479`), so a loop that walks on the spot counts as standing. Then
balance runs frame by frame, with no blend into or out of it (`physics.rs:526-533`); on the Walk it
holds frames 0, 7-13, 17 and 24-29.

Two things to fix: a stepping body isn't standing (decide from what the feet do, not only the
centre of mass), and correction fades in and out over several frames. Find out what causes the 11
jumps in `Locomotion/Run` that fall on no balance edge, flight or sliding frame.

## Acceptance
On every built-in, no physics root jump over 1 m/s above the keyed speed, except a reasoned list in
the item's result; `fixtures/physics-balance.json` still balances a standing pose; a new
`fixtures/bug-physics-balance-jump.json` checks the Walk; the owner approves any visual golden that
changes.
