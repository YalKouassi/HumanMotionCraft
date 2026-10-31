"""The optional sample clips' setup, without any sample clip: a made-up BVH with SOMA's joint names on
the real rig, and the checks on what the download holds and when it may run.

    uv run --with pytest --with numpy python -m pytest tools/posefill/tests/test_samples.py
"""

from __future__ import annotations

import pathlib
import sys

import pytest

HERE = pathlib.Path(__file__).resolve().parent
TOOL = HERE.parent
REPO = TOOL.parent.parent
sys.path.insert(0, str(TOOL))

import setup_samples  # noqa: E402

np = pytest.importorskip("numpy")


def test_only_soma_clips_and_their_licence_are_downloaded():
    assert len(setup_samples.CLIPS) == 38
    assert all(n.startswith("soma/") and n.endswith(".bvh") for n in setup_samples.CLIPS)
    assert set(setup_samples.TEXTS) == {"LICENSE.txt", "README.md"}


def test_no_sample_clip_is_in_the_repo_outside_the_ignored_reference_copy():
    tracked = [p for p in (REPO / "assets").rglob("*.bvh")] + [p for p in (REPO / "fixtures").rglob("*.bvh")]
    assert not tracked


def test_refuses_without_the_install(tmp_path):
    with pytest.raises(SystemExit, match="not installed"):
        setup_samples.main(["--dir", str(tmp_path / "samples"), "--accept"])


def test_refuses_without_acceptance(tmp_path):
    (tmp_path / "install.json").write_text("{}", encoding="utf-8")
    with pytest.raises(SystemExit, match="licence accepted first"):
        setup_samples.main(["--dir", str(tmp_path / "samples")])
    assert not (tmp_path / "samples" / "motions").exists()


def made_up_bvh(path: pathlib.Path, frames: int = 8) -> None:
    """A standing figure in centimetres, Y up, at 120 fps, with the joints the conversion reads."""
    joints = [("Hips", None, (0, 95, 0)), ("Spine1", "Hips", (0, 15, 0)), ("Neck1", "Spine1", (0, 40, 0)), ("Head", "Neck1", (0, 12, 0))]
    for s, x in (("Left", 1), ("Right", -1)):
        joints += [(f"{s}Arm", "Spine1", (x * 18, 32, 0)), (f"{s}ForeArm", f"{s}Arm", (x * 28, 0, 0)), (f"{s}Hand", f"{s}ForeArm", (x * 25, 0, 0)),
                   (f"{s}Leg", "Hips", (x * 9, -5, 0)), (f"{s}Shin", f"{s}Leg", (0, -42, 0)), (f"{s}Foot", f"{s}Shin", (0, -42, 0)), (f"{s}ToeBase", f"{s}Foot", (0, -6, 14))]
    kids: dict = {}
    for name, parent, _ in joints:
        kids.setdefault(parent, []).append(name)
    offsets = {n: o for n, _, o in joints}

    def write(name: str, depth: int) -> list[str]:
        pad = " " * depth
        head = "ROOT" if depth == 0 else "JOINT"
        out = [f"{pad}{head} {name}", pad + "{", f"{pad} OFFSET {offsets[name][0]} {offsets[name][1]} {offsets[name][2]}"]
        out.append(f"{pad} CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation" if depth == 0
                   else f"{pad} CHANNELS 3 Zrotation Yrotation Xrotation")
        for k in kids.get(name, []):
            out += write(k, depth + 1)
        if name not in kids:
            out += [f"{pad} End Site", pad + " {", f"{pad}  OFFSET 0 1 0", pad + " }"]
        return out + [pad + "}"]

    lines = ["HIERARCHY", *write("Hips", 0), "MOTION", f"Frames: {frames}", "Frame Time: 0.008333"]
    for f in range(frames):
        lines.append(" ".join(["0", "95", str(f * 0.5)] + ["0"] * (3 + 3 * (len(joints) - 1))))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def test_conversion_puts_a_made_up_clip_on_the_rig_at_30_fps(tmp_path):
    sys.path.insert(0, str(REPO / "training"))
    import ac_train.bvh
    import ac_train.quat
    import ac_train.retarget
    import ac_train.rig

    sys.modules["ac_rig"] = sys.modules["ac_train"]
    sys.modules["ac_rig.bvh"] = ac_train.bvh
    sys.modules["ac_rig.quat"] = ac_train.quat
    sys.modules["ac_rig.retarget"] = ac_train.retarget
    sys.modules["ac_rig.rig"] = ac_train.rig
    import convert_samples

    skel = ac_train.rig.Skeleton.load(REPO / "assets" / "rig" / "default_rig.json")
    f = tmp_path / "walk_001__A1.bvh"
    made_up_bvh(f)
    clip = convert_samples.convert(f, skel)
    assert clip["name"] == "walk_001__A1"
    assert clip["fps"] == pytest.approx(30.0, abs=0.01)
    assert len(clip["root"]) == 2  # 8 frames at 120 fps
    assert len(clip["rotations"][0]) == len(skel.names)
    assert np.isfinite(np.array(clip["rotations"])).all() and np.isfinite(np.array(clip["root"])).all()


def test_training_skips_a_marked_clip(tmp_path, capsys):
    sys.path.insert(0, str(REPO / "training"))
    import json

    from ac_train import datasets
    from ac_train.rig import Skeleton

    source = next((REPO / "assets" / "library" / "animations").glob("*/Walk 12.clip.json"))
    d = json.loads(source.read_text(encoding="utf-8"))
    d["evaluation_only"] = True
    (tmp_path / "marked.clip.json").write_text(json.dumps(d), encoding="utf-8")
    skel = Skeleton.load(REPO / "assets" / "rig" / "default_rig.json")
    takes = list(datasets._clip_takes(datasets.clips_set("t", tmp_path), skel, None))
    assert takes == []
    assert "evaluation-only" in capsys.readouterr().out
