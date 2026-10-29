//! A models folder other than the bundled one: the app's own (`ac_io::library::app_models_dir`),
//! where a model trained on data that isn't shipped lives.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::ai::Inbetweener;

fn bundled() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models")
}

#[test]
fn an_empty_folder_has_no_model_and_a_folder_with_one_loads_it() {
    let dir = std::env::temp_dir().join(format!("ac-ai-models-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("mine")).unwrap();
    assert!(!ac_ai::has_model(&dir));
    assert!(ac_ai::shared_in(&dir).unwrap().is_none());
    assert!(!ac_ai::has_model(&dir.join("missing")));

    // a copy of the bundled model under another tag: the only one there, so it is the default
    for f in ["weights.json", "weights.bin"] {
        std::fs::copy(bundled().join("parts").join(f), dir.join("mine").join(f)).unwrap();
    }
    assert!(ac_ai::has_model(&dir));
    let m = ac_ai::Model::load_default_in(&dir).unwrap();
    assert_eq!(
        m.info().bones,
        ac_ai::Model::load_default_in(&bundled()).unwrap().info().bones
    );
    assert!(ac_ai::shared_in(&dir).unwrap().is_some());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_broken_model_is_an_error_not_no_model() {
    let dir = std::env::temp_dir().join(format!("ac-ai-models-broken-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bad")).unwrap();
    std::fs::write(dir.join("bad/weights.json"), "{ not json").unwrap();
    assert!(ac_ai::has_model(&dir));
    assert!(ac_ai::shared_in(&dir).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
