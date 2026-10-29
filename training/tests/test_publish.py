"""Build stamps, publishing the retargeted sets, and fetching them back."""

import dataclasses
import json

import numpy as np
import pytest

from ac_train import fetch as F
from ac_train import publish as P
from ac_train import stamp


def make_set(rig, name, commit="abc1234", crc=None):
    rig.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(rig / f"{name}.npz", rotations=np.zeros((30, 2, 4), np.float16),
                        roots=np.zeros((30, 3), np.float32), starts=np.array([0, 20]), takes=np.array(["a", "b"]),
                        rig_crc32=np.array(stamp.rig_crc32() if crc is None else crc), code=np.array(stamp.code_hash()),
                        commit=np.array(commit))
    scan = {"set": name, "takes": [{"take": "a", "frames": 20, "cuts": [], "reject": ""},
                                   {"take": "b", "frames": 10, "cuts": [], "reject": ""}]}
    (rig / f"{name}.quality.json").write_text(json.dumps(scan))


def test_the_rig_stamp_ignores_line_endings(tmp_path):
    (tmp_path / "a.json").write_bytes(b'{"a": 1,\n "b": 2}\n')
    (tmp_path / "b.json").write_bytes(b'{"a": 1,\r\n "b": 2}\r\n')
    assert stamp.rig_crc32(tmp_path / "a.json") == stamp.rig_crc32(tmp_path / "b.json")


def test_a_set_for_another_rig_stops_and_an_unstamped_one_is_said():
    ok = {"rig_crc32": np.array(stamp.rig_crc32()), "code": np.array(stamp.code_hash()), "commit": np.array("x")}
    assert stamp.check("s", ok) == []
    assert "other pipeline code" in stamp.check("s", {**ok, "code": np.array("old")})[0]
    assert "no build stamp" in stamp.check("s", {})[0]
    with pytest.raises(SystemExit, match="another rig"):
        stamp.check("s", {**ok, "rig_crc32": np.array(1)})


def test_publish_leaves_out_what_the_licence_forbids(tmp_path, monkeypatch):
    # a set someone adds whose licence forbids derived copies
    mine = dataclasses.replace(P.REGISTRY["cmu"], name="mine", licence="CC BY-NC-ND 4.0", commercial=False, share="never")
    monkeypatch.setitem(P.REGISTRY, "mine", mine)
    rig = tmp_path / "rig"
    make_set(rig, "cmu")
    make_set(rig, "mine")
    m = P.publish(["cmu", "mine"], tmp_path / "out", rig=rig, manifest=tmp_path / "m.json")
    assert {f["set"] for f in m["files"]} == {"cmu"}
    assert sorted(p.name for p in (tmp_path / "out" / m["tag"]).iterdir()) == ["NOTES.md", "cmu.npz", "cmu.quality.json"]
    assert json.loads((tmp_path / "m.json").read_text())["tag"] == "rig-data-abc1234"
    with pytest.raises(SystemExit, match="nothing to publish"):
        P.publish(["mine"], tmp_path / "out", rig=rig, manifest=tmp_path / "m.json")


def test_publish_refuses_a_set_that_isnt_ready(tmp_path):
    rig = tmp_path / "rig"
    make_set(rig, "cmu", commit="abc1234+changes")
    with pytest.raises(SystemExit, match="uncommitted"):
        P.publish(["cmu"], tmp_path / "out", rig=rig, manifest=tmp_path / "m.json")
    make_set(rig, "cmu", crc=1)
    with pytest.raises(SystemExit, match="another rig"):
        P.publish(["cmu"], tmp_path / "out", rig=rig, manifest=tmp_path / "m.json")
    make_set(rig, "cmu")
    (rig / "cmu.quality.json").write_text(json.dumps({"takes": [{"take": "a", "frames": 30}]}))
    with pytest.raises(SystemExit, match="older than the set"):
        P.publish(["cmu"], tmp_path / "out", rig=rig, manifest=tmp_path / "m.json")


def test_fetch_checks_every_file_and_never_overwrites_yours(tmp_path):
    rig, here = tmp_path / "rig", tmp_path / "here"
    make_set(rig, "cmu")
    m = P.publish(["cmu"], tmp_path / "out", rig=rig, manifest=tmp_path / "m.json")
    release = tmp_path / "out" / m["tag"]
    kw = {"source": release, "rig": here, "manifest": tmp_path / "m.json"}
    assert sorted(F.fetch(**kw)) == ["cmu.npz", "cmu.quality.json"]
    assert (here / "cmu.npz").read_bytes() == (rig / "cmu.npz").read_bytes()
    assert F.fetch(**kw) == []  # up to date
    (here / "cmu.quality.json").write_text("{}")  # built here: kept
    assert F.fetch(**kw) == []
    assert F.fetch(force=True, **kw) == ["cmu.quality.json"]
    # a corrupt download is refused and leaves the file here as it was
    (here / "cmu.npz").write_bytes(b"mine")
    (release / "cmu.npz").write_bytes(b"corrupt")
    with pytest.raises(SystemExit, match="doesn't match"):
        F.fetch(force=True, **kw)
    assert (here / "cmu.npz").read_bytes() == b"mine"
    assert not [p for p in here.iterdir() if p.is_dir()]  # no temporary folder left


def test_fetch_refuses_a_release_for_another_rig(tmp_path):
    (tmp_path / "m.json").write_text(json.dumps({"format": P.FORMAT, "version": P.VERSION, "tag": "t",
                                                 "rig_crc32": 1, "files": []}))
    with pytest.raises(SystemExit, match="another rig"):
        F.fetch(rig=tmp_path / "rig", manifest=tmp_path / "m.json")
    with pytest.raises(SystemExit, match="nothing has been published"):
        F.fetch(rig=tmp_path / "rig", manifest=tmp_path / "none.json")
