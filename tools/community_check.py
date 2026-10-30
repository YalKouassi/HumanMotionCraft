#!/usr/bin/env python3
"""Community checker: downloads pending clips from the community server,

verifies their integrity and poses with `ac`, and posts back approval or refusal.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request

DEFAULT_SERVER = "https://ksnxwkxaunxutkmdhjzm.supabase.co"


def find_ac() -> Path:
    env_bin = os.environ.get("AC_BIN")
    if env_bin and Path(env_bin).is_file():
        return Path(env_bin)
    for candidate in [
        Path("target/release/ac.exe"),
        Path("target/release/ac"),
        Path("target/debug/ac.exe"),
        Path("target/debug/ac"),
    ]:
        if candidate.is_file():
            return candidate.resolve()
    which = shutil.which("ac")
    if which:
        return Path(which)
    raise RuntimeError("could not find `ac` binary; build it with `cargo build --release -p ac-cli`")


def post_checker(server: str, secret: str, payload: dict) -> dict:
    url = f"{server.rstrip('/')}/functions/v1/checker"
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Content-Type": "application/json",
            "x-checker-secret": secret,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            body = resp.read()
            return json.loads(body.decode("utf-8"))
    except urllib.error.HTTPError as e:
        err_msg = e.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {e.code} from checker endpoint: {err_msg}") from e


def download_bytes(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "HumanMotionCraft-Checker/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return resp.read()


def check_clip_file(ac_bin: Path, assets_dir: Path, clip_json_text: str) -> None:
    with tempfile.TemporaryDirectory(prefix="hmc-checker-") as tmpdir:
        temp_lib = Path(tmpdir)
        anim_dir = temp_lib / "animations" / "Checking"
        anim_dir.mkdir(parents=True, exist_ok=True)
        clip_path = anim_dir / "candidate.clip.json"
        clip_path.write_text(clip_json_text, encoding="utf-8")

        # 1. Test clip.open
        proc_open = subprocess.run(
            [
                str(ac_bin),
                "--assets",
                str(assets_dir),
                "--library",
                str(temp_lib),
                "run",
                "clip.open",
                json.dumps({"id": "Checking/candidate"}),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc_open.returncode != 0:
            err = proc_open.stderr.strip() or proc_open.stdout.strip() or "failed to open clip"
            raise ValueError(f"clip.open failed: {err}")

        # 2. Test clip.possible
        proc_possible = subprocess.run(
            [
                str(ac_bin),
                "--assets",
                str(assets_dir),
                "--library",
                str(temp_lib),
                "run",
                "clip.possible",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc_possible.returncode != 0:
            err = proc_possible.stderr.strip() or proc_possible.stdout.strip() or "clip.possible failed"
            raise ValueError(f"clip.possible command error: {err}")

        try:
            possible_out = json.loads(proc_possible.stdout)
        except json.JSONDecodeError:
            raise ValueError(f"malformed output from clip.possible: {proc_possible.stdout[:200]}")

        level = possible_out.get("level", "typical")
        if level == "impossible":
            imp = possible_out.get("impossible", {})
            worst_frame = imp.get("worst_frame")
            breaches = possible_out.get("breaches", [])
            breach_names = [b.get("name") for b in breaches if isinstance(b, dict) and b.get("name")]
            detail = f"on frame {worst_frame}" if worst_frame is not None else ""
            if breach_names:
                detail += f" ({', '.join(breach_names[:3])})"
            raise ValueError(f"clip contains impossible poses {detail}".strip())


def main() -> int:
    secret = os.environ.get("CHECKER_SECRET")
    if not secret:
        print("CHECKER_SECRET environment variable is missing", file=sys.stderr)
        return 1

    server = os.environ.get("COMMUNITY_SERVER", DEFAULT_SERVER)
    assets_dir = Path(os.environ.get("ASSETS_DIR", "assets")).resolve()
    ac_bin = find_ac()

    print(f"Connecting to {server}...")
    pending = post_checker(server, secret, {"action": "pending"})
    clips = pending.get("clips", [])
    if not clips:
        print("No pending clips to check.")
        return 0

    print(f"Found {len(clips)} clip(s) pending check.")
    for item in clips:
        clip_id = item.get("id")
        url = item.get("url")
        expected_sha = item.get("sha256")
        if not clip_id or not url:
            continue

        print(f"[{clip_id}] Downloading from {url}...")
        try:
            raw = download_bytes(url)
            if expected_sha:
                actual_sha = hashlib.sha256(raw).hexdigest()
                if actual_sha.lower() != expected_sha.lower():
                    raise ValueError(f"sha256 mismatch (expected {expected_sha}, got {actual_sha})")

            # Try gzip decompression; fallback to plain json
            try:
                uncompressed = gzip.decompress(raw)
            except Exception:
                uncompressed = raw

            text = uncompressed.decode("utf-8")
            # Verify JSON shape
            json.loads(text)

            # Run engine checks
            check_clip_file(ac_bin, assets_dir, text)

            # Passed
            print(f"[{clip_id}] Passed checks, publishing...")
            post_checker(server, secret, {"action": "result", "id": clip_id, "ok": True})
        except Exception as e:
            reason = str(e)
            print(f"[{clip_id}] Refused: {reason}")
            post_checker(
                server,
                secret,
                {"action": "result", "id": clip_id, "ok": False, "reason": reason[:1000]},
            )

    return 0


if __name__ == "__main__":
    sys.exit(main())
