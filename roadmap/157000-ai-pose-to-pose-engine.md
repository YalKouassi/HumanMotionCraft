# Pose-to-pose motion from MotionBricks

- **Status:** in progress: Python helper built, Rust crate next (2026-10-30)
- **Requires:** 113000, 114000
- **Source:** owner request (2026-10-30), after the licence check. This replaces the decision of
  2026-10-28 to use only Kimodo as a third-party generator.

## What
MotionBricks (NVlabs) makes motion from poses, not text: it takes 4 recent poses and 1 to 4 target
poses and makes 18 to 58 frames at 30 fps. Add it as a second optional AI engine beside motion from
text, so people can fill between keys, save the result to their library, and we can generate
training clips. It doesn't replace the text engine, and `ai.fill` keeps using our own model.

### Licences (read 2026-10-30, through a page summariser)

| Part | Licence | Rule for us |
|---|---|---|
| Code | Apache 2.0 | use and port it |
| SOMA model | NVIDIA Open Model Agreement | use it, downloaded at install |
| Sample clips (`assets/motions/`) | NVIDIA Sample Data Evaluation License | never ship, train on, or use as poses for anything we keep |
| BONES-SEED | Bones licence | never accept it or use it |
| TensorRT packages | NVIDIA TensorRT Software License Agreement | not read in full; the install fetches them and the repo and release never carry them |
| Warp | Apache 2.0 | use it, fetched at install |

- **Sample clips:** the upstream demos pick their poses from these clips, and anything derived from
  them is held to the same evaluation-only limit. Poses must come only from the person's own keys,
  or for training from the sets in the `REGISTRY`. The install must leave `assets/motions/` out,
  including when it comes as part of GitHub's archive.
- **Model:** the agreement makes whoever uses or distributes the model cover NVIDIA against
  third-party claims, so the app downloads it at install and the repo and release never carry it.
- **Credit:** `ATTRIBUTION.md` and `NOTICE` get the line "Licensed by NVIDIA Corporation under the
  NVIDIA Open Model Agreement".
- **Training on outputs:** the agreement doesn't mention it, so nothing forbids it. It has the same
  catch as the `kimodo` set: the model learned from Bones' RigPlay data, and this distils data that
  Bones keeps from generative training.
- **First step of the build:** read the agreement, the sample licence and the TensorRT licence that
  ships with the package line by line, and open the Hugging Face card (`nvidia/MotionBricks`), which
  the check skipped. Stop and ask the owner if the wording differs from the table or TensorRT's terms
  stop the install from fetching it.



see D:/CreativeFusion/AnimationCraft/reference repos

### The engine
- **Names:** other products' names stay out of code, as with `tools/textmotion`: `tools/posefill/`,
  crate `ac-posefill`, commands `posefill.install`, `posefill.helper`, `posefill.fill`. Only the
  licence records, the install's download source and this file name the product.
- **Helper:** an optional Python process, installed and started the way `text.install` and
  `text.helper` do it, with an `Engine` seam and a fake in the tests. Code shared with
  `ac-textmotion` moves first, in its own behaviour-preserving commit. `AGENTS.md`'s exception to
  "Rust only at runtime" gains this helper.
- **Runtime:** TensorRT through upstream's Apache-licensed engine, so this needs an NVIDIA GPU (owner,
  2026-10-30). That spares us porting the frame trimming and endpoint correction, and gives live
  control (158000) the speed it needs from one runtime. There is no CPU or other-GPU fallback:
  `posefill.install` checks for the GPU and refuses with the reason. The first launch builds a
  TensorRT plan, which takes a few minutes, so the panel says so.
- **Rig:** reuse the conversion between our rig and SOMA in `tools/textmotion/engine.py`.

### The fill
- `posefill.fill` fills between two keys and writes AI keys with their own model tag. Edits next to
  them must leave them alone, as they do `text.fill`'s keys, but `ac_core::edits::GENERATED` only
  recognises the tag `"text"` (`crates/ac-core/src/edits.rs:21`), so that check has to learn the new
  tag. It is one undo step. Saving uses `clip.save` and `clip.save_as`, with nothing new.
- **Context:** the 4 recent poses are the frames before key A. With fewer than 4 frames before it,
  the first pose is repeated and the result says so.
- **Gap length:** a gap outside 18 to 58 model frames is refused with the limits and the reason. A
  longer gap chains calls through the keys inside it, and with no key inside to aim for it is refused
  and asks for one. Nothing is invented to bridge it, and no gait or preset stands in.
- **Frame rate:** the clip's rate converts to 30 fps and back, as `fits` does in
  `crates/ac-textmotion/src/lib.rs:295`.
- **UI:** its own panel or a section beside Motion from text, with Install, the licences installing
  accepts, and Fill. The window fills on a thread and runs as before without the install.

### Training clips, last
Build this once the owner has judged the fill by eye. A generated set in the `REGISTRY`, like
`kimodo`, with its poses taken from the free sets and only owner-approved clips kept. A row in
`training/README.md` records how it is made. Retraining is a separate request.

### Not known yet
- Quality on our rig: the model card warns that unusual target poses lower it. Found 2026-10-30:
  walk, wave and stand fills between library poses come out as walks, raises and settles (the
  strips under `target/shots/2026-10-30/posefill/`); the owner judges from the Rust side's MP4s.
- Speed on lower-end cards: published timings are for an RTX 4090 (2.5 ms for the plan alone).
  Found 2026-10-30: 3.5 ms for the plan and 4.5 ms for a whole generation on an RTX 3080 Ti.
- Whether the model depends on anything in `assets/motions/` at load, such as a default pose. If it
  does, stop and ask the owner. Found 2026-10-30: the ONNX models are in the repo at
  `motionbricks/assets/onnx/` with their own `LICENSE.md`, and the upstream Smart Primitive documents
  point at the sample clips, so we don't use those either (158000).

Packaging (124000) must ship `tools/posefill` and the rig modules, as it must for `tools/textmotion`.

## Acceptance
- The line-by-line licence reading is written into this file, with any difference raised with the
  owner.
- Without the install, the panel and every `posefill.*` command say how to install it, and the app
  otherwise behaves as before. `ai.fill` and its goldens don't change.
- On a machine without it, Install runs with step progress and Stop, and resumes after Stop. The
  install folder has no `assets/motions/` afterwards, and no sample clip is ever read.
- `fixtures/pose-fill.json` and `crates/ac-posefill/tests` pass on the fake engine: a fill, one undo
  step, the tag, each refused gap with its reason, the short-context note, and non-finite or wrong-shape
  helper output refused.
- With the install, contact sheets show fills of about 1 s and 2 s between our keys, and a chained
  fill through a middle key, each meeting its keys on our rig. Save a fill to the library and reload
  it.
- On a machine without an NVIDIA GPU, `posefill.install` refuses and says why.
- Seconds per call on the GPU, with the card named, are recorded under `## Result`.
- `ATTRIBUTION.md` and `NOTICE` carry the credit, and `AGENTS.md` and `docs/commands.md` are updated.
  `cargo xtask check` passes.
- The owner judges the motion by eye and approves the panel from screenshots, and only then does
  the training set get built. The owner approves its clips.

## Licences read line by line (2026-10-30)

Read in the local copies under `reference repos/` (upstream at commit 54fc563, v0.2.0, 2026-10-23)
and on the public pages; the full report with every quote is
`target/shots/2026-10-30/posefill-licences.md` (gitignored). **The table above holds.** The
differences are details (listed after the table); nothing in TensorRT's terms stops the installer
from fetching it. Then, during the build, the copy of the TensorRT agreement inside the installed
wheels was opened (the step the report could not do without a download): see item 11.

| Part | Where it was read | Licence | Clauses that matter | Matches the table? |
|---|---|---|---|---|
| Code (`motionbricks/`, `app/`, `docs/examples/`, `tools/onnx/`) | `LICENSE` (header + Apache text), `pyproject.toml` (`license = "Apache-2.0 AND CC-BY-4.0"`) | Apache-2.0 | §4: keep notices, state changes. README: "Code: Apache 2.0, including code examples and snippets in the documentation." | Yes |
| NVIDIA-authored docs text and media (`docs/*.md`, `docs/media/`) | `LICENSE` lines 276-285 | CC-BY-4.0 | "does not change the separate terms for models, sample motion data, rig assets, or third-party materials" | New row; matters only if we quote docs or images (credit NVIDIA) |
| SOMA ONNX model (`mmm_v6_soma_fp32.onnx` 757,864,273 B, `_fp16` 380 MB, `soma_n1.json`) | `motionbricks/assets/onnx/LICENSE.md`; the agreement (v. March 9, 2026, released April 2, 2026) | NVIDIA Open Model Agreement | preamble: "Works are commercially usable. You are free to create and distribute Derivative Works. NVIDIA does not claim ownership to any outputs"; §2 perpetual, royalty-free, sublicensable licence to reproduce, derive, distribute; §3(c) the credit line "Licensed by NVIDIA Corporation under the NVIDIA Open Model Agreement" when a NOTICE file is shipped; §7 "You will indemnify and hold harmless NVIDIA from and against any claim by any third party arising out of or related to your use or distribution of the Works, Derivative Works thereof, or output"; no clause on training on outputs, no revocation clause | Yes. The agreement would allow redistributing the model; "never carry it" is the owner's choice because of §7, not a licence rule |
| SOMA/G1 checkpoints on Hugging Face (not needed: the ONNX is bundled) | `nvidia/MotionBricks` card and API (`gated: false`, no extra prompt) | NVIDIA Open Model Agreement | "The model agreement does not replace the software, sample-data or third-party asset terms. This repository does not include the sample motions or the training dataset." Training data: "Bones RigPlay 1 (RP1)" | Yes; the card adds no terms |
| Sample clips (`assets/motions/`: 38 SOMA BVH + 37 G1 CSV) | `assets/motions/LICENSE.txt` (v. Jan. 16, 2026), `README.md` | NVIDIA Sample Data Evaluation License | §1 "internally ... solely to evaluate and test NVIDIA technologies"; §2(c) no distributing the dataset or derivatives; §6 destroy on termination; README: derived from RP1, the notice "does not apply the sample license to model weights, ONNX models, robot assets, rig metadata, Smart Primitive documents, or scene files" | Yes, word for word: never committed, never installed, never read |
| Smart Primitive documents and scenes (`assets/smart_primitives/`, `assets/scenes/`) | the files; `docs/smart_primitive_editor.md` | none of their own | every target pose is a clip path plus a time into `assets/motions/`; "A Target Pose saves its clip and sample time, not the sampled poses" | Yes (158000): nothing usable without the evaluation-only clips anyway |
| BONES-SEED (RP1's public sibling) | bones.studio/info/seed-license; HF `bones-studio/seed` (`gated: auto`) | BONES-SEED Dataset License Agreement (AIdvisory sp. z o.o.) | academic users and qualifying startups only; §6 no "generative model whose primary purpose or substantial use is to generate ... human motion sequences"; no mirrors | Yes: never accept it; the clips and the model reach us under NVIDIA's terms only |
| TensorRT runtime packages (`tensorrt-cu12`, `-bindings`, `-libs` == 10.8.0.43) | PyPI JSON (`license: "Proprietary"`); docs.nvidia.com's SLA page; the README line 256; and now the wheels' own `LICENSE.txt` | NVIDIA software licence agreement + TensorRT supplement | §1.1 "Install and use"; no copying, distributing or sublicensing; "cannot use the SDK in any manner that would cause it to become subject to an open source software license"; supplement: "licensed ... only for use in systems with NVIDIA GPUs" | Yes in substance; details 1, 2 and 11 below |
| `licenses/tensorrt-LICENSE.txt` in the repo | that file | Apache-2.0 | README: applies to TensorRT OSS, "not the separately installed tensorrt-cu12 ... runtime packages" | Not the SLA the first step pointed at (difference 1) |
| CUDA runtime (`nvidia-cuda-runtime-cu12`, pulled by `tensorrt-cu12-libs`) | PyPI JSON; CUDA EULA; the wheel's `licenses/License.txt` | NVIDIA CUDA Toolkit EULA | install and use; `cudart.dll` is even distributable | New row; fetched at install, never in the repo or release |
| Warp (`warp-lang` 1.17.0) | `licenses/warp-LICENSE.txt` | Apache-2.0 | standard | Yes |
| Newton (`newton` 1.6.0) | `licenses/newton-LICENSE.txt` | Apache-2.0 | viewer and G1 only; the helper never imports it | New row; not installed |
| OpenUSD (`usd-core` 26.3, reads the SOMA skeleton USD) | `licenses/usd-core-LICENSE.txt` | Tomorrow Open Source Technology License 1.0 (Apache 2.0 with §6 Trademarks changed) | permissive | New row; fetched at install |
| SOMA rig asset (`motionbricks/assets/human/soma_base_skel_minimal_no_root.usd`, 2,036,435 B, Git LFS) | the file (no licence string); `LICENSE` ("rig assets" have "separate terms" that are never described); `pyproject.toml` package data under the Apache half; SOMA-X is Apache-2.0 | not written down; best reading Apache-2.0 | SOMA-X README: SMPL-Body was used to make the topology correspondence (SMPL itself is never touched) | New row: fetched at install, never committed; ask the owner only if a plan ever wants to ship it |
| Unitree G1 model and meshes | `licenses/unitree-LICENSE.txt` | BSD-3-Clause | | New row; its files are LFS pointers in the archive and are dropped |
| Demo-only deps (pyglet, imgui-bundle with the Roboto font, trimesh, Daniel Holden code, numpy, scipy, Pillow) | `licenses/*.txt` | permissive | | New row; only numpy and scipy are installed |
| ONNX exporter deps (`tools/onnx`) | `tools/onnx/licenses/*.txt` | permissive | | Not needed: the ONNX files are bundled |
| `motionbricks-blender` (local copy) | `LICENSE` | add-on GPL-3.0-or-later; `walker/` and `recorder/` Apache-2.0 | | Never opened for this build (the GPL rule); not in the table |
| `motionbricks-studio` (local copy) | `LICENSE`, README | MIT | "Every pose is authored here from the SOMA T-pose ... the sample motions ... are never used" | Not in the table; the same rule we adopted |

Differences from the table (none changes a use / don't-use decision):

1. The TensorRT licence "that ships with the package" in the repo (`licenses/tensorrt-LICENSE.txt`)
   is the Apache notice for TensorRT OSS; the runtime packages are governed by the agreement inside
   the wheels and published at docs.nvidia.com. The table's name is close enough; the pointer was
   what to fix (see 11 for what the wheel holds).
2. TensorRT pulls `nvidia-cuda-runtime-cu12` (CUDA EULA, proprietary): same rule, fetched at install.
3. The credit line is conditional: OMA §3(c) requires it only when a NOTICE file is shipped; the
   ONNX folder ships `LICENSE.md` with that sentence and no NOTICE. Adding it to `ATTRIBUTION.md`
   and `NOTICE` as planned is still right, just not strictly compelled.
4. The agreement allows redistributing the model: "the repo and release never carry it" is the
   owner's policy because of §7's indemnity (and 1.1 GB of LFS), not a licence rule. Stated here so
   nobody later "fixes" it.
5. The SOMA rig USD has no explicit licence (above): fetch at install, do not commit.
6. Smart Primitive documents and scene files hold only clip paths and sample times.
7. New row: documentation and media are CC-BY-4.0.
8. The HF card is not gated and adds no terms.
9. BONES-SEED's licence is the "BONES-SEED Dataset License Agreement" with AIdvisory sp. z o.o.,
   gated on HF, not simply a "Bones licence"; the rule stands.
10. Open point for the owner, not a licence difference: HMC is GPL-3.0 and TensorRT's agreement says
    the software may not be used "in any manner that would cause the Licensed Software to become
    subject to an Open Source License". We never distribute TensorRT; the user installs it into a
    private venv from NVIDIA's index, as `text.install` fetches proprietary CUDA PyTorch wheels, so
    nothing we convey includes it. Keeping `tools/posefill`'s Python under Apache-2.0 (it ports
    Apache-2.0 upstream code) removes even the theoretical argument.
11. **Found at install (2026-10-30):** the text inside the wheels
    (`.venv/Lib/site-packages/tensorrt_cu12_bindings-10.8.0.43.dist-info/LICENSE.txt`, the same
    47,141 bytes in `tensorrt_cu12_libs` and `tensorrt_cu12/licenses/`) is **not** the "LICENSE
    AGREEMENT FOR NVIDIA SOFTWARE DEVELOPMENT KITS" (v. May 24, 2021) that docs.nvidia.com's SLA
    page shows and the report read. It is the click-through "NVIDIA SOFTWARE LICENSE AGREEMENT"
    (© 2021) with "12. TensorRT SUPPLEMENT TO SOFTWARE LICENSE AGREEMENT". Read line by line; the
    clauses that matter: §1.1 grants "a non-exclusive, non-transferable license ... to have
    Authorized Users install and use the Software"; §2.1(iii) no making it "available ... to third
    parties ... as an application services provider or service bureau ... or (d) otherwise by means
    of the internet"; §2.1(vii) no distributing it "as a stand-alone product"; §2.1(xii) no use "in
    any manner that would cause the Licensed Software to become subject to an Open Source License"
    (the same as the other text's §2.5, item 10 above); §4 modifications of the software are
    NVIDIA's (the plan cache is not a modification of TensorRT but an output, and it stays local);
    §9 consent that "the Software may access, collect non-personally identifiable information about
    your Enterprise computer systems"; §12.1 only `libnvinfer` and `libnvinfer_plugin` may be
    redistributed, inside an application with "additional material functionality"; §12.2 "Each
    TensorRT Licensed Software is licensed to you for an initial duration of one year starting from
    the date of delivery or download. The licenses granted will automatically renew for successive
    one year periods, provided that NVIDIA reserves the right to terminate licenses upon ninety
    days (90) days written notice". The §1.2 limited-purpose licences (evaluation, academic,
    test/development) apply only when a licence "is provided for one of the purposes indicated",
    which a public pypi download is not. **Conclusion: same as before.** A person installing it on
    their own machine at their own request, from NVIDIA's index, is inside §1.1 and outside every
    restriction; the repo and release carry nothing of it; the install names the agreement and
    links it, and `licences.txt` in the install folder points at the wheel's own text as the one
    that governs. The careful call (the owner was not asked, per the brief): the wording differs
    in title and version from the table's "NVIDIA TensorRT Software License Agreement", not in what
    it allows, so this is recorded here rather than raised as a stop. The one-year auto-renewing
    term (§12.2) is new and worth the owner's eye: NVIDIA could end a user's TensorRT licence with
    90 days' notice, which would end their use of this engine (not of the app).

What the install downloads, from where, and what never goes in the repo or release:

| Download | Source | Size | Repo / release may carry it? |
|---|---|---|---|
| the package source (`motionbricks/`, `licenses/`, `LICENSE`, `README.md`, `pyproject.toml`) | GitHub's archive of `NVlabs/motionbricks` at the pinned commit, checked by a tree hash; `assets/`, `app/`, `docs/`, `tests/`, `tools/` and every LFS pointer file are never written | 80 files, small | in principle yes (Apache-2.0); fetched instead |
| `mmm_v6_soma_fp32.onnx` + `soma_base_skel_minimal_no_root.usd` | `media.githubusercontent.com/media/NVlabs/motionbricks/<commit>/...` (the archive holds only 3-line LFS pointers), sha256 pinned (`856419dc...`, `4ab93d78...`) | 758 MB + 2 MB | no (owner's rule; licence not written down for the USD) |
| `numpy==2.4.3`, `scipy==1.17.1`, `warp-lang==1.17.0`, `usd-core==26.3` | pypi.org | about 600 MB | no (fetched) |
| `tensorrt-cu12`, `-bindings`, `-libs` == 10.8.0.43 (+ `nvidia-cuda-runtime-cu12` 12.9.79) | `--extra-index-url https://pypi.nvidia.com` (pypi.org holds only stubs that would run a nested pip) | about 1.3 GB | never |
| the TensorRT plan (`trt-cache/*.plan`, 765 MB) and Warp's kernels (`warp-cache/`) | built locally on first use | 766 MB | never (an OMA derivative; GPU-specific) |
| `assets/motions/**`, `assets/smart_primitives/**`, `assets/scenes/**` | none | none | never downloaded, never read; the setup checks they are absent |

Do the ONNX models depend on anything in `assets/motions/` at load? **No** (checked in the library
code): `get_assets_dir()` resolves to the package's own `motionbricks/assets/`; the manifest holds
only joint names, token limits, sample rate and tensor specs; a search for `assets/motions` or
`.bvh` over `motionbricks/` finds nothing; the reference pose comes from the USD skeleton. Only demo
and example code under `app/` and `docs/examples/` reads the clips, and none of it is installed.

## Python half built (2026-10-30)

Built and run on this machine (Windows 11, NVIDIA GeForce RTX 3080 Ti 12 GB, driver 617.42).
Files: `tools/posefill/setup_posefill.py`, `engine.py`, `helper.py`, `tests/fake_engine.py`,
`tests/test_helper.py`. Review material under `target/shots/2026-10-30/posefill/` (gitignored):
the install logs, `summary.json` (every call's timings), `run_fills.py` (how the fills were made),
`strip.py` and the `*.clip.strip.png` stick-figure strips, `strip-report.json`, and the clips
`walk-1s`, `walk-1s-4targets`, `walk-1.9s`, `walk-chain-2s`, `idle-to-wave-1s`,
`wave-to-idle-0.8s` as version-1 `.clip.json` (format `humanmotioncraft.clip`, the context poses as
manual keys on frames 0-3, key A on 3, the model's frames tagged `source: "ai", model:
"posefill"`, key B manual) for the Rust side to load and make MP4s from.

### The install (`tools/posefill/setup_posefill.py`)
- Folder `%LOCALAPPDATA%/<product>/posefill` (or `AC_POSEFILL_DIR`); `install.json` version 1
  (`upstream_commit`, `model`, `python`, `helper`, `env`, `versions`, `gpu`, `platform`) written
  last, as textmotion's. Refuses without an NVIDIA GPU (`nvidia-smi` on PATH; tested: "no NVIDIA
  GPU found (nvidia-smi gave none): the pose-to-pose engine runs as a TensorRT plan on an NVIDIA
  GPU, and there is no CPU or other-GPU path, so it can't be installed here", exit 1, nothing
  written) and on non-x86-64 machines; needs about 6 GB free.
- Six steps with textmotion's `== [n/6] ...` / `== done` / `== stopped` lines (not JSON lines: the
  brief said "the JSON-lines style textmotion's setup uses", but textmotion's setup prints these
  step lines and `ac_textmotion::setup::feed` parses them, so the same Rust reader serves both;
  the helper is JSON lines). `--stop-on-eof` as textmotion's. Resumable: a second run skips the
  source ("at 54fc563aeaaf already"), the model files ("present, hash matches") and reinstalls
  nothing (3 s whole, with the check).
- Steps: 1 the package from GitHub's archive at commit `54fc563aeaaf...` (tree hash
  `7d167ff2...` over the 80 kept files; 352 archive files dropped: `assets/`, `app/`, `docs/`,
  `tests/`, `tools/`, dot files and 40 LFS pointers); 2 the ONNX (758 MB) and the USD (2 MB) from
  GitHub's LFS media endpoint, sha256 checked; 3 `uv venv` Python 3.12 (the TensorRT bindings on
  pypi.nvidia.com are cp312 wheels), the pinned packages, TensorRT with `--extra-index-url`, and a
  `.pth` file pointing at `upstream/` (no wheel is built: one would copy the model into
  site-packages); 4 `helper.py`, `engine.py` and `ac_rig/` (rig, quat, retarget copied from
  `training/ac_train`, as textmotion does); 5 the check that `upstream/assets`, `app`, `docs`,
  `tests`, `tools` and any `.bvh`/`.csv` are absent; 6 the helper fills one second between two
  rest poses, which builds the TensorRT plan, so the app's first fill does not wait for it (a
  moved or rebuilt install still builds it on first use, and the helper says so).
- Licences: printed before step 1 ("Installing means you accept: ...") and written to
  `licences.txt` with names and links (code Apache-2.0; model NVIDIA Open Model Agreement;
  TensorRT SLA + supplement; CUDA EULA; Warp Apache-2.0; OpenUSD TOST 1.0; numpy/scipy BSD), plus
  the path of the TensorRT text inside the wheel and the note that the sample clips are never
  installed.
- Wall time: 117 s for steps 1-3 (downloads on this connection) + 62 s for steps 4-6 (55 s of it
  the plan build) = **about 3 minutes**. Size: **4.4 GB** (`.venv` 2.9 GB, `upstream/` 726 MB
  with the model, `trt-cache/` 730 MB, Python 68 MB, `warp-cache/` 1.4 MB). The fetched TensorRT,
  Warp, USD and CUDA wheels were removed from the uv cache afterwards (`uv cache clean <package>`).

### The helper protocol (`helper.py`, JSON lines on stdin/stdout, replies carry the request's `id`)
- `{"op":"info"}` -> `{protocol: 1, ops, model: "mmm_v6_soma_fp32", fps: 30, min_frames: 18,
  max_frames: 58, context: 4, max_targets: 4, plan: "built" | "not built (the first fill builds
  it)", gpu, versions: {tensorrt, warp, numpy}, loaded}` (+ `load_seconds`, `plan_was_built`,
  `joints` once loaded). Nothing is loaded by `info`.
- `{"op":"load"}` -> loads (progress lines: "building the TensorRT plan for this GPU (a few
  minutes, once)", then "building the TensorRT plan: Timing graph nodes 51%" ... "ready (plan
  built in 55 s on NVIDIA GeForce RTX 3080 Ti)"; a later run says "loading the TensorRT plan" and
  "ready (plan loaded in 1 s ...)").
- `{"op":"fill", "rig": "<default_rig.json>", "frames": 31, "fps": 30, "context": [1..4 poses, the
  last on key A], "targets": [1..4 poses, the first on key B]}`, a pose being `{"rotations":
  [[x,y,z,w] x 104 in rig order], "root": [x,y,z] in hip heights}` (what `pose.get` and
  `clip.sample` give). `frames` counts the poses returned, key A's and key B's included, so a gap
  of 30 frames is `frames: 31`. Result: `{rotations: frames x 104 x 4, roots: frames x 3, notes,
  model_frames, tokens, seconds (GPU wall for the generation), stages_ms: {encode, inference,
  decode, post_process}, gpu, contacts: frames x 4 (the model's heel/toe confidences: left heel,
  left toe, right heel, right toe)}`; the first frame is exactly key A and the last exactly key B
  (blended on, errors 5e-8). A context of fewer than 4 poses repeats the first and says so in
  `notes`. The frame rate converts to 30 fps and back as textmotion's does.
- Refused with the reason, the helper carrying on: an unknown op; a gap outside the model's range
  ("61 frames at 30 fps is 61 model frames; the model makes 18 to 58 (0.6 to 1.93 s): a longer gap
  is filled through a key inside it"); 0 or more than 4 context or target poses; a pose with the
  wrong bone count, a short root, or a non-finite number; a non-integer `frames`, an `fps` outside
  1-1000, a missing `rig`; and engine output that is non-finite or of the wrong shape (checked in
  the helper against `frames x bones x 4` and `frames x 3`; the tests prove both through the fake).
- Everything the model prints goes to stderr (stdout is duplicated and swapped, as textmotion's
  helper does); `AC_POSEFILL_ENGINE=<file.py>` loads another engine module (the tests' fake).

### The engine (`engine.py`)
- The model's token count is fixed to the one nearest the requested length (`round((n + 6) / 4)`,
  6-16), so a 31-frame fill is 9 tokens = 30 model frames stretched to 31 and said in `notes`
  ("30 model frames stretched to 31 (9 tokens of 4 frames, less 6 of boundary)"); 58 frames is
  16 tokens exactly. Letting the model choose a duration (its default 8-14) would then need a
  larger time stretch to the user's gap, so it was not done.
- rig -> model: each model joint takes its rig bone's world rotation carried over from rest
  (both rests stand; a constant per-joint frame K lines the two rests up lateral-to-lateral, or
  up-to-up for bones along the lateral line), the model body keeps its own bone lengths, its hips
  are placed so its two hip joints' midpoint lands on the rig's (scaled by leg length). 30 model
  joints come from 30 rig bones (`TRANSFER` in `engine.py`: Hips/root, Spine1-2/Chest from
  spine05/03/01, Neck1-2/Head, shoulders/arms/forearms/hands, legs/shins/feet/toes); the USD
  skeleton's other 47 joints (fingers, eyes, jaw, ends) keep their reference locals. The model is
  Y up; a rig point (x, y, z) is the model's (x, z, -y).
- model -> rig: the model body's joint positions become the 17 landmarks, `ac_rig.retarget`
  (scale_to + retarget, the same path as motion from text) puts the rig on them, the frame count is
  fitted, and `blend_ends` bends each end exactly onto its key. The upstream engine's own endpoint
  correction and boundary trimming are used as they are (no port).
- Plan cache in `<install>/trt-cache`, Warp's kernel cache in `<install>/warp-cache`, so nothing
  lands in the working directory. The build's phases are polled from a thread every 2 s for the
  progress lines. fp32 model (fp16 would halve the download and the plan; not tried).
- Determinism: the ONNX graph has no seed input and the same request gave the same numbers in the
  three repeated calls (not a guarantee across GPUs or TensorRT builds).

### Numbers on the RTX 3080 Ti (`summary.json`)
- Plan build 55 s (once); plan load 1.07 s on later starts; `load` wall 1.4 s.
- First fill of a process 77 ms (the CUDA graph is baked then); every later fill **4.5 ms on the
  GPU** (encode 0.26 ms, inference 3.48 ms, decode 0.09 ms, post-process 0.015 ms) and **about 40
  ms wall per call** through the helper, the retarget and JSON included (31 frames; 58 ms for 58
  frames). The install's own check fill was 4.23 s, Warp's kernel compile included.
- Fills made (keys from the built-in library: Walk 12, Standing/Idle, Gestures/Wave, through the
  CLI copy's `clip.sample`): `walk-1s` (frame 10 -> 40, 31 frames), `walk-1s-4targets` (frames
  40-43 as the target token), `walk-1.9s` (10 -> 67, 58 frames, the longest one call makes),
  `walk-chain-2s` (10 -> 40 -> 70: two calls, the second's context the first's last 4 frames, 61
  frames), `idle-to-wave-1s`, `wave-to-idle-0.8s` (25 frames), and a 1-pose context (padded,
  said). A 2 s gap in one call (61 frames) is refused as over 58.
- Against the clip the keys were cut from (Walk 12), the model's frames sit a mean 7.5 cm RMS per
  joint from the source (max 13 cm) for `walk-1s`, 6.1 cm with the 4-pose target, 8.8 cm over 1.9
  s, 9.2 cm chained (max 23 cm around the middle key): its own gait between the same keys, not a
  copy. Feet stay between -0.5 cm and 23 cm above the floor; root travel -0.91 m in 1 s and
  -1.78 m in 2 s along -Y like the source; the largest joint move in a frame is 16 cm (a swinging
  foot). The strips (`*.clip.strip.png`) show alternating strides and a wave that raises the arm
  sideways then up; looked at, not garbage. The owner judges from MP4s.

### Tests
`uv run --with pytest python -m pytest tools/posefill/tests` from the repo root or
`../tools/posefill/tests` from `training/` (11 tests, no GPU, no numpy): info before and after a
load, a fill meeting both keys with rounded numbers and progress lines, the short-context note,
each refused gap with the limits in the message, every bad request refused while the helper keeps
serving, non-finite and wrong-shape engine output refused, a non-JSON line, and the real process
over a pipe with the fake engine speaking only JSON lines on stdout. `uvx ruff check --config
training/pyproject.toml tools/posefill` is clean.

### Shared with `tools/textmotion/`, copied for now: for the Rust-side agent to factor out
- `engine.py`: `SOMA` and `RIG` landmark maps, `soma_to_rig`, `rig_to_soma`, `rig_landmarks`,
  `resample`, `_smooth`, `blend_ends`, `_leg`, `_fit_length` (identical to textmotion's; posefill's
  `resample` calls `_fit_length`).
- `helper.py`: the stdout/stderr swap, the reply loop with per-request `progress`, the pose check.
- `setup_posefill.py`: `product_name`, `default_dir` (one `<product>/<name>` rule), `step`,
  `kill_tree`/`popen`/`run`/`stop_on_eof`, `check_space`, `need`, `venv_bin`, `sha256`, `curl`,
  `download`, `tree_hash`, `writable_then_retry`, `uv_python`, the `ac_rig/` copy. One
  `tools/_setup_common.py` (or a package) would serve both; it is not moved here because
  `tools/textmotion/` was out of this task's write set.
- Rust: `crates/ac-textmotion/src/install.rs` (dir + `install.json` reading), `setup.rs` (the
  `== [n/N]` feed, `Run`, uv fetching), `helper.rs` (the JSON-lines process) and `uv_fetch.rs` are
  all what `ac-posefill` needs with the folder name and the ops changed: move them to a shared
  crate (or `ac-io`) first, in a behaviour-preserving commit, as the item says.

### What the Rust crate still needs (`ac-posefill`)
- `install.rs`: `%LOCALAPPDATA%/<product>/posefill` or `AC_POSEFILL_DIR`; `install.json` version
  1 with `python`, `helper`, `env`; the `HOW` hint naming `posefill.install` and
  `python tools/posefill/setup_posefill.py` (an NVIDIA GPU required, about 6 GB).
- `posefill.install` (the setup with `--stop-on-eof`, Stop, resume, the step lines),
  `posefill.helper` (start/stop/status: `info`, and `loaded`/`plan` for the panel's "building the
  plan" message), `posefill.fill`: the gap A-B at the playhead between two keys of yours; context =
  the 4 frames ending on A sampled at 30 fps (fewer before A: repeat the first and say so);
  targets = B (and up to 3 frames after B when keys exist there: the 4-pose target matched the
  source closer, 6.1 vs 7.5 cm); `frames = B - A + 1` at 30 fps; refuse outside 18-58 with the
  limits; a longer gap chains through the keys inside it (the second call's context is the first
  fill's last 4 frames, as `walk-chain-2s` was made) and with none is refused; the clip's rate
  converted as `fits` does; the result written as AI keys tagged `"posefill"` in one undo step;
  non-finite or wrong-shape helper output refused (the helper already refuses it; the Rust side
  checks again).
- `ac_core::edits::GENERATED` learns the `"posefill"` tag beside `"text"`.
- `Engine` seam with the stand-in for fixtures (`fixtures/pose-fill.json`: a fill, one undo step,
  the tag, each refused gap, the short-context note, bad output refused).
- The panel (Install with the licences list above and Stop, the plan-build notice, Fill), the
  `ATTRIBUTION.md`/`NOTICE` credit line, `AGENTS.md`'s exception and folder row, `docs/commands.md`,
  packaging (124000) shipping `tools/posefill` with the rig modules, `cargo xtask check`.
- Review: MP4s of the six clips above through `clip.video`, and a fill saved to the library and
  reloaded.
