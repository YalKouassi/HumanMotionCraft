# SPDX-License-Identifier: Apache-2.0
"""Download the engine's optional sample clips (the human ones: 38 SOMA BVH files) and put them on
the rig. Off by default: the app runs this only after the person accepts their licence in the Pose
fill panel (`posefill.samples`), and by hand it needs --accept.

    python tools/posefill/setup_samples.py [--dir <folder>] --accept

They are under the NVIDIA Sample Data Evaluation License: for evaluating and testing NVIDIA's
technology only, not to be shared, nor anything made from them. So they come from NVIDIA to this
machine and nowhere else: the repo and release never carry them, they are never trained on, and the
clips the app makes from them are marked evaluation-only, which blocks export and community upload.

The folder (default: `samples/` in the pose fill install) gets:
  motions/         LICENSE.txt, README.md and soma/*.bvh, checked against the hashes below
  clips/           each file on the rig at 30 fps, read by `posefill.samples` (action add)
  accepted.json    who said yes to what, written by the app before this runs
  install.json     written last, read by the app
"""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import time

import setup_posefill as base

HERE = base.HERE
REPO = base.REPO
SOURCE = f"https://raw.githubusercontent.com/{base.UPSTREAM_REPO}/{base.UPSTREAM_COMMIT}/assets/motions/"
MEDIA = f"https://media.githubusercontent.com/media/{base.UPSTREAM_REPO}/{base.UPSTREAM_COMMIT}/assets/motions/"
LICENCE = f"https://github.com/{base.UPSTREAM_REPO}/blob/{base.UPSTREAM_COMMIT}/assets/motions/LICENSE.txt"
STEPS = 3
NEED_GB = 1
# the licence and the note beside the clips: path, (sha256, size)
TEXTS = {
    "LICENSE.txt": ("474d99d5bcd285cee044eb172d277d1f287cc8d68978144c25079acf33c3eec8", 9521),
    "README.md": ("3c9d43d76e0abd87c8257134f3f6e901c99ed9baef51bfd55d5ea9db3932001b", 6063),
}
# the clips, from GitHub's large-file store: path, (sha256, size)
CLIPS = {
    "soma/avoidance/neutral_avoid_obstacle_bend_down_jog_ff_180_R_001__A544.bvh":
        ("4560ddfc0a216e07356426ab1af83b3e13785ed76312a6657503d7bb97585cb9", 1577274),
    "soma/avoidance/neutral_avoid_obstacle_bend_down_walk_ff_180_R_001__A543.bvh":
        ("1b9ad236e4b846b22c1f3b258d45b064f073f272759e254c7dd3386d8e471012", 2648450),
    "soma/jump/jump_over_obstacle_0_5m_R_001__A415.bvh":
        ("40f11025af983acf94c7028bdaee0743be7b9641e57545e45b67c0ef42e4413d", 1428924),
    "soma/jump/jump_over_obstacle_0_5m_R_002__A437.bvh":
        ("2ea888343a7adee808e96a792f704bbf7d058c4201c9476165eb3b61a62e9bc9", 1358894),
    "soma/jump/jump_over_obstacle_0_75m_R_102__A416.bvh":
        ("74fa726177530e7eac029e879d1747529d5eb8cc71ec3d5dcf10f3b732bdb5d4", 1302608),
    "soma/jump/jump_over_obstacle_0_75m_R_103__A416.bvh":
        ("2cf169575f4efc2687eb54f5d133e33286b1b853dc9afe6952bf9518338f032a", 1341652),
    "soma/jump/jump_over_obstacle_no_hand_0_75m_R_103__A416.bvh":
        ("363ac41c75e741dde9dd047dc9a5ecf9bb9c94105876506f620309f3c277882b", 1331891),
    "soma/locomotion/bs_1d_linear/neutral_jog_ff_180_R_001__A534.bvh":
        ("321883221739a6bad5f786bdcc8edc5ff6c7d7a443d47b08d1dac03036840e2a", 1568243),
    "soma/locomotion/bs_1d_linear/neutral_walk_ff_180_R_002__A536_M.bvh":
        ("e7c98757c2c95f3cbbbaa2a63e329a937b8a7549fed0856c066164b1a5200f31", 3203080),
    "soma/locomotion/bs_1d_linear/Neutral_walk_forward_002__A057.bvh":
        ("c0f4e63cec94c58196105a6d9f03a2ed1c94c8ff4fba15dd0a33cc512877904a", 2587387),
    "soma/locomotion/bs_1d_linear/run_loop_180_R_003__A326.bvh":
        ("9151bb7ff149e62dc47d3beb555f890c72352f87980aedabbd4ce456a7f70976", 565877),
    "soma/locomotion/bs_1d_linear/run_loop_180_R_003__A331.bvh":
        ("155bebc7049b794f1629fb385cdfe81e981e51acc3ee05d015716fceceae75e0", 541594),
    "soma/locomotion/bs_2d_directional/jog_ff_loop_270_R_very_fast_002__A455.bvh":
        ("3c1f8b1657c053c8f7e2acb1dd015660a9e62cb2b3f28e231dead02a44b81054", 423599),
    "soma/locomotion/bs_2d_directional/jog_ff_loop_270_R_very_fast_002__A455_M.bvh":
        ("717b834da088e0190eff026997cec5c9872ebab7a27ba1dcd25a9b1f99cd4ec9", 421808),
    "soma/locomotion/bs_2d_directional/jog_ff_loop_360_R_very_fast_001__A454.bvh":
        ("bc6b95d3a731b56ea970cb41bb1e418384ef761e8c3995c3ff3dc4d6d44ff221", 516108),
    "soma/locomotion/bs_2d_directional_crouch/crouch_ff_loop_180_R_001__A196.bvh":
        ("3f1d559dd6a89ba3bca10da79a9ba9fad42957869142c6b0571037493536d056", 1391071),
    "soma/locomotion/bs_2d_directional_crouch/crouch_ff_loop_270_R_002__A196.bvh":
        ("8f0a9d3f2458294d0644d164d9be1c8ee20f7a62949d28a417e32388b8a5c964", 1633871),
    "soma/locomotion/bs_2d_directional_crouch/crouch_ff_loop_270_R_002__A196_M.bvh":
        ("3818877d62da85e21f4e87eefb707deb41c432cf1acfa961240f5457f6141081", 1633704),
    "soma/locomotion/bs_2d_directional_crouch/crouch_ff_loop_360_R_001__A196.bvh":
        ("a53411f725891508af99e5e3e4176b1b279d35d30ca0c59dc163b03224b017f7", 1833169),
    "soma/locomotion/idle/crouch_idle_001__A529.bvh":
        ("1c7ad4237ea40d54ca3658e441a46bfe614852d2154089efd667f9448858b660", 1733948),
    "soma/locomotion/idle/idle_loop_001__A268.bvh":
        ("defaa54c9eaa060f4c091868b86e6840ba4201edb6dcb89f7c496e997892ff80", 2710513),
    "soma/locomotion/idle/legs_relax_002__A407.bvh":
        ("1fea14a5b427c1a434b20cd291e339dc86af7d7a95ce5773ba5bb6d331dd215e", 2864654),
    "soma/locomotion/idle/neutral_looking_around_R_001__A542.bvh":
        ("0793dc27fc145c14b3a4ccbc9f2b5c95124cba903403aba329884f54c0fcd0f3", 2568791),
    "soma/locomotion/idle/warm_up_neck_001__A359.bvh":
        ("cf436bcf201a993c15d284f2651f7be47836e57cfb6c8db5059820235da63929", 8017352),
    "soma/sitting/sit_on_chair_loop.bvh":
        ("7fb7c39c9c7a89a51a4c22979d43289855d50fdd6cee5c08063a993e066f24c5", 1304791),
    "soma/sitting/sit_on_chair_start_R_001__A047.bvh":
        ("c7833fb129622564e04b4cce6c3c4cec917aaab5792b2b52ac396fdb2868fa2c", 2248065),
    "soma/sitting/sit_on_chair_stop_R_001__A047.bvh":
        ("30c34de41c680dd531a0a3dec0c5438dd655f3d63b9897597fd7b2d072ec6572", 1525805),
    "soma/variations/angry_combat_quickstep_ff_idle_090_R_001__A365.bvh":
        ("7a55f4daa8b974b46663f58006d09cafa6656fc4a61fb27facd8f38438a94634", 1895332),
    "soma/variations/angry_combat_quickstep_ff_idle_090_R_001__A365_M.bvh":
        ("37910baa14baa2107bcf374b8c70ca8fb2afbe46358b9bbd5e84d04d5cd042ad", 1888343),
    "soma/variations/angry_combat_quickstep_ff_idle_360_R_001__A365.bvh":
        ("3f27edaf169be89a25ff417a13e0a5e353ea816e0b738623af1f242579b24076", 2200303),
    "soma/variations/angry_gun_walk_ff_loop_090_R_001__A393.bvh":
        ("5a9667f4d45e1f28e17320ee535bd0697e302ba2e41b44a21cce2427c5c8b968", 824397),
    "soma/variations/combat_quickstep_ff_idle_180_R_002__A365.bvh":
        ("5e175607b1aad62a779b03fbfff7e37320a763529147d6e5b5d3aae3ff7138fa", 2241123),
    "soma/variations/dance_vouge_vogue_sequence_180_R_002__A316.bvh":
        ("3eb042573216e5b01e32fd8e3e7c8fbc5ea0adad0c79170ac66e63f28039a34d", 1769220),
    "soma/variations/dancecards1_AB_injured_L_leg_001__A005.bvh":
        ("78678529a78e1b1288df3491e0d5506162520a98b6f6c997d2b104c61fbba14b", 32144465),
    "soma/variations/injured_L_leg_grab_walk_ff_180_002__A549.bvh":
        ("581531db85a822a2dc3f52f3809783d76901cf168ac1b852ff284d01c5928470", 6077672),
    "soma/variations/injured_torso_grab_walk_ff_180_001__A549.bvh":
        ("835405d588c5122a8c7dda5c96535419e62373081eab7cc19cd552839fb178ea", 7128875),
    "soma/variations/skipping_R_002__A360.bvh":
        ("a20fcb9d92a27972dd31c23d6fac02bb1943608ec3c288ea0c7b7e82add9dada", 2884518),
    "soma/variations/zombie_walk_180_R_003__A330.bvh":
        ("57871838f6dc3446fbfe3dc79f944ca970890ba72a171e038e3952a637d749e8", 4307610),
}


def step(n: int, msg: str) -> None:
    print(f"\n== [{n}/{STEPS}] {msg}", flush=True)


def default_dir() -> pathlib.Path:
    return base.default_dir() / "samples"


def fetch(url: str, out: pathlib.Path, want: str, size: int) -> None:
    if out.exists() and out.stat().st_size == size and base.sha256(out) == want:
        return
    base.download(url, out)
    got = base.sha256(out)
    if got != want:
        out.unlink()
        raise SystemExit(f"{out.name} from GitHub does not hash as the pinned file ({got}): run again")


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description="Download the optional evaluation-only sample clips")
    ap.add_argument("--dir", type=pathlib.Path, help="where they go (default: samples/ in the pose fill install)")
    ap.add_argument("--accept", action="store_true", help="you have read the licence and accept it")
    ap.add_argument("--stop-on-eof", action="store_true", help="end when stdin closes (the app runs it so)")
    args = ap.parse_args(argv)
    if args.stop_on_eof:
        base.stop_on_eof()
    root = (args.dir or default_dir()).resolve()
    install = root.parent
    if not (install / "install.json").is_file():
        raise SystemExit(f"pose fill is not installed in {install}: install it first")
    if not args.accept and not (root / "accepted.json").is_file():
        raise SystemExit(f"these clips need their licence accepted first: read {LICENCE}, then pass --accept")
    root.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(root).free / 1e9 < NEED_GB:
        raise SystemExit(f"the sample clips need about {NEED_GB} GB free on the disk of {root}")
    print("These clips are for evaluating NVIDIA's technology only. Check the licence before use:")
    print(f"  {LICENCE}")
    print("You are responsible for how you use them.")

    step(1, f"Sample clips from GitHub ({len(CLIPS)} files, {sum(s for _, s in CLIPS.values()) / 1e6:.0f} MB)")
    motions = root / "motions"
    for name, (want, size) in TEXTS.items():
        fetch(SOURCE + name, motions / name, want, size)
    for n, (name, (want, size)) in enumerate(CLIPS.items()):
        print(f"  {n + 1}/{len(CLIPS)} {pathlib.Path(name).name}", flush=True)
        fetch(MEDIA + name, motions / name, want, size)

    step(2, "Putting them on the rig")
    py = base.venv_bin(install, "python")
    shutil.copy2(REPO / "training" / "ac_train" / "bvh.py", install / "ac_rig" / "bvh.py")
    base.run([py, "-I", HERE / "convert_samples.py", "--posefill", install, "--rig", REPO / "assets" / "rig" / "default_rig.json",
              "--motions", motions, "--out", root / "clips"])

    step(3, "Done")
    # written last: the app takes install.json as "installed"
    (root / "install.json").write_text(json.dumps({
        "version": base.INSTALL_VERSION,
        "upstream_commit": base.UPSTREAM_COMMIT,
        "model": "sample clips",
        "python": str(py),
        "helper": str(install / "helper.py"),
        "finished": time.strftime("%Y-%m-%d"),
    }, indent=2), encoding="utf-8")
    print(f"\nThe sample clips are in {root}")
    print("\n== done", flush=True)


if __name__ == "__main__":
    main()
