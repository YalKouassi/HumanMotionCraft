# Export dialog: product names allowed, root mode remembered

- **Status:** not started
- **Requires:** none
- **Source:** the 2026-10-31 review of 122500 (findings 3 and 4); the owner's decision

## What
The presets keep naming products, because people need to see which fits their engine, so `AGENTS.md`'s rule on
other products' names gets an exception for export presets. The dialog remembers the joint profile but opens on
`hips` every time; it should remember the root mode too (a `#[serde(default)]` field).

## Acceptance
`AGENTS.md` states the exception and `cargo xtask names` passes. Choose ground and restart: the dialog opens on
ground, and an old preferences file still loads.
