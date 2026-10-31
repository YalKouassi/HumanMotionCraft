//! uv for the install when the person has none: a pinned release from uv's GitHub page, fetched
//! with the system's own `curl` (or `wget`) and unpacked with its `tar` (both come with Windows 10
//! and later, macOS and desktop Linux), checked against the hash pinned here, into `<install>/uv`.
//! uv then downloads the Python the setup runs on, so nothing need be installed first and no
//! account is needed. The setup runs these as child processes ([`crate::setup`]) so the window
//! never waits on the network.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The uv release fetched.
pub const VERSION: &str = "0.13.0";
/// Its download, in MB (the largest of the archives below), for the requirement's note.
pub const MB: u32 = 16;

/// One platform's archive of the release.
struct Asset {
    target: &'static str,
    /// From the release's `<archive>.sha256` files.
    sha256: &'static str,
    /// A zip (Windows) with the programs at its top, else a tar.gz with them in a folder.
    zip: bool,
}

fn asset() -> Option<Asset> {
    let (target, sha256, zip) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => (
            "x86_64-pc-windows-msvc",
            "088962f9e7b7bd9ea740c04c650b2a21c8928c345bd99ac24350dc924dba656c",
            true,
        ),
        ("windows", "aarch64") => (
            "aarch64-pc-windows-msvc",
            "cb54028b59aa87f11cf6dec293ce000420043a49a8aea522e822a671321f8932",
            true,
        ),
        // the static (musl) build: it runs on any Linux, whatever its C library
        ("linux", "x86_64") => (
            "x86_64-unknown-linux-musl",
            "b0828fc4cdc47d65951e94b5f0a15a5364420e8dcd88723f3b21e7a2eb30bbc3",
            false,
        ),
        ("linux", "aarch64") => (
            "aarch64-unknown-linux-musl",
            "281ca6cf43f4e26e35474839ad4861be86de42af2682f74134460f5b2071b96d",
            false,
        ),
        ("macos", "x86_64") => (
            "x86_64-apple-darwin",
            "5f44dcbde809b632f47c36fadb241cb4d6f9af71d0c8f172b5d2026d3dde742c",
            false,
        ),
        ("macos", "aarch64") => (
            "aarch64-apple-darwin",
            "a9c1b29002cf3c83f07fa9cd8a887a3be0107d90e23189721221e7257db8e3d6",
            false,
        ),
        _ => return None,
    };
    Some(Asset { target, sha256, zip })
}

impl Asset {
    fn file(&self) -> String {
        format!("uv-{}.{}", self.target, if self.zip { "zip" } else { "tar.gz" })
    }

    fn url(&self) -> String {
        format!(
            "https://github.com/astral-sh/uv/releases/download/{VERSION}/{}",
            self.file()
        )
    }
}

/// Where the fetched uv lives in the install folder `dir`.
pub fn home(dir: &Path) -> PathBuf {
    dir.join("uv")
}

/// The fetched uv, there or not.
pub fn exe(dir: &Path) -> PathBuf {
    home(dir).join(crate::setup::exe("uv"))
}

/// The programs the fetch runs: Windows' own (in System32, so another `tar` on PATH, which may
/// not read zips, is never picked), else the first on PATH.
fn programs() -> Result<(Downloader, PathBuf), String> {
    if cfg!(windows) {
        let sys = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join("System32");
        let (curl, tar) = (sys.join("curl.exe"), sys.join("tar.exe"));
        if curl.is_file() && tar.is_file() {
            return Ok((Downloader::Curl(curl), tar));
        }
        return Err(
            "Windows' curl.exe and tar.exe were not found (Windows 10 has had them since 2018): \
                    install uv yourself with `winget install astral-sh.uv`"
                .into(),
        );
    }
    let down = crate::setup::on_path("curl")
        .map(Downloader::Curl)
        .or_else(|| crate::setup::on_path("wget").map(Downloader::Wget))
        .ok_or("neither curl nor wget was found to download it with: install curl, or uv yourself (https://docs.astral.sh/uv/)")?;
    let tar = crate::setup::on_path("tar").ok_or(
        "tar was not found to unpack it with: install tar, or uv yourself (https://docs.astral.sh/uv/)",
    )?;
    Ok((down, tar))
}

enum Downloader {
    Curl(PathBuf),
    Wget(PathBuf),
}

/// Whether uv can be fetched here (`Err`: why not, and what to do instead).
pub fn possible() -> Result<(), String> {
    asset().ok_or_else(|| {
        format!(
            "uv has no build for {} on {}: install it yourself (https://docs.astral.sh/uv/)",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    })?;
    programs().map(|_| ())
}

fn archive(dir: &Path, a: &Asset) -> PathBuf {
    home(dir).join(a.file())
}

/// What the log says as the fetch begins.
pub fn announce() -> String {
    format!("Downloading uv {VERSION} (about {MB} MB), which installs Python and everything else\n")
}

/// The download, made ready to run (its progress goes to the log).
pub fn download(dir: &Path) -> Result<Command, String> {
    let a = asset().ok_or("uv has no build for this computer")?;
    let (down, _) = programs()?;
    std::fs::create_dir_all(home(dir)).map_err(|e| format!("can't make {}: {e}", home(dir).display()))?;
    let to = archive(dir, &a);
    let mut cmd = match down {
        Downloader::Curl(p) => {
            let mut c = Command::new(p);
            c.args([
                "-fL",
                "--retry",
                "3",
                "--retry-delay",
                "5",
                "--progress-bar",
                "-o",
            ]);
            c
        }
        Downloader::Wget(p) => {
            let mut c = Command::new(p);
            c.args(["--tries=4", "--progress=dot:mega", "-O"]);
            c
        }
    };
    cmd.arg(to).arg(a.url());
    Ok(cmd)
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("can't read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(buf.get(..n).unwrap_or_default());
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Check the download against the pinned hash (a wrong one is deleted), then the unpacking, ready
/// to run.
pub fn unpack(dir: &Path) -> Result<Command, String> {
    let a = asset().ok_or("uv has no build for this computer")?;
    let (_, tar) = programs()?;
    let file = archive(dir, &a);
    let got = sha256(&file)?;
    if got != a.sha256 {
        let _ = std::fs::remove_file(&file); // a bad download; the error below says what happened
        return Err(format!(
            "the uv downloaded from {} is not uv {VERSION}'s release (its hash is {got}); Install tries again",
            a.url()
        ));
    }
    let mut cmd = Command::new(tar);
    cmd.arg("-xf").arg(&file).arg("-C").arg(home(dir));
    if !a.zip {
        cmd.arg("--strip-components=1");
    }
    Ok(cmd)
}

/// After unpacking: the uv to run (the archive is deleted).
pub fn finish(dir: &Path) -> Result<PathBuf, String> {
    let uv = exe(dir);
    if !uv.is_file() {
        return Err(format!(
            "uv's archive unpacked, but {} is not there",
            uv.display()
        ));
    }
    if let Some(a) = asset() {
        // only space: a leftover archive is fetched over next time
        let _ = std::fs::remove_file(archive(dir, &a));
    }
    Ok(uv)
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_computer_has_a_build_and_its_hash_is_whole() {
        let Some(a) = super::asset() else {
            return; // only Windows, Linux and macOS on x86_64 or aarch64 have one
        };
        assert_eq!(a.sha256.len(), 64);
        assert!(
            a.sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(a.url().ends_with(if a.zip { ".zip" } else { ".tar.gz" }));
        assert!(a.url().contains(super::VERSION));
    }

    #[test]
    fn a_file_hashes_as_sha256_says() {
        let path = std::env::temp_dir().join(format!("ac-uv-fetch-{}.txt", std::process::id()));
        assert!(std::fs::write(&path, b"abc").is_ok());
        let got = super::sha256(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            got.as_deref(),
            Ok("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }
}
