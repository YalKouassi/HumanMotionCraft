# Linux: the app builds from source

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** none
- **Source:** GitHub issue [#1](https://github.com/YalKouassi/HumanMotionCraft/issues/1) (Fedora, x86_64), reviewed 2026-10-30

## What
`cargo run --release -p ac-app` fails on any Linux host, so nobody can build the app there. macOS
hosts are affected the same way.

**Cause.** `crates/ac-app/build.rs:17` names `winresource` in code that is always compiled. The
check at `build.rs:14` is a runtime one, so it skips the call but not the compile. The dependency
is only declared for Windows (`crates/ac-app/Cargo.toml:37`), and that `cfg` on a build dependency
is evaluated for the host, so on a Linux host the crate is missing. The issue's diagnosis is right
about the file, but its suggested fix isn't the best one (see below).

**Fix.** Move `winresource` to a plain `[build-dependencies]` and keep the runtime check.
`CARGO_CFG_TARGET_OS` is the correct test because it reads the target, while `#[cfg(windows)]` in a
build script reads the host and would silently drop the version resource from a cross-compile to
Windows. This also makes the cross-compile behaviour described at `build.rs:7-9` true; today a
non-Windows host can't compile the script at all. `winresource` and its dependencies (`toml`,
`version_check`) are already in `Cargo.lock`, and the cost is a few extra small crates at build
time on non-Windows hosts.

**Guard.** CI only builds the app on Windows (`.github/workflows/ci.yml:35`; the `ubuntu-latest` job
is the Python training checks), which is how this went unnoticed. Add a Linux job that compiles the
workspace, using the same `CRAFT_LIBS_TOKEN` step as the Windows gate.

**Checked 2026-10-30 from Windows** with `cargo check --workspace --all-targets --target
x86_64-unknown-linux-gnu`:

- It compiles `ac-ai`, `ac-core`, `ac-io`, `ac-media`, `ac-render`, `ac-review`, `ac-textmotion`,
  the `craft-*` libraries, `eframe`, `egui`, `winit` and `rfd` with no errors.
- It does not reach `ac-community`, `ac-cli` or `ac-app`: they depend on `ring`, which needs a Linux
  C compiler that machine doesn't have. So those three are unproven, as is everything at run time.
- It cannot reproduce the bug, because the build script compiles for the Windows host.
- `winit` resolves with both the `x11` and `wayland` features on.
- `ac-app/src/app/files/mod.rs` and `ac-textmotion` already have Linux branches.

**Not in this item.** A Linux release build, a Linux launcher entry and Linux visual goldens. The
goldens are rendered on WARP, so Linux CI compiles but doesn't run the visual tests. A published
Linux release is the owner's decision and gets its own item if wanted.

## Acceptance
Agreed by the owner 2026-10-30. The owner has no Linux machine, so CI and the reporter supply the
evidence and the owner makes the call from it:

- On a Linux x86_64 host, `cargo build --release --locked -p ac-app -p ac-cli` succeeds on
  `ubuntu-latest` and in a `fedora:latest` container. Any system packages this needs (at least a C
  compiler) are listed in the README.
- A new `ubuntu-latest` CI job runs `cargo check --workspace --all-targets --locked` and passes.
- The built app opens a window, loads a fixture and plays it on a Linux desktop, confirmed by the
  issue's reporter, ideally with a screenshot or short video. Anything broken at run time becomes
  its own numbered item.
- If the reporter doesn't answer, the issue is closed once CI passes, with a request to reopen it if
  the app doesn't run. The item stays `built, waiting for the owner` until the owner decides whether
  the passing build is enough.
- Windows is unchanged: `cargo xtask check` passes, and the `release.yml` ProductVersion check still
  passes on a manual run.
- Issue #1 is answered, asking the reporter to try the fix, and closed once the above holds.

## Result
Built 2026-10-30 on `session/2026-10-30-roadmap` (not pushed; the issue reply waits for the merge).

**What changed.**
- `crates/ac-app/Cargo.toml`: `winresource` is a plain `[build-dependencies]` entry (was
  `[target.'cfg(windows)'.build-dependencies]`), with a comment saying why. `Cargo.lock` is
  unchanged: `winresource`, `toml` and `version_check` were already in it, and
  `cargo tree --locked -p ac-app -i winresource` resolves without a rewrite.
- `crates/ac-app/build.rs`: the `CARGO_CFG_TARGET_OS` check is kept; the header comment now says
  what is true (the test reads the target, the crate compiles on every host, a cross-compile to
  Windows still embeds the resource).
- `.github/workflows/ci.yml`: a `linux` job on `ubuntu-latest`, with the same
  `CRAFT_LIBS_TOKEN` step as the Windows gate, `apt-get install build-essential`,
  `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache`, then
  `cargo check --workspace --all-targets --locked`. It compiles only: the visual goldens are WARP
  renders, so tests and fixtures stay on Windows.
- `README.md`: "Build on Linux" (Debian/Ubuntu `build-essential git`, Fedora `gcc git`, then
  `cargo build --release --locked -p ac-app -p ac-cli`).

**The package list, and why it is short.** `cargo tree --target x86_64-unknown-linux-gnu -e features`
shows every windowing library resolved with its dlopen feature: `wayland-sys` and
`wayland-backend` `dlopen`, `khronos-egl` `dynamic`, `x11rb` `dl-libxcb`, `x11-dl`,
`xkbcommon-dl`; `rfd` 0.17 uses the xdg portal over its own Wayland client, no GTK. So no `-dev`
package is linked. `cargo tree -i cc` names the only C users: `ring` (via `rustls` via `ureq` in
`ac-community`) and `wayland-backend`, whose build script in dlopen mode emits only check-cfg
lines (seen in `target/x86_64-unknown-linux-gnu/debug/build/wayland-backend-*/output`). Hence: a
C compiler, and `git` for the git dependencies (`CARGO_NET_GIT_FETCH_WITH_CLI`). `pkg-config` is
not needed (`x11-dl`'s build script probes it and tolerates its absence; `wayland-sys` with
`dlopen` never calls it), so it is not listed. If a Fedora container run shows otherwise, add the
package to the README and this file rather than to CI alone.

**Proven on Windows (2026-10-30).**
- `cargo check -p ac-app --target x86_64-unknown-linux-gnu --keep-going` with the fix: `ac-ai`,
  `ac-core`, `ac-io`, `ac-media`, `ac-render`, `ac-textmotion`, the `craft-*` crates, `eframe`,
  `egui`, `egui-winit`, `egui-wgpu`, `smithay-client-toolkit`, `accesskit_unix` check clean for the
  Linux target; the build scripts of `ac-app`, `rfd`, `winit` and `wgpu` compile and run. The
  `ac-app` build script's recorded output for the Linux target is just its two `rerun-if` lines:
  it returned on the `CARGO_CFG_TARGET_OS` test without touching `winresource`. The one failure,
  verbatim: `error: failed to run custom build command for ring v0.17.14` ...
  `ToolNotFound: failed to find tool "x86_64-linux-gnu-gcc": program not found`. That is the
  missing Linux C compiler on this machine, as the owner saw, so `ac-community`, `ac-cli` and
  `ac-app` itself remain unproven here. This run still cannot reproduce the original bug, because
  the build script compiles for the Windows host either way; the fix is proven by reasoning (the
  crate is now declared for every host) and will be proven by the Linux job.
- `cargo xtask check` on Windows: `check passed` (fmt, clippy, 271 tests including the visual
  goldens, 76 fixtures, names, assets, docs), log in `target/xtask-check-120400.log`.

**Only CI on Linux (or the reporter) can prove.** That `ring` builds with the runner's gcc and the
three crates above check; that the `--locked` build succeeds with no extra package; that the app
opens a window, loads a fixture and plays it (run time: Vulkan driver, Wayland/X11 at run time).
The `fedora:latest` container build from the acceptance is not in CI (one Linux job is the guard;
Fedora differs only in package names) and is left to the reporter, who is on Fedora. The
`release.yml` ProductVersion check is unchanged in code; its next manual run is the owner's.

**Decisions made without asking.** One Linux job, not a matrix; `cargo check` only, no tests
(WARP goldens); `pkg-config` left out of the list with the reason above; the acceptance's
Fedora container is not run in CI.

## Draft reply to issue #1 (not posted)
Thanks for the clear report, and for finding the file. You are right that `crates/ac-app/build.rs`
is the cause: it names `winresource` in code that is always compiled, but the crate was declared
only under `[target.'cfg(windows)'.build-dependencies]`, and that `cfg` on a build dependency is
evaluated for the *host*, so on Linux (and macOS) the crate was missing and the script could not
compile. The runtime `CARGO_CFG_TARGET_OS` check inside it was fine; it is the right test because
it reads the target, so a cross-compile to Windows keeps its version resource.

The fix on `main` is to make `winresource` a plain build dependency (a few small crates at build
time on non-Windows hosts, nothing at run time). CI now compiles the whole workspace on
`ubuntu-latest` as well, so this cannot come back unnoticed.

Could you try again on your Fedora machine?

```bash
sudo dnf install gcc git
git pull
cargo build --release --locked -p ac-app -p ac-cli
./target/release/humanmotioncraft
```

Only a C compiler is needed (for `ring`, the TLS library); the Wayland, X11, xkbcommon and EGL
libraries are loaded at run time from your desktop, so no `-dev` packages. If the window opens,
please load a fixture and press play, e.g.
`./target/release/humanmotioncraft --script fixtures/ai-fill.json` (and
`./target/release/ac fixtures` runs them all headless), and tell us whether it plays; a screenshot
or short clip would be ideal. Anything that misbehaves at run time
gets its own issue. If I don't hear back, I'll close this once CI is green, and please reopen if
the app doesn't run for you.
