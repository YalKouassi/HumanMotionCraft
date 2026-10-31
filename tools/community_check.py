#!/usr/bin/env python3
"""Community checker: runs the app's own checks on the clips people shared, and publishes or
refuses each (roadmap 150000).

    python tools/community_check.py            # check every pending clip
    python tools/community_check.py --pending  # only say how many are pending (and write the
                                               # GitHub Actions output `count`): no build needed

Environment: ``CHECKER_SECRET`` (the server's side door, never printed), ``COMMUNITY_SERVER``
(default the live server), ``ASSETS_DIR`` (default ``assets``), ``AC_BIN`` (default the release
``ac`` under ``target/``).

For each pending clip: the file is downloaded (capped at what the upload accepts: 10 MB gzipped),
its sha256 is checked against the server's record before anything parses it, it is inflated with a
cap (100 MB), and `ac` runs ``clip.open`` then ``clip.possible`` on it in one ``ac script`` run on
a scratch library (two ``ac run`` calls would be two sessions, and the second would measure an
empty document), under a time limit. The app's verdict decides: ``published``, or ``failed`` with
the reason, which deletes the files on the server. A problem of the checker's own (a download that
fails, a record with no hash, a result that could not be posted) is printed, leaves the clip
pending for the next run and makes the run exit non-zero; it never marks a clip failed.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import zlib
from pathlib import Path

DEFAULT_SERVER = "https://ksnxwkxaunxutkmdhjzm.supabase.co"
#: the upload's own caps (services/community/supabase/functions/upload/index.ts)
MAX_GZ = 10 * 1024 * 1024
MAX_RAW = 100 * 1024 * 1024
#: seconds for one HTTP call, and for the app's checks on one clip
HTTP_TIMEOUT_S = 30
AC_TIMEOUT_S = 300
#: where the candidate sits in the scratch library
CLIP_ID = "Checking/candidate"
#: the longest reason posted back
MAX_REASON = 1000


class CheckerProblem(Exception):
    """Something wrong on our side (network, the server's record, the build), not the clip's: the
    clip stays pending and the run says so."""


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
    raise CheckerProblem("could not find the `ac` binary; build it with `cargo build --release -p ac-cli`")


def post_checker(server: str, secret: str, payload: dict) -> dict:
    """One call to the checker's side door. The secret goes in a header and is never printed."""
    url = f"{server.rstrip('/')}/functions/v1/checker"
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json", "x-checker-secret": secret},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT_S) as resp:
            return json.loads(read_capped(resp, MAX_GZ))
    except urllib.error.HTTPError as e:
        err_msg = e.read(4096).decode("utf-8", errors="replace")
        raise CheckerProblem(f"HTTP {e.code} from the checker endpoint: {err_msg}") from e
    except (urllib.error.URLError, OSError, ValueError) as e:
        raise CheckerProblem(f"the checker endpoint could not be reached: {e}") from e


def read_capped(stream, most: int) -> bytes:
    """Read a stream of at most ``most`` bytes; one byte more is refused before it is kept."""
    out = bytearray()
    while True:
        chunk = stream.read(min(65536, most + 1 - len(out)))
        if not chunk:
            return bytes(out)
        out += chunk
        if len(out) > most:
            raise ValueError(f"the file is larger than {most} bytes")


def download(url: str, most: int = MAX_GZ) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "HumanMotionCraft-Checker/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT_S) as resp:
            return read_capped(resp, most)
    except (urllib.error.URLError, OSError) as e:
        raise CheckerProblem(f"could not download the file: {e}") from e


def verify_sha256(raw: bytes, expected: str) -> None:
    """The bytes must be the ones the server recorded, before anything reads them."""
    actual = hashlib.sha256(raw).hexdigest()
    if actual != expected.strip().lower():
        raise ValueError(f"sha256 mismatch (the server recorded {expected}, the file is {actual})")


def inflate(gz: bytes, most: int = MAX_RAW) -> bytes:
    """A gzip file's content, at most ``most`` bytes; anything else is refused with why."""
    if len(gz) < 2 or gz[0] != 0x1F or gz[1] != 0x8B:
        raise ValueError("the file is not gzipped (the upload stores clips gzipped)")
    d = zlib.decompressobj(16 + zlib.MAX_WBITS)
    try:
        out = d.decompress(gz, most + 1)
    except zlib.error as e:
        raise ValueError(f"the file is not a valid gzip stream: {e}") from e
    if len(out) > most or d.unconsumed_tail:
        raise ValueError(f"the clip is larger than {most} bytes unzipped")
    if not d.eof:
        raise ValueError("the gzip stream is cut short")
    return out


def script_steps(clip_id: str = CLIP_ID) -> list[dict]:
    """The app's checks, in one session: open the clip, then measure the document it is now."""
    return [
        {"command": "clip.open", "params": {"id": clip_id}},
        {"command": "clip.possible", "params": {}},
    ]


def judge(results: list[dict]) -> None:
    """The verdict from an ``ac --jsonl script`` run's lines (``{command, result}``): a level of
    ``impossible`` is a refusal naming the worst frame and the joints past their limits."""
    possible = next((r.get("result") for r in results if r.get("command") == "clip.possible"), None)
    if not isinstance(possible, dict):
        raise CheckerProblem("the app gave no clip.possible result")
    level = possible.get("level")
    if level is None:
        raise CheckerProblem("the app's clip.possible result has no level")
    if level == "impossible":
        imp = possible.get("impossible") or {}
        worst_frame = imp.get("worst_frame") if isinstance(imp, dict) else None
        breaches = possible.get("breaches") or []
        joints = [b.get("joint") for b in breaches if isinstance(b, dict) and b.get("joint")]
        detail = f" on frame {worst_frame}" if worst_frame is not None else ""
        if joints:
            detail += f" ({', '.join(dict.fromkeys(joints[:3]))})"
        raise ValueError(f"the clip has impossible poses{detail}")


def check_clip_text(ac_bin: Path, assets_dir: Path, clip_json_text: str, timeout_s: float = AC_TIMEOUT_S) -> None:
    """Run the app's checks on a clip's text in a scratch library; a refusal is a ``ValueError``."""
    with tempfile.TemporaryDirectory(prefix="hmc-checker-") as tmpdir:
        tmp = Path(tmpdir)
        temp_lib = tmp / "library"
        group, name = CLIP_ID.split("/")
        anim_dir = temp_lib / "animations" / group
        anim_dir.mkdir(parents=True, exist_ok=True)
        (anim_dir / f"{name}.clip.json").write_text(clip_json_text, encoding="utf-8")
        script = tmp / "check.json"
        script.write_text(json.dumps(script_steps()), encoding="utf-8")
        cmd = [str(ac_bin), "--assets", str(assets_dir), "--library", str(temp_lib), "--jsonl", "script", str(script)]
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace",
                                  check=False, timeout=timeout_s)
        except subprocess.TimeoutExpired:
            raise ValueError(f"the app's checks took longer than {timeout_s:.0f} s") from None
        if proc.returncode != 0:
            err = proc.stderr.strip() or proc.stdout.strip() or "the app refused the clip"
            raise ValueError(f"the app refused it: {err}")
        results = []
        for line in proc.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                results.append(json.loads(line))
            except json.JSONDecodeError:
                raise CheckerProblem(f"the app printed something that is not JSON: {line[:200]}") from None
        judge(results)


def check_item(item: dict, ac_bin: Path, assets_dir: Path) -> None:
    """Download, check the hash, inflate, parse and run the app on one pending clip. A refusal is a
    ``ValueError`` with the reason; our own trouble is a ``CheckerProblem``."""
    raw = download(item["url"])
    verify_sha256(raw, item["sha256"])
    text = inflate(raw).decode("utf-8")
    json.loads(text)
    check_clip_text(ac_bin, assets_dir, text)


def process(clips: list[dict], post, check) -> int:
    """Check each pending clip and post its verdict. Returns how many were left pending because
    of a problem of our own (printed); the clip is marked failed only by a refusal."""
    problems = 0
    for item in clips:
        clip_id, url, sha = item.get("id"), item.get("url"), item.get("sha256")
        if not clip_id or not url or not sha:
            print(f"[{clip_id or '?'}] the server's record lacks an id, a url or a sha256: left pending")
            problems += 1
            continue
        print(f"[{clip_id}] checking {url}")
        try:
            check(item)
        except ValueError as e:
            reason = str(e)[:MAX_REASON]
            verdict = {"action": "result", "id": clip_id, "ok": False, "reason": reason}
            print(f"[{clip_id}] refused: {reason}")
        except CheckerProblem as e:
            print(f"[{clip_id}] not checked ({e}): left pending for the next run")
            problems += 1
            continue
        else:
            verdict = {"action": "result", "id": clip_id, "ok": True}
            print(f"[{clip_id}] passed the checks")
        try:
            reply = post(verdict)
        except CheckerProblem as e:
            print(f"[{clip_id}] the verdict could not be posted ({e}): left pending for the next run")
            problems += 1
            continue
        print(f"[{clip_id}] now {reply.get('status', '?')}")
    return problems


def github_output(name: str, value: str) -> None:
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as f:
            f.write(f"{name}={value}\n")


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    only_count = "--pending" in args
    secret = os.environ.get("CHECKER_SECRET")
    if not secret:
        print("CHECKER_SECRET environment variable is missing", file=sys.stderr)
        return 1
    server = os.environ.get("COMMUNITY_SERVER", DEFAULT_SERVER)
    try:
        pending = post_checker(server, secret, {"action": "pending"})
    except CheckerProblem as e:
        print(e, file=sys.stderr)
        return 1
    clips = pending.get("clips") or []
    print(f"{len(clips)} clip(s) pending check")
    if only_count:
        github_output("count", str(len(clips)))
        return 0
    if not clips:
        return 0
    try:
        ac_bin = find_ac()
    except CheckerProblem as e:
        print(e, file=sys.stderr)
        return 1
    assets_dir = Path(os.environ.get("ASSETS_DIR", "assets")).resolve()
    problems = process(
        clips,
        post=lambda verdict: post_checker(server, secret, verdict),
        check=lambda item: check_item(item, ac_bin, assets_dir),
    )
    if problems:
        print(f"{problems} clip(s) left pending because of a problem of the checker's own (above)", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
