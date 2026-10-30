# Library panel: browse without opening, filter, group, star

- **Status:** done (2026-10-29)
- **Requires:** none
- **Source:** owner request, 2026-10-29 (a review of the left panel's UX: steps 1 to 4 agreed)

## What
Browsing the 224 built-ins meant opening each one: a click opened the animation (or applied the
pose), the built-ins were a tab of their own so a search couldn't reach them from Animations, and
groups were plain labels.

1. **Select, then open.** A click selects and shows a preview popover beside the panel: a larger
   picture that plays the animation at its own speed, its details, Open or Apply, Star, and
   "Duplicate as my own" on a built-in (`crates/ac-app/src/app/library/preview.rs`). A
   double-click or Enter opens or applies. Escape or × hides the popover.
2. **Source switch, not a tab.** Two tabs, Animations and Poses (with counts), each with All /
   Mine / Built-in. Built-ins carry a drawn lock on their thumbnail. `--ui tab=builtin` and old
   preferences still work (Animations, built-ins only).
3. **Real groups.** Headers fold away, with a count, remembered per library (the preferences'
   `library_panel.folded`). Searching or a filter unfolds every group with a match and leaves out
   the rest. Drag one of your own rows onto a group's header to move it there (or onto Starred to
   star it). "Go to group" scrolls to a group and unfolds it.
4. **Search and filters.** Every search word must match the name, group or part. Chips: Short /
   Medium / Long (under 2 s, 2 to 6 s, over 6 s), Loops, Changed (an edited built-in, or the
   open animation with unsaved changes), a body part for poses, and Starred. Order: name (by
   group), recently used, recently modified, length. Starred and Recent (the last 5 used) come
   first when sorted by name.

Commands (`crates/ac-io/src/find.rs`): `library.find` (the panel's filter, as one command for
agents and scripts) and `library.favourite` (stars, kept in `<library>/favourites.json`, follow an
item when it is renamed or regrouped). `library.list` items gain `favourite`, `fps`, `seconds`,
`loop` and `modified`.

## Acceptance
- `fixtures/library-find.json`: hand-worked lengths (30, 90 and 240 frames at 30 fps: 1 s short,
  3 s medium, 8 s long), every word matching, loops, both sort orders, a star following a move,
  and refusals for the wrong kind's filters.
- `cargo xtask check` passes.
- Screenshots in `target/shots/2026-10-29/library-panel/`: the default panel, the popover on a
  selected built-in (the document stays as it was), Starred with a folded group, a two-word
  search, the poses tab, a drag onto a group's header, and a double-click opening the animation.
- The owner tries it and agrees.

## Result
Confirmed by the owner, 2026-10-29. `fixtures/library-find.json` passes (1 s short, 3 s medium,
8 s long; a star follows Mine/Hop to Other/Hop), `cargo xtask check` passes (63 fixtures), and
the screenshots are in `target/shots/2026-10-29/library-panel/` (`02.png` the popover with the
document still untitled, `03-starred-folded.png`, `04-search.png`, `05-poses.png`,
`06-dragging.png`, `08-double-click-opened.png`).
