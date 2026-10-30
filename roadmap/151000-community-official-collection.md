# Built-ins slimmed: most of the collection moves to the community

- **Status:** not started (the owner chose this, 2026-10-30; acceptance proposed)
- **Requires:** 149000, 150000
- **Source:** owner's choice of option B, 2026-10-30 ("Option B i guess. without C")

## What
The built-in animations take up 69 MB, nearly all of `assets/library`. Keep a starter set built
in and publish the rest to the community under an Official account, with a badge. No new clips are
generated for it (no option C).
- **Starter set:** about two dozen, a few of each type, plus every clip a feature needs (the two
  straight CMU runs used by travel) and every clip a fixture names. Poses stay built in.
- **The rest:** uploaded by a script on the maintainer's side and published at once. Licences:
  - 100STYLE: CC-BY-4.0, credited;
  - the generated clips: CC0;
  - CMU: a third licence value, "free with acknowledgement", which the server and the Share form
    learn. Only the Official account can use it.
- **Existing libraries keep what they have.** `library.sync` turns a built-in that moved to the
  community into a community install (origin `community:<id>`). It doesn't move it to `.retired`.
- `ATTRIBUTION.md` and `assets/library` rows follow the move.

## Acceptance
- `assets/library/animations` under 10 MB, and every fixture and feature that names a built-in
  still finds it.
- The community lists the moved clips as Official, with the right licence and credit for each.
- `library.sync` on a library from before the move keeps every moved clip as a community install
  and retires nothing (a test in `crates/ac-io/tests/builtins.rs`).
- `cargo xtask check` passes.
