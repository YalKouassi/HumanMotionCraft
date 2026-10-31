# Export dialog: product names allowed, root mode remembered

- **Status:** done (2026-10-31)
- **Requires:** none
- **Source:** the 2026-10-31 review of 122500 (findings 3 and 4); the owner's decision

## What
The presets keep naming products, because people need to see which fits their engine, so `AGENTS.md`'s rule on
other products' names gets an exception for export presets. The dialog remembers the joint profile but opens on
`hips` every time; it should remember the root mode too (a `#[serde(default)]` field).

## Acceptance
`AGENTS.md` states the exception and `cargo xtask names` passes. Choose ground and restart: the dialog opens on
ground, and an old preferences file still loads.

## Result
Built 2026-10-31, not yet compiled or run by this worker; the integration build checks it.
`AGENTS.md` states the exception without spelling a name. `cargo xtask names` needs no change: hashing
the preset names by hand against its lists finds none of them there. `export_root` is a new optional
field on the saved file prefs and on `AppState`. The dialog keeps the root and the joint profile as
soon as they are chosen, not only on export, so "choose ground and restart" holds without exporting;
the profile moved with it because a preset sets both. Tests: `the_export_root_is_remembered_and_an_older_file_has_none`
(`files/state.rs`), `the_dialog_opens_on_the_root_chosen_last` (`files/export.rs`). The restart itself
has not been tried in the window.

## Closed
Accepted by the owner (2026-10-31).
