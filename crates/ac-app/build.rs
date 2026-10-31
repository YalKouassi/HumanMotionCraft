//! Windows only: embed version info (VERSIONINFO) into `humanmotioncraft.exe`.
//!
//! Craft Unofficial Launcher reads the ProductVersion of a portable install's executable to
//! confirm it unpacked the version it chose, and rolls the install back when it is missing. The
//! version comes from Cargo.toml (`CARGO_PKG_VERSION`), so it always matches the release tag.
//!
//! On every other target this does nothing. The test reads `CARGO_CFG_TARGET_OS` (the target) at
//! run time, not `cfg(windows)` (the host), so a cross-compile to Windows from macOS or Linux
//! still embeds the resource, and `winresource` is a plain build dependency so that the script
//! compiles on every host. A missing resource compiler is a warning, so such a cross-compile
//! still links, unless `HUMANMOTIONCRAFT_REQUIRE_WINRES=1` (set by the release workflow) turns it
//! into an error.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=HUMANMOTIONCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "HumanMotionCraft")
        .set("FileDescription", "HumanMotionCraft")
        .set("CompanyName", "HumanMotionCraft contributors")
        .set(
            "LegalCopyright",
            "Copyright (c) the HumanMotionCraft authors. GPL-3.0-or-later.",
        )
        .set("OriginalFilename", "humanmotioncraft.exe")
        .set("InternalName", "humanmotioncraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("HUMANMOTIONCRAFT_REQUIRE_WINRES").is_some() {
            println!("cargo:warning=embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=humanmotioncraft.exe built without version resources: {e}");
    }
}
