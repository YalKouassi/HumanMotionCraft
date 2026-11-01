# Port the owner's FBX animations into a private clip library

- **Status:** done (2026-11-01)
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
- **Owner's decision (2026-10-31):** no review group by group. Everything is ported, and the owner
  deletes bad clips as they go. A deleted clip stays deleted when the port runs again.
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

## Decision (2026-10-31, later)
The owner will not review group by group: port everything and delete bad clips as they go. This
replaces the screened groups and numbered reel in the acceptance.

## Result
**First step built (2026-10-31): the `.meta` bone map and clip ranges.** This is the Decision's first
step only. The pilot and the bulk port are not started, so the item stays open.
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

**Reader work built (2026-10-31), generated files only.** No real file was read, and this worktree
built nothing: the numbers below are worked by hand or come from the generator, and the integration
worktree runs the tests.
- **Text FBX imports** (`ac_io::fbx::text`): the same records as binary, the same caps plus a total of
  values, and a refusal that names the line.
- **FBX 6.1 imports, text or binary** (`ac_io::fbx::legacy`): its named objects, `Properties60`,
  `Connect` and `Takes` become a 7.x scene for the one scene reader. `GlobalSettings` is looked for in
  `Objects`, where FBX 6 keeps it, which may explain the 11 refusals. A file without it is still refused
  with a reason: axes and units are never assumed. Two models of one name are refused, since FBX 6
  connects by name.
- **Scale inheritance changes the pose, and is fixed.** All three `InheritType`s are computed. The old
  code's plain matrix product was type 1, while it named type 0 the default it read everything as.
  Types 0 and 1 differ under a non-uniform parent scale, and type 2 differs under any parent scale.
  With no scale above a node all three agree, so unscaled skeletons don't change. A type 0 or 2 node
  below a shear (a type 1 child of a non-uniform scale) is refused, as is a type outside the three.
- **Later animation layers change the pose, and such takes are refused.** Layers blend, and this reader
  doesn't. Refused: a later layer that moves anything, a soloed one, an animated weight, a first layer
  muted or under full weight. Passed over with a note: muted, weight 0, or additive with zero offsets.
  The first layer is now the first connected, as the format orders them, not the first in the file.
- **Reading the first take doesn't change the pose.** A take is a separate animation, not a blend, so
  the result stands and the existing note says what wasn't read. A `.meta` clip of another take is
  still refused. The pilot should show whether reading further takes is worth building.
- **`ImageData`'s property type `c`** is passed over and not inflated.
- **152000's low leftovers, all done:** an unreadable folder is listed in `failed`; an animated scale is
  inherited like any scale; key times that aren't whole ticks are refused in those words; `TimeMode` left
  at default has a note.
- **Found on the way:** cubic curves were read as straight lines with no word when their keys sat on
  frames. A cubic curve with keys more than a frame apart now gets a note. The curve is still straight.
- Generated fixtures: `walk-text-74.fbx`, `walk-text-61.fbx`, `walk-binary-61.fbx`, `walk-text-cut.fbx`.
  Fixture `fbx-text-and-6`; tests in `crates/ac-io/tests/fbx.rs` and the `text`, `legacy`, `scene` and
  `binary` modules.

**For the pilot to check on real files:**
- The generated FBX 6 files come from our own writer, so they prove only our reading of the layout.
  Least sure: the binary form of `Key` (a `C` byte for the kind), the cubic `Key` grammar, and the
  `InheritType` formulas (the format as remembered, with no outside check yet). A cubic form this reader
  doesn't know is refused, and `KeyCount` must match.
- How many files the layer refusal stops (7 of the 24 had a second layer or take). If many, blending is
  the next build.
- How many cubic notes appear, and whether those clips look wrong.
- A text file with a record over 1,048,576 values (a large old-style mesh) is refused whole.

## Pilot (2026-10-31)
Three varied packs, run in integration, clips and manifests outside the repo. Small climbing pack: 7 files listed, 4 clips imported, 34 refused, almost all named `.meta` clips whose take name isn't the file's first take, so multi-take files lose clips: to fix before the bulk port. Mocap conversations: 57 of 57. Weapon pack: 50 clips, 1 file with no animation; 10 clips note a hip jump in the first frames. Owner's review reel is outside the repo. Nothing here proves the poses right until the owner has reviewed it.

## Several takes and the bulk port (2026-10-31)
Built from generated files only: no real file was read, and this worktree built nothing, so the
integration worktree runs the tests.
- **Several takes.** A `.meta` clip's `takeName` now picks that take of the file. A name the file
  doesn't have refuses that clip and lists the takes. The other clips still import, and a take that
  can't be read refuses only its own clips. A first take no clip names isn't read, so it can't stop
  the file. A file with no clips still reads its first take as one clip, with the note.
- **`clip.port {list, root, to, limit?}`.** The list is a text file, one path a line. The output
  folder holds `port-manifest.jsonl` and `clips/<pack>/<action>/`. The pack is the first folder under
  `root`, the action the file's own folder. It refuses an output folder inside a repository, and a root
  the paths don't lie under.
- **One outcome each.** `imported` with its clips and those left out; `duplicate` with `of`; `refused`
  with the reason. A duplicate has the same FBX bytes and the same bone map and clips in its `.meta`,
  so a copy cut differently is kept. A file that can't be read now (the drive away) gets no line, is
  listed in `unreachable` and is tried next run.
- **Re-run.** A finished source is never looked at again, and with nothing to do no file is written.
  A line is added once a source is finished; a last line cut off by a stop is dropped and done again.
- **Facing.** The checks alone would leave out most clips cut from the middle of a take, and this item
  says every clip starts facing -Y. So a clip more than 15° off is turned about the vertical through
  its first root, and the angle is recorded. One within 15° stays as it is, because hip sway on the
  first frame would otherwise send a straight walk off at an angle. The check then runs on the result.
  A hip line near upright from above (a body on its side) can't be read: left as it was, with a note.
- **Other checks.** Values finite. Feet swapped from 50% of the importer's checks. Floor: a floor more
  than 0.03 m below z = 0 fails. A floor over 0.2 m up is `hanging` and stays where it is. A clip moved
  down more than 0.05 m with no foot still for 3 frames (under 0.25 m/s) is `hanging` and the move is
  taken back. Failures are listed in `left_out` and write no clip; a file with none kept is `refused`.
  Bone lengths are the rig's by construction. Clips also record `in_place` (root within 0.5 m).
- Generated fixtures: `fixtures/data/port/`. Fixture `fbx-port`, tests `crates/ac-io/tests/fbx_port.rs`
  and the unit tests in `fbx/port.rs`.

**For the bulk run to show:**
- A hanging clip whose feet hold still and that the import moved down up to 0.2 m is still pushed down.
  Feet alone can't tell it from standing. A body standing on a prop over 0.2 m up counts as `hanging`.
- The library panel lists one level of groups (`crates/ac-io/src/library.rs`), so it doesn't yet show
  `clips/<pack>/<action>/`. The Library acceptance line is still open, as are the Private tests.

## Library
Built 2026-10-31 for browsing several thousand private clips and deleting bad ones. This worktree built
and ran nothing: no number below was measured, and the integration worktree runs the tests.

**Opening a folder:** `humanmotioncraft --library <dir>` for one run, or Settings > Library folder from
the next launch. Clips must be `<dir>/animations/<group>/<name>.clip.json`, one group level; a clip one
folder deeper isn't listed, so the port's "pack and action" must fit in one folder name. A folder with
clips in any other layout is left untouched and the status line says what to move. `library.sync` copies
the built-ins into the folder: marked, protected, hidden by the Mine filter.

**Delete:** the cross on the row under the pointer or the selected one, the Delete key, the preview's
Delete, or the right-click menu. In a library other than the app's own it goes at once: `clip.delete`, the
next row selected, the status line says the file is removed. It asks first in the app's own library (the
existing convention), and anywhere when the open animation has unsaved changes. Built-ins stay. No
`library.list` runs: the window's list is mended in place, and the clip's two thumbnails are removed.

**Decided:** a delete without a question is permanent. Undo only brings back an animation that was open,
which the old status line claimed for every clip; it now says so only when true.

**Open, for the bulk port:** re-running the port would bring a deleted clip back, because the manifest
marks it finished only by its file. The port should record what the owner deleted.

**Thousands of clips, what stalled and what changed:**
- Every `library.list` opened and parsed every file. Now `<library>/.index/<kind>.json` keeps each file's
  result against its size and time, so only new or changed files are read, on up to 8 threads. `items()`
  still reads everything and writes nothing, so a listing of `assets/library` leaves no file there.
- Every frame cloned the whole list, searched it, looked up each row's `builtin` by a scan of the list and
  checked `order_keys` by a scan: quadratic. The filtered list is now worked out only when its inputs
  change, rows off screen only take their room, and a right-click menu's data is read only when it opens.
- Search sorted with an allocating comparison; the sort key is now worked out once per item.
- Thumbnails: a budget of 8 ms a frame instead of 2 a frame, at least one each frame. In memory at most 512
  thumbnails (about 110 KB each) and 32 previews (about 3.7 MB each), oldest out; with the list no longer
  refreshed on delete, nothing else would have freed them.
- The 1.5 s library watch opened every file for its size and time on the UI thread. It now reads the
  folder's own record.

**To check at build, with the full set:**
- A first listing reads every file once, before the window opens. Running
  `ac --library <dir> run library.list` once warms it. How long it takes is not known.
- A clip's first thumbnail and first preview parse it and draw on the UI thread, so one long clip can
  still take longer than a frame. The budget can't split that. If it shows, parsing moves off the thread.
- Starring still runs a whole `library.list` and drops the thumbnails, which come back from disk.
- `LibraryPoses` (anchors in a long AI fill) still parses every clip of the open library, in memory, on
  the first such fill.
- Off-screen rows no longer report a `library.item.<id>` to `ui_layout`, so an agent must scroll to them.
- Windows' folder record of a file's size and time is taken to be current once the file is closed.

Tests: `crates/ac-io/tests/library.rs` (the index, threads, opening a folder, the layout note) and the tests
in `crates/ac-app/src/app/library.rs` and `library/menu.rs` (6,000 rows, rows on screen, the cap, the next
row, when it asks, the status line). All expected numbers are worked by hand in the tests' comments.

## Bulk run (2026-10-31)
All 6,489 sources have one outcome: 5,297 imported (6,433 clips, 4.9 GB, outside the repo), 116 duplicates, 1,076 refused. Largest refused groups: 321 no joint found for neck, 186 not humanoid by name with no bone map, 56 floor below z = 0, 14 `.meta` naming a head the file lacks, 12 feet swapped. `library.list` opens the folder; timing in the panel at this size isn't measured. The owner deletes bad clips as they go. The acceptance's agreement of every refused group is still to do.

## Refused groups, what the importer now reads (2026-10-31)
Built from generated files only: this worktree built and ran nothing, and no real file was read, so how many of the 1,076 this frees isn't known.
- **A stale take.** A clip naming a take the file doesn't have, in a file of exactly one take, reads that take with the clip's frame range and a note naming the stale take. With several takes it is still refused.
- **No neck.** With every landmark but the neck, from the bone map or the guess, the head's parent is read as the neck, with a note. If a landmark already has that joint, it is still refused.
- **Loose bone names.** A `.meta` bone not found exactly matches a joint ignoring spaces and case, with a note, only if one joint matches. Several matches refuse, as do two of the map's bones landing on one joint.
- **`clip.port {retry_refused: true}`.** Sources whose latest outcome is `refused` are tried again, and a changed outcome is a new line; the last line of a source is its outcome. Imported and duplicate stay skipped. A refusal that comes out the same adds no line, so a repeat changes nothing. A refused source wrote no clip, so a retry can't bring a deleted clip back.

**For the retry to show:**
- An imported source isn't tried again, so clips it left out (a stale take among them) stay left out. Getting those needs a clip-level retry, not built.
- A head hanging directly from a chest no landmark has reads that chest as the neck. The rule allows it, and how those clips look is the owner's to see.
- A duplicate of a refused source stays a duplicate; if the first is imported, its clips stand for both.

## Refused groups, second pass (2026-10-31)
Built from generated files and the joint lists given to this worktree: it built and ran nothing, and read no real file, so how many refusals this frees isn't known.
- **Two more name conventions.** A first word `B` or `humanoid` is a rig's prefix and is read past, so `B_L_Thigh` is the left thigh and `humanoid_ R Hand` the right hand. Hips come from `Pelvis`, sides from the `L`/`R` words.
- **A clip running past its take** is cut to the take, with a note giving the frames cut. No overlap, or under 2 frames left, is still refused. This is the one code path, so `clip.import` and `clip.import_batch` cut too.
- **A floor below z = 0 in `clip.port`** is moved up so the feet rest on z = 0, with a note and the distance in `raised`. Hanging handling is unchanged. A floor up to 0.03 m below stays as it is.
- **`clip.import` is unchanged here, and never refused this:** it keeps its note and leaves the floor where it is. The move is in `clip.port`'s own step.

**For the retry to show:**
- The `humanoid_` list given stopped at the left hand, so its legs are taken to follow the same form. A rig with no toe joint is still refused.
- A clip with no still feet and a floor far below z = 0 is raised, not called hanging. How those look is the owner's to see.
- The `feet_on_floor` check no longer fails any clip, so the 56 refused for it should import on a retry.

## Refused files fixed (2026-10-31)
A stale take name on a one-take file, a neck from the head's parent, loose bone names, the B and humanoid name forms, a clip range cut to its take, and a floor below z = 0 raised. After retrying: 6332 imported (7488 clips), 116 duplicates, 41 refused. Clips read from a stale take or a head's parent as neck have only been through the automatic checks; the owner deletes bad ones as they go.

## Closed (2026-11-01)
The owner agrees the 41 refused files, the panel at full size and the rest of the acceptance. Not built, accepted by the owner: tests that these clips can't be uploaded to the community library or read by `ac_train`. 117 clips from 47 imported files stay left out.

The 22 long compilation clips are kept. Each was compared frame by frame with the pack's single clips: only one reached 95 % of its moving frames, most 20 to 60 %, so they can't be called duplicates (the method may undercount). The 38 long mocap clips are 19 pairs, the same body motion with and without root travel, and are kept.
