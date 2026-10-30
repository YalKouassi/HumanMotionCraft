//! The data left under a former product name's folder moves to the current name's, once.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ac_core::product;
use ac_io::data_folder::move_former;

#[test]
fn a_former_folder_moves_to_the_current_name_once() {
    let base = std::env::temp_dir().join(format!("{}-test-data-folder-{}", product::ID, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (former, _) = product::FORMER[0];
    let old = base.join(former);
    std::fs::create_dir_all(old.join("library/animations/G")).unwrap();
    std::fs::write(old.join("library/animations/G/Walk.clip.json"), "{}").unwrap();
    std::fs::create_dir_all(old.join("models/parts")).unwrap();
    // the current folder exists already (the settings moved first) but holds no library
    std::fs::create_dir_all(base.join(product::NAME)).unwrap();
    std::fs::write(base.join(product::NAME).join("config.toml"), "").unwrap();

    let note = move_former(&base).unwrap().unwrap();
    assert!(note.starts_with("moved your library and models from"), "{note}");
    let now = base.join(product::NAME);
    assert!(now.join("library/animations/G/Walk.clip.json").is_file());
    assert!(now.join("models/parts").is_dir());
    assert!(now.join("config.toml").is_file());
    assert!(!old.exists(), "the empty former folder is gone");
    assert!(move_former(&base).is_none(), "nothing left to move");

    // a former library beside a current one: the current one wins, nothing moves
    std::fs::create_dir_all(old.join("library")).unwrap();
    assert!(move_former(&base).is_none());
    assert!(old.join("library").is_dir());
    std::fs::remove_dir_all(base).unwrap();
}
