# Remove the undocumented preset aliases

- **Status:** built, waiting for the owner
- **Requires:** none
- **Source:** the 2026-10-31 review of 139000 (finding 3); the owner's decision

## What
`preset` accepts aliases its help and `docs/commands.md` don't name
(`crates/ac-core/src/commands/keys.rs:650`). Only the three named presets stay.

## Acceptance
`ai-settings-presets` passes with the three names and refuses an old alias, listing the three.

## Result
Built 2026-10-31, not yet compiled or run by this worker; the integration build checks it.
`preset` now matches `preserve`, `natural` and `follow_style` exactly, so the aliases, other case and
other separators are all refused. `ai-settings-presets` passes the three and refuses four old
spellings. Fixtures can't check a message, so `a_preset_alias_is_refused_listing_the_three`
(`crates/ac-core/tests/commands.rs`) checks that the refusal lists the three. The panel only sends
the exact names, and `docs/commands.md` and the schema don't change.
