"""The export format round trip, and the committed model against the Rust port's goldens.

``crates/ac-ai/tests/golden/parts-net.json`` holds one forward pass of the committed network; the
Rust port is checked against it. Here the committed ``weights.bin`` is read back the way the app
reads it, loaded into the PyTorch network, and run on the same inputs: export, golden and port
then agree on one file. Needs torch (CPU is enough).
"""

import json

import numpy as np
import pytest

torch = pytest.importorskip("torch", reason="torch is not installed (uv sync, or pip install torch)")

from ac_train import ASSETS, REPO  # noqa: E402
from ac_train.export import read_weights, write_weights  # noqa: E402
from ac_train.model import BONES, Inbetweener  # noqa: E402

MODEL = ASSETS / "models" / "parts"
GOLDEN = REPO / "crates" / "ac-ai" / "tests" / "golden" / "parts-net.json"


def test_main_is_written_only_after_the_export_and_only_beside_it(tmp_path, monkeypatch):
    from ac_train import export as module

    done = []
    monkeypatch.setattr(module, "export", lambda *a: done.append(a))
    out = tmp_path / "models" / "my-run"
    out.mkdir(parents=True)
    module.main(["my-run", "--checkpoint", "best", "--out", str(out), "--golden", str(tmp_path / "g"), "--main"])
    assert len(done) == 1
    assert json.loads((out.parent / "main.json").read_text(encoding="utf-8")) == {"tag": "my-run"}
    assert sorted(p.name for p in out.parent.iterdir()) == ["main.json", "my-run"]

    # a failed export chooses nothing
    (out.parent / "main.json").unlink()

    def fail(*a):
        raise SystemExit("refused")

    monkeypatch.setattr(module, "export", fail)
    with pytest.raises(SystemExit, match="refused"):
        module.main(["my-run", "--out", str(out), "--golden", str(tmp_path / "g"), "--main"])
    assert not (out.parent / "main.json").exists()

    # the choice names the folder the app looks in: never the repo's assets by default
    for args in (["my-run", "--main"], ["my-run", "--out", str(tmp_path / "elsewhere"), "--main"]):
        with pytest.raises(SystemExit):
            module.main(args)
        assert not (tmp_path / "main.json").exists()


def test_weights_round_trip(tmp_path):
    torch.manual_seed(0)
    net = Inbetweener(styles=3, given=True)
    n = write_weights(net.state_dict(), {"tag": "t", "styles": ["a", "b", "c"]}, tmp_path)
    back, card = read_weights(tmp_path)
    assert card["tag"] == "t"
    assert sum(a.size for a in back.values()) == n
    for name, t in net.state_dict().items():
        assert np.array_equal(back[name], t.numpy().astype(np.float32)), name


def test_committed_model_matches_the_golden():
    tensors, card = read_weights(MODEL)
    net = Inbetweener(styles=len(card.get("styles", [])), given=bool(card.get("given")),
                      contacts=bool(card.get("contacts")), path=bool(card.get("path")),
                      context_after=int(card.get("context_after", 0)), multi_key=bool(card.get("multi_key", False)))
    net.load_state_dict({k: torch.from_numpy(v.copy()) for k, v in tensors.items()})
    net.eval()
    g = json.loads(GOLDEN.read_text(encoding="utf-8"))
    T = g["T"]
    f = lambda k: torch.tensor([g[k]], dtype=torch.float32)  # noqa: E731
    mask = torch.tensor(g["mask"], dtype=torch.float32).reshape(1, T, 1)
    # one value a frame, or two on a multi_key model (to the next known frame, from the last)
    ttg = torch.tensor(g["ttg"], dtype=torch.float32).reshape(1, T, -1)
    style = torch.tensor([g["style"]]) if g["style"] is not None else None
    gmask = None
    if card.get("given"):
        gmask = torch.zeros(1, len(BONES))
        gmask[0, g["given"]] = 1
    # a model with the root path is given the golden's path on the gap frames, as the export was
    assert bool(card.get("path")) == ("path" in g), "the golden and the card disagree on the root path"
    path = f("path") if card.get("path") else None
    pmask = torch.tensor(g["pmask"], dtype=torch.float32).reshape(1, T, 1) if card.get("path") else None
    with torch.no_grad():
        y, contacts = net(f("known"), f("interp"), mask, ttg, style, gmask, with_contacts=True, path=path, pmask=pmask)
    worst = float((y[0] - torch.tensor(g["out"])).abs().max())
    assert worst < 1e-4, f"the committed weights give {worst:.2e} off the golden"
    # a model with the contact head pins its foot contacts too
    assert (contacts is not None) == ("contacts" in g), "the golden and the card disagree on foot contacts"
    if contacts is not None:
        worst = float((contacts[0] - torch.tensor(g["contacts"])).abs().max())
        assert worst < 1e-4, f"the committed weights give contacts {worst:.2e} off the golden"
