//! The review headless, on a small set written here: reading the npz, unpacking the rotations,
//! ranking the queue, deciding and forgetting, and what is refused.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::{Path, PathBuf};

use ac_review::review::Review;
use serde_json::{Map, Value, json};

fn npy(descr: &str, shape: &str, data: &[u8]) -> Vec<u8> {
    let mut h = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {shape}, }}");
    while (10 + h.len() + 1) % 64 != 0 {
        h.push(' ');
    }
    h.push('\n');
    let mut out = b"\x93NUMPY\x01\x00".to_vec();
    out.extend((h.len() as u16).to_le_bytes());
    out.extend(h.as_bytes());
    out.extend(data);
    out
}

fn text(words: &[&str], width: usize) -> Vec<u8> {
    let mut b = Vec::new();
    for w in words {
        let mut cs: Vec<u32> = w.chars().map(|c| c as u32).collect();
        cs.resize(width, 0);
        b.extend(cs.iter().flat_map(|c| c.to_le_bytes()));
    }
    b
}

/// Two takes ("walk_a": 10 frames, "jump_b": 6), 3 bones; frame f of the set has root x = f and
/// bone 1 turned (x = 0.5 in half floats: 0x3800). `rig`: the build stamp's rig CRC, if any.
fn write_set_for(dir: &Path, rig: Option<i64>) -> PathBuf {
    let frames = 16;
    let mut rot = Vec::new();
    for _ in 0..frames {
        for b in 0..3 {
            let q: [u16; 4] = if b == 1 {
                [0x3800, 0, 0, 0x3c00]
            } else {
                [0, 0, 0, 0x3c00]
            };
            rot.extend(q.iter().flat_map(|x| x.to_le_bytes()));
        }
    }
    let roots: Vec<u8> = (0..frames)
        .flat_map(|f| [f as f32, 0.0, 1.0])
        .flat_map(|x| x.to_le_bytes())
        .collect();
    let starts: Vec<u8> = [0i64, 10].iter().flat_map(|x| x.to_le_bytes()).collect();
    let path = dir.join("tiny.npz");
    let mut arrays = vec![
        ("rotations", npy("<f2", &format!("({frames}, 3, 4)"), &rot)),
        ("roots", npy("<f4", &format!("({frames}, 3)"), &roots)),
        ("starts", npy("<i8", "(2,)", &starts)),
        ("takes", npy("<U6", "(2,)", &text(&["walk_a", "jump_b"], 6))),
        ("labels", npy("<U12", "(2,)", &text(&["a walk", "a jump"], 12))),
        ("fps", npy("<f8", "()", &30f64.to_le_bytes())),
    ];
    if let Some(c) = rig {
        arrays.push(("rig_crc32", npy("<i8", "()", &c.to_le_bytes())));
    }
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in arrays {
        z.start_file(format!("{name}.npy"), opts).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap();
    let scan = json!({"set": "tiny", "takes": [
        {"take": "walk_a", "frames": 10, "cuts": [[2, 4, "glitch"]], "reject": "", "cut_frames": 3},
        {"take": "jump_b", "frames": 6, "cuts": [[0, 5, "float"]], "reject": "100% cut", "cut_frames": 6},
    ]});
    std::fs::write(dir.join("tiny.quality.json"), scan.to_string()).unwrap();
    path
}

fn write_set(dir: &Path) -> PathBuf {
    write_set_for(dir, None)
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ac-review-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => panic!("params are an object"),
    }
}

#[test]
fn a_set_reads_unpacks_and_plays() {
    let dir = scratch("read");
    write_set(&dir);
    let r = Review::open(&dir, &dir.join("decisions.json")).unwrap();
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    let s = &r.sets[0];
    assert_eq!((s.name.as_str(), s.frames, s.bones, s.fps), ("tiny", 16, 3, 30.0));
    assert_eq!(s.takes[1].name, "jump_b");
    assert_eq!(
        (s.takes[1].start, s.takes[1].frames, s.takes[1].label.as_str()),
        (10, 6, "a jump")
    );
    assert!(!s.unpacked());
    s.unpack_job().run().unwrap();
    assert!(s.unpacked());
    let m = s.motion(1).unwrap();
    assert_eq!(m.roots.len(), 6);
    assert_eq!(m.roots[0], [10.0, 0.0, 1.0]); // the take's first frame is the set's 10th
    let q = m.rotations[0][1];
    let n = (0.25f64 + 1.0).sqrt(); // (0.5, 0, 0, 1) normalised
    assert!(
        (q[0] - 0.5 / n).abs() < 1e-6 && (q[3] - 1.0 / n).abs() < 1e-6,
        "{q:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn deciding_is_saved_and_reorders_the_queue() {
    let dir = scratch("decide");
    write_set(&dir);
    let file = dir.join("review").join("decisions.json");
    let mut r = Review::open(&dir, &file).unwrap();
    // the scan's rejection first
    let list = r.run("review.list", &p(json!({}))).unwrap();
    let ids: Vec<&str> = list["takes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["tiny/jump_b", "tiny/walk_a"]);
    // cut with no cuts given takes the scan's; overlapping ranges are joined
    let saved = r
        .run("review.decide", &p(json!({"id": "tiny/walk_a", "status": "cut"})))
        .unwrap();
    assert_eq!(saved["cuts"], json!([[2, 4]]));
    let saved = r
        .run(
            "review.decide",
            &p(json!({"id": "tiny/walk_a", "status": "cut", "cuts": [[2, 4], [5, 7]], "tags": ["tired"], "by": "agent"})),
        )
        .unwrap();
    assert_eq!(saved["cuts"], json!([[2, 7]]));
    assert_eq!(
        (saved["frames"].clone(), saved["by"].clone()),
        (json!(10), json!("agent"))
    );
    // decided: off the open list; and on disk
    let open = r.run("review.list", &p(json!({"show": "open"}))).unwrap();
    assert_eq!(open["total"], 1);
    let again = Review::open(&dir, &file).unwrap();
    assert_eq!(again.decisions.takes.len(), 1);
    assert_eq!(again.decisions.takes["tiny/walk_a"].tags, ["tired"]);
    // keep: no cuts kept; forget: the scan's verdict again
    let saved = r
        .run("review.decide", &p(json!({"id": "tiny/walk_a", "status": "ok"})))
        .unwrap();
    assert!(saved.get("cuts").is_none());
    assert_eq!(
        r.run("review.clear", &p(json!({"id": "tiny/walk_a"}))).unwrap()["cleared"],
        true
    );
    assert_eq!(
        r.run("review.clear", &p(json!({"id": "tiny/walk_a"}))).unwrap()["cleared"],
        false
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bad_decisions_are_refused_and_change_nothing() {
    let dir = scratch("refuse");
    write_set(&dir);
    let file = dir.join("decisions.json");
    let mut r = Review::open(&dir, &file).unwrap();
    for bad in [
        json!({"id": "tiny/nope", "status": "ok"}),
        json!({"id": "tiny/walk_a", "status": "maybe"}),
        json!({"id": "tiny/walk_a", "status": "cut", "cuts": [[3, 99]]}),
        json!({"id": "tiny/walk_a", "status": "cut", "cuts": [[0, 9]]}),
        json!({"id": "tiny/walk_a", "status": "cut", "cuts": "all"}),
        json!({"status": "ok"}),
    ] {
        assert!(r.run("review.decide", &p(bad.clone())).is_err(), "{bad}");
    }
    assert!(r.decisions.takes.is_empty());
    assert!(!file.exists());
    let e = r.run("review.decid", &p(json!({}))).unwrap_err();
    assert!(e.contains("review.decide"), "{e}");
    // a decisions file that won't read is an error, never replaced
    std::fs::write(&file, "{ broken").unwrap();
    assert!(Review::open(&dir, &file).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_set_built_for_another_rig_is_left_out() {
    let dir = scratch("rig");
    let rig = dir.join("rig.json");
    std::fs::write(&rig, "{\"bones\": 3}\r\n").unwrap();
    // line endings don't count: the stamp was taken from an LF checkout
    let crc = ac_review::review::rig_crc32(b"{\"bones\": 3}\n");
    assert_eq!(crc, ac_review::review::rig_crc32(b"{\"bones\": 3}\r\n"));
    write_set_for(&dir, Some(i64::from(crc)));
    let mut r = Review::open(&dir, &dir.join("decisions.json")).unwrap();
    r.check_rig(&rig).unwrap();
    assert_eq!((r.sets.len(), r.problems.len()), (1, 0), "{:?}", r.problems);
    write_set_for(&dir, Some(i64::from(crc ^ 1)));
    let mut r = Review::open(&dir, &dir.join("decisions.json")).unwrap();
    r.check_rig(&rig).unwrap();
    assert!(r.sets.is_empty());
    assert!(r.problems[0].contains("another rig"), "{:?}", r.problems);
    write_set_for(&dir, None);
    let mut r = Review::open(&dir, &dir.join("decisions.json")).unwrap();
    r.check_rig(&rig).unwrap();
    assert_eq!(r.sets.len(), 1);
    assert!(r.problems[0].contains("no build stamp"), "{:?}", r.problems);
    std::fs::remove_dir_all(&dir).ok();
}

/// 137000: the model's flags (`flags.json`) filter the queue and show on a take; a missing file
/// is said, a stale one (the set rebuilt since) is left out and said; nothing is decided.
#[test]
fn the_models_flags_list_worst_first_and_stale_or_missing_ones_are_said() {
    let dir = scratch("flags");
    write_set(&dir);
    let decisions = dir.join("decisions.json");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    // no file: stated by review.summary and review.take, and `show: model` refuses with the same words
    let mut r = Review::open(&dir, &decisions).unwrap();
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    let s = r.run("review.summary", &p(json!({}))).unwrap();
    assert_eq!(s["flags"]["present"], false);
    assert!(
        s["flags"]["note"]
            .as_str()
            .unwrap()
            .contains("python -m ac_train.flag"),
        "{s}"
    );
    let e = r.run("review.list", &p(json!({"show": "model"}))).unwrap_err();
    assert!(e.contains("no model flags") && e.contains("ac_train.flag"), "{e}");
    let t = r.run("review.take", &p(json!({"id": "tiny/walk_a"}))).unwrap();
    assert!(
        t["model"].is_null() && t["model_note"].as_str().unwrap().contains("no model flags"),
        "{t}"
    );
    assert!(r.run("review.list", &p(json!({"show": "modle"}))).is_err());
    // the fixture's digest is the tiny set's (ac_train.flag.digest agrees: tests/test_flag.py)
    assert_eq!(
        ac_review::flags::digest([("walk_a".to_string(), 10), ("jump_b".to_string(), 6)]),
        408843836
    );
    std::fs::copy(fixtures.join("flags.json"), dir.join("flags.json")).unwrap();
    let mut r = Review::open(&dir, &decisions).unwrap();
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(
        (r.flags.present, r.flags.model.as_str(), r.flags.gap),
        (true, "parts/best", 15)
    );
    assert!(
        r.flags
            .note
            .starts_with("1 of 1 takes flagged by parts/best (gap 15, every window"),
        "{}",
        r.flags.note
    );
    let list = r.run("review.list", &p(json!({"show": "model"}))).unwrap();
    assert_eq!(list["total"], 1);
    let row = &list["takes"][0];
    assert_eq!(row["id"], "tiny/walk_a");
    assert_eq!(
        row["model"],
        json!({"rank": 1, "loss": 0.41, "worst": 0.9, "worst_at": 3, "windows": 4})
    );
    // the other filters still work, and say which takes the model flags
    let all = r.run("review.list", &p(json!({"show": "all"}))).unwrap();
    assert_eq!(all["total"], 2);
    let by_id: Vec<(&str, bool)> = all["takes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["id"].as_str().unwrap(), t["model"].is_null()))
        .collect();
    assert_eq!(by_id, [("tiny/jump_b", true), ("tiny/walk_a", false)]);
    let t = r.run("review.take", &p(json!({"id": "tiny/walk_a"}))).unwrap();
    assert!(
        t["model"]["why"].as_str().unwrap().starts_with("mean loss 0.410"),
        "{t}"
    );
    assert_eq!(t["model"]["weighted"]["pos"], 0.4);
    assert!(t.get("model_note").is_none());
    let s = r.run("review.summary", &p(json!({}))).unwrap();
    assert_eq!(
        (s["flags"]["present"].clone(), s["flags"]["takes"].clone()),
        (json!(true), json!(1))
    );
    // flagging decided nothing
    assert!(r.decisions.takes.is_empty() && !decisions.exists());
    // stale: made when walk_a was 11 frames; left out and said, the rest of the review as before
    std::fs::copy(fixtures.join("flags-stale.json"), dir.join("flags.json")).unwrap();
    let mut r = Review::open(&dir, &decisions).unwrap();
    assert_eq!(r.problems.len(), 1, "{:?}", r.problems);
    assert!(
        r.problems[0].contains("tiny's model flags are stale") && r.problems[0].contains("1 left out"),
        "{:?}",
        r.problems
    );
    assert!(r.flags.present && r.flags.takes.is_empty());
    assert_eq!(
        r.run("review.list", &p(json!({"show": "model"}))).unwrap()["total"],
        0
    );
    assert_eq!(r.run("review.list", &p(json!({}))).unwrap()["total"], 2);
    // not a flags file, or one from a newer writer: an error, the review still opens
    std::fs::write(
        dir.join("flags.json"),
        "{\"format\": \"x.review\", \"version\": 1}",
    )
    .unwrap();
    let r = Review::open(&dir, &decisions).unwrap();
    assert!(r.problems[0].contains("not a flags file"), "{:?}", r.problems);
    assert!(!r.flags.present);
    std::fs::write(
        dir.join("flags.json"),
        "{\"format\": \"ac-train.flags\", \"version\": 2}",
    )
    .unwrap();
    let r = Review::open(&dir, &decisions).unwrap();
    assert!(r.problems[0].contains("newer"), "{:?}", r.problems);
    std::fs::remove_dir_all(&dir).ok();
}
