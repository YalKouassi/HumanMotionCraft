# Decision: a simpler interface for beginners

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** none
- **Source:** 115100; owner decision 2026-10-30

## Decision
Simple and Advanced UI modes. Simple mode provides a beginner-focused layout with essential
controls and clean viewport workspace, hiding advanced tool panels (Layers, Physics, Review,
Scene boxes, AI, Text motion, Pins, Range). Advanced mode makes all tool panels available.
All commands remain accessible in menus and the command palette so nothing is unreachable.

## Acceptance
Whichever is chosen, `every_command_is_in_a_menu_and_the_palette` keeps passing, so nothing
becomes unreachable.

## Result
2026-10-30. Added `UiMode` (`Simple` | `Advanced`) persisted in `layout.ui_mode`. Action `app.simple_mode`
in View menu and header bar toggle (`Mode: Simple` / `Mode: Advanced`) switches modes instantly.
`every_command_is_in_a_menu_and_the_palette` passes.
