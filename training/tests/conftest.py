"""Tests for ``ac_train``. Run from ``training/``: ``uv run --with pytest python -m pytest``.

They need numpy and the committed assets only. Tests that need torch skip without it; tests that
need the gitignored datasets (``training/data``) skip without them, and say so (``-rs``).
"""

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
