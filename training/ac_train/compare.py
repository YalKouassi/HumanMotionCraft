"""Two or more model cards side by side: joint error and foot slide by gap bin and by motion type.

    python -m ac_train.compare --card parts-natural:best:natural --card parts-lowweights:best:natural
    python -m ac_train.compare --card parts-natural:best --card parts-lowweights:best

A card is ``<tag>[:<checkpoint>[:<subset>]]``: ``training/data/models/<tag>/model.json``'s
``evaluation`` (``model``) or ``evaluation_best`` (``best``), with ``_<subset>`` when the
evaluation was restricted (``ac_train.evaluate --subset``, roadmap 153000). Every row gives each
card's model number beside the interpolation's, the way the cards do, so a model that only beats
the interpolation where the interpolation is easy shows. Nothing is written.
"""

from __future__ import annotations

import argparse
import json
import pathlib

from . import MODELS

METRICS = (("joint_m", "joint mm", 1000.0), ("slide_ms", "slide m/s", 1.0))


def load(spec: str, models: pathlib.Path = MODELS) -> tuple[str, dict]:
    """``tag[:checkpoint[:subset]]`` -> (name, its evaluation block); a card or a block that is not
    there is an error naming the command that writes it."""
    parts = spec.split(":")
    tag, ck, subset = parts[0], parts[1] if len(parts) > 1 else "model", parts[2] if len(parts) > 2 else ""
    path = models / tag / "model.json"
    if not path.exists():
        raise SystemExit(f"{path} is missing: no model of that tag")
    card = json.loads(path.read_text(encoding="utf-8"))
    key = ("evaluation_best" if ck == "best" else "evaluation") + (f"_{subset}" if subset else "")
    ev = card.get(key)
    if not ev:
        raise SystemExit(f"{path} has no {key}: run python -m ac_train.evaluate --tag {tag}"
                         + (" --checkpoint best" if ck == "best" else "") + (f" --subset {subset} <the filter>" if subset else ""))
    return f"{tag}:{ck}" + (f":{subset}" if subset else ""), ev


def rows_of(ev: dict) -> list[tuple[str, dict]]:
    out = [(f"gap {k}", r) for k, r in ev.get("bins", {}).items()]
    out.append(("all", ev.get("all", {})))
    out += [(f"type {k}", r) for k, r in sorted(ev.get("by_type", {}).items())]
    return out


def table(cards: list[tuple[str, dict]]) -> str:
    """One line a row (gap bins, all, motion types), one pair of columns a card and metric:
    the model's number / the interpolation's, and the row's windows."""
    names = [n for n, _ in cards]
    rows = []
    for _, ev in cards:
        for name, _ in rows_of(ev):
            if name not in rows:
                rows.append(name)
    width = 26
    head = f"{'':14}" + "".join(f"{n[:width - 2]:>{width}}" for n in names for _ in METRICS)
    sub = f"{'':14}" + "".join(f"{m[1] + ' (windows)':>{width}}" for _ in names for m in METRICS)
    lines = [head, sub]
    for row in rows:
        cells = []
        for _, ev in cards:
            r = dict(rows_of(ev)).get(row)
            for key, _, scale in METRICS:
                if not r or key not in r.get("model", {}):
                    cells.append(f"{'-':>{width}}")
                    continue
                m, i = r["model"][key] * scale, r["interp"][key] * scale
                cells.append(f"{m:.3g} / {i:.3g} ({r.get('windows', '?')})".rjust(width))
        lines.append(f"{row:14}" + "".join(cells))
    lines.append("each cell: the model's number / plain interpolation's (windows); lower is better")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Model cards side by side, by gap bin and motion type")
    ap.add_argument("--card", action="append", required=True, help="<tag>[:model|best[:<subset>]] (repeatable)")
    args = ap.parse_args(argv)
    print(table([load(c) for c in args.card]))


if __name__ == "__main__":
    main()
