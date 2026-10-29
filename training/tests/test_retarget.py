"""Retarget sanity: the rig's own motion, read back as landmarks, retargets onto the rig again.

A built-in CC0 loop is sampled, its joints placed by forward kinematics, and those positions
handed to the retarget as if they were mocap. Same skeleton, so the limbs must land where they
were: the retarget aims each segment and solves the arms and legs in their bend planes with the
rig's own lengths, which here are the source's.
"""

import numpy as np
import pytest

from ac_train import ASSETS
from ac_train.clip import Clip
from ac_train.retarget import LANDMARKS, retarget, scale_to
from ac_train.rig import Skeleton

# landmark -> the rig joint (bone head) that stands for it
JOINT = {
    "hips": "root", "neck": "neck01", "head": "head", "spine": "spine05",
    **{f"{c}_{k}": f"{b}.{s}" for s, c in (("L", "l"), ("R", "r")) for k, b in (
        ("shoulder", "upperarm01"), ("elbow", "lowerarm01"), ("wrist", "wrist"),
        ("hip", "upperleg01"), ("knee", "lowerleg01"), ("ankle", "foot"), ("toe", "toe3-1"))},
}
CLIP = "Locomotion/Walk"


@pytest.fixture(scope="module")
def skel():
    return Skeleton.load()


def joints(skel, rots, roots):
    return np.stack([skel.fk(r, o)[1] for r, o in zip(rots, roots)])


def test_rest_pose_is_the_rest_heads(skel):
    rots = np.tile([0.0, 0.0, 0.0, 1.0], (len(skel.names), 1))
    _, P = skel.fk(rots, np.zeros(3))
    assert np.allclose(P, skel.heads)


def test_the_rigs_own_walk_retargets_onto_itself(skel):
    clip = Clip.load(ASSETS / "library" / "animations" / f"{CLIP}.clip.json")
    rots, roots = clip.sample_all()
    P = joints(skel, rots, roots)
    src = {k: P[:, skel.index[JOINT[k]]] for k in (*LANDMARKS, "spine")}
    scaled = scale_to(skel, src)
    # the same legs: nothing to scale
    assert np.allclose(scaled["hips"], src["hips"], atol=1e-9)
    r2, o2 = retarget(skel, scaled)
    assert r2.shape == rots.shape and o2.shape == roots.shape
    assert np.allclose(np.linalg.norm(r2, axis=-1), 1.0, atol=1e-9)
    Q = joints(skel, r2, o2)
    miss = {k: np.linalg.norm(Q[:, skel.index[JOINT[k]]] - src[k], axis=-1).max() for k in LANDMARKS}
    # the two-bone solve reaches the ends exactly (same lengths); the feet aim at the toes
    for k in ("l_wrist", "r_wrist", "l_ankle", "r_ankle", "l_toe", "r_toe"):
        assert miss[k] < 1e-3, f"{k} lands {miss[k] * 100:.2f} cm off"
    # the rest is approximate by design (the spine bends evenly, a near-straight limb's bend plane
    # is interpolated in time): a few cm, never a flipped limb
    for k, m in miss.items():
        assert m < 0.05, f"{k} lands {m * 100:.2f} cm off"


def test_a_longer_arm_bends_the_rigs_as_much(skel):
    """An actor with longer arms (the legs set the scale): the rig's arm reaches in proportion, so
    it bends as the actor's does instead of locking straight short of the wrist (ARM_REACH)."""
    clip = Clip.load(ASSETS / "library" / "animations" / f"{CLIP}.clip.json")
    rots, roots = clip.sample_all()
    P = joints(skel, rots, roots)
    src = {k: P[:, skel.index[JOINT[k]]] for k in (*LANDMARKS, "spine")}
    long = dict(src)
    for c in ("l", "r"):  # the same arm, 10% longer about the shoulder: the same bend
        for k in ("elbow", "wrist"):
            long[f"{c}_{k}"] = src[f"{c}_shoulder"] + 1.1 * (src[f"{c}_{k}"] - src[f"{c}_shoulder"])
    r2, o2 = retarget(skel, scale_to(skel, long))
    Q = joints(skel, r2, o2)
    # the wrist where the rig's own arm put it (locked straight it fell 1.7 cm short); the elbow as
    # close as the same-length retarget gets it (3.8 cm on near-straight frames; 6.2 cm before)
    for k, most in (("l_wrist", 0.002), ("r_wrist", 0.002), ("l_elbow", 0.045), ("r_elbow", 0.045)):
        miss = np.linalg.norm(Q[:, skel.index[JOINT[k]]] - src[k], axis=-1).max()
        assert miss < most, f"{k} lands {miss * 100:.2f} cm off the actor's bend"
