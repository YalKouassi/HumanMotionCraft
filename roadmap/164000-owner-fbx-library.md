# Port the owner's FBX animations into a private clip library

- **Status:** in progress: the `.meta` step is built, the pilot is next; acceptance agreed by the owner (2026-10-31)
- **Requires:** 152000
- **Source:** owner request (2026-10-31)

## What
Bring the owner's whole collection of humanoid FBX animations onto the rig as `.clip.json`: 6,489
files from 27 purchased packs, listed in an inventory the owner keeps outside the repo. Each clip is
oriented, retargeted and checked on our rig. 152000 built the importer for one file; this item is
the bulk port and whatever real files show that importer still lacks.

**Private.** The clips and the manifest live outside the repo. They are never published to the
community library and never bundled in a release. Generic importer code may go in the repo; pack
names, paths and clips may not. Fixtures stay generated (152000).

**No FBX copied.** Only the clips are written. The manifest records each clip's source path and hash,
so a port can be re-run while the source drive is attached and a finished clip doesn't need it.

**Not for training.** Every clip is left out of training by default. The owner may choose single
clips later, and only those clips are used. That choice has to be squared first with 152000's rule
that store packs stay out of `training/` unless their publisher has consented in writing.

### What real files show (2026-10-31, today's `ac`, no clip kept)
- **Most files don't import yet.** 24 of a random 150 binary files imported, and 5 of 27 files taken
  one per pack.
- **Bone names are the main cause.** 111 of the 150 were refused as not humanoid. The importer
  doesn't know names such as `Bip001 Pelvis`, `B_Pelvis` or `<prefix> Pelvis`.
- **Unity's `.meta` files already hold the answer.** 6,316 of the 6,333 `.meta` files beside the FBX
  carry a humanoid bone map, including all 46 name refusals checked. Read that map first and guess
  from names only without one.
- **Text FBX.** 664 of the files are text (ASCII), which the reader refuses.
- **Old binary files.** 520 files are FBX 6.1. 11 of the 150 were refused for having no
  `GlobalSettings`, which fits that share, but the link isn't confirmed. Their axes and units need
  another source.
- **`ImageData`.** 3 of the 150 were refused on a property type the reader doesn't know (`c`) in a
  node the animation doesn't need.
- **One file, many clips.** At least 187 `.meta` files split one take into several clips, up to 90
  in one file. The import reads the take as one long clip (one came in at 2,889 frames). Use the
  `.meta` frame ranges, names and loop flags.
- **Duplicates.** Whole packs are present twice, and a bundle repeats packs that are also listed on
  their own. There are 6,116 distinct file names among the 6,489. Port one copy of each and record
  the others.
- **Possible wrong poses.** On 20 of the 24 imported files the importer reads an unsupported scale
  inheritance type as the default. On 7 it reads only the first animation layer or first take. Find
  out whether these change the result, then fix them or refuse the file.

### Orienting and fitting
- Every clip starts facing -Y, and left stays left. Keep root motion, and say which clips are in
  place.
- Hanging, climbing and swinging clips have no floor. They must not be pushed down to z = 0 or
  flagged as a bad floor.
- Fingers, face and props stay at rest, as in 152000. A weapon clip keeps its stance without the
  weapon.
- Review with physics off: the physics pass sits lying bodies up.

### Running it
- A resumable batch driven by the manifest. `clip.import_batch` takes one folder and one `joints`
  correction, which doesn't fit 27 packs.
- The manifest gives every source file one outcome: imported (with its clips), duplicate, or refused
  (with the reason). Nothing is dropped silently.
- Clips are grouped by pack and action in the library. The library panel has about 220 clips today,
  so check that it copes with several thousand.

## Acceptance
- **Accounted for:** every file in the inventory has one manifest outcome, the counts add up, and
  the owner agrees every refused group.
- **Reads:** text FBX and FBX 6.1 either import or are refused with a reason, and each case has a
  generated fixture. A generated file with a Unity bone map and clip ranges imports as named clips.
- **Automatic checks on every clip:** values are finite, the first frame faces -Y within 15°, the
  feet aren't swapped, bone lengths are the rig's, and the feet rest on z = 0 where the source's rest
  on its floor. Failures are listed in the manifest and left out of the library until fixed.
- **Owner's eye:** clips are screened and grouped, with one short numbered clip per group beside its
  source (physics off). The owner answers with the numbers to reject, and the reject list is fixed or
  refused before the item is done.
- **Private:** `git status` shows no clip, manifest, pack name or path. A test shows these clips
  can't be uploaded to the community library. A test shows `ac_train` reads none of them unless a
  clip was chosen by name.
- **Re-run:** running the port again changes nothing and skips finished files.
- **Library:** the panel opens, searches and plays a clip from the full set without a visible stall.

## Decision (2026-10-31)
The owner agrees the acceptance above, with these terms:
- **Order:** first the `.meta` bone map and clip ranges, then a pilot of about three varied packs, then
  the full set.
- **Training:** no clip from this collection trains anything for now. If the owner names a clip later,
  that pack's licence is checked first, and 152000's rule on the publisher's written consent still applies.
- **152000's low leftovers** (an unreadable folder passed over, animated scale inheritance, the `KeyTime`
  wording, a default `TimeMode` note) are handled here, where real files show which matter.

## Result
**First step built (2026-10-31): the `.meta` bone map and clip ranges.** This is the Decision's first
step only. The pilot, the bulk port, text FBX and FBX 6.1 are not started, so the item stays open.
- `ac_io::fbx::unity_meta` reads `<file>.fbx.meta`. Its humanoid bone map says which joint is which.
  A `joints` correction outranks the map, and the map outranks the guess from names.
- A landmark the map leaves out (toes and neck are optional there) is guessed from names, and the
  notes say so. A `.meta` with no map falls back to the guess, also noted. `bone_map` in the result
  says which was used.
- `clipAnimations` cut the take into named clips, each with its loop flag on `Clip.loop_` and retargeted
  and levelled on its own. `clip.import` opens the first or the one named by `clip`, and lists them all.
  `clip.import_batch` writes `<file> - <clip>.clip.json` each, and a refused clip is listed by name
  while the rest are written.
- Refused with a reason: a `.meta` that isn't a model's or can't be read, a bone the FBX lacks, two
  bones for one humanoid bone, one bone for two, a clip of another take, outside the take or under 2
  frames, and two clips of one name.
- Generated fixtures only: `fixtures/data/walk-cut.fbx` and its `.meta`, from `ac_train.fbxwrite`.
  Fixture `fbx-unity-meta`, tests `crates/ac-io/tests/fbx_meta.rs`.

**For the pilot to check on real files:**
- Clip frames are read as the file's own timeline, so a take starting at frame 100 has its clip's
  `firstFrame: 100` on sample 0. No generated file can confirm this reading.
- A clip's `takeName` must equal the take read (the first). A stale or normalised name is refused.
- A clip with one frame is refused, as a whole short take is. Single-pose clips may need another answer.
- Fractional ranges are rounded to whole frames, with a note.
