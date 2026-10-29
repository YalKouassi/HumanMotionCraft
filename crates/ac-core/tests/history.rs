//! The undo history as the app's history panel reads it: undo steps oldest first, redo steps
//! next-first, and a jump of several steps lands on the document of that step.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::{Document, Session};
use serde_json::json;

#[test]
fn redo_history_lists_the_next_redo_first() {
    let mut s = Session::new(Document::new(common::skeleton()));
    let start = s.document.clone();
    s.run("pose.set_root", &json!({ "root": [0.1, 0.0, 1.0] }))
        .unwrap();
    let one = s.document.clone();
    s.run("pose.set_root", &json!({ "root": [0.2, 0.0, 1.0] }))
        .unwrap();
    s.run("pose.reset", &json!({})).unwrap();
    assert_eq!(s.history().len(), 3);
    assert!(s.redo_history().is_empty());

    // back two steps: the two undone steps wait to be redone, the nearer one first
    for _ in 0..2 {
        s.run("edit.undo", &json!({})).unwrap();
    }
    assert!(s.document == one);
    assert_eq!(s.history().len(), 1);
    assert_eq!(
        s.redo_history(),
        vec![s.redo_label().unwrap()]
            .into_iter()
            .chain(["Reset pose"])
            .collect::<Vec<_>>()
    );
    assert_eq!(s.redo_history().len(), 2);

    // all the way back, then forward again
    s.run("edit.undo", &json!({})).unwrap();
    assert!(s.document == start);
    assert_eq!(s.redo_history().len(), 3);
    assert_eq!(s.redo_history().last(), Some(&"Reset pose"));
}
