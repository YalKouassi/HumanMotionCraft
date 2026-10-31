# Remove the undocumented preset aliases

- **Status:** not started
- **Requires:** none
- **Source:** the 2026-10-31 review of 139000 (finding 3); the owner's decision

## What
`preset` accepts aliases its help and `docs/commands.md` don't name
(`crates/ac-core/src/commands/keys.rs:650`). Only the three named presets stay.

## Acceptance
`ai-settings-presets` passes with the three names and refuses an old alias, listing the three.
