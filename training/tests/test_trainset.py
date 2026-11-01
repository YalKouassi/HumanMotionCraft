"""ac_train.trainset: a named set as a run's takes, its refusals and what the card says. Generated
clips only; nothing is trained."""

import json

import numpy as np
import pytest

from ac_train import datasets, selection as S
from ac_train import trainset as T
from ac_train.clip import Clip, Key
from ac_train.rig import Skeleton

FORMAT = "someapp.training-set"


def write_set(folder, clips, name="s", public=False, version=2):
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"{name}.set.json"
    path.write_text(json.dumps({"format": FORMAT, "version": 1, "name": name, "set_version": version, "public": public,
                                "clips": clips}), encoding="utf-8")
    return path


def keep(cid, labels=(), cuts=()):
    return {"id": cid, "status": "keep", "labels": list(labels), "cuts": [list(c) for c in cuts]}


def write_clip(library, group, name, frames, fps=30.0, fmt="someapp.clip", **extra):
    """A clip of the rig's rest pose travelling forward: two keys, ``frames`` long."""
    skel = Skeleton.load()
    bones = list(skel.names)
    rest = np.tile([0.0, 0.0, 0.0, 1.0], (len(bones), 1))
    clip = Clip(bones=bones, fps=fps, name=name, group=group)
    clip.set_key(Key(0, rest, np.zeros(3), "linear"))
    clip.set_key(Key(frames - 1, rest, np.array([0.0, -5.0, 0.0]), "linear"))
    d = {**clip.to_dict(), "format": fmt, **extra}
    folder = library / "animations" / group
    folder.mkdir(parents=True, exist_ok=True)
    (folder / f"{name}.clip.json").write_text(json.dumps(d), encoding="utf-8")


def rig_set(rig, name, takes):
    """A set in ``rig``: {take: frames}, the arrays the plan reads."""
    rig.mkdir(parents=True, exist_ok=True)
    frames = list(takes.values())
    np.savez(rig / f"{name}.npz", roots=np.zeros((sum(frames), 3)), starts=np.cumsum([0, *frames[:-1]]),
             takes=np.array(list(takes)), labels=np.array(list(takes)))


def test_a_clip_the_app_wrote_loads():
    skel = Skeleton.load()
    d = Clip(bones=list(skel.names), name="x").to_dict()
    assert Clip.from_dict({**d, "format": "someapp.clip"}).name == "x"
    assert Clip.from_dict(d).name == "x"  # older files
    with pytest.raises(ValueError, match="not a clip file"):
        Clip.from_dict({**d, "format": "someapp.pose"})


def test_the_file_is_checked(tmp_path):
    p = write_set(tmp_path, [keep("cmu/a", ["walk"], [(1, 2)])])
    ts = T.load(p)
    assert (ts.name, ts.version, ts.public) == ("s", 2, False)
    assert ts.clips[0] == T.SetClip("cmu/a", "keep", ("walk",), ((1, 2),))
    for bad, why in [
        ([{"id": "cmu/a", "status": "maybe"}], "status"),
        ([keep("no-slash")], "an id is"),
        ([keep("library:a/../b")], "an id is"),
        ([keep("library:../b")], "an id is"),
        ([keep("cmu/a", cuts=[(5, 2)])], "cuts"),
        ([keep("cmu/a"), keep("cmu/a")], "listed twice"),
    ]:
        with pytest.raises(SystemExit, match=why):
            T.load(write_set(tmp_path, bad))
    wrong = tmp_path / "other.set.json"
    wrong.write_text(p.read_text(encoding="utf-8"), encoding="utf-8")
    with pytest.raises(SystemExit, match="its name is"):
        T.load(wrong)


def test_the_hash_follows_the_content_not_the_layout(tmp_path):
    p = write_set(tmp_path, [keep("cmu/a")])
    first = T.load(p).hash
    d = json.loads(p.read_text(encoding="utf-8"))
    p.write_text(json.dumps(dict(reversed(list(d.items()))), indent=3), encoding="utf-8")
    assert T.load(p).hash == first
    assert T.load(write_set(tmp_path, [keep("cmu/a", ["walk"])])).hash != first


def test_a_library_clip_is_refused_inside_a_checkout(tmp_path):
    (tmp_path / ".git").mkdir()
    folder = tmp_path / "deep" / "sets"
    with pytest.raises(SystemExit, match="inside a git checkout"):
        T.load(write_set(folder, [keep("library:Pack - Act/Clip")]))
    assert T.load(write_set(folder, [keep("cmu/a")])).name == "s"  # a rig take is no secret
    (folder / "sources.json").write_text(json.dumps({"Pack": "mine"}), encoding="utf-8")
    with pytest.raises(SystemExit, match="inside a git checkout"):
        T.load(write_set(folder, [keep("cmu/a")]))
    (folder / "sources.json").write_text(json.dumps({"cmu": "mine"}), encoding="utf-8")
    assert T.load(write_set(folder, [keep("cmu/a")])).sources == {"cmu": "mine"}


def test_sources_unmarked_counts_as_purchased():
    marks = {"Pack": "mine", "Long - Name": "mine", "Bought": "purchased"}
    assert T.source_of("library:Pack - Walk/a", marks) == "mine"
    assert T.source_of("library:Long - Name - Walk/a", marks) == "mine"  # the longest mark
    assert T.pack_of("library:Long - Name - Walk/a") == "Long"  # no marks to go by
    assert T.source_of("library:Bought - Run/a", marks) == "purchased"
    assert T.source_of("library:Other - Run/a", marks) == "purchased"
    assert T.source_of("cmu/a", {}) == "free"
    assert T.source_of("myclips/a", {}) == "purchased"
    assert T.source_of("myclips/a", {"myclips": "mine"}) == "mine"


@pytest.mark.parametrize("marks", [{}, {"Pack": "purchased"}], ids=["unmarked", "purchased"])
def test_a_public_set_with_a_purchased_or_unmarked_clip_is_refused(tmp_path, marks):
    write_set(tmp_path, [keep("library:Pack - Act/Clip")], public=True)
    (tmp_path / "sources.json").write_text(json.dumps(marks), encoding="utf-8")
    with pytest.raises(SystemExit, match="is public and keeps 1 clips"):
        T.prepare("s", tmp_path)


def test_a_public_set_passes_on_free_and_mine_and_a_rejected_clip_does_not_count(tmp_path):
    library = tmp_path / "lib"
    write_clip(library, "Pack - Act", "Clip", 90)
    write_set(tmp_path, [keep("cmu/a"), keep("library:Pack - Act/Clip"), {"id": "library:Other - X/Y", "status": "reject"}], public=True)
    (tmp_path / "sources.json").write_text(json.dumps({"Pack": "mine"}), encoding="utf-8")
    named = T.prepare("s", tmp_path, library, scanned=False)
    assert named.names == ["cmu", "library"]
    assert named.select.include == {"cmu/a", "library:Pack - Act/Clip"}
    assert not named.select.exclude  # the rejected library clip is no id of a read set


def test_a_set_that_keeps_nothing_is_refused_or_it_would_read_every_take(tmp_path):
    write_set(tmp_path, [{"id": "cmu/a", "status": "reject"}])
    with pytest.raises(SystemExit, match="keeps no clips"):
        T.prepare("s", tmp_path)


def test_a_noncommercial_set_needs_its_flag_and_is_never_public(tmp_path, monkeypatch):
    monkeypatch.setitem(T.REGISTRY, "nc", datasets.clips_set("nc", "x"))
    write_set(tmp_path, [keep("nc/a")])
    with pytest.raises(SystemExit, match="--noncommercial"):
        T.prepare("s", tmp_path)
    assert T.prepare("s", tmp_path, noncommercial=True).names == ["nc"]
    write_set(tmp_path, [keep("nc/a")], public=True)
    (tmp_path / "sources.json").write_text(json.dumps({"nc": "mine"}), encoding="utf-8")
    with pytest.raises(SystemExit, match="is public"):
        T.prepare("s", tmp_path, noncommercial=True)


def test_only_the_clips_the_set_keeps_are_opened(tmp_path, monkeypatch):
    library = tmp_path / "lib"
    write_clip(library, "A - B", "Named", 90)
    write_clip(library, "A - B", "Other", 90)
    write_clip(library, "A - B", "Rejected", 90)
    write_set(tmp_path, [keep("library:A - B/Named"), {"id": "library:A - B/Rejected", "status": "reject"}])
    opened = []
    real = Clip.load
    monkeypatch.setattr(T.Clip, "load", classmethod(lambda cls, path: (opened.append(path.name), real(path))[1]))
    named = T.prepare("s", tmp_path, library, scanned=False)
    assert opened == ["Named.clip.json"]
    assert list(named.extra["library"]["takes"]) == ["A - B/Named"]


def test_a_library_clip_it_cannot_train_on_stops_the_run(tmp_path):
    library = tmp_path / "lib"
    write_clip(library, "G", "Eval", 90, evaluation_only=True)
    write_clip(library, "G", "Rig", 90, bones=["one"], keys=[])
    write_set(tmp_path, [keep("library:G/Eval")])
    with pytest.raises(SystemExit, match="give --library"):
        T.prepare("s", tmp_path)
    with pytest.raises(SystemExit, match="evaluation-only"):
        T.prepare("s", tmp_path, library)
    write_set(tmp_path, [keep("library:G/Rig")])
    with pytest.raises(SystemExit, match="another rig"):
        T.prepare("s", tmp_path, library)
    write_set(tmp_path, [keep("library:G/Missing")])
    with pytest.raises(SystemExit, match="can't read it"):
        T.prepare("s", tmp_path, library)


def test_cuts_stay_cut_when_a_clip_is_read_at_30_fps():
    assert T.cut_frames([(30, 60)], 60.0) == ((15, 30),)
    # 24 fps: 2 and 3 are 2.5 and 3.75 at 30, rounded outwards to 2 and 4
    assert T.cut_frames([(2, 3)], 24.0) == ((2, 4),)
    assert T.cut_frames([(2, 3)], 30.0) == ((2, 3),)


def test_the_takes_and_minutes_of_a_mixed_set(tmp_path, monkeypatch):
    """Hand-worked, at 1800 frames a minute:
    - Lib/Long, 1800 frames, labels jump: 1.00 min, type jump;
    - Lib/Cut, 900 frames, cut 0..449 (450 frames): 450 left, 0.25 min;
    - rig/walk_a, 900 frames, no cuts: 0.50 min; rig/walk_b is rejected; rig/walk_c isn't named.
    Read: 3 of the 4 clips, 1.00 + 0.25 + 0.50 = 1.75 min."""
    monkeypatch.setattr(S, "RIG", tmp_path / "rig")
    rig_set(tmp_path / "rig", "rigset", {"walk_a": 900, "walk_b": 900, "walk_c": 900})
    library = tmp_path / "lib"
    write_clip(library, "Lib", "Long", 1800)
    write_clip(library, "Lib", "Cut", 900)
    write_set(tmp_path, [keep("library:Lib/Long", ["jump", "Heavy"]), keep("library:Lib/Cut", cuts=[(0, 449)]),
                         keep("rigset/walk_a"), {"id": "rigset/walk_b", "status": "reject"}])
    named = T.prepare("s", tmp_path, library, scanned=False)
    recs = S.plan_sets(named.names, {}, named.select, scanned=False, extra=named.extra)
    named.check(recs)
    used = {r.id: r.used for r in recs if r.kept}
    assert used == {"library:Lib/Long": 1800, "library:Lib/Cut": 450, "rigset/walk_a": 900}
    by_id = {r.id: r for r in recs}
    assert by_id["library:Lib/Long"].type == "jump" and by_id["library:Lib/Long"].styles == ["heavy"]
    assert by_id["rigset/walk_b"].why == "list: excluded" and by_id["rigset/walk_c"].why == "list: not included"
    text = named.listing(recs)
    assert "library:Lib/Long  1.00 min  purchased  [jump, heavy]  kept" in text
    assert "library:Lib/Cut  0.25 min" in text and "rigset/walk_a  0.50 min" in text
    assert "3 of 4 clips read, 1.75 min" in text

    card = named.card(recs)
    assert card["name"] == "s" and card["set_version"] == 2 and card["hash"] == named.set.hash and card["public"] is False
    assert (card["clips"], card["clips_read"], card["minutes"], card["library_clips"]) == (4, 3, 1.75, 2)
    assert card["sources"] == {"purchased": 3}  # rigset is no registry set and unmarked
    assert "Lib" not in json.dumps(card), "a card may be exported: it names no clip or pack"
    rows = {c["id"]: c for c in named.clips(recs)["clips"]}
    assert rows["library:Lib/Cut"]["minutes"] == 0.25 and rows["rigset/walk_b"]["minutes"] == 0.0
    assert not any(r.set == "rigset" and r.kept and r.take == "walk_c" for r in recs)


def test_a_kept_clip_of_a_set_not_built_is_an_error(tmp_path, monkeypatch):
    monkeypatch.setattr(S, "RIG", tmp_path / "rig")
    write_set(tmp_path, [keep("rigset/walk_a")])
    named = T.prepare("s", tmp_path)
    recs = S.plan_sets(named.names, {}, named.select, scanned=False)
    with pytest.raises(SystemExit, match="1 kept clips"):
        named.check(recs)


def test_library_clips_are_held_out_by_pack_and_the_card_does_not_name_them():
    from ac_train import split

    assert split.group_of("library", "Pack - Walk/a") == split.group_of("library", "Pack - Run/b") == "Pack"
    assert T.public_held({"cmu": ["02"], "library": ["Pack"]}) == {"cmu": ["02"]}


def test_a_model_of_a_private_or_purchased_set_stays_out_of_the_repo():
    assert T.repo_refusal({"commercial": True}) == ""
    assert T.repo_refusal({"training_set": {"name": "s", "public": True, "sources": {"free": 3, "mine": 1}}}) == ""
    assert "private set s" in T.repo_refusal({"training_set": {"name": "s", "public": False, "sources": {"free": 3}}})
    assert "1 purchased" in T.repo_refusal({"training_set": {"name": "s", "public": True, "sources": {"purchased": 1}}})


def test_export_refuses_such_a_model_before_writing(tmp_path, monkeypatch):
    pytest.importorskip("torch")
    from ac_train import REPO, export

    card = {"commercial": False, "training_set": {"name": "s", "public": False, "sources": {"purchased": 1}}}
    monkeypatch.setattr(export.infer, "load", lambda tag, checkpoint: (None, card))
    with pytest.raises(SystemExit, match="stays out of the repo"):
        export.export("t", tmp_path / "w", REPO / "target" / "never")
    assert not (tmp_path / "w").exists()


def test_train_dry_run_prints_the_set_and_trains_nothing(tmp_path, monkeypatch, capsys):
    """One clip, 1800 frames: 1.00 min. A refusal stops the dry run too."""
    pytest.importorskip("torch")
    from ac_train import train

    monkeypatch.setattr(train, "MODELS", tmp_path / "models")
    library = tmp_path / "lib"
    write_clip(library, "Pack - Act", "Clip", 1800)
    write_set(tmp_path, [keep("library:Pack - Act/Clip", ["walk"])])
    args = ["--set", "s", "--set-folder", str(tmp_path), "--library", str(library), "--dry-run", "--unscanned",
            "--decisions", str(tmp_path / "none.json")]
    train.main(args)
    out = capsys.readouterr().out
    assert "library:Pack - Act/Clip  1.00 min  purchased  [walk]  kept" in out
    assert "1 of 1 clips read, 1.00 min" in out and "--dry-run: nothing trained" in out
    assert not (tmp_path / "models").exists()
    write_set(tmp_path, [keep("library:Pack - Act/Clip")], public=True)
    with pytest.raises(SystemExit, match="is public"):
        train.main(args)
    with pytest.raises(SystemExit):
        train.main([*args, "--natural"])  # --set says which takes
