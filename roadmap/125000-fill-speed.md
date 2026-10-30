# Fill speed

- **Status:** not started
- **Requires:** 103000
- **Source:** old `ROADMAP.md` row 7

## Acceptance
One 30-frame AI fill takes under 50 ms in a release build, as measured by `ac fixtures --timings`.

## Notes
Batching gaps together (outside review §12) is not the way: each gap's context is the gap before
it, so `ai.fill_all` is sequential. Look at the forward pass itself and the post-processes
(`ease_borders`, `plant_feet`, `keep_possible`). A styled fill runs the network twice
(`crates/ac-ai/src/lib.rs:363-370`).
