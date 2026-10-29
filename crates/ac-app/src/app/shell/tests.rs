//! Registry-wide checks of the shell: every UI entry (menu item, shortcut, palette row) runs a
//! registered command or an app action, every command is reachable from the menus and the
//! palette, every spec's shortcut parses, and the default keys don't clash.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use craft_keymap::{Active, Chord, ConflictKind, Keymap, Lookup, Overrides, Platform};

use super::actions::{self, ACTIONS};
use super::{keys, menus, palette, scoped};

/// A session with every command the app registers.
fn session() -> ac_core::Session {
    let rig = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/rig/default_rig.json");
    let text = std::fs::read_to_string(rig).expect("the rig");
    let skeleton = ac_core::Skeleton::from_json(&text).expect("rig parses");
    let mut s = ac_core::Session::new(ac_core::Document::new(skeleton));
    assert!(ac_io::register(&mut s).is_empty());
    s
}

#[test]
fn every_ui_entry_runs_a_command() {
    let s = session();
    for row in menus::rows(&s) {
        if row.label == "---" {
            continue;
        }
        assert!(
            actions::runnable(&s, &row.command.id),
            "menu item {:?} runs nothing",
            row.command.id
        );
    }
    let (catalog, notes) = keys::catalog(&s);
    assert!(notes.is_empty(), "{notes:?}");
    for b in &catalog {
        assert!(
            actions::runnable(&s, &b.command.id),
            "shortcut entry {:?} runs nothing",
            b.command.id
        );
    }
    for e in palette::entries(&s) {
        assert!(
            actions::runnable(&s, e.id),
            "palette entry {:?} runs nothing",
            e.id
        );
    }
}

#[test]
fn every_command_is_in_a_menu_and_the_palette() {
    let s = session();
    let rows = menus::rows(&s);
    let entries = palette::entries(&s);
    for spec in s.commands() {
        if actions::internal(spec.id) {
            continue;
        }
        assert!(
            rows.iter().any(|r| r.command.id == spec.id),
            "{} is in no menu",
            spec.id
        );
        assert!(
            entries.iter().any(|e| e.id == spec.id),
            "{} is not in the palette",
            spec.id
        );
        assert!(menus::MENUS.contains(&menus::menu_of(spec.id)));
    }
    for a in ACTIONS {
        assert!(menus::MENUS.contains(&a.menu), "{} names no menu", a.id);
        assert!(
            rows.iter().any(|r| r.command.id == a.id),
            "{} is in no menu",
            a.id
        );
    }
}

#[test]
fn spec_shortcuts_parse_and_defaults_dont_clash() {
    let s = session();
    let (catalog, notes) = keys::catalog(&s);
    assert!(notes.is_empty(), "{notes:?}");
    // each spec shortcut became a default binding
    for spec in s.commands() {
        if let Some(text) = spec.shortcut {
            let b = catalog
                .iter()
                .find(|b| b.command.id == spec.id)
                .expect("in the catalog");
            assert!(!b.defaults.is_empty(), "{} lost its shortcut {text}", spec.id);
        }
    }
    for platform in [Platform::Windows, Platform::Mac] {
        let km = Keymap::new(catalog.clone(), scoped::scopes(), platform).with_overrides(Overrides::new());
        let clashes: Vec<_> = km
            .conflicts()
            .into_iter()
            .filter(|c| c.kind == ConflictKind::Clash)
            .map(|c| format!("{}: {} and {}", c.chord, c.a.0.id, c.b.0.id))
            .collect();
        assert!(clashes.is_empty(), "{platform:?}: {clashes:?}");
        // every context key names a context the keymap knows (else it would never run)
        for b in &catalog {
            for d in &b.defaults {
                if let Some(s) = &d.scope {
                    assert!(
                        scoped::CONTEXTS.iter().any(|(c, _)| *c == s.as_str()),
                        "{}: unknown context {s}",
                        b.command.id
                    );
                }
            }
        }
    }
}

/// Each key runs one thing, chosen by where the pointer and the keyboard are.
#[test]
fn one_key_one_action_by_context() {
    let s = session();
    let (catalog, _) = keys::catalog(&s);
    let km = Keymap::new(catalog, scoped::scopes(), Platform::Windows);
    let run = |key: &str, contexts: &[&str]| {
        let active = Active {
            panel: None,
            contexts,
            typing: false,
        };
        match km.lookup(&Chord::parse(key).expect("chord"), false, &active) {
            Lookup::Run(c) => c.id,
            other => format!("{other:?}"),
        }
    };
    use scoped::{LIBRARY, TIMELINE, TIMELINE_CLIPBOARD, TIMELINE_FOCUS, TIMELINE_SELECTION, VIEWPORT};
    let over_timeline = [TIMELINE, TIMELINE_SELECTION, TIMELINE_CLIPBOARD];
    for (key, contexts, want) in [
        ("Ctrl+C", &[][..], "pose.copy"),
        ("Ctrl+C", &[TIMELINE][..], "pose.copy"),
        ("Ctrl+C", &over_timeline[..], "app.copy_keys"),
        ("Ctrl+V", &[TIMELINE][..], "pose.paste"),
        ("Ctrl+V", &[TIMELINE, TIMELINE_CLIPBOARD][..], "app.paste_keys"),
        (
            "Ctrl+Shift+V",
            &[TIMELINE, TIMELINE_CLIPBOARD][..],
            "app.paste_keys_mirrored",
        ),
        ("Left", &[][..], "app.frame_prev"),
        ("Left", &[TIMELINE][..], "app.frame_prev"),
        ("Left", &[TIMELINE, TIMELINE_FOCUS][..], "app.select_prev_key"),
        (
            "Shift+Right",
            &[TIMELINE, TIMELINE_FOCUS][..],
            "app.extend_next_key",
        ),
        ("Tab", &[TIMELINE, TIMELINE_FOCUS][..], "app.select_next_key"),
        ("Home", &[][..], "app.frame_start"),
        ("Home", &[TIMELINE_FOCUS][..], "app.select_first_key"),
        ("Up", &[][..], "app.key_prev"),
        ("Up", &[TIMELINE_FOCUS][..], "app.key_prev"),
        ("Up", &[LIBRARY][..], "app.lib_prev"),
        ("Delete", &[][..], "app.delete_keys"),
        ("Delete", &[TIMELINE, TIMELINE_FOCUS][..], "app.delete_keys"),
        ("Delete", &[LIBRARY][..], "app.lib_delete"),
        ("I", &[TIMELINE][..], "app.range_in"),
        ("Space", &[TIMELINE, TIMELINE_FOCUS][..], "app.play"),
        ("K", &[VIEWPORT][..], "key.set"),
        ("Shift+A", &[][..], "ai.toggle"),
        ("F", &[VIEWPORT][..], "app.frame_selection"),
        ("1", &[VIEWPORT][..], "app.view_front"),
        ("Ctrl+1", &[VIEWPORT][..], "app.view_back"),
        ("Ctrl+0", &[VIEWPORT][..], "app.zoom_reset"),
        ("E", &[VIEWPORT][..], "app.tool_rotate"),
    ] {
        assert_eq!(run(key, contexts), want, "{key} in {contexts:?}");
    }
    // a context key does nothing outside its context
    for (key, contexts) in [
        ("F", &[][..]),
        ("1", &[TIMELINE][..]),
        ("I", &[VIEWPORT][..]),
        ("Escape", &[TIMELINE][..]),
        ("F2", &[][..]),
    ] {
        assert_eq!(run(key, contexts), "None", "{key} in {contexts:?}");
    }
}

#[test]
fn frame_stepping_keys_are_the_arrows_home_and_end() {
    let s = session();
    let (catalog, _) = keys::catalog(&s);
    let km = Keymap::new(catalog, scoped::scopes(), Platform::Windows);
    for (id, key) in [
        ("app.frame_prev", "Left"),
        ("app.frame_next", "Right"),
        ("app.key_prev", "Up"),
        ("app.key_next", "Down"),
        ("app.frame_start", "Home"),
        ("app.frame_end", "End"),
        ("app.palette", "Ctrl+Shift+P"),
        ("app.maximise", "Ctrl+Space"),
        ("edit.undo", "Ctrl+Z"),
        ("clip.save", "Ctrl+S"),
    ] {
        let shown: Vec<String> = km
            .bindings(&id.into())
            .iter()
            .map(|b| b.trigger.display(Platform::Windows))
            .collect();
        assert!(shown.iter().any(|s| s == key), "{id}: {shown:?}");
    }
}

#[test]
fn undo_and_redo_say_what_they_did() {
    use serde_json::json;
    let mut s = session();
    s.run("key.set", &json!({})).expect("key.set");
    let undone = s.run("edit.undo", &json!({})).expect("undo");
    assert_eq!(
        actions::result_text("edit.undo", "Undo", &undone),
        "Undid: Set key"
    );
    let redone = s.run("edit.redo", &json!({})).expect("redo");
    assert_eq!(
        actions::result_text("edit.redo", "Redo", &redone),
        "Redid: Set key"
    );
    let none = s.run("edit.redo", &json!({})).expect("redo");
    assert_eq!(
        actions::result_text("edit.redo", "Redo", &none),
        "nothing to redo"
    );
    assert_eq!(
        actions::result_text("frame.set", "Go to frame", &json!({})),
        "Go to frame: done"
    );
}

/// What Export writes, Import reads back into the app's keymap (and a stranger is refused).
#[test]
fn exported_shortcuts_import_back() {
    let session = session();
    let (mut mine, _) = keys::Keys::new(&session, Overrides::default(), Default::default());
    let play = craft_keymap::CommandRef::from("app.play");
    mine.keymap
        .assign(
            &play,
            craft_keymap::Binding::parse("F9").unwrap(),
            craft_keymap::keymap::Policy::Steal,
        )
        .unwrap();
    let mut sets = craft_keymap::Profiles::default();
    let name = sets
        .save_as("Mine", mine.keymap.overrides().clone(), &[])
        .unwrap();
    let json = sets.export(&name, &[]).unwrap();

    let (mut theirs, _) = keys::Keys::new(&session, Overrides::default(), Default::default());
    let mut state = craft_keymap::ui::DialogState::new();
    let options = craft_keymap::ui::Options::default();
    let builtins = keys::builtins();
    let got = craft_keymap::ui::ShortcutsDialog::new(&mut theirs.keymap, &craft_keymap::ui::English)
        .sets(&mut theirs.sets, &builtins)
        .options(&options)
        .import_json(&mut state, &json);
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(theirs.keymap.overrides(), mine.keymap.overrides());
    assert_eq!(got.as_deref(), Ok("Mine"), "a new set, active");
    assert_eq!(theirs.sets.active(), "Mine");
    let bad = craft_keymap::ui::ShortcutsDialog::new(&mut theirs.keymap, &craft_keymap::ui::English)
        .options(&options)
        .import_json(&mut state, "{\"format\": \"other\"}");
    assert!(bad.is_err());
}
