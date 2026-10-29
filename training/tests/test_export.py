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
    net = Inbetweener(styles=len(card.get("styles", [])), given=bool(card.get("given")))
    net.load_state_dict({k: torch.from_numpy(v.copy()) for k, v in tensors.items()})
    net.eval()
    g = json.loads(GOLDEN.read_text(encoding="utf-8"))
    T = g["T"]
    f = lambda k: torch.tensor([g[k]], dtype=torch.float32)  # noqa: E731
    mask = torch.tensor(g["mask"], dtype=torch.float32).reshape(1, T, 1)
    ttg = torch.tensor(g["ttg"], dtype=torch.float32).reshape(1, T, 1)
    style = torch.tensor([g["style"]]) if g["style"] is not None else None
    gmask = None
    if card.get("given"):
        gmask = torch.zeros(1, len(BONES))
        gmask[0, g["given"]] = 1
    with torch.no_grad():
        y = net(f("known"), f("interp"), mask, ttg, style, gmask)
    worst = float((y[0] - torch.tensor(g["out"])).abs().max())
    assert worst < 1e-4, f"the committed weights give {worst:.2e} off the golden"
