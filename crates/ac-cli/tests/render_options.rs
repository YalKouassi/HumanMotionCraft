//! `ac render` and `ac video`'s picture options, through the command line (roadmap 160000).
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn ac(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ac"))
        .arg("--assets")
        .arg(repo().join("assets"))
        .args(args)
        .output()
        .unwrap()
}

/// A scratch folder holding a script that opens the built-in Walk.
fn scratch(name: &str) -> (PathBuf, String) {
    let dir = repo().join("target/test-scratch/render-options").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("walk.json");
    std::fs::write(
        &script,
        r#"[{"command": "clip.open", "params": {"id": "Locomotion/Walk"}}]"#,
    )
    .unwrap();
    (dir, script.to_string_lossy().into_owned())
}

fn render(script: &str, out: &PathBuf, extra: &[&str]) -> (u32, u32, Vec<u8>) {
    let out_text = out.to_string_lossy();
    let mut args = vec![
        "render", "--script", script, "--frame", "8", "--size", "200x150", "--out", &out_text,
    ];
    args.extend_from_slice(extra);
    let r = ac(&args);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    ac_render::offscreen::unpng(&std::fs::read(out).unwrap()).unwrap()
}

#[test]
fn a_transparent_picture_is_the_opaque_body_and_nothing_else() {
    let (dir, script) = scratch("transparent");
    let (w, h, rgba) = render(
        &script,
        &dir.join("a.png"),
        &["--background", "transparent", "--no-floor"],
    );
    let alpha: Vec<u8> = rgba.chunks(4).map(|p| p[3]).collect();
    assert_eq!(alpha.len(), (w * h) as usize);
    assert!(
        alpha.iter().all(|a| *a == 0 || *a == 255),
        "every pixel is the body or nothing"
    );
    let body = alpha.iter().filter(|a| **a == 255).count();
    // the mannequin takes a real part of the picture, and the corners are clear of it
    assert!(
        body > 500 && body < alpha.len() / 2,
        "{body} of {} pixels",
        alpha.len()
    );
    assert_eq!(alpha[0], 0);
    assert_eq!(alpha[alpha.len() - 1], 0);
}

#[test]
fn a_flat_background_has_no_grid() {
    let (dir, script) = scratch("flat");
    let (_, _, rgba) = render(
        &script,
        &dir.join("a.png"),
        &["--background", "#00ff00", "--no-floor"],
    );
    let green = rgba.chunks(4).filter(|p| p == &[0, 255, 0, 255]).count();
    let body = rgba.chunks(4).filter(|p| p[1] != 255).count();
    assert!(body > 500, "{body}");
    assert_eq!(green + body, rgba.len() / 4);
    // the bottom row is where the floor would run
    let row = &rgba[rgba.len() - 200 * 4..];
    assert!(
        row.chunks(4).all(|p| p == [0, 255, 0, 255]),
        "no grid along the bottom"
    );
    let (_, _, usual) = render(&script, &dir.join("b.png"), &[]);
    assert_eq!(usual.chunks(4).filter(|p| p == &[0, 255, 0, 255]).count(), 0);
}

/// The three-quarter view's own values give its picture.
#[test]
fn your_own_camera_with_a_views_values_is_that_view() {
    let (dir, script) = scratch("camera");
    let (_, _, named) = render(&script, &dir.join("a.png"), &["--view", "three-quarter"]);
    let (_, _, own) = render(&script, &dir.join("b.png"), &["--camera", "0.6,0.25,3.6"]);
    let (n, _) = ac_render::offscreen::diff(&named, &own, 2);
    assert_eq!(n, 0, "{n} pixels differ");
    let (_, _, moved) = render(&script, &dir.join("c.png"), &["--camera", "0.6,0.25,6"]);
    assert!(
        ac_render::offscreen::diff(&named, &moved, 2).0 > 100,
        "further back looks different"
    );
}

/// Needs ffmpeg, as `clip.video`'s other MP4 checks do.
#[test]
fn an_mp4_is_the_body_on_flat_green() {
    let (dir, script) = scratch("green");
    let out = dir.join("a.mp4");
    let r = ac(&[
        "video",
        "--script",
        &script,
        "--to",
        "4",
        "--size",
        "320x240",
        "--no-floor",
        "--background",
        "#00ff00",
        "--out",
        &out.to_string_lossy(),
    ]);
    let said = String::from_utf8_lossy(&r.stderr).to_string();
    if said.contains("ffmpeg") {
        eprintln!("skipped: {said}");
        return;
    }
    assert!(r.status.success(), "{said}");
    let png = dir.join("frame.png");
    let ffmpeg = std::env::var("AC_FFMPEG").unwrap_or_else(|_| "C:/tools/ffmpeg/bin/ffmpeg.exe".into());
    let got = Command::new(ffmpeg)
        .args(["-y", "-v", "error", "-i"])
        .arg(&out)
        .args(["-frames:v", "1"])
        .arg(&png)
        .output();
    let Ok(got) = got else {
        eprintln!("skipped: no ffmpeg to read the frame back");
        return;
    };
    assert!(got.status.success(), "{}", String::from_utf8_lossy(&got.stderr));
    let (w, h, rgba) = ac_render::offscreen::unpng(&std::fs::read(&png).unwrap()).unwrap();
    assert_eq!((w, h), (320, 240));
    // H.264 shifts colours a little
    let greenish = |p: &[u8]| p[0] < 40 && p[1] > 215 && p[2] < 40;
    let row = &rgba[rgba.len() - 320 * 4..];
    assert!(row.chunks(4).all(greenish), "no grid along the bottom");
    assert!(greenish(&rgba[..4]));
    assert!(
        rgba.chunks(4).filter(|p| !greenish(p)).count() > 500,
        "the body is there"
    );
}

#[test]
fn refusals_say_why() {
    let (dir, script) = scratch("refusals");
    for ext in ["mp4", "gif"] {
        let out = dir.join(format!("a.{ext}"));
        let r = ac(&[
            "video",
            "--script",
            &script,
            "--background",
            "transparent",
            "--out",
            &out.to_string_lossy(),
        ]);
        assert!(!r.status.success());
        let said = String::from_utf8_lossy(&r.stderr).to_string();
        assert!(said.contains("PNG"), "{said}");
        assert!(!out.exists());
    }
    let out = dir.join("a.png");
    for (flag, value, word) in [
        ("--camera", "NaN,0,3", "azimuth"),
        ("--camera", "0,0,1e9", "distance"),
        ("--camera", "0,2,3", "elevation"),
        ("--camera", "0,0", "azimuth,elevation,distance"),
        ("--background", "green", "#rrggbb"),
    ] {
        let r = ac(&[
            "render",
            "--script",
            &script,
            flag,
            value,
            "--out",
            &out.to_string_lossy(),
        ]);
        assert!(!r.status.success(), "{value}");
        let said = String::from_utf8_lossy(&r.stderr).to_string();
        assert!(said.contains(word), "{value}: {said}");
    }
}
