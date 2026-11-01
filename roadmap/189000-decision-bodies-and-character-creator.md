# Decision: other bodies and the character creator

- **Status:** owner's decision first
- **Requires:** none
- **Source:** owner's request (2026-11-01): a later character creator as a separate app, humans only, using
  this rig, with the foundation for swapping in other meshes that follow the same rig

## Question
Four answers shape 190000 to 194000. Each has a recommendation.

**1. What counts as "the same rig"?**

- **Same joints and same lengths:** the simplest. Only the surface changes, so height and limb length can
  never change.
- **Same joints, own lengths (recommended):** the same 104 names and parents and the same rest directions,
  but each body may have its own bone lengths.

The recommended version fits how the rig works. Every bone rests with no rotation of its own, and a pose
is only rotations applied to rest positions (`crates/ac-core/src/rig.rs:3-10`). Rotations therefore carry
over to any lengths, and the root offset is already in hip heights. The app still animates only the built-in
rig, so AI Fill, physics, ranges and the library stay as they are (144000). Other lengths move hands and
feet, so contacts need the fixes in 192000.

**2. Which humans?**

Recommended: adults only, with neutral bodies like the mannequin, no anatomical detail and no child bodies.
There are no clothes, the bodies could be exported and shared, and the rig, AI Fill and ranges are built
around one adult body.

**3. Where does the app live?**

Recommended: its own crate and window in this repo, as `ac-review` is, so it shares the renderer, the rig
and the body check. It joins the launcher as its own app. It stays Rust at runtime.

**4. Can bodies be shared?**

Recommended: local files only at first. Community sharing needs its own item and checks.

## Acceptance
All four are answered here with the date, and 190000 to 194000 are changed to match.
