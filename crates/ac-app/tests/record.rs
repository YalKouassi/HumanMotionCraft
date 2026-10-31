//! `--record` and `--size` in a real window (roadmap 159000). A window needs a display and takes
//! over the screen while the tests run, so these are run by hand:
//! `cargo test -p ac-app --test record -- --ignored --test-threads=1`
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = repo().join("target/test-scratch/record").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn window(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_humanmotioncraft"))
        .arg("--assets")
        .arg(repo().join("assets"))
        .args(["--prefs", "memory", "--open", "Locomotion/Walk"])
        .args(args)
        .output()
        .unwrap()
}

/// Width, height and frames, as ffprobe counts them.
fn probe(file: &Path) -> (u32, u32, u32) {
    let ffmpeg = ac_media::ffmpeg();
    let ffprobe = Path::new(&ffmpeg).with_file_name(format!(
        "ffprobe{}",
        Path::new(&ffmpeg)
            .extension()
            .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()))
    ));
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-count_frames", "-select_streams", "v"])
        .args([
            "-show_entries",
            "stream=width,height,nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let n: Vec<u32> = text.trim().split(',').map(|x| x.parse().unwrap()).collect();
    (n[0], n[1], n[2])
}

fn said(r: &Output) -> String {
    String::from_utf8_lossy(&r.stderr).to_string()
}

#[test]
#[ignore = "opens a window"]
fn a_recording_has_a_picture_for_each_frame_and_closes() {
    let out = scratch("whole").join("walk.mp4");
    let r = window(&["--record", out.to_str().unwrap()]);
    assert!(r.status.success(), "{}", said(&r));
    let (_, _, frames) = probe(&out);
    assert_eq!(frames, 33);
}

#[test]
#[ignore = "opens a window"]
fn the_size_is_exact_and_an_mp4_rounds_down_to_even() {
    let dir = scratch("sizes");
    let shot = dir.join("a.png");
    let r = window(&["--screenshot", shot.to_str().unwrap(), "--size", "1001x701"]);
    assert!(r.status.success(), "{}", said(&r));
    assert_eq!(probe_picture(&shot), (1001, 701));
    let video = dir.join("a.mp4");
    let r = window(&[
        "--record",
        video.to_str().unwrap(),
        "--to",
        "2",
        "--size",
        "1281x721",
    ]);
    assert!(r.status.success(), "{}", said(&r));
    assert_eq!(probe(&video), (1280, 720, 3));
    let gif = dir.join("a.gif");
    let r = window(&[
        "--record",
        gif.to_str().unwrap(),
        "--to",
        "2",
        "--size",
        "321x241",
    ]);
    assert!(r.status.success(), "{}", said(&r));
    assert_eq!(probe(&gif), (321, 241, 3));
}

fn probe_picture(file: &Path) -> (u32, u32) {
    let (w, h, _) = ac_render::offscreen::unpng(&std::fs::read(file).unwrap()).unwrap();
    (w, h)
}

/// A file where a folder is needed: nothing can be written under it, on any machine.
#[test]
#[ignore = "opens a window"]
fn a_failed_write_says_why_and_exits_non_zero() {
    let dir = scratch("fails");
    let not_a_folder = dir.join("file");
    std::fs::write(&not_a_folder, "").unwrap();
    for (flag, name) in [
        ("--record", "a.mp4"),
        ("--record", "a.gif"),
        ("--screenshot", "a.png"),
    ] {
        let path = not_a_folder.join(name);
        let mut args = vec![flag, path.to_str().unwrap()];
        if flag == "--record" {
            args.extend(["--to", "1"]);
        }
        let r = window(&args);
        assert!(!r.status.success(), "{name}");
        assert!(said(&r).contains(flag), "{name}: {}", said(&r));
    }
    let r = window(&[
        "--record",
        dir.join("a.mp4").to_str().unwrap(),
        "--from",
        "9",
        "--to",
        "2",
    ]);
    assert!(!r.status.success());
}
