"""A set from a folder of the app's own clip files (the FBX import's output, roadmap 152000): read
as they are, in-place takes left out, and the scan runs on it."""

import shutil

import numpy as np

from ac_train import ASSETS, datasets, quality
from ac_train.clip import Clip
from ac_train.rig import Skeleton

LOOP = ASSETS / "library" / "animations" / "Locomotion" / "Walk.clip.json"  # the CC0 loop: on the spot
TRAVELS = ASSETS / "library" / "animations" / "Locomotion" / "Walk 12.clip.json"  # 100STYLE: 2.5 m


def test_in_place_is_stepping_without_travel():
    skel = Skeleton.load()
    r, o = Clip.load(LOOP).sample_all()
    assert datasets.in_place(skel, r, o)
    r, o = Clip.load(TRAVELS).sample_all()
    assert not datasets.in_place(skel, r, o)
    # standing still is not in place: the feet do not step
    still = np.tile(r[:1], (60, 1, 1)), np.tile(o[:1], (60, 1))
    assert not datasets.in_place(skel, *still)


def test_a_folder_of_clips_builds_a_set_the_scan_reads(tmp_path, monkeypatch):
    folder = tmp_path / "clips"
    (folder / "sub").mkdir(parents=True)
    shutil.copy(LOOP, folder / "Walk loop.clip.json")
    shutil.copy(TRAVELS, folder / "sub" / "Walk forward.clip.json")
    (folder / "notes.clip.json").write_text("not json", encoding="utf-8")
    rig = tmp_path / "rig"
    monkeypatch.setattr(datasets, "RIG", rig)
    monkeypatch.setattr(quality, "RIG", rig)
    out = datasets.build(datasets.clips_set("myclips", folder))
    d = np.load(out, allow_pickle=True)
    assert list(d["takes"]) == ["walk_forward"], "the loop is in place and the broken file is skipped"
    assert list(d["labels"]) == ["Walk 12"]
    assert d["rotations"].shape == (90, 104, 4) and d["roots"].shape == (90, 3)
    assert bool(d["commercial"]) is False and str(d["licence"]) == "your own clips"
    assert int(d["rig_crc32"]) > 0, "stamped like every set"
    record = quality.scan_set("myclips")
    assert (rig / "myclips.quality.json").exists()
    assert record["takes"][0]["take"] == "walk_forward" and record["takes"][0]["frames"] == 90
    assert record["takes"][0]["reject"] == ""
