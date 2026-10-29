"""ac_train.anatomy: the angles behind assets/rig/ranges.json (ac_core::anatomy has the same maths;
crates/ac-core/tests/ranges.rs checks the two agree on the file's sample poses)."""

import numpy as np

from ac_train import anatomy, rig


def rest(skel):
    return np.tile(np.array([0.0, 0.0, 0.0, 1.0]), (1, len(skel.names), 1))


def test_rest_pose_reads_the_rigs_a_pose():
    skel = rig.Skeleton.load()
    d = skel.heads[skel.index["lowerarm01.L"]] - skel.heads[skel.index["upperarm01.L"]]
    d = d / np.linalg.norm(d)
    angle = np.degrees(np.arccos(-d[2]))
    h = np.hypot(d[0], d[1])
    for side in ("L", "R"):
        a = anatomy.Joint(skel, "shoulder", side).angles(skel, rest(skel))[0]
        assert abs(a[0] - angle * -d[1] / h) < 1e-9
        assert abs(a[1] - angle * d[0] / h) < 1e-9
    for joint in ("ankle", "trunk", "neck"):
        assert np.allclose(anatomy.Joint(skel, joint).angles(skel, rest(skel)), 0.0, atol=1e-9)


def test_chest_bend_is_trunk_flexion():
    skel = rig.Skeleton.load()
    pose = rest(skel).copy()
    h = np.radians(10.0)
    pose[0, skel.index["spine01"]] = [np.sin(h), 0.0, 0.0, np.cos(h)]
    a = anatomy.Joint(skel, "trunk").angles(skel, pose)[0]
    assert np.allclose(a, [20.0, 0.0, 0.0], atol=1e-9)
