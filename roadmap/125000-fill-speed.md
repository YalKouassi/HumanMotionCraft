# Fill speed

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 103000
- **Source:** old `ROADMAP.md` row 7

## Acceptance
One 30-frame AI fill takes under 50 ms in a release build, as measured by `ac fixtures --timings`. ACCURACT OVER SPEED, IF IT IS SLOWER IT IS FINE.

## Notes
Batching gaps together (outside review §12) is not the way: each gap's context is the gap before
it, so `ai.fill_all` is sequential. Look at the forward pass itself and the post-processes
(`ease_borders`, `plant_feet`, `keep_possible`). A styled fill runs the network twice
(`crates/ac-ai/src/lib.rs:363-370`).

## Result (2026-10-30)

Measured with `ac fixtures --only ai-fill-speed --timings` (release build, `fixtures/ai-fill-speed.json`:
keys at frames 0 and 31, 30 frames between) and the split from
`cargo test -p ac-ai --release --test speed -- --ignored --nocapture` (`crates/ac-ai/tests/speed.rs`,
medians of 9 on the library walk, 41-frame window).

| | before | after |
|---|---|---|
| `ai.fill`, 30 frames, style any (one pass of the network), quiet machine | 136–141 ms | 14–20 ms |
| the same with other work running on the machine | — | 22–31 ms |
| `ai.fill`, 30 frames, style walk (two passes, blended), quiet machine | 275–282 ms | 25–39 ms |
| the same with other work running | — | 43–65 ms |
| forward pass alone, 41 frames (bench median) | 139.6 ms | 12.4 ms |
| around the network (sampling, the window, the 6D round trip) | 1.1 ms | 0.6 ms |
| the document's post-processes (`ease_borders`, `plant_feet`, `keep_possible`) | ~1–2 ms | ~1–2 ms |

The fixture times one cold call each (the first fill in the process), so they sit above the bench
medians; a quiet machine is the measurement the acceptance names. The styled fill, two passes, is
near the 50 ms line only when the machine is busy.

Where the time went: all of it in `Linear::apply` (`crates/ac-ai/src/net.rs`), one scalar dot
product per output, which the compiler cannot vectorise because an f32 sum must keep its order.
What changed: the weights are packed at load time into blocks of 16 outputs (`[block][k][lane]`),
so one pass over an input row feeds 16 independent accumulators (two rows at a time share each
weight load) and the compiler packs them into SIMD lanes. Each output is still
`bias + ((−0 + x0·w0) + x1·w1) + …` in f32, the very sequence `Iterator::sum` folds, so every
number is **bit-identical**: a unit test (`blocked_linear_is_bit_identical_to_the_dot_product`)
compares the kernel with the plain expression bit for bit, and a dump of 60 filled poses (unstyled,
and styled with trajectory 0.5 and head 0.5) from the old and the new binary was byte-identical
(`cmp`). The goldens (`crates/ac-ai/tests/golden`, 10 fill cases and the network case) pass
unchanged, and no fixture's blessed numbers moved. No unsafe, no threads, no new dependencies, f64
maths in the core untouched (the network was f32 already).

The owner's note on the acceptance line ("accuracy over speed") is honoured by construction: the
output is the same to the bit, so there is nothing to review by eye and no MP4 was made for this
item; the fixture's front view got its visual golden (`tests/visual/ai-fill-speed-front.png`) as
every fixture view has.

Not done, and why: the styled fill still runs the network twice (the styled and the "any" pass
are blended), which is what the reference does; sharing work between the two passes would change
nothing but the input projection (2.7 M of 197 M multiply-adds) and is not worth the complexity.
Skipping the network when `ai_force` is 0 was left out: `interp + 0 × (pred − interp)` can differ
from `interp` in the sign of a zero, so it would not be bit-identical. Further speed (AVX2 lanes)
needs `#[target_feature]` dispatch, which is `unsafe` to call, so it is off the table.
