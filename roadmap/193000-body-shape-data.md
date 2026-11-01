# Body shape data for the creator

- **Status:** owner's decision first: the licence
- **Requires:** 189000
- **Source:** owner's request (2026-11-01): a character creator

## What
The creator's sliders need data saying how the surface and the joints move for each shape, and we have none:
the mannequin was exported once with no morph targets (`ATTRIBUTION.md:11`).

Options:

1. **The source the mannequin came from (recommended, if the licence allows).** It is the same mesh family,
   so its shapes should fit our vertices and rig with the least work.
2. **Our own shapes,** sculpted on the mannequin. The licence is simple, but it's a lot of art for good results.
3. **Fit shapes from scans.** The most work, and scan data often has limits on its use.

Not checked yet: whether option 1's shape data is under the same licences as the mesh, and whether it covers
joint positions. Use its data only. The tools around that kind of data can be GPL-family, and our rule is
never to read, copy or link such code (`AGENTS.md`), so check each licence before opening anything.

Bake the data once into an asset on our vertex order: shape offsets for the surface and for the joints, limited
to the humans agreed in 189000. Any baking script lives outside the app's runtime. The app reads the result
in Rust and gets the same bytes from the same inputs.

## Acceptance
- The licence of every piece of data is recorded in `ATTRIBUTION.md` and `assets/SOURCES.json`, and the owner
  has agreed it.
- The baked asset loads in Rust, and all sliders at zero reproduce the mannequin to under 1 mm.
- Every slider at both extremes, and 100 seeded random mixes, pass `body.check` (190000).
- The owner approves a contact sheet of the extremes, front and side.
