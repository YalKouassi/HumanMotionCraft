# Reliability and realism review: recheck of an outside review

- **Status:** done (2026-10-30; checked against the code at `78837aa`)
- **Requires:** none
- **Source:** an outside review the owner pasted on 2026-10-30, written from a source dump, without
  running the app

Every claim was checked against the code, and the shipped clips were measured. This file records
each verdict and where the work now lives. Everything accepted has its own file.

## Measured
All 55 clips in `assets/library/animations`, headless, with `target/debug/ac.exe` and `clip.metrics`,
`clip.critique`, `clip.possible`, `feet.lock` and `physics.solve`.

| What | Result |
|---|---|
| Worst slide on a planted foot, keys / with physics | median 7.2 / 4.1 cm; over 10 cm on 16 / 4 clips |
| Root jumps from the physics pass (over 1 m/s above the keyed speed) | 39, on 9 clips: 22 where balance turns on or off, 6 on sliding frames, 11 unexplained (all in `Locomotion/Run`), 0 in flights |
| `Locomotion/Walk`, root speed | keys at most 0.1 m/s; with physics 4.1 m/s, about 14 cm in one frame |
| `Locomotion/Run`, worst jump | 19.4 m/s above the keyed speed, about 65 cm in one frame |
| `clip.critique`, worst finding per clip | high on 26, medium on 25, low on 1, none on 3 |
| Limit findings against `clip.possible` | 25 clips have medium or high limit findings, yet read typical or unusual |
| `feet.lock` on the keys | largest velocity change at a key up on 43 clips, down on 0; slide down on 31, up on 9; keys 2374 to 4723 |
| `On the floor/On the ground 10`, frame 76 | physics moves the root about 0.56 m; penetration 1.14 to 4.97 cm |

## Claims
| Claim | Verdict | Where now |
|---|---|---|
| Physics has ground correction, contact detection, foot locking, static balance, ballistic jumps | Correct (`crates/ac-core/src/physics.rs:1-24`) | |
| Contact is a distance and speed test | Correct: 5 cm and 0.5 m/s, per foot point, per frame, with no minimum duration (`physics.rs:35`, `:43`, `:503-505`) | 117400 |
| Sliding is corrected through the root | Correct: the fulcrum rule only moves the root, by the mean of the slips (`physics.rs:514-522`) | 117400 |
| Balance is static, applied when the centre of mass is slow | Correct, and a bug the review missed: loops that walk on the spot count as still, and balance switches per frame with no blend | 115600, 118000 |
| Foot orientation isn't kept, only position | Half right: `two_bone` keeps a foot's world orientation by default (`crates/ac-core/src/ik.rs:159`), so `feet.lock` keeps each frame's own turn. Nothing holds one orientation for a contact, and nothing rolls the foot | 117700 |
| The in-betweener solves a group of bones, the rest keep their spline | Correct: fingers, toes, twist bones and the head (`crates/ac-ai/src/lib.rs:7-8`) | 119800 |
| Critique ranks findings and gives a one-undo fix | Correct as written (`crates/ac-core/src/critique.rs`), but misleading: the fix it suggests most does harm, and it flags nearly every shipped clip | 116200, 118600 |
| Expose review through a button | Agreed. `clip.critique` is already in File and the palette as "Review animation", but its answer only reaches the status line (`crates/ac-app/src/app/shell/actions.rs:453-455`). The Physics and Range panels already mark problems on the timeline | 118900 |
| The app has a palette, library, block-out wizard, dope sheet, inspector, review | Correct, except that review has no panel | |
| Build on fixtures, visual goldens and UI automation | Agreed. `tools/drive_ui.py` isn't in CI (`docs/testing.md:15`) | 115300, 116800 |
| Undo must reverse every operation | Worth checking: the direct round trip test covers two commands (`crates/ac-core/tests/commands.rs:117`) | 116500 |
| Save and reopen must preserve the animation | Partly covered (`crates/ac-io/tests/library.rs:11`, `fixtures/recovery-restore.json`) | 117100 |
| Evaluation measures joint error, slide, floor, continuity, jerk, impossible poses, against interpolation | Correct (126000) | |
| Next, add evaluation scenarios | Mostly built: 126000 already breaks down by motion type and has plant-change and turn slices. Starts, stops, crouches and landings are missing, and crouch is filed under `ground` (`training/ac_train/labels.py`) | 126500 |
| Add data where evaluation finds gaps | Agreed, free-licence sets only (112000) | 134750 |
| Basic and Advanced modes | The owner decides | 119500 |
| Simpler default workflow | Agreed; the block-out wizard and welcome page are the start | 119200 |
| Arm swing, spine, head and follow-through | The owner decides how; it must leave manual keys alone | 119800 |

## Rejected or narrowed
- **Anticipation before stepping, and recovery after losing balance, in the physics pass:** left out.
  A pass that corrects motion can't invent an intention the keys don't have.
- **A collection of example bug clips from the review's list:** the review names examples, not
  reproduced bugs. Only reproduced bugs go in (115300).
- **Carrying as body response to loads:** the rig has no load model, so 126500 only scores the
  `carry` takes.
- **"Pause adding major features":** the owner's call. New items take the lowest numbers, and the
  open AI and community items are left as they are.

## Missed by the review
- The physics pass tilts lying bodies up (102000's known issue): 115900.
- The roadmap already has much of its AI list: 128000 to 143000, 138000 for fill diagnostics.
- AI Fill stays AI: no realism or workflow item here routes it through the Walk loop.

The images and "Pasted text" links in the review are not sources and were ignored.
