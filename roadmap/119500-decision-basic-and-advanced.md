# Decision: a simpler interface for beginners

- **Status:** owner's decision first
- **Requires:** none
- **Source:** 115100

## Question
The review proposes Basic mode (timeline, viewport, posing, playback, keys, simple interpolation) and
Advanced mode (layers, AI controls, physics, range, rig controls). Nothing like it exists now; panels
dock and can be hidden (`crates/ac-app/src/app/shell/layout.rs`).

Recommendation: a default dock layout for beginners plus a "show all panels" preset, not a mode. A
mode splits the menus and the help, and hidden features look missing. Decide after 119200, because
the guided path may remove most of the need.

## Acceptance
To be agreed with the owner. Whichever is chosen, `every_command_is_in_a_menu_and_the_palette` keeps
passing, so nothing becomes unreachable.
