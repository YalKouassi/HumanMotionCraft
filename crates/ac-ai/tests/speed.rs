//! Where a fill's time goes (roadmap 125000): the network's forward pass against the rest of the
//! fill (sampling, the window's frames, the 6D round trip). Ignored: it prints times, checks
//! nothing, and only means something in a release build:
//! `cargo test -p ac-ai --release --test speed -- --ignored --nocapture`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::time::Instant;

use ac_ai::Model;
use ac_core::Skeleton;
use ac_core::ai::{FillSettings, Inbetweener};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The median of `n` runs of `f`, in milliseconds.
fn median_ms(n: usize, mut f: impl FnMut()) -> f64 {
    let mut t: Vec<f64> = (0..n)
        .map(|_| {
            let s = Instant::now();
            f();
            s.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    t.sort_by(f64::total_cmp);
    t[n / 2]
}

#[test]
#[ignore = "prints timings; run by hand in a release build"]
fn where_a_fill_spends_its_time() {
    let m = Model::load(&repo().join("assets/models/parts")).unwrap();
    let sk =
        Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
            .unwrap();
    let clip = ac_core::clipfile::load_clip(
        &std::fs::read_to_string(repo().join("assets/library/animations/Locomotion/Walk.clip.json")).unwrap(),
    )
    .unwrap();
    let net = m.net();
    // a 30-frame gap after 10 frames of context: 41 frames in the window
    let (ctx, gap) = (m.info().context, 30);
    let t = ctx + gap + 1;
    let f = net.feat;
    let known: Vec<f32> = (0..t * f).map(|i| ((i % 13) as f32 - 6.0) * 0.05).collect();
    let interp = known.clone();
    let mask: Vec<f32> = (0..t)
        .map(|i| if i < ctx || i == t - 1 { 1.0 } else { 0.0 })
        .collect();
    let ttg: Vec<f32> = (0..t).map(|i| (t - 1 - i) as f32 / 45.0).collect();
    let gmask = vec![0.0f32; net.given];
    let forward = median_ms(9, || {
        net.forward_contacts(&known, &interp, &mask, &ttg, Some(0), Some(&gmask), None)
            .unwrap();
    });
    let plain = FillSettings::default();
    let styled = FillSettings {
        style: "walk".into(),
        ..FillSettings::default()
    };
    let (a, b) = (10, 41);
    let fill = median_ms(9, || {
        m.fill(&sk, &clip, &ac_core::clip::NoLinks, a, b, &plain, None)
            .unwrap();
    });
    let fill_styled = median_ms(9, || {
        m.fill(&sk, &clip, &ac_core::clip::NoLinks, a, b, &styled, None)
            .unwrap();
    });
    eprintln!("forward pass, {t} frames: {forward:.1} ms (median of 9)");
    eprintln!(
        "fill {a}-{b} unstyled (one pass): {fill:.1} ms; around the network: {:.1} ms",
        fill - forward
    );
    eprintln!("fill {a}-{b} styled (two passes): {fill_styled:.1} ms");
}
