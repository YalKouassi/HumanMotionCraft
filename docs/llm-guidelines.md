# Agent guidelines: feature by feature, confirmed by image

An agent may write the code, but **only a person marks a feature done, after looking at pictures
of it.** `AGENTS.md` holds the rules; `docs/testing.md` says what the tests are.

## The loop
1. **Pick** the next row in the roadmap and read its acceptance line (write one and get it agreed
   if missing).
2. **Fixture first**: `fixtures/<feature>.json` (below), with expectations worked out by hand and
   `errors` entries. Every command the feature adds has typed `inputs`, a fixture that uses it and
   an `errors` entry.
3. **Parity**: if the earlier Python prototype (cf-animate, not public) already did it, the port
   must match its checked-in golden (`tests/fixtures/golden/`) within the golden's stated tolerance.
4. **Implement** the command; the UI only calls it.
5. **Gate**: crate tests while working; `cargo xtask check` before committing.
6. **Render and look** at your own images first (`--screenshot`, MCP `render_preview` / `ui_screenshot`, contact sheets).
7. **Ask**, one feature at a time:

   > **Feature:** Two-bone IK drag (`pose.drag`)
   > **Fixture:** `drag-wrist-reach`: the left wrist dragged 5 cm up
   > **Checks:** 6/6 pass (wrist at target ±1 mm, shoulder unmoved, 2 refused inputs)
   > **Parity:** `two_bone.json` 90/90 within 1e-7
   > **Images:** front, side (attached)
   > **Look for:** elbow keeps its bend plane; no shoulder pop
   > **Known limits:** no stretch yet
   > Confirm, or tell me what's wrong?

8. **Record**: tick the row only after the owner says yes; never on a green CI alone.
9. **Commit** the feature, its fixture and the roadmap change together.

## Fixture format

```json
{
  "$schema": "./schema.json",
  "name": "drag-wrist-reach",
  "feature": "pose.drag",
  "description": "What a reviewer should see, and how the expected numbers were worked out: wrist.L rest head (0.4456329, -0.1788175, 0.1637656) + (0, 0, 0.05).",
  "script": [
    { "command": "pose.set_root", "params": { "root": [0, 0, 0] } },
    { "command": "pose.drag", "params": { "bone": "wrist.L", "by": [0, 0, 0.05] } }
  ],
  "expect": { "bones": { "wrist.L": { "position": [0.4456329, -0.1788175, 0.2137656] } }, "tolerance": 1e-6 },
  "errors": [ { "command": "pose.drag", "params": { "bone": "no.such.bone", "by": [0, 0, 0.05] } } ],
  "views": ["front", "side"]
}
```

- `ac new-command <area.verb>` writes a skeleton of this (and the command's stub). Start a
  fixture with `"$schema": "./schema.json"`: `fixtures/schema.json` (generated, `ac fixtures
  --schema`) lets an editor complete command ids and keys, and a misspelt key is refused.
- A step's `expect` checks that command's JSON result (numbers within `tolerance`, objects key by
  key). Query commands (`pose.get`, `bone.world`) prove what a step changed.
- `errors`: each must be refused with a non-empty message and leave the document unchanged.
- Numbers are worked out by hand, and the description shows how. Numbers copied from the
  program's own output are allowed only when marked `"pinned": "why"`.
- One fixture per behaviour; edge cases get their own fixture. A fixture is never deleted to make
  CI pass; if the behaviour really changes, update it and say so in the confirmation request.

## Agent manners
- Change the document only through commands. In attached mode, say what you are about to do; the
  person can undo with Ctrl+Z.
- If a picture looks off but every check passes, the checks are incomplete: add the missing one.
- Report failures honestly; show the failing output.
