# Flag the takes the model fails on

- **Status:** done (2026-10-31)
- **Requires:** 126000
- **Source:** 115000 (review 2 §14)

## What
After training, evaluate every take's windows and put the worst 1 % into the `ac-review` queue
with their loss. They go for a person to look at, never an automatic rejection: high loss is often
just hard motion (acrobatics), not bad data.

## Acceptance
`ac-review run review.list` can filter to the flagged takes; flagging writes nothing to
`training/review/decisions.json` by itself.

## Result
2026-10-30. Built as `python -m ac_train.flag` (`training/ac_train/flag.py`) and a reader in the
review app (`crates/ac-review/src/flags.rs`).

**How it flags.** Every window of every take of every set the registry allows is filled by the
model (clean context, a gap of 15 frames, nothing given, the take's own styles as training gives
them) and given training's loss, one number a window
(`ac_train.losses.window_terms`: the same terms `terms` averages over a batch, with the card's
`loss_weights`). Windows keep clear of the scan's cuts and the review's decisions, as training's
do (`ac_train.review.starts`), since a cut is known bad already; takes training leaves out
(rejected by the scan or the review) or too short for one window are skipped and counted, not
evaluated: they are in the queue already. A take's loss is the mean over its windows (`--by worst`
ranks by the worst window instead); the worst `ceil(1 %)` of the evaluated takes, at least one, are
written to `training/data/rig/flags.json` (gitignored, beside the scans the review app reads) with
their loss, the worst window's loss and frame, each loss term weighted, and a `why` line. The file
names each set's takes and lengths by a CRC-32 (`digest`, the same in Python and Rust), so flags
made before a set was rebuilt are told apart.

**Decisions made here (the owner was not asked).** Every window, not a stride: the run is fast
(about 20,000 windows a second on the RTX 3080 Ti; `--stride n` exists for a quicker look). One
gap of 15 frames, the middle of the app's range, rather than every gap: the ranking is about the
data, not the gap bin, and one gap keeps the run to minutes. Ranking by the mean window loss, not
the worst window: a retarget or capture problem lifts the whole take, while a single bad moment
the scan missed shows up as `worst_at` in the file and in the take's panel. The worst 1 % is
rounded up (1 of 101 takes is 2), so a small set still flags one.

**The review app.** `review.list {"show": "model"}` lists the flagged takes worst first (rank,
loss, worst, worst_at, windows on each row; every other filter's rows say `model: null` for the
rest); `review.take` adds `why` and the weighted terms; `review.summary` reports the file
(`flags`: present, note, model, path, takes). In the window the filter gains **Model fails**, a
flagged take's queue row ends `loss 1.028 #1`, and its panel has a "The model" section with the
why line, a link to the worst window and a reminder that a high loss is often hard motion. Rule 1:
no flags file is said (`review.summary`'s `flags.note`, `review.take`'s `model_note`, the window's
filter bar; `show: "model"` refuses with the command that makes the file) and never silent; a set
rebuilt since the flags were made has its flags left out and a `problems` line saying so; a file
that is not a flags file or is newer than the reader is a `problems` line and the review opens
without it. Nothing here reads or writes `decisions.json`.

**Tests.** `training/tests/test_flag.py` (the 1 % rule, the ranking's order and ties, the file's
format and `why` line, no takes, the digest's numbers shared with the Rust fixture);
`crates/ac-review/tests/review.rs::the_models_flags_list_worst_first_and_stale_or_missing_ones_are_said`
on the fixture files `crates/ac-review/tests/data/flags.json` and `flags-stale.json` (listing,
the take view, the summary, no file, a stale file, not a flags file, a newer version; the decisions
file is never written).

**The run** (`uv run python -m ac_train.flag --tag parts-lowweights --checkpoint best`, the model
`assets/models/parts` ships from): 4,581 of 5,243 takes evaluated (662 skipped: left out by the
scan or the review, or shorter than 26 frames), 2,928,489 windows, **146 s** wall time including
loading; median take loss 0.141; 46 takes flagged. The worst 20:

| # | take | loss | worst (frame) | windows | what it is |
|---|---|---|---|---|---|
| 1 | cmu/flip_cmu_88_09 | 1.028 | 1.722 (71) | 36 | stretch and cartwheel |
| 2 | cmu/jump_cmu_75_03 | 0.899 | 0.952 (10) | 4 | jump |
| 3 | accad/flip_accad_flip | 0.848 | 1.963 (40) | 61 | the character-figure flip |
| 4 | cmu/jump_cmu_49_05 | 0.831 | 0.918 (11) | 16 | jump |
| 5 | cmu/any_cmu_78_14 | 0.825 | 1.417 (9) | 18 | OffensiveMoveSpinLeft (basketball) |
| 6 | cmu/run_cmu_128_04 | 0.823 | 0.937 (14) | 9 | Run Duck Underneath |
| 7 | cmu/walk_cmu_78_19 | 0.811 | 1.150 (20) | 19 | subject 78 (basketball moves) |
| 8 | cmu/any_cmu_78_01 | 0.802 | 1.027 (22) | 17 | LeftTightTurn |
| 9 | cmu/stretch_cmu_88_10 | 0.788 | 1.195 (53) | 91 | stretch |
| 10 | cmu/flip_cmu_89_04 | 0.784 | 2.065 (303) | 202 | flip |
| 11 | accad/run_accad_female1_c03_run | 0.762 | 0.924 (13) | 11 | run |
| 12 | addbiomechanics/jump_addbiomechanics_uhlrich2023_subject3_djasym2_segment_1 | 0.753 | 0.940 (13) | 14 | drop jump |
| 13 | cmu/run_cmu_128_09 | 0.737 | 0.820 (22) | 18 | Run Roll Underneath |
| 14 | cmu/walk_cmu_78_18 | 0.734 | 0.882 (14) | 28 | subject 78 |
| 15 | addbiomechanics/jump_addbiomechanics_uhlrich2023_subject10_dj3_segment_1 | 0.726 | 0.835 (15) | 11 | drop jump |
| 16 | addbiomechanics/jump_addbiomechanics_uhlrich2023_subject10_djasym3_segment_1 | 0.710 | 0.844 (15) | 9 | drop jump |
| 17 | addbiomechanics/jump_addbiomechanics_uhlrich2023_subject10_djasym1_segment_1 | 0.701 | 0.812 (14) | 11 | drop jump |
| 18 | cmu/run_cmu_127_24 | 0.699 | 1.608 (20) | 60 | run |
| 19 | cmu/flip_cmu_88_04 | 0.698 | 1.505 (140) | 142 | flip |
| 20 | cmu/jump_cmu_143_07 | 0.698 | 1.073 (24) | 16 | jump |

The rest (21–46): CMU jumps, runs and subject 78's basketball feints, `dance_cmu_85_15`
(90TwistsFall), `walk_cmu_104_12`, and sixteen more AddBiomechanics `uhlrich2023` drop jumps
(every flag's biggest term is `pos`). As the item expected, the list is hard motion (flips, jumps,
drop jumps, basketball spins and feints), not obviously bad data: a person should look at the
CMU subject 78 takes and `run_cmu_127_24` / `run_cmu_128_11` (worst windows 1.6–1.7, well above
their means) first. Nothing was decided: `training/review/decisions.json` is unchanged.

Proof shot: `target/shots/2026-10-30/137000/review-flip.png` (the window opened on
`cmu/flip_cmu_88_09` at frame 71: the queue row's `loss 1.028 #1`, the "The model" section, the
link to the worst window).

## Review (2026-10-31)
Reviewed `training/ac_train/flag.py`, `crates/ac-review/src/flags.rs`, the review commands
(`crates/ac-review/src/review.rs`) and `training/tests/test_flag.py` (commit c1beb99).
- **Acceptance holds:** `flag.py` reads `decisions.json` through `review.load_decisions` (read only,
  `training/ac_train/review.py:79`) and writes only `--out`; the test asserts `flags.json` is the only file
  written. `ac-review` filters by the flags (`Show::Model`, `review.rs:55`; rows carry `model`), refuses
  `show: "model"` with the command that makes the file, and `flags.rs` reads nothing else. The CRC-32
  digest hashes the same text in both languages.
- **Fixed (medium, Rule 1):** a take whose loss came back NaN or infinite sorted anywhere in `rank` and
  `json.dumps` wrote `NaN`, which `serde_json` refuses, so one bad window would have made the review app
  report "not a flags file" and show no flags at all. Such takes are left out, listed under `unusable` in
  the file and printed first (they are worth a look themselves). Test
  `test_a_take_with_a_loss_that_is_not_a_number_is_left_out_and_named` (not run here: `test_flag.py`
  skips without torch; the function has no torch in it).
- **Not fixed, noted (low):** `flags.rs::load` reads the whole file before checking `MAX_FLAGS`, with no
  size cap (a dev-only reader of a file the pipeline writes); two flags with one id keep the last silently;
  flags of a set not loaded stay, as the doc comment says.
- The Result's claims hold; the run's numbers are the owner's to read.

Accepted by the owner (2026-10-31).

## Decisions on the review (2026-10-31)
The owner: the low leftovers stay as they are.
