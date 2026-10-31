# Engine check of the export in Unity

- **Status:** not started; left for now (the owner, 2026-10-31)
- **Requires:** 122000
- **Source:** 122000, split off by the owner 2026-10-31

## What
122000 checked the exported `.glb` files in Blender. This is its Unity half. Unity isn't installed
here, and installing it needs the owner's Unity account. `DONE_122000-engine-check-export.md`, under
`## Left`, has the steps. 146000's export onto a character was also never opened in Unity, so check
one of those here too.

## Acceptance
Three clips in each of the three bone profiles play in Unity with the same poses as the app, standing
on the ground, with no mirrored sides and root motion on the root bone; screenshots in
`target/shots/<date>/engine-check/unity/`.

## Decision (2026-10-31)
The owner: stays parked until a Unity user reports a problem.
