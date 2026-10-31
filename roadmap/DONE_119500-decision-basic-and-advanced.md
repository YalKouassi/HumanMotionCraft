# Decision: a simpler interface for beginners

- **Status:** done (2026-10-31)
- **Requires:** none
- **Source:** 115100; owner decision 2026-10-30

## Decision
Simple and Advanced UI modes. Simple mode provides a beginner-focused layout with essential
controls and clean viewport workspace, hiding advanced tool panels (Layers, Physics, Review,
Scene boxes, AI, Text motion, Pose fill, Pins, Range). Advanced mode makes all tool panels available.
All commands remain accessible in menus and the command palette so nothing is unreachable.

## Acceptance
Whichever is chosen, `every_command_is_in_a_menu_and_the_palette` keeps passing, so nothing
becomes unreachable.

## Result
2026-10-30. Added `UiMode` (`Simple` | `Advanced`) persisted in `layout.ui_mode`. Action `app.simple_mode`
in View menu and header bar toggle (`Mode: Simple` / `Mode: Advanced`) switches modes instantly.
`every_command_is_in_a_menu_and_the_palette` passes.

## Review (2026-10-31)
Code review of commit `119f430` (the part for this item) and the code as it is now.

| # | Finding | Evidence | Severity | Done |
|---|---|---|---|---|
| 1 | `every_command_is_in_a_menu_and_the_palette` is static: `menus::rows` and `palette::entries` do not read the mode, so no mode can hide a command. The acceptance holds by construction. | `crates/ac-app/src/app/shell/tests.rs:53–81` | — | checked |
| 2 | `set_ui_mode` dropped the dock's `Result` for each panel it hides (`let _ =`): a panel the dock refuses to close stayed open while the status said "switched to simple mode" (Rule 1). | `crates/ac-app/src/app/shell/layout.rs` (`set_visible` in the Simple arm, before this review) | low | fixed: the status names any panel still open |
| 3 | The header's Mode button carried its own copy of the switch instead of running `app.simple_mode`, so there were two entry points and the shell test could not see one of them. | `crates/ac-app/src/app/shell/menus.rs:247–252` (before this review) | low | fixed: the button runs the action |
| 4 | The Result says Advanced mode "makes all tool panels available" and the switch is "instant": switching to Advanced only changes the label and the status; the panels Simple closed stay closed until opened one by one from View. They are available in both modes (View menu, palette), so nothing is unreachable, but the file claimed more than the code does. | `crates/ac-app/src/app/shell/layout.rs` (`UiMode::Advanced` arm) | medium (a roadmap claim) | not fixed: whether Advanced should reopen the panels Simple closed (or restore the default dock) is the owner's call; the Decision text now lists Pose fill among the hidden panels, which it is |
| 5 | The mode is applied only at the moment of switching; on start the saved `dock_json` carries the closed panels, so a Simple layout survives a restart, but `app.reset_layout` reopens everything while the header still says Simple. | `crates/ac-app/src/app/shell/layout.rs` (`reset_layout`, `set_ui_mode`) | low | not fixed: same decision as 4 |
| 6 | The saved preference follows the rules: its own `#[serde(default)]` field on `Layout`, lower-case serialized names, never retyped. | `crates/ac-app/src/app/prefs.rs:326–351` | — | checked |

Accepted by the owner (2026-10-31).
