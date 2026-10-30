# Editing audit: timeline, IK, root and layers

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 115300
- **Source:** 115100

## What
The review lists what must be dependable, but no reproduced bugs. Test each and turn what fails into
a fix with a regression test:
- **Timeline:** selecting, dragging, moving, deleting and copying keys.
- **IK:** dragging a hand or foot through its range gives no flip or snap in the knee or elbow.
- **Root:** moving the body leaves pinned limbs where they are and doesn't distort the others.
- **Layers:** a part layer doesn't change bones outside its parts.

`tools/drive_ui.py` covers the timeline and viewport gestures, but needs a window, so it isn't in
nextest or CI (`docs/testing.md:15`). Say whether the Windows CI runner can run it.

## Acceptance
Each of the four has a fixture or a `drive_ui.py` check that drives it; IK is swept in fixed steps
with a limit on the joint's change per step; `python tools/drive_ui.py` passes; the result lists what
failed and each fix; CI runs `drive_ui.py`, or the result says why it can't.

## Result
2026-10-30. No bug found in these four.
- **Timeline and root:** `python tools/drive_ui.py` passes all 108 checks on the release build.
- **IK:** `crates/ac-core/tests/audit.rs` sweeps a foot round a circle in 1 cm steps and a hand past
  full reach and back in 2 cm steps. The knee stays under 5 cm a step and the elbow under 8 cm.
- **Root:** the same file moves the body with both feet pinned. They stay within 1 mm.
- **Layers:** `crates/ac-cli/tests/part_layer.rs`: a Left arm layer changes no bone outside the arm, on
  any frame.

**CI doesn't run `drive_ui.py`.** It needs a window and a release build, and has never been tried on a
hosted Windows runner. That can only be tested by pushing a workflow, so it isn't added here.
