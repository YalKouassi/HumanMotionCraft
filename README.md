# HumanMotionCraft

HumanMotionCraft is a standalone, pure-Rust humanoid keyframe animation application with a native `wgpu` + `egui` UI. It combines a 104-bone humanoid rig, two-bone and full-body IK, timeline layers and motion-matching travel, a physics-assisted solver, and a pure-Rust learned in-betweener—all built on a deterministic, command-first core that can be driven from the desktop UI, headless CLI (`ac`), or an MCP agent (`ac mcp`).


---

## What HumanMotionCraft Is, Will Be, and Is NOT

### What HumanMotionCraft Is
* **Single-purpose humanoid animator:** Built around a calibrated 104-bone humanoid rig and mannequin (`Z`-up, metres and radians internally, `Y`-up glTF on export).
* **Two-bone & full-body IK with pins:** Drag limbs with analytical two-bone IK (`pose.drag`) or solve the whole body with damped least squares and empirical per-bone rotation limits (`pose.reach`), holding pinned hands and flat feet in place.
* **Layers, curves & motion-matching travel:** Keyframe with 6 interpolation modes, stack override/additive layers, or generate foot-locked locomotion along paths (`layer.travel` / `layer.plan`).
* **Physics-assisted solver (`ac_core::physics`):** Enforces floor and box support (`surface.*`), detects foot contacts, pins fulcrums to eliminate foot skating, balances static poses using a de Leva center-of-mass model, and computes ballistic jump arcs.
* **Pure-Rust learned in-betweening (`ac-ai`):** Runs a trained transformer in-betweener directly in plain Rust (`f32` forward pass—no Python, PyTorch, or ONNX runtime) with style guidance, `ai_force` blending, automatic re-filling on adjacent key edits, and foot planting.
* **Command-first & agent-ready:** Every operation is a typed, undoable command in `ac-core` (`Session::run`). The UI, keyboard shortcuts, JSON fixtures, headless CLI, and Model Context Protocol (MCP) server all execute the exact same commands.
* **Game-ready `.glb` export:** Export rigged `.glb` animations (`clip.export`) using the `full` (104-bone), `game` (68-bone), or `body` (36-bone) profiles, or a custom bone list.

### What HumanMotionCraft Will Be
* **Multi-humanoid animation:** Animating multiple interacting humanoids in a shared scene (combat exchanges, partner movement, dialogue, and inter-character contact constraints).
* **Long-horizon AI fill:** Pose-database retrieval anchors for filling spans longer than 45 frames.
* **Expanded curve, dope-sheet & retargeting workflows:** Richer graph editing and motion import/retargeting tools.

### What HumanMotionCraft Is NOT
* **Not a quadruped, creature, or arbitrary custom-rig animator:** HumanMotionCraft focuses on doing **one thing well: humanoids**. Committing to a single calibrated humanoid rig means joint limits, center-of-mass tables, foot fulcrums, locomotion libraries, and learned in-betweeners work together out of the box with zero rigging setup. *(If quadrupeds, creatures, or arbitrary rigs are tackled in the future, the rig-agnostic curve, layer, command, and solver foundations in `ac-core` will be split into a shared core repository rather than turning HumanMotionCraft into a general-purpose rigging tool.)*
* **Not a character creator, sculpting, or weight-painting tool:** There are no body or face morph sliders, hair, or cloth simulations. You author motion on the built-in mannequin and export `.glb` clips to drive your own characters in your game engine or DCC.
* **Not a full 3D DCC, scene layout, or rendering suite:** No material graphs, lighting rigs, or camera sequencers—only the viewport, mannequin, onion-skin ghosts, and simple contact boxes needed to judge weight, arcs, and timing.
* **Not a Python or C++ ML runtime wrapper:** Training lives offline in Python, while the shipping application and CLI are 100% safe Rust (`unsafe_code = "forbid"`, deterministic `f64` math in core); the optional motion-from-text install is a separate Python helper process the app starts only when asked.
* **No non-commercial strings attached:** The Rust code is GPL-3.0-or-later (what you make with it is yours), the rig, mannequin and static poses are Apache-2.0 / CC0, the library clips (40 from CMU, 104 from 100STYLE, 85 we generated) are free for any use, and the in-betweener weights (`assets/models/parts`) are trained on those free-licence sets only (see [Licence & Attribution](#licence--attribution)).

---

## Quickstart

### Prerequisites
* Rust toolchain supporting `edition = "2024"` (`rust-version = "1.95"` in [`Cargo.toml`](Cargo.toml)).
* A GPU/driver supporting `wgpu` (Vulkan, Metal, or DX12; Windows WARP software adapter is also supported for headless rendering and visual tests).

### Run the Desktop App (`humanmotioncraft`)
```bash
cargo run --release -p ac-app
```

Optional startup flags:
```text
humanmotioncraft [--assets <dir>] [--library <dir>] [--open <id>] [--frame <n>]
               [--script <json>] [--screenshot <png>] [--control <port>]
               [--prefs <dir>|memory] [--ui <name>=<value> ...]
```

### Run the Headless CLI (`ac`)
```bash
# List every registered command, its typed inputs, and whether it is undoable
cargo run --release -p ac-cli -- commands

# Run a single command on a scratch session
cargo run --release -p ac-cli -- run pose.reset '{}'

# Run all hand-worked feature fixtures
cargo run --release -p ac-cli -- fixtures

# Render a frame or multi-frame contact strip to PNG
cargo run --release -p ac-cli -- render --script fixtures/ai-fill.json --frames 0,8,15 --strip --out target/strip.png
```

### Model Context Protocol (`ac mcp`)
`ac mcp` exposes the command registry, viewport preview renderer, and contact-sheet generator over stdio:
```bash
# Headless MCP session
cargo run --release -p ac-cli -- mcp

# Live MCP session attached to a running desktop window
# 1. Start the app with a localhost control port:
cargo run --release -p ac-app -- --control 7811
# 2. Point the MCP server at that port (shares the window's session & undo stack, adds ui_screenshot):
cargo run --release -p ac-cli -- mcp --live 7811
```

### Motion from text (optional)

The **Motion from text** panel (and the `text.generate` / `text.fill` commands) makes animation from
a prompt such as "A person waves hello with their right hand.": a new stretch from the playhead, or
the gap between two of your keys, held to both. It needs a one-time install, best with an NVIDIA
GPU (12 GB is enough), 32 GB of RAM, and about 35 GB of free disk (the install is 22 GB). Press **Install** in
the panel: it lists what the install needs first (on Windows, the Visual Studio C++ build tools,
the same ones Rust uses; nothing else need be installed and no account is needed: without `uv`
the install downloads it, and uv brings its own Python), then downloads and sets everything up in the
background with its progress, Stop and the log, and the window stays usable meanwhile (an hour or
more). Stop, or quitting the app, ends it; Install carries on from where it stopped. The same
setup runs from a terminal too:

```bash
python tools/textmotion/setup_textmotion.py
```

This installs NVIDIA's Kimodo model and its text encoder, which is built on Meta Llama 3 (see
`NOTICE` for their licences; installing means you accept them), into your local data folder.
Without it the app works as before. Making motion takes about a minute the first time, while
the model loads, and seconds after that; the window stays usable while it works.

---

## Workspace Layout

| Path | Description |
|---|---|
| `crates/ac-core` | 104-bone rig, quaternions, FK, two-bone & full-body IK, joint limits, physics solver, clip/layer/curve model, command registry, and `Session` (undo/redo). Pure `f64` math; no UI or GPU dependencies. |
| `crates/ac-ai` | Plain-Rust transformer in-betweener (`f32` inference) and the `ai.fill` pipeline. Depends only on `ac-core`. |
| `crates/ac-io` | Library management, `.glb` animation export (`full` / `game` / `body` profiles), clip/pose serialization, and atomic file writes. |
| `crates/ac-render` | `wgpu` renderer: GPU-skinned mannequin, joint/bone overlays, floor grid, scene boxes, onion-skin ghosts, and offscreen PNG readback. |
| `crates/ac-cli` | Headless `ac` binary: command runner, script execution, fixture verification, image/video rendering, and stdio MCP server. |
| `crates/ac-textmotion` | Motion from text (`text.install`, `text.generate`, `text.fill`, `text.helper`) through the optional helper process. Depends only on `ac-core`. |
| `crates/ac-media` | Video and pictures of the animation: `clip.video` (MP4 through ffmpeg, or GIF) and `clip.sheet` (contact sheets). |
| `crates/ac-review` | The training-data review app (developers only, never shipped). |
| `xtask/` | Repo tasks: `cargo xtask check`, the whole pre-commit gate. |
| `crates/ac-app` | Desktop `humanmotioncraft` binary (`eframe` + `wgpu` backend) and localhost live-control bridge. |
| `assets/` | Baked 104-bone rig (`default_rig.json`), joint limits (`limits.json`), neutral mannequin (`mannequin.glb`), pose/animation library, and in-betweener weights (`models/MANIFEST.json`). |
| `fixtures/` | Deterministic JSON command scripts with hand-worked expectations covering every registered command. |
| `training/` | Python (offline only): retargets motion-capture datasets onto the rig, trains and exports the in-betweener, and cuts the built-in mocap clips. Datasets live in the gitignored `training/data/`. See [How to retrain](training/README.md#how-to-retrain). |
| `tools/textmotion/` | Python (optional): installs the motion-from-text model and the helper the app talks to, and generates the `kimodo` training set. Never needed to run the app. |

---

## Testing & Verification

HumanMotionCraft enforces `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` and `unsafe_code = "forbid"` across production crates, and runs every command under `catch_unwind` so a bad input never crashes the session.

```bash
# The whole gate CI runs: fmt, clippy, tests (nextest, else cargo test), fixtures, name/asset/doc guards
cargo xtask check

# Or its parts: format & lint checks
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# Full test suite (unit tests, golden parity, command extremes, fixtures, and visual goldens)
cargo test --workspace
```

See [`docs/testing.md`](docs/testing.md) and [`docs/llm-guidelines.md`](docs/llm-guidelines.md) for details on the fixture format and testing layers.

---

## Licence & Attribution

* **Source Code:** Licensed under the [GNU General Public License v3.0 or later](LICENSE) (`GPL-3.0-or-later`). The licence covers the program, not its output: animations, poses and exports you make are yours.
* **Rig, Mannequin & Static Poses:** Derived from [Anny](https://github.com/naver/anny) (Apache-2.0) and the MakeHuman community MPFB2 mesh and pose packs (CC0 1.0). See [`NOTICE`](NOTICE) and [`ATTRIBUTION.md`](ATTRIBUTION.md).
* **Bundled Mocap Clips:** Each clip in `assets/library/animations/` records its source in `"dataset"`.
  * **CMU** (`"dataset": "cmu"`, 28 clips): the [CMU Graphics Lab Motion Capture Database](http://mocap.cs.cmu.edu), free for research and commercial use. *The data used in this project was obtained from mocap.cs.cmu.edu. The database was created with funding from NSF EIA-0196217.*
  * **100STYLE** (`"dataset": "100style"`, 104 clips: a walk for each of 95 styles, and the plain locomotion travel uses): Ian Mason, Sebastian Starke and Taku Komura's [100STYLE](https://www.ianxmason.com/100style), **CC BY 4.0**: any use, with credit (in [`NOTICE`](NOTICE)).
  * **Generated** (`"dataset": "kimodo"`, 85 clips): made with NVIDIA's [Kimodo](https://github.com/nv-tlabs/kimodo), whose model licence claims no ownership of what it generates.
* **Learned In-Betweener Weights:** `assets/models/parts/` is trained by [`training/`](training/README.md) on the free-licence sets only: the CC0 loops, CMU, 100STYLE and our Kimodo-generated motion. Any use, like the clips. The repo supplies only these free-licence sets. You can add a non-commercial set of your own and train a model for yourself (`--noncommercial`, see [How to retrain](training/README.md#how-to-retrain)); that model and its clips stay on your machine.
* See [`ATTRIBUTION.md`](ATTRIBUTION.md), [`NOTICE`](NOTICE) and [`assets/models/MANIFEST.json`](assets/models/MANIFEST.json) for full provenance.

### Licence FAQ

**Is HumanMotionCraft free?**
Yes. Free to download, use, study and change, for anyone.

**Can I sell the animations I make with it?**
Yes, whether you made them with keyframes, IK, physics, travel, the AI fill, the built-in poses or
the library clips.
They're yours. Put them in a commercial game, film, ad or YouTube video, or sell them as an asset
pack. You don't owe us money and you don't have to credit us (we'd love a credit, though).

**Do I need to credit anyone?**
Only if you ship the mannequin itself. An exported `.glb` carries the built-in mannequin mesh, which
comes from Anny (Apache-2.0). If you ship that mesh in your game or asset pack, include Anny's
notice (the Anny lines in [`NOTICE`](NOTICE)). If you only use the motion on your own characters,
there's nothing to credit.

**What about the AI fill?**
The bundled in-betweener (`assets/models/parts/`) learned only from data that allows any use, so
the frames it fills are yours like the rest. If you add a non-commercial set and train your own model with `--noncommercial`
(see [Retraining the In-Betweener](#retraining-the-in-betweener)), the app warns you, and frames
that model fills are non-commercial.

**Do my exported `.glb` files or project files have to be GPL?**
No. The GPL covers the program's code, not the files you make with it. Your game, your characters
and your exports can use any licence you like, including closed and commercial.

**Does my game have to be open source because I used this app?**
No. You used a tool. Your game doesn't contain HumanMotionCraft's code, so the GPL doesn't reach it.

**Can my studio use it at work?**
Yes, on any number of machines. You can also change it for in-house use without publishing your
changes, as long as you don't give the changed app to anyone outside the studio.

**Can I fork it?**
Yes. If you share your fork, it has to stay GPL: you publish its source, keep the copyright and
licence notices, and give everyone the same freedoms.

**Can someone sell copies of HumanMotionCraft?**
The GPL allows it, but the seller has to give buyers the full source, and those buyers may then
give it away for free. Nobody can turn it into a closed, paid-only app.

**Can I copy its code into my own closed-source program?**
No. Code taken from HumanMotionCraft brings the GPL with it, so the program that uses it has to be
GPL too.

**I'm still not sure.**
Read [`LICENSE`](LICENSE), [`NOTICE`](NOTICE) and [`ATTRIBUTION.md`](ATTRIBUTION.md). This
section is a plain-English summary, not legal advice: if the two disagree, the licence files win.

## Retraining the In-Betweener

The model and the mocap clips are reproducible. The retargeted free-licence sets (CMU, 100STYLE, the Kimodo-generated set, the CC0 loops) download ready-made from this repo's GitHub release with `python -m ac_train.fetch` (about 400 MB), or you can build them from the public datasets. [`training/README.md`](training/README.md#how-to-retrain) covers both, plus training (an NVIDIA GPU, about 3 hours), exporting to `assets/models/` with the `ac-ai` goldens, and cutting library clips.

```bash
cd training && uv sync
uv run python -m ac_train.fetch                         # the retargeted sets and their scans (or: ac_train.datasets, then ac_train.quality)
uv run python -m ac_train.train --minutes 180 --tag parts
uv run python -m ac_train.export parts                  # weights + goldens; then cargo nextest run -p ac-ai
```
