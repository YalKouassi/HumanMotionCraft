# Training

The Python side of HumanMotionCraft: it retargets motion capture onto the app's rig, trains the
in-betweener (AI fill), exports the weights the app runs in plain Rust, and cuts the built-in mocap
clips. Nothing here runs at app time.

| Module | What |
|---|---|
| `ac_train.datasets` | the dataset registry (source, licence, joint map), readers (BVH, glb) and the retarget to `training/data/rig/<set>.npz` |
| `ac_train.train` | trains the network on the retargeted sets into `training/data/models/<tag>/` |
| `ac_train.export` | writes `assets/models/<tag>/` (the weights the app loads) and the `ac-ai` goldens |
| `ac_train.library` | cuts 3-second clips from the retargeted sets into a library folder |
| `ac_train.infer` | the reference fill (PyTorch) that the goldens pin the Rust port to |

Everything under `training/data/` is gitignored: the downloads, the retargeted sets, the PyTorch
checkpoints and the clips that may not be committed.

## Checks

From `training/` (CI runs the first two on every push):

```bash
uvx ruff check .                                   # lint (config in pyproject.toml)
uv run --with pytest python -m pytest -rs          # tests: quaternions, retarget, export round trip
uv run python -m ac_train.pipeline --quick         # retarget, train 20 steps, export, cargo test -p ac-ai
```

The tests need only numpy and the committed assets; the export tests also need torch, and the
retargeted-set tests need `training/data/rig` (each skips and says why). The quick pipeline writes
to a temporary folder (`AC_TRAIN_OUT`) and checks the Rust port on the model it just made
(`AC_AI_MODEL`, `AC_AI_GOLDEN`), so `training/data`, `assets/models` and the goldens stay as they
are. `--keep` keeps the folder; `--no-cargo` skips the Rust step.

## Datasets

| Set | Source | Licence | In git |
|---|---|---|---|
| `builtin_loops` | the CC0 loops already in `assets/library` | CC0 1.0 | yes (they are the assets) |
| `cmu` | [CMU Graphics Lab Motion Capture Database](http://mocap.cs.cmu.edu), BVH conversion from [una-dinosauria/cmu-mocap](https://github.com/una-dinosauria/cmu-mocap) | free for research and commercial use | clips cut from it |
| `100style` | [100STYLE](https://www.ianxmason.com/100style) (Ian Mason, Sebastian Starke, Taku Komura), [Zenodo](https://zenodo.org/record/8127870) | CC BY 4.0 (commercial use allowed, with credit) | clips cut from it |
| `kimodo` | motion we generate with [Kimodo](https://github.com/nv-tlabs/kimodo) (NVIDIA, model `Kimodo-SOMA-RP-v1.2`) from the prompts in `training/kimodo/prompts.tsv` | the model is under the NVIDIA Open Model License, which claims no ownership of what it generates | clips the owner approved; the prompts are committed, the BVHs are not |

The repo supplies these four sets only, and every one allows commercial use, so the committed model
`assets/models/parts` and everything cut into `assets` carry no non-commercial terms; the model's
card (`weights.json`) lists the sets.

**Your own non-commercial data.** You may add a set of your own under a non-commercial licence: give
it a `Dataset` entry in `REGISTRY` (`ac_train/datasets.py`) with `commercial=False` (leave `share` at its default, `"never"`, so publish never shares it). The tools then keep it on your machine:
training reads it only with `--noncommercial`, export refuses to write such a model into the repo
(it goes to your app's own models folder, step 5), the library cutter puts its clips in the
gitignored `training/data/library` and never in `assets`, and publish never shares it. Don't commit
that registry entry.

100STYLE is one actor walking and running in 100 styles. Each style has eight takes (forwards,
backwards and sideways walks and runs, idle, and transitions), named `<Style>_<gait>.bvh`. Only the
frames in `Frame_Cuts.csv` are read, since each file starts with setup frames. A take's type comes
from its gait: `walk_100style_FW_007` is style 7 walking forwards. The style itself stays out of the
take name, so a style called "Kick" is still a walk. It is kept in the set's `labels` instead
("Kick: ... (forwards walk)"), and becomes the style word `kick` (see "Styles" below).

CMU's files are only numbered, so a take's type (walk, dance, fight, ...) comes from its line in
the set's index (`cmu-mocap-index-text.txt`, see `CMU_TYPES` in `datasets.py`). Takes that need
scenery the rig does not have (playground, climbing, ladders, stairs, uneven terrain) are skipped.
Frame 0 of every CMU file is an added T-pose and is dropped.

The `kimodo` set is generated, not downloaded. `training/kimodo/prompts.tsv` lists each prompt
with a name, a take type and a length in seconds (at most 10). A take's type comes from that type
column alone (`carry_kimodo_carry_box_s1`), never from the prompt's words. Each prompt is made once
a seed, so `_s1`, `_s2` and `_s3` are three different takes of the same prompt.

## How to retrain

Needs [uv](https://docs.astral.sh/uv/), about 10 GB of disk, and an NVIDIA GPU for training
(retargeting and export run on the CPU). Every command runs from `training/`.

### 1. Set up

```bash
cd training
uv sync            # numpy and CUDA torch into training/.venv
```

### 2a. Or fetch the sets already retargeted

The retargeted sets (all four, with their scans, about 400 MB) are published as this repository's
GitHub release `rig-data-c551151` (`training/rig-data.json`). This downloads them over plain HTTPS
(no login needed; a private repository's release goes through the GitHub CLI) and checks each
file's SHA-256, so steps 2 to 3b can be skipped:

```bash
uv run python -m ac_train.fetch                          # every published set
uv run python -m ac_train.fetch cmu 100style             # some of them
uv run python -m ac_train.fetch --from <folder>          # from a copy of the release's files
```

A set already here and the same is left alone; one that differs (built here) is kept unless
`--force`. Release data built for another rig than `assets/rig/default_rig.json` is refused. A set
the release doesn't have is built here (steps 2 and 3). A copy without a GitHub `origin` (a ZIP
download, or a fork without the release): download the release's files and use `--from <folder>`.

To publish (whoever built the sets, from a clean commit):

```bash
uv run python -m ac_train.publish                        # every built set (all are shareable)
```

It checks each set is stamped, fits today's rig, was built from committed code and has a current
scan, copies the files to `data/publish/<tag>/` with `NOTES.md` (licences and credits), writes
`training/rig-data.json` (commit it) and prints the `gh release create` command. It uploads
nothing itself.

### 2. Download the datasets into `training/data/raw/`

```bash
mkdir -p data/raw && cd data/raw

# CMU (about 3 GB of BVH)
git clone --depth 1 https://github.com/una-dinosauria/cmu-mocap.git cmu-mocap

# 100STYLE (1.5 GB zip, 3.2 GB of BVH): only 100STYLE.zip is needed, not the 15 GB labelled data
curl -L -o 100STYLE.zip https://zenodo.org/api/records/8127870/files/100STYLE.zip/content
unzip 100STYLE.zip            # the files must end up as 100STYLE/<Style>/<Style>_<gait>.bvh,
                              # with 100STYLE/Dataset_List.csv and 100STYLE/Frame_Cuts.csv
cd ../..
```

The readers look for `cmu-mocap/data/**/*.bvh`, `100STYLE/*/*.bvh` and `kimodo/*.bvh` (step 2b).
A set you skip is reported and left out.

### 2b. Generate the `kimodo` set

This needs `uv` and Git, an NVIDIA GPU (12 GB is enough), 32 GB of RAM, about 35 GB of free disk
(the install is 22 GB), and on Windows the Visual Studio C++ build tools (the same ones Rust uses).
One command installs everything (the app's Motion from text panel, **Install**, runs the same setup), pinned to exact versions and checked by
hash: Kimodo's code, its weights, and its text encoder (LLM2Vec on Meta Llama 3 8B Instruct,
downloaded under the [Meta Llama 3 Community License](https://www.llama.com/llama3/license/); by
installing it you accept that licence). The same install gives the app its "motion from text"
commands (`text.generate`, `text.fill`).

```bash
python ../tools/textmotion/setup_textmotion.py         # prints where it installed (re-running is quick)
# then, with <install> the folder it printed (.venv/bin/python on Linux and macOS):
<install>/.venv/Scripts/python -I <install>/generate.py     --prompts kimodo/prompts.tsv --out data/raw/kimodo --seeds 1,2,3
```

Loading takes about a minute, then each clip takes seconds (on an RTX 3080 Ti); the text encoder
runs on the CPU unless the GPU has 24 GB. A clip already in `data/raw/kimodo` is skipped, so a run can be stopped and resumed, or
extended with more seeds. `AC_TEXTMOTION_DIR` installs somewhere other than the default
(`%LOCALAPPDATA%/<app>/textmotion`).

### 3. Retarget onto the rig

```bash
uv run python -m ac_train.datasets                       # every set, about 40 minutes
uv run python -m ac_train.datasets cmu --limit 20        # a quick check on 20 takes
```

Writes `data/rig/<set>.npz` and prints the takes, frames and hours of each set. The build drops
setup frames at a take's start (the hips jumping metres in a frame) and moves each take's floor
to z = 0. Each set is stamped with what it was built from (`ac_train/stamp.py`: a CRC-32 of the
rig file, a hash of the pipeline code, the commit): training, the library and the review app
refuse a set built for another rig, and say when one has no stamp or came from other code.

### 3b. Scan for bad data

```bash
uv run python -m ac_train.quality                        # every set in data/rig, about 2 minutes
uv run python -m ac_train.quality kimodo                 # one set
```

Writes `data/rig/<set>.quality.json`: per take its numbers, the frame ranges to cut and why
(`glitch`, `teleport`, `sink`, `skate`, `float`, `frozen`), and whether the whole take is rejected.
It prints how many hours each set loses and to what. The thresholds and what each reason means are
in `ac_train/quality.py`. The npz files are left whole: training and the library skip the cuts
(step 4). Most flags are CMU feet twitching for a frame or two (CMU's foot rotations are noisy:
their fastest turns are three times the other sets'); they are hard to see at full speed, and they
cost little, because only a couple of frames either side go.

### 3c. Review the takes

```bash
cargo run -p ac-review --release                         # the review window (developers only)
cargo run -p ac-review --release -- unpack               # unpack every set first (about 10 s, 3 GB)
cargo run -p ac-review --release -- run review.summary   # headless, for agents: JSON out
cargo run -p ac-review --release -- run review.list '{"show": "flagged", "limit": 20}'
cargo run -p ac-review --release -- run review.decide '{"id": "cmu/run_cmu_16_54", "status": "cut", "tags": ["tired"], "by": "agent"}'
```

The queue on the left puts takes nobody has decided first, the scan's rejections and the worst
flagged first among them. The bar under the body shows the scan's flags in red and the frames the
decision cuts in orange. Keys: Space plays, Left/Right step (Shift: 10), Up/Down change take,
F jumps to the next flag, Shift-drag on the bar or I/O select frames (Esc drops the selection),
K keeps the whole take, C cuts (the flagged ranges plus the selection), X takes the selection out
of the cuts, R rejects the take, Backspace forgets the decision. Drag the body to turn the camera.

Decisions go in `training/review/decisions.json` (checked in; one entry per `set/take`: `status`
`ok`/`cut`/`reject`, `cuts` as inclusive frame ranges, `type`, `tags`, `note`, `frames`, `by`).
`type` and `tags` are labels, and labels become the model's styles (below); anything else you want
to remember about a take goes in `note`.
A take with no decision falls back to the scan, so nothing needs reviewing before training: review
what the scan may have got wrong. The rotations are unpacked once to `data/rig/.review-cache/` and
unpacked again when the npz changes.

What training would use, without training (`--dropped` lists the takes left out and why):

```bash
uv run python -m ac_train.review
```

### 3d. Styles

The model's styles are the labels in the training data, nothing else (`ac_train/labels.py`). A
take's labels are its motion type, then its style words:

- the **type** is the review's `type` when it gives one, else a word from the take name (`walk`,
  `run`, `jump`, `idle`, `combat`, `ground`, `acrobatic`, `fall`, `dance`, `gesture`, `sit`,
  `carry`, `sports`, `exercise`; the first type with a word in the name wins, and a name that
  matches none has no type);
- the **style words** are 100STYLE's style name when it is a way of moving (`labels.STYLE100`:
  33 of the 100, e.g. `natural`, `crouched`, `elderly`, `limp-left`, `zombie`) and the review's
  `tags`, added. The other 67 are held poses or props (Teapot, Arms folded, On phone), arm
  actions you would key (Waving, Punch) or paths (Spin): the keys already say those, so they
  are no style. Their takes still train under their type, and the library keeps a clip of each.

Labels are normalised (lower case, `-` between words: "Old Man" is `old-man`), and a plain
synonym of a type ("fight", "running") is that type. A label becomes a style when the training
takes have at least `--min-style-minutes` of it (5 by default); rarer ones are left out and
printed. So tag freely: a new tag becomes a style at the next training run once there is enough
of it, and the app offers whatever the model card lists. A take can have several styles; the
network adds them up, and learns each one alone too (each is dropped a quarter of the time).
Mirrored windows swap `left`/`right` and `clock`/`anti-clock` labels. Rare types are drawn
more often (up to three times; `labels.weights`).

What the next training run would learn, without training:

```bash
uv run python -m ac_train.labels
```

### 4. Train

```bash
uv run python -m ac_train.train --minutes 180 --tag parts                        # the model that ships
uv run python -m ac_train.train --minutes 180 --noncommercial --tag parts-own    # with non-commercial sets you added: never shipped
```

By default training reads only the sets whose licence allows commercial use (`commercial=True` in
`ac_train/datasets.py`: every set the repo supplies), so the model in `assets/models`, and what
people make with it, carries no non-commercial terms. `--noncommercial` also reads non-commercial
sets you added yourself (see *Your own non-commercial data* above) for a model of your own: it stays on your machine (step 5). Naming
a non-commercial set in `--sets` without `--noncommercial` is refused.

Training reads each set's scan and the review's decisions (`ac_train/review.py`): rejected takes
are left out and no training window touches a cut. A decision made on a different length of take
(the set was rebuilt with another trim) is set aside and printed; the scan applies. A set with no
scan, or one rebuilt since, stops training with the command that fixes it (`--unscanned` trains on
every frame instead, and says so). `model.json` records what each set gave (`data`). The library
(step 6) reads them the same way.

Validation holds out 5 % of the takes (the same ones every run). Every 500 steps it
prints the mean joint error of the network against plain interpolation on gaps of 5, 15 and 30
frames. Writes `data/models/<tag>/model.pt` and `model.json` (sets, licences, metrics).

### 5. Export to the app

```bash
uv run python -m ac_train.export parts
```

Writes `assets/models/parts/weights.{bin,json}` and the goldens
`crates/ac-ai/tests/golden/parts-{net,fill}.json`, then check the Rust port against them:

```bash
cd .. && cargo nextest run -p ac-ai
```

If you export under another tag, add it to `assets/models/MANIFEST.json` (with `commercial` set
from the card) and a row to `ATTRIBUTION.md`.

A model trained with `--noncommercial` is never written into the repo (export refuses). Export it
into the app's own models folder instead; the app then uses it in place of the bundled one, says
so in the status line, and shows the card's non-commercial warning:

```bash
uv run python -m ac_train.export parts-own --out "$APPDATA/HumanMotionCraft/models/parts" --golden data/golden-own
```

Delete that folder to go back to the bundled model. Scripts, screenshots and agent sessions
always use the bundled one.

### 6. Library clips

The committed clips come from CMU, 100STYLE and the generated `kimodo` set (85 clips chosen from
96 cut, by contact sheet). Clips from a non-commercial set you added are baked into the gitignored
`training/data/library/` instead and installed into your own app library (`myset` is your set):

```bash
uv run python -m ac_train.library --sets myset --out data/library
cd .. && cargo run --release -p ac-cli -- --library "$APPDATA/HumanMotionCraft/library" run library.install '{"from": "training/data/library"}'
```

The built-in locomotion and style walks come from `ac_train.style_clips`: 100STYLE's *Neutral* and
*Crouched* takes give the plain walks, side steps, stand and crouch walks travel uses, CMU's two
straight runs its runs, and every other 100STYLE style one forwards walk in `Walk styles/`. Each
clip is 3 seconds that walk on (little turning or drift, never standing still) and faces the front
on its first frame; a style without such a stretch gets no clip and is named:

```bash
uv run python -m ac_train.style_clips --out ../assets/library
```

To add clips from a set to the built-in library by type (only free-licence sets; the others are
refused):

```bash
uv run python -m ac_train.library --sets cmu,100style --out ../assets/library
```

Numbering carries on after the clips already there, in `assets/library` and in the local
`training/data/library` ("Walk 5", "Walk 6", ...), so a bake never renames a clip that a fixture or
a saved animation names, and never reuses a name one of your local clips has. Each type gets at
most one clip a take (and one a 100STYLE style). Each clip records its `dataset`.
