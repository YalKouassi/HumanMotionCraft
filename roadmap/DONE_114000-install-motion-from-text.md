# Install motion from text from the app

- **Status:** done
- **Requires:** 113000
- **Source:** old `ROADMAP.md` row 6

## What
The Motion from text panel lists what the install needs (`text.install` checks the source tree,
`uv`, the C++ build tools and the GPU; without a `uv` of the person's own the install fetches a
pinned, hash-checked one, uv brings its own Python into the install, and Kimodo comes as GitHub's
archive checked by hash, so neither Python nor Git nor any account is needed), names the licences
installing accepts, and Install runs `setup_textmotion.py` in the background (through `uv run`),
with its numbered step, Stop and the log (owner's decision, 2026-10-29). The setup stops when the
app closes its stdin, and starting again resumes. It needs the source tree (the setup copies
training's rig maths), so packaging (124000) must ship `tools/textmotion` and those modules with
the app.

## Acceptance
On a machine without the install, the panel lists what's needed and Install runs the setup with
step progress and Stop; after Stop, Install resumes; when it finishes the panel offers Generate
without restarting the app; `fixtures/text-install.json` passes; screenshots of each state.
