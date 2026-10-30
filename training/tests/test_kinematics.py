"""ac_train.kinematics: the torch body measurements agree with the numpy rig and anatomy."""

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from ac_train import anatomy, kinematics, rig  # noqa: E402
from ac_train.model import BONES, quat_to_mat  # noqa: E402


def random_pose(skel, n, seed=0, spread=0.6):
    """n poses: the solved bones turned at random, every other bone at rest (as the data has them)."""
    rng = np.random.default_rng(seed)
    q = np.tile([0.0, 0.0, 0.0, 1.0], (n, len(skel.names), 1))
    for b in BONES:
        v = rng.normal(size=(n, 3)) * spread
        a = np.linalg.norm(v, axis=1, keepdims=True)
        q[:, skel.index[b]] = np.concatenate([v / a * np.sin(a / 2), np.cos(a / 2)], 1)
    root = rng.normal(size=(n, 3)) * 0.2
    return q, root


def test_points_match_the_rigs_fk():
    skel = rig.Skeleton.load()
    body = kinematics.Body(skel)
    q, root = random_pose(skel, 4)
    idx = [skel.index[b] for b in BONES]
    local = quat_to_mat(torch.tensor(q[:, idx], dtype=torch.float32))
    pts = body.points(local, torch.tensor(root * skel.hip_height, dtype=torch.float32)).numpy()
    for f in range(len(q)):
        _, P = skel.fk(q[f], root[f])
        want = np.array([P[skel.index[n]] for n in kinematics.POINTS]) - skel.heads[0]
        assert np.allclose(pts[f], want, atol=1e-5)


def test_angles_match_anatomy():
    skel = rig.Skeleton.load()
    body = kinematics.Body(skel)
    q, _ = random_pose(skel, 64, seed=3)
    idx = [skel.index[b] for b in BONES]
    world = body.world(quat_to_mat(torch.tensor(q[:, idx], dtype=torch.float32)))
    got = body.angles(world)
    for (joint, side), v in got.items():
        want = anatomy.Joint(skel, joint, side).angles(skel, q)
        # twist wraps at +-180: compare on the circle
        d = (v.numpy() - want + 180.0) % 360.0 - 180.0
        assert np.abs(d).max() < 0.05, (joint, side)


def test_rest_is_possible_and_a_reversed_knee_is_not():
    skel = rig.Skeleton.load()
    body = kinematics.Body(skel)
    J = len(BONES)
    local = torch.eye(3).expand(2, J, 3, 3).clone()
    k = BONES.index("lowerleg01.L")
    a = torch.tensor(np.radians(-60.0))  # the shin swung forward past straight: the knee bent backwards
    local[1, k] = torch.tensor([[1.0, 0, 0], [0, torch.cos(a), -torch.sin(a)], [0, torch.sin(a), torch.cos(a)]])
    bad, over = body.impossible(body.world(local), hinge=True)
    assert not bad[0] and bad[1]
    assert over[0] == 0 and over[1] > 0


def test_planted_needs_a_flat_still_foot():
    skel = rig.Skeleton.load()
    body = kinematics.Body(skel)
    J = len(BONES)
    lift = skel.hip_height - skel.heads[0][2]
    local = torch.eye(3).expand(1, 4, J, 3, 3)
    root = torch.tensor([[[0.0, 0, lift]] * 4], dtype=torch.float32)
    root[0, 3, 0] = 0.1  # the last frame slides 10 cm: 3 m/s
    pts = body.points(local, root)
    assert torch.allclose(body.foot_heights(pts), torch.zeros(1, 4, 4), atol=1e-5)
    p = body.planted(pts)[0]
    assert p.tolist() == [[False, False], [True, True], [True, True], [False, False]]
