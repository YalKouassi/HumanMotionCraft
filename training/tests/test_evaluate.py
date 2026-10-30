"""ac_train.evaluate: the metrics on a tiny made-up window, with the numbers worked by hand.

The figure stands at rest, feet flat on the floor, on frames 0..4; key A is frame 1, key B frame 4,
so the gap is frames 2 and 3. The "fill" steps the whole body 0.1 m along X on both gap frames.
"""

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from ac_train import evaluate, rig  # noqa: E402
from ac_train.kinematics import Body  # noqa: E402
from ac_train.model import BONES, features  # noqa: E402

A, B, T = 1, 4, 5


def window(skel, dx=(0.0,) * T, dz=(0.0,) * T, dy=None):
    lift = skel.hip_height - skel.heads[0][2]
    dy = dy or (0.0,) * len(dx)
    local = torch.eye(3).expand(1, len(dx), len(BONES), 3, 3)
    pos = torch.tensor([[[x, y, lift + z] for x, y, z in zip(dx, dy, dz)]], dtype=torch.float32)
    return features(local, pos, skel.hip_height)


def poses(skel, body, **kw):
    return evaluate.Pose(window(skel, **kw), body)


def scenes(skel, body, a=A, b=B, **kw):
    return {k: bool(v[0]) for k, v in evaluate.scenarios(poses(skel, body, **kw), body, a, b).items()}


def metrics(pred, truth, body):
    m = evaluate.measure(evaluate.Pose(pred, body), evaluate.Pose(truth, body), body, A, B)
    return {k: float(v[0]) if v.dim() == 1 else v for k, (v, _) in m.items()}


def test_a_fill_that_steps_off_the_planted_feet():
    skel = rig.Skeleton.load()
    body = Body(skel)
    truth = window(skel)
    pred = window(skel, dx=(0.0, 0.0, 0.1, 0.1, 0.0))
    m = metrics(pred, truth, body)
    # every point is 0.1 m off on both gap frames; the mid-gap frame (2) too
    assert m["joint_m"] == pytest.approx(0.1, abs=1e-6)
    assert m["end_m"] == pytest.approx(0.1, abs=1e-6)
    assert m["root_mid_m"] == pytest.approx(0.1, abs=1e-6)
    assert m["rot_deg"] == pytest.approx(0.0, abs=1e-3)
    # the recorded feet are planted on both gap frames; the fill's move 0.1 m into frame 2 (3 m/s)
    # and stay on frame 3 (0 m/s): 1.5 m/s on average, half the frames over the 0.5 m/s skate limit
    assert m["slide_ms"] == pytest.approx(1.5, abs=1e-4)
    assert m["skate_share"] == pytest.approx(0.5)
    assert m["floor_m"] == pytest.approx(0.0, abs=1e-6) and m["sink_share"] == 0.0
    # leaving A: 0.1 m in a frame against 0 (3 m/s); arriving at B: -0.1 m in a frame (3 m/s)
    assert m["seam_a_ms"] == pytest.approx(3.0, abs=1e-4)
    assert m["seam_b_ms"] == pytest.approx(3.0, abs=1e-4)
    # X over frames 0..4 is 0, 0, .1, .1, 0: third differences -0.2 and 0, mean 0.1 m a frame^3
    assert m["jerk_ms3"] == pytest.approx(0.1 * 30**3, rel=1e-4)
    assert m["impossible_per_1000"] == 0.0


def test_a_fill_that_sinks():
    skel = rig.Skeleton.load()
    body = Body(skel)
    truth = window(skel)
    pred = window(skel, dz=(0.0, 0.0, -0.06, -0.03, 0.0))
    m = metrics(pred, truth, body)
    # 6 and 3 cm under the floor: 4.5 cm on average, one frame past the 5 cm sink limit
    assert m["floor_m"] == pytest.approx(0.045, abs=1e-5)
    assert m["sink_share"] == pytest.approx(0.5)
    assert m["slide_ms"] == pytest.approx(0.0, abs=1e-5)  # straight down: no slide along the floor


def test_rows_weigh_by_frames_and_the_score_is_a_share_of_the_interpolation():
    acc = evaluate.Acc()
    acc.add({"floor_m": (torch.tensor([0.1, 0.4]), torch.tensor([1.0, 3.0]))})
    assert acc.result()["floor_m"] == pytest.approx((0.1 + 1.2) / 4)
    ev = {"inputs": {"clean": {"all": {"model": {"joint_m": 0.01, "seam_a_ms": 0.2}, "interp": {"joint_m": 0.04, "seam_a_ms": 0.4}}}}}
    assert evaluate.score(ev) == pytest.approx((0.25 + 0.5) / 2)


def test_hard_finds_a_turn():
    skel = rig.Skeleton.load()
    body = Body(skel)
    lift = skel.hip_height - skel.heads[0][2]
    # 1 m/s along X leaving A, 1 m/s along Y arriving at B: a 90 degree turn
    path = np.array([[0, 0], [0, 0], [1 / 30, 0], [1 / 30, 1 / 30], [1 / 30, 2 / 30]])
    local = torch.eye(3).expand(1, T, len(BONES), 3, 3)
    pos = torch.tensor([[[x, y, lift] for x, y in path]], dtype=torch.float32)
    h = evaluate.hard(evaluate.Pose(features(local, pos, skel.hip_height), body), body, A, B)
    assert h["turn"].tolist() == [True]


def test_posed_windows_hold_key_a_and_end_on_a_turned_stepped_key_b():
    rots, roots = evaluate.library_poses()
    assert rots.shape == (50, len(BONES), 4) and roots.shape == (50, 3)
    q, r = evaluate.posed_windows((rots, roots), 8, 10, np.random.default_rng(3))
    T = evaluate.CONTEXT + 10 + 1
    assert q.shape == (8, T, len(BONES), 4) and r.shape == (8, T, 3)
    # key A held over the context; key B from the frame after it, its root no further than the step
    assert torch.equal(q[:, 0], q[:, evaluate.CONTEXT - 1]) and torch.equal(r[:, 0], r[:, evaluate.CONTEXT - 1])
    step = (r[:, -1, :2] - r[:, 0, :2]).norm(dim=-1)
    assert (step <= evaluate.POSED_STEP * 10 + 1e-6).all()
    # the same seed, the same windows
    q2, _ = evaluate.posed_windows((rots, roots), 8, 10, np.random.default_rng(3))
    assert torch.equal(q, q2)


def test_grounded_poses_stand_on_the_floor():
    skel = rig.Skeleton.load()
    body = Body(skel)
    rots, roots = evaluate.ground(evaluate.library_poses(), body, "cpu")
    q, r = evaluate.posed_windows((rots, roots), 16, 1, np.random.default_rng(0))
    from ac_train.train import prepare

    f, _, _, _, _, _ = prepare(q, r, 1, body, "cpu")
    low = body.foot_heights(evaluate.Pose(f, body).points[:, evaluate.CONTEXT - 1]).min(-1).values
    assert low.abs().max() < 1e-3


def test_a_start_and_a_stop_are_the_hips_leaving_and_reaching_still():
    skel = rig.Skeleton.load()
    body = Body(skel)
    # from key A the hips hold still for a frame (0 m/s), then go 5 cm a frame (1.5 m/s) into key B
    going = scenes(skel, body, dx=(0.0, 0.0, 0.0, 0.05, 0.1))
    assert going["start"] and not going["stop"]
    stopping = scenes(skel, body, dx=(0.0, 0.0, 0.05, 0.1, 0.1))
    assert stopping["stop"] and not stopping["start"]
    # at 3 cm a frame (0.9 m/s) throughout it neither starts nor stops; standing doesn't either
    steady = scenes(skel, body, dx=(0.0, 0.03, 0.06, 0.09, 0.12))
    assert not steady["start"] and not steady["stop"]
    still = scenes(skel, body)
    assert not still["start"] and not still["stop"]


def test_hip_speed_is_how_far_from_the_recording():
    skel = rig.Skeleton.load()
    body = Body(skel)
    truth = poses(skel, body, dx=(0.0, 0.0, 0.0, 0.05, 0.1))  # 0, 1.5, 1.5 m/s from key to key
    fill = poses(skel, body, dx=(0.0, 0.0, 0.0, 0.0, 0.1))  # 0, 0, 3 m/s
    m = evaluate.scene_measure(fill, truth, body, A, B)
    # off by 0, 1.5 and 1.5: 1 m/s on average
    assert float(m["hip_speed_ms"][0][0]) == pytest.approx(1.0, abs=1e-3)


def test_a_crouch_is_low_hips_with_the_feet_down():
    skel = rig.Skeleton.load()
    body = Body(skel)
    low = 0.4 * skel.hip_height  # the whole body 40% down: hips at 0.6 of standing, in the 0.4 to 0.75
    assert scenes(skel, body, dz=(-low,) * T)["crouch"]
    assert not scenes(skel, body)["crouch"]  # standing
    # 70% down: hips at 0.3 of standing, under the 0.4 of a crouch: lying or sitting
    assert not scenes(skel, body, dz=(-0.7 * skel.hip_height,) * T)["crouch"]


def test_a_flight_is_scored_on_its_shape_and_its_landing():
    skel = rig.Skeleton.load()
    body = Body(skel)
    # 6 frames, keys at 1 and 5: in the air on frames 2 and 3, down again on frame 4
    a, b = 1, 5
    n = (0.0,) * 6
    up = (0.0, 0.0, 0.2, 0.2, 0.0, 0.0)
    assert scenes(skel, body, a=a, b=b, dx=n, dz=up)["flight"]
    # one frame in the air is a stride, not a flight
    assert not scenes(skel, body, a=a, b=b, dx=n, dz=(0.0, 0.0, 0.2, 0.0, 0.0, 0.0))["flight"]
    truth = poses(skel, body, dx=n, dz=up)
    fill = poses(skel, body, dx=n, dz=(0.0, 0.0, 0.2, 0.1, 0.05, 0.0))
    m = evaluate.scene_measure(fill, truth, body, a, b)
    # root off by 0 and 0.1 on the two frames in the air: 5 cm; 5 cm off on the frame it lands
    assert float(m["flight_root_m"][0][0]) == pytest.approx(0.05, abs=1e-5)
    assert float(m["flight_root_m"][1][0]) == 2
    assert float(m["landing_m"][0][0]) == pytest.approx(0.05, abs=1e-5)
    assert float(m["landing_m"][1][0]) == 1
    # no flight: nothing to weigh
    rest = poses(skel, body, dx=n, dz=n)
    m = evaluate.scene_measure(rest, rest, body, a, b)
    assert float(m["flight_root_m"][1][0]) == 0 and float(m["landing_m"][1][0]) == 0
    # a gap of one frame (keys at 1 and 3) has no landing to weigh, and no flight of two frames
    assert not scenes(skel, body, a=1, b=3, dx=n, dz=up)["flight"]
    m = evaluate.scene_measure(fill, truth, body, 1, 3)
    assert float(m["landing_m"][1][0]) == 0


def test_a_turn_while_stepping_and_where_the_feet_land():
    skel = rig.Skeleton.load()
    body = Body(skel)
    # along X at 3 cm a frame (0.9 m/s), then along Y into key B: a 90 degree turn at over 0.5 m/s;
    # the feet stop being planted on frame 2 (they start to move), so a plant changes
    turning = dict(dx=(0.0, 0.0, 0.03, 0.06, 0.06), dy=(0.0, 0.0, 0.0, 0.0, 0.03), dz=(0.0,) * T)
    assert scenes(skel, body, **turning)["turn_step"]
    # straight on: no turn
    straight = dict(dx=(0.0, 0.0, 0.03, 0.06, 0.09), dz=(0.0,) * T)
    assert not scenes(skel, body, **straight)["turn_step"]
    # the recording stands on frames 2 and 3; the fill's feet are 0.1 m across the floor on both
    truth = poses(skel, body)
    fill = poses(skel, body, dx=(0.0, 0.0, 0.1, 0.1, 0.0))
    m = evaluate.scene_measure(fill, truth, body, A, B)
    assert float(m["foot_place_m"][0][0]) == pytest.approx(0.1, abs=1e-5)
