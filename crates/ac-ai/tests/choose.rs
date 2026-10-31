//! Choosing a model (roadmap 155000): the cards `ac_ai::models::Models` lists from the bundled
//! folder and the fixtures' test models (`python -m ac_train.tinymodel`), the checks that refuse
//! a stale add-on, and hostile weights files that must be refused, never crash.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_ai::models::Models;
use ac_core::Skeleton;
use ac_core::ai::{Inbetweener, ModelSource};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skeleton() -> Skeleton {
    Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
        .unwrap()
}

fn test_models() -> PathBuf {
    repo().join("fixtures/data/models")
}

#[test]
fn the_bundled_default_and_the_test_models_are_listed_and_checked() {
    let src = Models::bundled(&repo().join("assets"));
    let sk = skeleton();
    let cards = src.cards(&sk, &[test_models()]);
    let tags: Vec<&str> = cards.iter().map(|c| c.tag.as_str()).collect();
    assert_eq!(
        tags,
        ["parts", "stale", "tiny"],
        "bundled first, then the folder by name"
    );
    let parts = &cards[0];
    assert!(parts.default && parts.loads && parts.refused.is_none());
    assert_eq!(
        (parts.source.as_str(), parts.context, parts.max_gap),
        ("bundled", 10, 45)
    );
    assert_eq!(parts.hash.len(), 16);
    let tiny = cards.iter().find(|c| c.tag == "tiny").unwrap();
    assert!(tiny.loads && !tiny.default && tiny.base.is_none());
    assert_eq!(
        (tiny.name.as_str(), tiny.source.as_str()),
        ("Tiny test model", "folder")
    );
    assert_eq!(tiny.bones, ["root"]);
    let stale = cards.iter().find(|c| c.tag == "stale").unwrap();
    assert!(!stale.loads);
    assert_eq!(stale.base.as_ref().unwrap().tag, "parts");
    assert_eq!(
        stale.refused.as_deref(),
        Some(
            "an add-on made for parts with hash 0000000000000000, not the parts here (ai.models shows its hash)"
        )
    );
    // loading follows the cards: the refused one is an error that says why
    assert_eq!(
        src.load(&sk, &[test_models()], "tiny").unwrap().info().tag,
        "tiny"
    );
    let e = src.load(&sk, &[test_models()], "stale").unwrap_err();
    assert!(e.starts_with("model stale: an add-on made for parts"), "{e}");
    let e = src.load(&sk, &[test_models()], "tiney").unwrap_err();
    assert!(e.contains("no model \"tiney\"") && e.contains("tiny"), "{e}");
    // without the folder the test models are not there
    assert!(src.load(&sk, &[], "tiny").is_err());
}

#[test]
fn the_installed_folder_comes_first_and_is_the_default() {
    let dir = std::env::temp_dir().join(format!("ac-ai-choose-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    for f in ["weights.json", "weights.bin"] {
        std::fs::copy(test_models().join("tiny").join(f), dir.join("parts").join(f)).unwrap();
    }
    // the copy's card still says "tiny": the tag comes from the card, not the folder
    let src = Models::with_installed(&repo().join("assets"), &dir);
    let cards = src.cards(&skeleton(), &[]);
    assert_eq!(cards[0].source, "installed");
    assert_eq!(cards[0].tag, "tiny");
    assert!(
        cards[0].default,
        "the installed folder's model is what the app loads first"
    );
    assert!(!cards.iter().any(|c| c.tag == "parts" && c.default));
    std::fs::remove_dir_all(&dir).ok();
}

/// A weights file is hostile input: each of these must be refused with a message, not a panic.
#[test]
fn hostile_weights_files_are_refused() {
    let base: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(test_models().join("tiny/weights.json")).unwrap())
            .unwrap();
    let bin = std::fs::read(test_models().join("tiny/weights.bin")).unwrap();
    let dir = std::env::temp_dir().join(format!("ac-ai-hostile-{}", std::process::id()));
    let cases: Vec<(&str, Box<dyn Fn(&mut serde_json::Value)>)> = vec![
        (
            "heads 0",
            Box::new(|v: &mut serde_json::Value| v["card"]["heads"] = serde_json::json!(0)),
        ),
        (
            "heads huge",
            Box::new(|v: &mut serde_json::Value| v["card"]["heads"] = serde_json::json!(u64::MAX)),
        ),
        (
            "offset past the end",
            Box::new(|v: &mut serde_json::Value| v["tensors"]["pos"]["offset"] = serde_json::json!(u64::MAX)),
        ),
        (
            "shape overflow",
            Box::new(|v: &mut serde_json::Value| {
                v["tensors"]["pos"]["shape"] = serde_json::json!([u64::MAX, u64::MAX])
            }),
        ),
        (
            "no feed-forward",
            Box::new(|v: &mut serde_json::Value| {
                v["tensors"]["body.layers.0.linear1.weight"]["shape"] = serde_json::json!([0, 8])
            }),
        ),
        (
            "style not [styles, d]",
            Box::new(|v: &mut serde_json::Value| {
                v["tensors"]["style.weight"] = serde_json::json!({ "offset": 0, "shape": [3] })
            }),
        ),
        (
            "context huge",
            Box::new(|v: &mut serde_json::Value| v["card"]["context"] = serde_json::json!(u64::MAX)),
        ),
        (
            "context 0",
            Box::new(|v: &mut serde_json::Value| v["card"]["context"] = serde_json::json!(0)),
        ),
        (
            "no tag",
            Box::new(|v: &mut serde_json::Value| v["card"]["tag"] = serde_json::json!("  ")),
        ),
        (
            "base without hash",
            Box::new(|v: &mut serde_json::Value| v["card"]["base"] = serde_json::json!("parts")),
        ),
        (
            "bones not the features",
            Box::new(|v: &mut serde_json::Value| v["card"]["bones"] = serde_json::json!(["root", "head"])),
        ),
        (
            "tensors not an object",
            Box::new(|v: &mut serde_json::Value| v["tensors"] = serde_json::json!([1, 2])),
        ),
        (
            "card not an object",
            Box::new(|v: &mut serde_json::Value| v["card"] = serde_json::json!(7)),
        ),
    ];
    for (what, change) in cases {
        let mut v = base.clone();
        change(&mut v);
        let d = dir.join(what.replace(' ', "-"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("weights.json"), v.to_string()).unwrap();
        std::fs::write(d.join("weights.bin"), &bin).unwrap();
        let r = ac_ai::Model::load(&d);
        assert!(r.is_err(), "{what}: loaded");
        assert!(!r.unwrap_err().is_empty(), "{what}: no reason");
    }
    // a truncated binary
    let d = dir.join("short");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("weights.json"), base.to_string()).unwrap();
    std::fs::write(d.join("weights.bin"), &bin[..bin.len() / 2]).unwrap();
    assert!(ac_ai::Model::load(&d).is_err());
    // and the listing carries each as a card that does not load, never an error
    let src = Models::bundled(&repo().join("assets"));
    let cards = src.cards(&skeleton(), &[dir.clone()]);
    assert!(
        cards
            .iter()
            .filter(|c| c.source == "folder")
            .all(|c| !c.loads && c.refused.is_some())
    );
    std::fs::remove_dir_all(&dir).ok();
}
