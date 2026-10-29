"""Quaternion helpers (x, y, z, w; Hamilton product), checked on cases worked by hand."""

import math

import numpy as np

from ac_train import quat

Z90 = np.array([0.0, 0.0, math.sin(math.pi / 4), math.cos(math.pi / 4)])  # 90 deg about Z
X90 = np.array([math.sin(math.pi / 4), 0.0, 0.0, math.cos(math.pi / 4)])


def rotate(q, v):
    """v rotated by q: q v q*."""
    p = np.concatenate([v, [0.0]])
    return quat.mul(quat.mul(q, p), quat.conj(q))[:3]


def test_identity_and_inverse():
    q = quat.normalize(np.array([0.3, -0.2, 0.5, 0.8]))
    assert np.allclose(quat.mul(quat.IDENTITY, q), q)
    assert np.allclose(quat.mul(q, quat.conj(q)), quat.IDENTITY, atol=1e-12)


def test_rotation_by_hand():
    # 90 deg about Z takes +X to +Y; 90 deg about X takes +Y to +Z
    assert np.allclose(rotate(Z90, np.array([1.0, 0.0, 0.0])), [0.0, 1.0, 0.0], atol=1e-12)
    assert np.allclose(rotate(X90, np.array([0.0, 1.0, 0.0])), [0.0, 0.0, 1.0], atol=1e-12)
    # products compose right to left: first Z90, then X90 takes +X to +Y, then to +Z
    assert np.allclose(rotate(quat.mul(X90, Z90), np.array([1.0, 0.0, 0.0])), [0.0, 0.0, 1.0], atol=1e-12)


def test_slerp_ends_and_middle():
    assert np.allclose(quat.slerp(quat.IDENTITY, Z90, 0.0), quat.IDENTITY)
    assert np.allclose(quat.slerp(quat.IDENTITY, Z90, 1.0), Z90)
    half = np.array([0.0, 0.0, math.sin(math.pi / 8), math.cos(math.pi / 8)])  # 45 deg about Z
    assert np.allclose(quat.slerp(quat.IDENTITY, Z90, 0.5), half, atol=1e-12)


def test_log_exp_round_trip():
    rng = np.random.default_rng(0)
    q = quat.normalize(rng.normal(size=(64, 4)))
    q = q * np.sign(q[:, 3:])  # one hemisphere: log is the short way round
    assert np.allclose(quat.exp(quat.log(q)), q, atol=1e-10)
    # log of 90 deg about Z is half the rotation vector: (0, 0, pi/4)
    assert np.allclose(quat.log(Z90), [0.0, 0.0, math.pi / 4], atol=1e-12)


def test_align_picks_the_near_hemisphere():
    assert np.allclose(quat.align(-Z90, Z90), Z90)
    assert np.allclose(quat.align(Z90, Z90), Z90)


def test_euler_round_trip():
    rng = np.random.default_rng(1)
    q = quat.normalize(rng.normal(size=(32, 4)))
    back = quat.from_euler(quat.to_euler(q))
    assert np.allclose(np.abs(np.sum(back * q, -1)), 1.0, atol=1e-9)  # same rotation (q or -q)
