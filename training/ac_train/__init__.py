"""HumanMotionCraft's training: retarget mocap onto the rig, train the in-betweener, export it, and
cut the built-in animation library. Python, and never a runtime dependency of the app.

Data lives under ``training/data`` (gitignored): ``raw/`` downloads, ``rig/`` retargeted sets,
``models/`` PyTorch checkpoints, ``library/`` clips that may not be committed. See README.md.
"""

import os
import pathlib

REPO = pathlib.Path(__file__).resolve().parents[2]
ASSETS = REPO / "assets"
DATA = REPO / "training" / "data"
# where retargeted sets (rig/) and checkpoints (models/) are written and read: training/data, or
# the folder AC_TRAIN_OUT names (``python -m ac_train.pipeline --quick`` works in a temporary one
# and leaves training/data alone). Downloads are always read from training/data/raw.
OUT = pathlib.Path(os.environ["AC_TRAIN_OUT"]) if os.environ.get("AC_TRAIN_OUT") else DATA
MODELS = OUT / "models"
