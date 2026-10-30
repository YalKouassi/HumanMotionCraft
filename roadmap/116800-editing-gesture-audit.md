# Editing audit: timeline, IK, root and layers

- **Status:** not started; the acceptance line needs the owner's agreement
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
