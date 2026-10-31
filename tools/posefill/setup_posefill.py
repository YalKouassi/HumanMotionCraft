"""Install the optional pose-to-pose motion engine (NVIDIA's open pose-conditioned motion model,
run through TensorRT) for the app's `posefill.*` commands. Nothing here is needed to run the app;
without it, `posefill.*` say how to install it.

    python tools/posefill/setup_posefill.py [--dir <folder>]

The app's panel runs this too (Install, `posefill.install`): it reads the numbered step lines
(`== [3/6] ...`) from the log, the same lines `tools/textmotion/setup_textmotion.py` prints, and
with `--stop-on-eof` the setup ends, with everything it started, when the app closes its stdin
(Stop, or the app quitting). Run it again to repair or update: finished steps are skipped.

Needs an NVIDIA GPU (the engine is a TensorRT plan; there is no CPU or other-GPU path, so the
setup refuses without one), `uv` on PATH (the app fetches its own when there is none), and about
6 GB of free disk. Nothing else need be installed first: uv downloads the Python the install runs
on, and no account is needed anywhere.

The folder (default `%LOCALAPPDATA%/<product>/posefill`, or `AC_POSEFILL_DIR`) gets:
  upstream/        the engine's Python package at a pinned commit, from GitHub's archive of it
                   (checked by hash). Only the package itself: the archive's sample motion clips
                   (`assets/motions/`, evaluation-only), its Smart Primitive and scene documents,
                   demo app, docs, tests and tools are never written, and the setup checks they
                   are absent before it finishes
  upstream/motionbricks/assets/  the model (ONNX, 758 MB) and the rig file it poses (USD, 2 MB),
                   fetched from GitHub's large-file store and checked against their hashes
  python/          the Python 3.12 uv downloads, so the install doesn't depend on any other
  .venv/           Python 3.12 with TensorRT, Warp, OpenUSD, numpy and scipy
  trt-cache/       the TensorRT plan the first run builds for this GPU (a few minutes)
  warp-cache/      Warp's compiled kernels
  helper.py, engine.py   copied from here
  ac_rig/          the rig maths the helper retargets with (training/ac_train's rig, quat, retarget)
  licences.txt     the licences installing accepts, with their links
  install.json     what was installed, read by the app
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.request
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent

# the upstream engine (Apache-2.0 code) at a pinned commit: v0.2.0, 2026-10-23
UPSTREAM_REPO = "NVlabs/motionbricks"
UPSTREAM_COMMIT = "54fc563aeaaf819a753d72cc9fdc6713989cdd43"
UPSTREAM_ZIP = f"https://github.com/{UPSTREAM_REPO}/archive/{UPSTREAM_COMMIT}.zip"
# tree_hash of the files kept from the archive: GitHub may recompress an archive, but not change what is in it
UPSTREAM_TREE = "7d167ff2ef89e9719a2c4237d475c1a13f44d8c8c4bedcf5be433d0ccd429105"
# the large files the package needs, served by GitHub's LFS media endpoint: path in the package,
# sha256 (the LFS pointers in the archive say the same), size
LFS = f"https://media.githubusercontent.com/media/{UPSTREAM_REPO}/{UPSTREAM_COMMIT}/"
MODEL_FILES = {
    "motionbricks/assets/onnx/soma/mmm_v6_soma_fp32.onnx":
        ("856419dc72aeb14e591eebf4dd0ea05734a2506ddb00bcd39dd6dac2ad60f4fb", 757864273),
    "motionbricks/assets/human/soma_base_skel_minimal_no_root.usd":
        ("4ab93d78a0f1c06d3bcbedee5ceebb5d9d5b002fd0a88638e9399ed80b61871d", 2036435),
}
MODEL = "mmm_v6_soma_fp32"
# from the archive, only these: the package, its licence files and its build description
KEEP_PREFIXES = ("motionbricks/", "licenses/")
KEEP_FILES = ("LICENSE", "README.md", "pyproject.toml")
# never written, and checked absent at the end: the evaluation-only sample clips and what points at them
NEVER = ("assets", "app", "docs", "tests", "tools")
LFS_HEADER = b"version https://git-lfs.github.com/spec/v1"
PYTHON = "3.12"  # the TensorRT bindings on pypi.nvidia.com are cp312 wheels
# pinned as upstream's lock file (2026-10-23) pins them
PACKAGES = ["numpy==2.4.3", "scipy==1.17.1", "warp-lang==1.17.0", "usd-core==26.3"]
TENSORRT = "10.8.0.43"
TENSORRT_PACKAGES = [f"tensorrt-cu12=={TENSORRT}", f"tensorrt-cu12-bindings=={TENSORRT}", f"tensorrt-cu12-libs=={TENSORRT}"]
NVIDIA_INDEX = "https://pypi.nvidia.com"
SCRIPTS = ["helper.py", "engine.py"]
RIG_MODULES = ["rig.py", "quat.py", "retarget.py"]
INSTALL_VERSION = 1
STEPS = 6
NEED_GB, NEED_GB_RESUMED = 6, 2

# what installing accepts: (part, licence, where to read it)
LICENCES = [
    ("the engine's code (upstream package, commit " + UPSTREAM_COMMIT[:12] + ")", "Apache License 2.0",
     f"https://github.com/{UPSTREAM_REPO}/blob/{UPSTREAM_COMMIT}/LICENSE"),
    ("the motion model (ONNX) and the rig file it poses", "NVIDIA Open Model Agreement",
     "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-agreement/"),
    (f"TensorRT {TENSORRT} (tensorrt-cu12, -bindings, -libs, from {NVIDIA_INDEX})",
     "NVIDIA Software Development Kit licence agreement with the TensorRT supplement",
     "https://docs.nvidia.com/deeplearning/tensorrt/latest/reference/sla.html"),
    ("the CUDA runtime (nvidia-cuda-runtime-cu12, pulled in by TensorRT)", "NVIDIA CUDA Toolkit EULA",
     "https://docs.nvidia.com/cuda/eula/index.html"),
    ("Warp 1.17.0 (warp-lang)", "Apache License 2.0", "https://github.com/NVIDIA/warp/blob/v1.17.0/LICENSE.md"),
    ("OpenUSD 26.3 (usd-core)", "Tomorrow Open Source Technology License 1.0",
     "https://github.com/PixarAnimationStudios/OpenUSD/blob/release/LICENSE.txt"),
    ("numpy 2.4.3 and scipy 1.17.1", "BSD 3-Clause", "https://numpy.org/doc/stable/license.html"),
]
NOT_INSTALLED = (
    "The upstream archive's sample motion clips (assets/motions/, NVIDIA Sample Data Evaluation License,\n"
    "evaluation-only), its Smart Primitive and scene documents, demo app, docs, tests and tools are not\n"
    "installed, never read, and the install checks they are absent. The TensorRT plan built on first use\n"
    "(trt-cache/) stays in this folder: it is a derivative of the model and is never shared."
)


def product_name() -> str:
    """The product's name from `ac-core` (the one place it is spelled)."""
    text = (REPO / "crates" / "ac-core" / "src" / "product.rs").read_text(encoding="utf-8")
    m = re.search(r"macro_rules!\s*product_name\s*\{\s*\(\)\s*=>\s*\{\s*\"([^\"]+)\"", text)
    if not m:
        raise SystemExit("could not read the product name from crates/ac-core/src/product.rs")
    return m.group(1)


def default_dir() -> pathlib.Path:
    """`AC_POSEFILL_DIR`, else the per-user local data folder (the app looks in the same place)."""
    if os.environ.get("AC_POSEFILL_DIR"):
        return pathlib.Path(os.environ["AC_POSEFILL_DIR"])
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
        if not base:
            raise SystemExit("neither LOCALAPPDATA nor APPDATA is set: pass --dir")
        return pathlib.Path(base) / product_name() / "posefill"
    if sys.platform == "darwin":
        return pathlib.Path.home() / "Library" / "Application Support" / product_name() / "posefill"
    base = os.environ.get("XDG_DATA_HOME") or str(pathlib.Path.home() / ".local" / "share")
    return pathlib.Path(base) / product_name() / "posefill"


def step(n: int, msg: str) -> None:
    print(f"\n== [{n}/{STEPS}] {msg}", flush=True)


# the processes running now, which --stop-on-eof ends with their children
CURRENT: list[subprocess.Popen] = []
STOPPING = threading.Event()


def kill_tree(p: subprocess.Popen) -> None:
    if os.name == "nt":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True, stdin=subprocess.DEVNULL, check=False)
    else:
        try:
            os.killpg(p.pid, signal.SIGTERM)
        except OSError:
            pass


def popen(cmd: list, **kw) -> subprocess.Popen:
    kw.setdefault("stdin", subprocess.DEVNULL)
    p = subprocess.Popen([str(c) for c in cmd], start_new_session=os.name != "nt", **kw)
    CURRENT.append(p)
    return p


def run(cmd: list, env: dict | None = None, cwd: pathlib.Path | None = None) -> None:
    print("  $ " + " ".join(str(c) for c in cmd), flush=True)
    p = popen(cmd, env=env, cwd=cwd)
    try:
        code = p.wait()
    finally:
        CURRENT.remove(p)
    if STOPPING.is_set():
        time.sleep(60)  # stop_on_eof ended it, and ends the setup now
    if code:
        what = " ".join(str(c) for c in cmd[:3])
        raise SystemExit(f"`{what}` failed (exit code {code}); the lines above say why")


def stop_on_eof() -> None:
    """End the setup, and what it started, when stdin closes (the app's Stop, or the app ending)."""
    def watch() -> None:
        try:
            while os.read(0, 65536):
                pass
        except OSError:
            pass
        STOPPING.set()
        print("\n== stopped: the app ended the install (run it again to carry on)", flush=True)
        for p in list(CURRENT):
            kill_tree(p)
        os._exit(3)
    threading.Thread(target=watch, daemon=True).start()


def check_space(root: pathlib.Path) -> None:
    there = root
    while not there.exists() and there.parent != there:
        there = there.parent
    free = shutil.disk_usage(there).free / 1e9
    need_gb = NEED_GB_RESUMED if (root / ".venv").exists() else NEED_GB
    if free < need_gb:
        raise SystemExit(f"{free:.0f} GB is free on the disk of {root}; the install needs about {need_gb} GB: "
                         "free some space, or install elsewhere with AC_POSEFILL_DIR")


def need(tool: str, hint: str) -> str:
    path = shutil.which(tool)
    if not path:
        raise SystemExit(f"`{tool}` is not on PATH: {hint}")
    return path


def venv_bin(root: pathlib.Path, name: str) -> pathlib.Path:
    sub = "Scripts" if os.name == "nt" else "bin"
    exe = ".exe" if os.name == "nt" else ""
    return root / ".venv" / sub / f"{name}{exe}"


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 24), b""):
            h.update(block)
    return h.hexdigest()


def curl() -> str | None:
    """The system's curl: Windows' own one checks certificates against Windows' store, which the
    Python uv downloads can't read whole (its downloads fail with CERTIFICATE_VERIFY_FAILED)."""
    if os.name == "nt":
        own = pathlib.Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "curl.exe"
        if own.exists():
            return str(own)
    return shutil.which("curl")


def download(url: str, to: pathlib.Path, tries: int = 4) -> None:
    to.parent.mkdir(parents=True, exist_ok=True)
    if c := curl():
        run([c, "-fL", "-sS", "--retry", str(tries - 1), "--retry-delay", "15", "-o", to, url])
        return
    for i in range(tries):
        try:
            with urllib.request.urlopen(url, timeout=120) as r, open(to, "wb") as fh:
                shutil.copyfileobj(r, fh)
            return
        except OSError as e:
            if i + 1 < tries:
                print(f"  download failed ({e}); retry {i + 1} of {tries - 1}", flush=True)
                time.sleep(15)
            else:
                print(f"  download failed ({e})", flush=True)
    raise SystemExit(f"could not download {url}")


def nvidia_gpu() -> str | None:
    """The first NVIDIA GPU's name and driver (nvidia-smi), or None without one."""
    smi = shutil.which("nvidia-smi")  # on PATH only: Windows would otherwise look in System32 whatever PATH says
    if not smi:
        return None
    try:
        out = subprocess.run([smi, "--query-gpu=name,driver_version,memory.total", "--format=csv,noheader"],
                             capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    first = out.strip().splitlines()[0].strip() if out.strip() else ""
    return first or None


def tree_hash(files: dict[str, bytes]) -> str:
    """sha256 over each file's path and contents, in path order."""
    h = hashlib.sha256()
    for name in sorted(files):
        h.update(f"{name}:{hashlib.sha256(files[name]).hexdigest()}\n".encode("utf-8"))
    return h.hexdigest()


def writable_then_retry(func, path, _exc) -> None:
    os.chmod(path, 0o600)
    func(path)


def kept(name: str) -> bool:
    return name in KEEP_FILES or name.startswith(KEEP_PREFIXES)


def upstream_source(root: pathlib.Path) -> None:
    """The engine's package at UPSTREAM_COMMIT from GitHub's archive (no Git needed); only the
    package, its licences and build description are written. `upstream/.commit` says which."""
    src = root / "upstream"
    mark = src / ".commit"
    if mark.exists() and mark.read_text(encoding="utf-8").strip() == UPSTREAM_COMMIT:
        print(f"  at {UPSTREAM_COMMIT[:12]} already", flush=True)
        return
    deps = root / "deps"
    deps.mkdir(parents=True, exist_ok=True)
    archive = deps / f"upstream-{UPSTREAM_COMMIT}.zip"
    print(f"  downloading {UPSTREAM_ZIP}", flush=True)
    download(UPSTREAM_ZIP, archive)
    top = f"motionbricks-{UPSTREAM_COMMIT}/"
    with zipfile.ZipFile(archive) as zf:
        files, dropped = {}, 0
        for i in zf.infolist():
            if i.is_dir() or not i.filename.startswith(top):
                continue
            name = i.filename[len(top):]
            if not kept(name):
                dropped += 1
                continue
            data = zf.read(i)
            # a large-file pointer (the robot meshes, the model files): the two the helper needs
            # are fetched whole in the next step, the rest is nothing we use
            if data.startswith(LFS_HEADER):
                dropped += 1
                continue
            files[name] = data
    got = tree_hash(files)
    if got != UPSTREAM_TREE:
        archive.unlink()
        raise SystemExit(f"the engine's archive from GitHub is not the one this setup was made for ({got}): run again")
    if src.exists():
        shutil.rmtree(src, onerror=writable_then_retry)
    for name, data in files.items():
        out = src / name
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(data)
    mark.write_text(UPSTREAM_COMMIT + "\n", encoding="utf-8")
    archive.unlink()
    print(f"  {len(files)} files at {UPSTREAM_COMMIT[:12]}, matching their hash; {dropped} of the archive's "
          "files left out (sample clips, demo app, docs, tests, tools, large-file pointers)", flush=True)


def model_files(root: pathlib.Path) -> None:
    """The model and the rig file, from GitHub's large-file store, checked by hash."""
    for rel, (want, size) in MODEL_FILES.items():
        out = root / "upstream" / rel
        if out.exists() and out.stat().st_size == size and sha256(out) == want:
            print(f"  {out.name}: present, hash matches", flush=True)
            continue
        print(f"  downloading {LFS + rel} ({size / 1e6:.0f} MB)", flush=True)
        download(LFS + rel, out)
        got = sha256(out)
        if got != want:
            out.unlink()
            raise SystemExit(f"{out.name} from GitHub does not hash as the pinned file ({got}): run again")
        print(f"  {out.name}: hash matches", flush=True)


def uv_python(root: pathlib.Path) -> None:
    os.environ["UV_PYTHON_INSTALL_DIR"] = str(root / "python")
    os.environ["UV_PYTHON_DOWNLOADS"] = "automatic"


def python_env(root: pathlib.Path) -> dict:
    py = venv_bin(root, "python")
    if not py.exists():
        if (root / ".venv").exists():
            print("  the environment there has no working Python: making it again", flush=True)
            shutil.rmtree(root / ".venv", onerror=writable_then_retry)
        run(["uv", "venv", "--quiet", "--managed-python", "-p", PYTHON, root / ".venv"])
    run(["uv", "pip", "install", "--quiet", "-p", py, *PACKAGES])
    # TensorRT's wheels are on NVIDIA's index; pypi.org holds only stubs that would run a nested pip
    run(["uv", "pip", "install", "--quiet", "-p", py, "--extra-index-url", NVIDIA_INDEX, *TENSORRT_PACKAGES])
    # the engine's package is imported from upstream/ as it is (no wheel is built: one would copy
    # the model into site-packages); a .pth file makes `import motionbricks` work in the venv
    site = subprocess.run([str(py), "-c", "import sysconfig; print(sysconfig.get_paths()['purelib'])"],
                          capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout.strip()
    (pathlib.Path(site) / "posefill-upstream.pth").write_text(str(root / "upstream") + "\n", encoding="utf-8")
    out = subprocess.run([str(py), "-c", "import tensorrt, warp, numpy, pxr, motionbricks; "
                          "print(tensorrt.__version__, warp.__version__, numpy.__version__, motionbricks.__version__)"],
                         capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout.split()
    return {"tensorrt": out[0], "warp": out[1], "numpy": out[2], "upstream": out[3]}


def tensorrt_licence(root: pathlib.Path) -> pathlib.Path | None:
    """The licence text inside the TensorRT wheels, if one is there (the roadmap asks for it to be
    compared with the one published at docs.nvidia.com)."""
    for p in sorted((root / ".venv").rglob("tensorrt*")):
        if p.is_dir():
            for name in ("LICENSE.txt", "LICENSE", "LICENSE.md"):
                if (p / name).is_file():
                    return p / name
    return None


def check_absent(root: pathlib.Path) -> None:
    """What must never be in the install: the archive's sample clips and the folders pointing at them."""
    bad = [str(root / "upstream" / n) for n in NEVER if (root / "upstream" / n).exists()]
    pointers = [str(p) for p in (root / "upstream" / "motionbricks").rglob("*")
                if p.is_file() and p.suffix.lower() in (".bvh", ".csv") ]
    if bad or pointers:
        raise SystemExit("the install holds files it must not: " + ", ".join(bad + pointers)
                         + "\ndelete upstream/ and run again")
    print("  no assets/motions, assets/smart_primitives, app, docs, tests or tools under upstream/; no clip files", flush=True)


def write_licences(root: pathlib.Path, tensorrt_text: pathlib.Path | None = None) -> None:
    lines = ["Installing accepted these licences (read each at its link):", ""]
    for part, lic, url in LICENCES:
        lines.append(f"- {part}: {lic}\n  {url}")
    if tensorrt_text:
        lines += ["", f"The TensorRT agreement as it ships in the installed wheel: {tensorrt_text}"]
    lines += ["", NOT_INSTALLED, ""]
    (root / "licences.txt").write_text("\n".join(lines), encoding="utf-8")


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description="Install the pose-to-pose motion engine for the app")
    ap.add_argument("--dir", type=pathlib.Path, help="where to install (default: AC_POSEFILL_DIR or the per-user data folder)")
    ap.add_argument("--stop-on-eof", action="store_true", help="end when stdin closes (the app runs it so)")
    ap.add_argument("--skip-check", action="store_true", help="leave out the final check (the plan build and a fill)")
    args = ap.parse_args(argv)
    if args.stop_on_eof:
        stop_on_eof()
    root = (args.dir or default_dir()).resolve()
    gpu = nvidia_gpu()
    if gpu is None:
        raise SystemExit("no NVIDIA GPU found (nvidia-smi gave none): the pose-to-pose engine runs as a TensorRT "
                         "plan on an NVIDIA GPU, and there is no CPU or other-GPU path, so it can't be installed here")
    if platform.machine().lower() not in ("amd64", "x86_64"):
        raise SystemExit(f"this is a {platform.machine()} machine: TensorRT's wheels are built for x86-64 only")
    need("uv", "install it from https://docs.astral.sh/uv/")
    check_space(root)
    root.mkdir(parents=True, exist_ok=True)
    uv_python(root)
    print(f"Installing the pose-to-pose engine into {root}")
    print(f"GPU: {gpu}")
    print("Installing means you accept:")
    for part, lic, url in LICENCES:
        print(f"  {part}: {lic}\n    {url}")
    write_licences(root)

    step(1, "Engine source (the package, without the archive's sample clips)")
    upstream_source(root)
    step(2, "Model and rig files (ONNX 758 MB, USD 2 MB)")
    model_files(root)
    step(3, "Python environment (TensorRT, Warp, OpenUSD, numpy, scipy)")
    versions = python_env(root)
    print(f"  tensorrt {versions['tensorrt']}, warp {versions['warp']}, numpy {versions['numpy']}, upstream {versions['upstream']}")
    lic = tensorrt_licence(root)
    print(f"  TensorRT's licence text in the wheel: {lic if lic else 'none found'}")
    write_licences(root, lic)
    step(4, "Helper scripts")
    for name in SCRIPTS:
        shutil.copy2(HERE / name, root / name)
    pkg = root / "ac_rig"
    pkg.mkdir(exist_ok=True)
    (pkg / "__init__.py").write_text(
        '"""Copied from training/ac_train by setup_posefill.py."""\n\n'
        "ASSETS = None  # Skeleton.load imports it; the helper always passes the rig's path\n",
        encoding="utf-8")
    for name in RIG_MODULES:
        shutil.copy2(REPO / "training" / "ac_train" / name, pkg / name)
    step(5, "Check: nothing evaluation-only came along")
    check_absent(root)
    install = {
        "version": INSTALL_VERSION,
        "upstream_commit": UPSTREAM_COMMIT,
        "model": MODEL,
        "python": str(venv_bin(root, "python")),
        "helper": str(root / "helper.py"),
        "env": {"PYTHONUTF8": "1"},
        "versions": versions,
        "gpu": gpu,
        "platform": platform.platform(),
    }
    step(6, "Check: the helper builds the TensorRT plan (a few minutes the first time) and fills one second")
    if args.skip_check:
        print("  skipped (--skip-check)", flush=True)
    else:
        py = venv_bin(root, "python")
        env = dict(os.environ, **install["env"])
        rig = str(REPO / "assets" / "rig" / "default_rig.json")
        rest = {"rotations": [[0.0, 0.0, 0.0, 1.0]] * 104, "root": [0.0, 0.0, 0.9403867]}
        req = json.dumps({"id": 1, "op": "fill", "rig": rig, "context": [rest] * 4, "targets": [rest], "frames": 30})
        p = popen([py, "-I", root / "helper.py"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                  stderr=subprocess.PIPE, text=True, env=env, encoding="utf-8")
        try:
            stdout, stderr = p.communicate(req + "\n", timeout=3600)
        except subprocess.TimeoutExpired:
            kill_tree(p)
            raise SystemExit("the helper did not answer the check within an hour") from None
        finally:
            CURRENT.remove(p)
        replies = [json.loads(line) for line in stdout.splitlines() if line.startswith("{")]
        for r in replies:
            if "progress" in r:
                print("  " + r["progress"], flush=True)
        done = [r for r in replies if r.get("id") == 1 and "result" in r]
        if not done:
            sys.stderr.write(stderr[-4000:])
            raise SystemExit("the helper did not answer the check; see the messages above")
        res = done[0]["result"]
        print(f"  {len(res['rotations'])} frames in {res.get('seconds', 0):.2f} s on the GPU", flush=True)
    # written last: the app takes install.json as "installed"
    (root / "install.json").write_text(json.dumps(install, indent=2), encoding="utf-8")
    print(f"\nThe pose-to-pose engine is installed in {root}")
    print("\n== done", flush=True)


if __name__ == "__main__":
    main()
