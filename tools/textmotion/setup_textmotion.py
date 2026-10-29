"""Install Kimodo (NVIDIA's text-to-motion model) for the app's optional motion from text and for
generating training clips. Nothing here is needed to run the app; without it, `text.*` say how to
install it.

    python tools/textmotion/setup_textmotion.py [--dir <folder>] [--cpu]

The app's Motion from text panel runs this too (Install, `text.install`): it reads the numbered
step lines (`== [3/6] ...`) from the log, and with `--stop-on-eof` the setup ends, with everything
it started, when the app closes its stdin (Stop, or the app quitting).

Needs `uv` and `git` on PATH, about 35 GB of free disk (the install is 22 GB) and 32 GB of RAM (the text encoder, Llama 3 8B,
runs on the CPU unless the GPU has 24 GB). On Windows the motion-correction extension is built
with the Visual Studio C++ build tools (the same ones Rust uses).

The folder (default `%LOCALAPPDATA%/<product>/textmotion`, or `AC_TEXTMOTION_DIR`) gets:
  kimodo/          the Kimodo source at a pinned commit
  .venv/           Python 3.10 with torch, Kimodo and its motion correction
  text_encoders/   Llama 3 8B Instruct (an ungated copy whose weights hash the same as Meta's)
                   and the LLM2Vec adapters, pointed at it, so no Hugging Face login is needed
  hf/              the Hugging Face cache (Kimodo's weights)
  helper.py, generate.py, engine.py   copied from here
  ac_rig/          the rig maths the helper retargets with (training/ac_train's rig, quat, retarget)
  install.json     what was installed, read by the app
Run it again to repair or update: finished steps are skipped.
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
import tarfile
import threading
import time
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent

KIMODO_GIT = "https://github.com/nv-tlabs/kimodo.git"
KIMODO_COMMIT = "e1c219c99b4b94ba48be1fb6a1cbc64ecee4c954"  # 2026-10-15, the v1.2 weights
MODEL = "nvidia/Kimodo-SOMA-RP-v1.2"
MODEL_REVISION = "0e3c89024d896064b6851201373f7834784a386b"
LLAMA = "NousResearch/Meta-Llama-3-8B-Instruct"
LLAMA_REVISION = "53346005fb0ef11d3b6a83b12c895cca40156b6c"
# sha256 of meta-llama/Meta-Llama-3-8B-Instruct's shards (Hugging Face's LFS records, 2026-10-28):
# the copy is used only when every shard matches Meta's own
LLAMA_SHARDS = {
    "model-00001-of-00004.safetensors": "d8cf9c4d0dd972e1a2131bfe656235ee98221679711a3beef6d46dadf0f20b5c",
    "model-00002-of-00004.safetensors": "8d4782b4a69ef03845159ce1a15e272aadaaf134dc138d68f616098e8531729c",
    "model-00003-of-00004.safetensors": "3acdd690e65c24f42a24581b8467af98bd3ca357444580f8012aacd2bd607921",
    "model-00004-of-00004.safetensors": "67e9ad31c8c32abf3a55ee7fc7217b3ecb35fd3c74d98a5bd233e0e4d6964f46",
}
ADAPTERS = {
    "McGill-NLP/LLM2Vec-Meta-Llama-3-8B-Instruct-mntp": "31474e395ada192e8ed1586db6be79fb3b70c9c0",
    "McGill-NLP/LLM2Vec-Meta-Llama-3-8B-Instruct-mntp-supervised": "baa8ebf04a1c2500e61288e7dad65e8ae42601a7",
}
EIGEN_URL = "https://gitlab.com/libeigen/eigen/-/archive/3.4.0/eigen-3.4.0.tar.gz"
EIGEN_SHA256 = "8586084f71f9bde545ee7fa6d00288b264a2b7ac3607b974e54d13e7162c1c72"
TORCH_INDEX = "https://download.pytorch.org/whl/cu128"
SCRIPTS = ["helper.py", "generate.py", "engine.py"]
RIG_MODULES = ["rig.py", "quat.py", "retarget.py"]
INSTALL_VERSION = 1
STEPS = 6
# free disk a fresh install needs (torch, Kimodo, 16 GB of text encoder, the weights), and a resumed one
NEED_GB, NEED_GB_RESUMED = 35, 5


def product_name() -> str:
    """The product's name from `ac-core` (the one place it is spelled)."""
    text = (REPO / "crates" / "ac-core" / "src" / "product.rs").read_text(encoding="utf-8")
    m = re.search(r"macro_rules!\s*product_name\s*\{\s*\(\)\s*=>\s*\{\s*\"([^\"]+)\"", text)
    if not m:
        raise SystemExit("could not read the product name from crates/ac-core/src/product.rs")
    return m.group(1)


def default_dir() -> pathlib.Path:
    """`AC_TEXTMOTION_DIR`, else the per-user local data folder (the app looks in the same place)."""
    if os.environ.get("AC_TEXTMOTION_DIR"):
        return pathlib.Path(os.environ["AC_TEXTMOTION_DIR"])
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
        if not base:
            raise SystemExit("neither LOCALAPPDATA nor APPDATA is set: pass --dir")
        return pathlib.Path(base) / product_name() / "textmotion"
    if sys.platform == "darwin":
        return pathlib.Path.home() / "Library" / "Application Support" / product_name() / "textmotion"
    base = os.environ.get("XDG_DATA_HOME") or str(pathlib.Path.home() / ".local" / "share")
    return pathlib.Path(base) / product_name() / "textmotion"


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
    # an empty stdin unless one is given: with --stop-on-eof the setup's own stdin is the app's pipe,
    # and on Windows a child can't even start on a pipe another thread is blocked reading
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
        # the file descriptor itself, not sys.stdin: a thread blocked in sys.stdin holds its lock,
        # and Python then can't shut down cleanly when the setup ends
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
    need_gb = NEED_GB_RESUMED if (root / "text_encoders").exists() else NEED_GB
    if free < need_gb:
        raise SystemExit(f"{free:.0f} GB is free on the disk of {root}; the install needs about {need_gb} GB: "
                         "free some space, or install elsewhere with AC_TEXTMOTION_DIR")


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


def download(url: str, to: pathlib.Path, tries: int = 4) -> None:
    for i in range(tries):
        try:
            with urllib.request.urlopen(url, timeout=120) as r, open(to, "wb") as fh:
                shutil.copyfileobj(r, fh)
            return
        except OSError as e:
            print(f"  download failed ({e}); retry {i + 1} of {tries - 1}", flush=True)
            time.sleep(15)
    raise SystemExit(f"could not download {url}")


def has_nvidia() -> bool:
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
                             capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout
        return bool(out.strip())
    except (OSError, subprocess.CalledProcessError):
        return False


def gpu_memory_mib() -> int:
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
                             capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout
        return max(int(x) for x in out.split())
    except (OSError, subprocess.CalledProcessError, ValueError):
        return 0


def msvc_generator() -> str | None:
    """The CMake generator for the newest Visual Studio with the C++ tools, on Windows."""
    if os.name != "nt":
        return None
    vswhere = pathlib.Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / \
        "Microsoft Visual Studio" / "Installer" / "vswhere.exe"
    if not vswhere.exists():
        raise SystemExit("Visual Studio's C++ build tools were not found (vswhere.exe is missing): install "
                         "'Desktop development with C++' or the Build Tools, as for Rust")
    out = subprocess.run([str(vswhere), "-latest", "-products", "*", "-requires",
                          "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "catalog_productLineVersion"],
                         capture_output=True, stdin=subprocess.DEVNULL, text=True).stdout.strip()
    years = {"2022": "Visual Studio 17 2022", "2019": "Visual Studio 16 2019", "18": "Visual Studio 18 2026"}
    if out not in years:
        raise SystemExit(f"no Visual Studio with the C++ tools found (vswhere said {out!r})")
    return years[out]


def clone(root: pathlib.Path) -> None:
    src = root / "kimodo"
    if not (src / ".git").exists():
        run(["git", "clone", "--quiet", KIMODO_GIT, src])
    run(["git", "-C", src, "fetch", "--quiet", "origin", KIMODO_COMMIT])
    run(["git", "-C", src, "checkout", "--quiet", KIMODO_COMMIT])


def eigen(root: pathlib.Path, generator: str | None) -> pathlib.Path:
    """Eigen's headers, installed locally: Kimodo's build would fetch them from GitLab, which is
    sometimes too busy to answer a git clone; the release archive is checked by its hash."""
    deps = root / "deps"
    prefix = deps / "eigen-install"
    if (prefix / "share" / "eigen3" / "cmake" / "Eigen3Config.cmake").exists():
        return prefix
    deps.mkdir(parents=True, exist_ok=True)
    archive = deps / "eigen-3.4.0.tar.gz"
    if not archive.exists() or sha256(archive) != EIGEN_SHA256:
        download(EIGEN_URL, archive)
    if sha256(archive) != EIGEN_SHA256:
        raise SystemExit(f"{archive} does not match Eigen 3.4.0's hash")
    with tarfile.open(archive) as tf:
        tf.extractall(deps, filter="data")
    cmake = venv_bin(root, "cmake")
    gen = ["-G", generator] if generator else []
    run([cmake, "-S", deps / "eigen-3.4.0", "-B", deps / "eigen-build", *gen, f"-DCMAKE_INSTALL_PREFIX={prefix}",
         "-DBUILD_TESTING=OFF", "-DEIGEN_BUILD_DOC=OFF", "-DEIGEN_BUILD_PKGCONFIG=OFF"])
    run([cmake, "--install", deps / "eigen-build"])
    return prefix


def python_env(root: pathlib.Path, cpu: bool) -> str:
    py = venv_bin(root, "python")
    if not py.exists():
        run(["uv", "venv", "--quiet", "-p", "3.10", root / ".venv"])
    torch_cmd = ["uv", "pip", "install", "--quiet", "-p", py, "torch"]
    if not cpu:
        torch_cmd += ["--index-url", TORCH_INDEX]
    run(torch_cmd)
    run(["uv", "pip", "install", "--quiet", "-p", py, "cmake", "numpy", "huggingface_hub[cli]"])
    generator = msvc_generator()
    prefix = eigen(root, generator)
    env = dict(os.environ)
    env["CMAKE_PREFIX_PATH"] = str(prefix)
    env["PATH"] = str(py.parent) + os.pathsep + env.get("PATH", "")
    if generator:
        env["CMAKE_GENERATOR"] = generator
    run(["uv", "pip", "install", "--quiet", "-p", py, f"{root / 'kimodo'}[all]"], env=env)
    out = subprocess.run([str(py), "-c", "import torch, kimodo, motion_correction; print(torch.__version__, "
                          "torch.cuda.is_available())"], capture_output=True, stdin=subprocess.DEVNULL, text=True, check=True).stdout.split()
    return f"torch {out[0]}, CUDA {'yes' if out[1] == 'True' else 'no'}"


def hf(root: pathlib.Path, repo: str, revision: str, to: pathlib.Path | None, *extra: str) -> None:
    env = dict(os.environ, HF_HOME=str(root / "hf"))
    cmd = [venv_bin(root, "hf"), "download", repo, "--revision", revision, *extra]
    if to is not None:
        cmd += ["--local-dir", to]
    run(cmd, env=env)


def text_encoder(root: pathlib.Path) -> None:
    te = root / "text_encoders"
    llama = te / "meta-llama" / "Meta-Llama-3-8B-Instruct"
    shards_ok = all((llama / f).exists() for f in LLAMA_SHARDS)
    if not shards_ok:
        hf(root, LLAMA, LLAMA_REVISION, llama, "--exclude", "original/*")
    for name, want in LLAMA_SHARDS.items():
        got = sha256(llama / name)
        if got != want:
            raise SystemExit(f"{llama / name} does not hash the same as Meta's release ({got}): delete it and run again")
    print("  Llama 3 8B Instruct: every shard matches Meta's release", flush=True)
    for repo, rev in ADAPTERS.items():
        dest = te / repo
        hf(root, repo, rev, dest)
        cfg_path = dest / "adapter_config.json"
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
        cfg["base_model_name_or_path"] = llama.as_posix()
        cfg_path.write_text(json.dumps(cfg, indent=2), encoding="utf-8")


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description="Install Kimodo for the app and for training-data generation")
    ap.add_argument("--dir", type=pathlib.Path, help="where to install (default: AC_TEXTMOTION_DIR or the per-user data folder)")
    ap.add_argument("--cpu", action="store_true", help="CPU-only torch (very slow; for machines without an NVIDIA GPU)")
    ap.add_argument("--stop-on-eof", action="store_true", help="end when stdin closes (the app runs it so)")
    args = ap.parse_args(argv)
    if args.stop_on_eof:
        stop_on_eof()
    root = (args.dir or default_dir()).resolve()
    need("uv", "install it from https://docs.astral.sh/uv/")
    need("git", "install Git")
    cpu = args.cpu or not has_nvidia()
    if cpu and not args.cpu:
        print("No NVIDIA GPU found (nvidia-smi): installing CPU-only torch. Generating will be very slow.")
    check_space(root)
    root.mkdir(parents=True, exist_ok=True)
    print(f"Installing Kimodo into {root}")

    step(1, "Kimodo source")
    clone(root)
    step(2, "Python environment (torch, Kimodo, motion correction)")
    torch_info = python_env(root, cpu)
    print(f"  {torch_info}")
    step(3, "Text encoder (Llama 3 8B Instruct and the LLM2Vec adapters)")
    text_encoder(root)
    step(4, f"Kimodo weights ({MODEL})")
    hf(root, MODEL, MODEL_REVISION, None)
    step(5, "Helper scripts")
    for name in SCRIPTS:
        shutil.copy2(HERE / name, root / name)
    # the rig maths the helper retargets with: training's own modules (numpy only), copied
    pkg = root / "ac_rig"
    pkg.mkdir(exist_ok=True)
    (pkg / "__init__.py").write_text(
        '"""Copied from training/ac_train by setup_textmotion.py."""\n\n'
        "ASSETS = None  # Skeleton.load imports it; the helper always passes the rig's path\n",
        encoding="utf-8")
    for name in RIG_MODULES:
        shutil.copy2(REPO / "training" / "ac_train" / name, pkg / name)
    # Llama on the GPU needs about 17 GB on top of the motion model; below that it runs on the CPU
    encoder_device = "cuda" if not cpu and gpu_memory_mib() >= 24000 else "cpu"
    install = {
        "version": INSTALL_VERSION,
        "kimodo_commit": KIMODO_COMMIT,
        "model": MODEL,
        "model_revision": MODEL_REVISION,
        "python": str(venv_bin(root, "python")),
        "helper": str(root / "helper.py"),
        "env": {
            "HF_HOME": str(root / "hf"),
            "TEXT_ENCODERS_DIR": (root / "text_encoders").as_posix(),
            "TEXT_ENCODER_MODE": "local",
            "TEXT_ENCODER_DEVICE": encoder_device,
            "PYTHONUTF8": "1",
        },
        "torch": torch_info,
        "platform": platform.platform(),
    }
    step(6, "Check: the helper loads the model and makes one second of motion")
    py = venv_bin(root, "python")
    env = dict(os.environ, **install["env"])
    req = json.dumps({"id": 1, "op": "generate", "prompt": "A person walks forward.", "frames": 30, "seed": 1,
                      "rig": str(REPO / "assets" / "rig" / "default_rig.json")})
    p = popen([py, "-I", root / "helper.py"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
              stderr=subprocess.PIPE, text=True, env=env)
    try:
        stdout, stderr = p.communicate(req + "\n", timeout=1800)
    except subprocess.TimeoutExpired:
        kill_tree(p)
        raise SystemExit("the helper did not answer the check within 30 minutes") from None
    finally:
        CURRENT.remove(p)
    replies = [json.loads(line) for line in stdout.splitlines() if line.startswith("{")]
    done = [r for r in replies if r.get("id") == 1 and "result" in r]
    if not done:
        sys.stderr.write(stderr[-4000:])
        raise SystemExit("the helper did not answer the check; see the messages above")
    # written last: the app takes install.json as "installed"
    (root / "install.json").write_text(json.dumps(install, indent=2), encoding="utf-8")
    print(f"\nKimodo is installed in {root}")
    print(f"  text encoder on the {encoder_device.upper()}; first use loads it (about a minute on the CPU)")
    print("\n== done", flush=True)  # the app reads this (with install.json) as finished


if __name__ == "__main__":
    main()
