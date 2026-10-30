//! The community commands on the fake server: what they refuse and why, what an install keeps,
//! and that a result made off the UI thread is the one the command returns. Each test runs on its
//! own thread, so each sees the fake as it starts (`fake.rs` has its table).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use ac_community::record::{RECORD, Record};
use serde_json::{Map, Value, json};

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn session(name: &str) -> (ac_core::Session, PathBuf) {
    ac_community::fake::use_fake(&assets().join("rig").join("default_rig.json"));
    let lib = std::env::temp_dir().join(format!(
        "{}-test-community-{name}-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&lib);
    let s = ac_io::setup::SessionBuilder::new(&assets())
        .library(&lib)
        .limits(false)
        .install(false)
        .commands(ac_community::SPECS)
        .strict()
        .build()
        .unwrap()
        .session;
    (s, lib)
}

fn refused(s: &mut ac_core::Session, id: &str, p: Value) -> String {
    match s.run(id, &p) {
        Ok(v) => panic!("{id} was accepted: {v}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn reporting_before_the_captcha_says_where_to_pass_it() {
    let (mut s, _) = session("captcha");
    let e = refused(&mut s, "community.report", json!({ "id": "c2" }));
    assert!(e.contains("captcha needed"), "{e}");
    assert!(
        e.contains("fake://verify.html") && e.contains("community.verify"),
        "{e}"
    );
    let e = refused(&mut s, "community.verify", json!({ "token": "nope" }));
    assert!(e.contains("didn't pass"), "{e}");
}

#[test]
fn a_damaged_file_installs_nothing() {
    let (mut s, lib) = session("damaged");
    let e = refused(&mut s, "community.install", json!({ "id": "c5" }));
    assert!(e.contains("doesn't match its hash"), "{e}");
    assert!(!lib.join("animations").join("Community").exists());
    assert!(!lib.join(RECORD).exists());
}

#[test]
fn your_own_clip_is_not_yours_to_rate_or_report() {
    let (mut s, _) = session("own");
    let e = refused(&mut s, "community.rate", json!({ "id": "c4", "stars": 5 }));
    assert!(e.contains("your own clip"), "{e}");
    s.run("community.verify", &json!({ "token": "pass" })).unwrap();
    let e = refused(&mut s, "community.report", json!({ "id": "c4" }));
    assert!(e.contains("your own clip"), "{e}");
}

#[test]
fn an_install_keeps_its_credit_and_where_it_came_from() {
    let (mut s, lib) = session("credit");
    let v = s.run("community.install", &json!({ "id": "c2" })).unwrap();
    assert_eq!(v["id"], "Community/Slow walk");
    let rec = Record::load(&lib).unwrap();
    let i = &rec.installs["Community/Slow walk"];
    assert_eq!((i.clip.as_str(), i.author.as_str()), ("c2", "Ben"));
    assert_eq!(
        (i.licence.as_str(), i.credit.as_str()),
        ("CC-BY-4.0", "Ben's mocap")
    );
    let file = lib
        .join("animations")
        .join("Community")
        .join("Slow walk.clip.json");
    let clip: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(
        (clip["group"].as_str(), clip["origin"].as_str()),
        (Some("Community"), Some("community:c2"))
    );
    // it came from someone else: not yours to share
    s.run("community.verify", &json!({ "token": "pass" })).unwrap();
    let e = refused(
        &mut s,
        "community.share",
        json!({ "id": "Community/Slow walk", "licence": "CC0-1.0" }),
    );
    assert!(e.contains("came from the community"), "{e}");
}

#[test]
fn a_result_made_off_the_ui_thread_is_the_one_returned() {
    let (mut s, _) = session("prepared");
    let p: Map<String, Value> = json!({ "search": "walk" }).as_object().cloned().unwrap();
    let ctx = ac_community::Ctx::of(&s.document);
    ac_community::prepare(&ctx, "community.browse", &p);
    let v = s.run("community.browse", &Value::Object(p)).unwrap();
    assert_eq!(v["clips"][0]["id"], "c2");
    assert!(ac_community::waits("community.browse") && !ac_community::waits("clip.save"));
}

#[test]
fn a_preview_must_be_a_small_png() {
    let (mut s, lib) = session("preview");
    s.run("clip.new", &json!({ "name": "Hop", "group": "Mine" }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
    s.run("clip.save", &json!({})).unwrap();
    s.run("community.verify", &json!({ "token": "pass" })).unwrap();
    let gif = lib.join("not-a.png");
    std::fs::write(&gif, b"GIF89a....").unwrap();
    let share = |path: &Path| json!({ "id": "Mine/Hop", "licence": "CC0-1.0", "preview": path });
    let e = refused(&mut s, "community.share", share(&gif));
    assert!(e.contains("isn't a PNG"), "{e}");
    let png = lib.join("hop.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\n....").unwrap();
    let v = s.run("community.share", &share(&png)).unwrap();
    assert_eq!(v["status"], "checking");
}
