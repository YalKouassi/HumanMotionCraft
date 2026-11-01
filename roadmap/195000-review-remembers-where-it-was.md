# Review app: a plain launch comes back to where it was

- **Status:** built, waiting for the owner
- **Requires:** 174000
- **Source:** owner request (2026-11-01): launching the review app should load the sets

## What
The window only knew a library given with `--library`, and looked for sets in one default folder, so a plain launch opened with no clips of the owner's, an empty set list and "none" as the set. With "none" open, K, C and R go to the decisions file.

The window now keeps its library, sets' folder and the set open in `review-session.json` beside the app's library, outside any repository (`crates/ac-review/src/remembered.rs`). What a launch leaves out comes from there; what it names wins. A set coming back opens the Styles view. Choosing "none" doesn't lose the remembered set. Whatever can't come back is said in the window. `ac-review run` reads only what it is given, and a `--screenshot` run writes nothing.

## Acceptance
The owner launches the review app with no options and finds their library and all their sets, on the set they last had open.

## Result
Unit tests in `remembered.rs`: a plain launch comes back, what a launch names wins, another folder doesn't take the set, what has gone is said, a bad file is an error. On the owner's machine, a window opened on one set and closed, then a launch with no options from another folder: the same library and set, the totals right, nothing written by the `--screenshot` run.
