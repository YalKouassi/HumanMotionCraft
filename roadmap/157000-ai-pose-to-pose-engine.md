# Pose-to-pose motion from MotionBricks

- **Status:** not started
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
- Quality on our rig: the model card warns that unusual target poses lower it.
- Speed on lower-end cards: published timings are for an RTX 4090 (2.5 ms for the plan alone).
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
