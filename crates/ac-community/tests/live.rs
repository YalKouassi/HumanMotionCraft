//! The live server, by hand only (`cargo test -p ac-community --test live -- --ignored`): CI never
//! reaches the network. It reads, and makes one anonymous account in a scratch folder.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ac_community::backend::{Backend, Query, Sort};
use ac_community::http::Http;

#[test]
#[ignore = "reaches the live server"]
fn live() {
    let dir = std::env::temp_dir().join(format!(
        "{}-community-live-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Http::new(dir.clone());

    let page = h.captcha_page().unwrap();
    assert!(page.ends_with("/verify.html"), "{page}");
    let q = Query {
        search: vec!["walk".into()],
        tags: vec![],
        sort: Sort::Top,
        page: 0,
        limit: 5,
    };
    let found = h.browse(&q).unwrap();
    assert!(found.len() <= 5);
    for l in &found {
        assert!(l.file.starts_with("https://") && l.sha256.len() == 64, "{l:?}");
    }
    let e = h.listing("00000000-0000-0000-0000-000000000000").unwrap_err();
    assert!(e.contains("no community clip"), "{e}");

    assert!(!h.account().unwrap().signed_in);
    assert!(h.mine().unwrap().is_empty());
    // sharing before the captcha: makes the account, and the server says why it refuses
    let e = h
        .share(&ac_community::backend::Upload {
            title: "live test".into(),
            description: String::new(),
            tags: vec![],
            licence: "CC0-1.0".into(),
            credit: String::new(),
            name: String::new(),
            clip_gz: ac_community::fake::gzip(b"{}").unwrap(),
            preview_png: None,
        })
        .unwrap_err();
    assert_eq!(e, ac_community::backend::CAPTCHA_NEEDED);
    let a = h.account().unwrap();
    assert!(a.signed_in && !a.verified, "{a:?}");
    assert!(dir.join(ac_community::http::ACCOUNT_FILE).is_file());
    // a new process on the same computer is the same account
    let mut again = Http::new(dir.clone());
    assert!(again.account().unwrap().signed_in);
    assert!(again.mine().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
