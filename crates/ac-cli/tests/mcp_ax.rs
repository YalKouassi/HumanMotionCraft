//! MCP for agents: JSON Schema per command, search and describe, resources, prompts, animation
//! previews, `expect` (and the fixtures it saves), session reset, events and progress.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}-mcp-ax-{name}-{}",
        ac_core::product::ID,
        std::process::id()
    ))
}

fn server(name: &str) -> ac_cli::mcp::Server {
    let lib = scratch(name);
    let _ = std::fs::remove_dir_all(&lib);
    let s = ac_cli::session(&repo().join("assets"), &lib).unwrap();
    ac_cli::mcp::Server::new(s, repo().join("assets"))
}

fn rpc(srv: &mut ac_cli::mcp::Server, method: &str, params: Value) -> Value {
    srv.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .unwrap()
}

fn call(srv: &mut ac_cli::mcp::Server, name: &str, args: Value) -> Value {
    rpc(srv, "tools/call", json!({ "name": name, "arguments": args }))["result"].clone()
}

fn body(result: &Value) -> Value {
    let t = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "text")
        .unwrap()["text"]
        .as_str()
        .unwrap();
    serde_json::from_str(t).unwrap()
}

#[test]
fn every_command_has_a_json_schema_and_an_example() {
    let mut srv = server("schema");
    let list = body(&call(&mut srv, "command_list", json!({ "schema": true })));
    for c in list.as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let s = &c["input_schema"];
        assert_eq!(s["type"], "object", "{id}");
        assert_eq!(s["additionalProperties"], false, "{id}");
        assert_eq!(
            s["properties"].as_object().unwrap().len(),
            c["inputs"].as_array().unwrap().len(),
            "{id}"
        );
        let d = call(&mut srv, "describe_command", json!({ "id": id }));
        assert_eq!(d["isError"], false, "{id}: {d}");
        let d = body(&d);
        // every example names its command, and fixture examples come from a real fixture step
        assert_eq!(d["example"]["arguments"]["id"], id);
    }
    let d = body(&call(
        &mut srv,
        "describe_command",
        json!({ "id": "pose.set_bone" }),
    ));
    let s = &d["input_schema"];
    assert_eq!(s["required"], json!(["bone", "rotation"]));
    assert!(
        s["properties"]["bone"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("wrist.L"))
    );
    assert_eq!(s["properties"]["rotation"]["minItems"], 4);
    assert_eq!(s["properties"]["rotation"]["maxItems"], 4);
    assert!(d["example"]["fixture"].is_string(), "{d}");
    // ranges, enums and defaults
    let set = body(&call(&mut srv, "describe_command", json!({ "id": "layer.set" })));
    assert_eq!(set["input_schema"]["properties"]["weight"]["minimum"], 0.0);
    assert_eq!(set["input_schema"]["properties"]["weight"]["maximum"], 1.0);
    assert_eq!(
        set["input_schema"]["properties"]["mode"]["enum"],
        json!(["override", "additive"])
    );
    let apply = body(&call(&mut srv, "describe_command", json!({ "id": "pose.apply" })));
    assert_eq!(apply["input_schema"]["properties"]["key"]["default"], true);
    // outputs
    let world = body(&call(&mut srv, "describe_command", json!({ "id": "bone.world" })));
    assert_eq!(world["output_schema"]["properties"]["position"]["maxItems"], 3);
    // an unknown id says what was meant
    let bad = call(&mut srv, "describe_command", json!({ "id": "pose.setbone" }));
    assert_eq!(bad["isError"], true);
    assert!(
        body(&bad)["error"].as_str().unwrap().contains("pose.set_bone"),
        "{bad}"
    );
}

#[test]
fn command_search_ranks_ids_labels_and_docs() {
    let mut srv = server("search");
    let first = |srv: &mut ac_cli::mcp::Server, q: &str| -> String {
        let l = body(&call(srv, "command_list", json!({ "query": q })));
        l[0]["id"].as_str().unwrap_or_default().to_string()
    };
    assert_eq!(first(&mut srv, "undo"), "edit.undo");
    assert_eq!(first(&mut srv, "export glb"), "clip.export");
    assert_eq!(first(&mut srv, "foot contacts"), "clip.contacts");
    let l = body(&call(
        &mut srv,
        "command_list",
        json!({ "query": "fade", "limit": 3 }),
    ));
    assert!(l.as_array().unwrap().len() <= 3);
    assert!(
        l[0]["matched"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.as_str().unwrap().starts_with("input"))
    );
    // area and query together
    let l = body(&call(
        &mut srv,
        "command_list",
        json!({ "area": "layer.", "query": "travel" }),
    ));
    assert_eq!(l[0]["id"], "layer.travel");
    assert!(
        l.as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"].as_str().unwrap().starts_with("layer."))
    );
    assert_eq!(
        body(&call(&mut srv, "command_list", json!({ "query": "zzzz" }))),
        json!([])
    );
}

#[test]
fn resources_and_prompts() {
    let mut srv = server("resources");
    let init = rpc(&mut srv, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    let caps = &init["result"]["capabilities"];
    assert!(caps["resources"].is_object() && caps["prompts"].is_object() && caps["tools"].is_object());
    let d = &init["result"]["determinism"];
    assert_eq!(d["version"], env!("CARGO_PKG_VERSION"));
    assert!(
        d["ai_model"]["hash"].as_str().unwrap().starts_with("fnv1a64:"),
        "{d}"
    );

    let list = rpc(&mut srv, "resources/list", json!({}));
    let uris: Vec<&str> = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert_eq!(uris, ["ac://state", "ac://skeleton", "ac://library"]);
    let t = rpc(&mut srv, "resources/templates/list", json!({}));
    assert_eq!(
        t["result"]["resourceTemplates"][0]["uriTemplate"],
        "ac://commands/{id}"
    );
    let read = |srv: &mut ac_cli::mcp::Server, uri: &str| -> Value {
        let r = rpc(srv, "resources/read", json!({ "uri": uri }));
        assert_eq!(r["result"]["contents"][0]["uri"], uri, "{r}");
        serde_json::from_str(r["result"]["contents"][0]["text"].as_str().unwrap()).unwrap()
    };
    let sk = read(&mut srv, "ac://skeleton");
    let rig = body(&call(&mut srv, "command_run", json!({ "id": "rig.info" })));
    assert_eq!(
        sk["bones"].as_array().unwrap().len(),
        rig["bones"].as_u64().unwrap() as usize
    );
    assert_eq!(sk["hip_height"], rig["hip_height"]);
    assert_eq!(sk["bones"][0]["parent"], Value::Null);
    assert!(
        sk["parts"]["Left arm"]
            .as_array()
            .unwrap()
            .contains(&json!("wrist.L"))
    );
    assert!(
        sk["bones"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["limits"]["lo"].is_array())
    );
    assert!((sk["standing_root"][2].as_f64().unwrap() - 0.9450101).abs() < 1e-6);
    assert_eq!(read(&mut srv, "ac://state")["frame"], 0.0);
    assert!(read(&mut srv, "ac://library")["poses"].is_array());
    assert_eq!(read(&mut srv, "ac://commands/frame.set")["id"], "frame.set");
    let bad = rpc(&mut srv, "resources/read", json!({ "uri": "ac://nope" }));
    assert_eq!(bad["error"]["code"], -32002);

    let p = rpc(&mut srv, "prompts/list", json!({}));
    let names: Vec<&str> = p["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "block_walk",
            "fix_foot_sliding",
            "review_contact_sheet",
            "export_game_engine"
        ]
    );
    let walk = rpc(
        &mut srv,
        "prompts/get",
        json!({ "name": "block_walk", "arguments": { "steps": "3" } }),
    );
    let text = walk["result"]["messages"][0]["content"]["text"].as_str().unwrap();
    assert!(
        text.contains("3 steps, 0.7 m each") && text.contains("faces -Y"),
        "{text}"
    );
    assert!(!text.contains("{steps}"));
    // a required argument missing, an unknown prompt: protocol errors with the reason
    let e = rpc(
        &mut srv,
        "prompts/get",
        json!({ "name": "export_game_engine", "arguments": {} }),
    );
    assert!(e["error"]["message"].as_str().unwrap().contains("path"), "{e}");
    assert_eq!(
        rpc(&mut srv, "prompts/get", json!({ "name": "nope" }))["error"]["code"],
        -32602
    );
}

/// The steps a test runs: two keys, the root rising one hip height between frame 0 and 20.
fn key_a_rise(srv: &mut ac_cli::mcp::Server) {
    let r = call(
        srv,
        "command_batch",
        json!({ "commands": [
            { "id": "physics.set", "params": { "on": false } },
            { "id": "pose.set_root", "params": { "root": [0, 0, 0.9450101] } },
            { "id": "key.set", "params": { "interp": "linear" } },
            { "id": "frame.set", "params": { "frame": 20 } },
            { "id": "pose.set_root", "params": { "root": [0, 0, 1.9450101] } },
            { "id": "key.set", "params": { "interp": "linear" } },
            { "id": "frame.set", "params": { "frame": 10 } },
        ] }),
    );
    assert_eq!(r["isError"], false, "{r}");
}

#[test]
fn expect_checks_now_and_saves_a_fixture_that_passes() {
    let mut srv = server("expect");
    key_a_rise(&mut srv);
    // halfway: the root head 0.8724132 + 0.8698308 / 2 (as ai-fill.json works it out)
    let r = body(&call(
        &mut srv,
        "expect",
        json!({ "tolerance": 1e-6, "checks": [
            { "bone": "root", "path": "position.2", "value": 1.3073286 },
            { "path": "frame", "value": 10 },
            { "expect": { "keys": { "$contains": [{ "frame": 20 }] } } },
            { "bone": "root", "path": "position.2", "value": 2.0 },
            { "command": "key.set", "path": "", "value": {} },
            { "command": "no.such", "path": "", "value": 1 },
        ] }),
    ));
    assert_eq!(r["passed"], 3, "{r}");
    assert_eq!(r["failed"], 3);
    assert_eq!(r["results"][3]["ok"], false);
    assert!((r["results"][3]["actual"].as_f64().unwrap() - 1.3073286).abs() < 1e-6);
    assert!(r["results"][4]["error"].as_str().unwrap().contains("queries"));

    // saved: the steps so far and the checks; a check that could not run is not saved
    let dir = scratch("expect-out");
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("rise.json");
    let unrunnable = body(&call(
        &mut srv,
        "expect",
        json!({ "checks": [{ "command": "no.such", "path": "", "value": 1 }], "save_as": file.to_string_lossy() }),
    ));
    assert!(
        unrunnable["saved"]["error"].as_str().unwrap().contains("no.such"),
        "{unrunnable}"
    );
    assert!(!file.exists());
    let saved = body(&call(
        &mut srv,
        "expect",
        json!({ "checks": [
            { "bone": "root", "path": "position.2", "value": 1.3073286 },
            { "expect": { "length": 21 } },
        ], "save_as": file.to_string_lossy(), "description": "root rises one hip height over 20 frames" }),
    ));
    // the batch is one step (edit.batch, as it ran); the feature is the last edit inside it
    assert_eq!(saved["saved"]["steps"], 1, "{saved}");
    let fx: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(fx["feature"], "key.set");
    assert_eq!(fx["script"][0]["command"], "edit.batch");
    assert_eq!(fx["script"].as_array().unwrap().len(), 3);
    // a path through a list keeps the list whole, says which items were copied
    let check = &fx["script"][1];
    assert_eq!(check["expect"]["position"][2], 1.3073286);
    assert!(
        check["pinned"].as_str().unwrap().contains("position.2"),
        "{check}"
    );
    let out = ac_cli::run_fixture(&repo().join("assets"), &fx);
    assert!(out.failures.is_empty(), "{:?}", out.failures);
    // never overwrites, never a relative path, never a repository's root
    for bad in [
        file.to_string_lossy().into_owned(),
        "rise.json".into(),
        repo().join("rise.json").to_string_lossy().into_owned(),
    ] {
        let r = body(&call(
            &mut srv,
            "expect",
            json!({ "checks": [{ "path": "frame", "value": 10 }], "save_as": bad }),
        ));
        assert!(r["saved"]["error"].is_string(), "{bad}: {r}");
    }
    assert!(!repo().join("rise.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn session_reset_and_events() {
    let mut srv = server("reset");
    key_a_rise(&mut srv);
    let u = call(&mut srv, "command_run", json!({ "id": "edit.undo" }));
    assert_eq!(u["isError"], false);
    let ev = body(&call(&mut srv, "ui_events", json!({ "since": 0 })));
    let kinds: Vec<&str> = ev["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect();
    // the batch is one command (edit.batch), then the undo of it, named by the batch
    assert_eq!(kinds, ["command", "command", "undo"], "{ev}");
    assert_eq!(ev["events"][0]["id"], "edit.batch");
    assert_eq!(ev["events"][2]["label"], "7 commands");
    let cursor = ev["cursor"].as_u64().unwrap();
    assert_eq!(
        body(&call(&mut srv, "ui_events", json!({ "since": cursor })))["events"],
        json!([])
    );

    let r = call(&mut srv, "session_reset", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    assert!(body(&r)["determinism"]["version"].is_string());
    let state = body(&call(&mut srv, "editor_state", json!({})));
    assert_eq!(state["keys"], json!([]));
    assert_eq!(state["frame"], 0.0);
    let u = body(&call(&mut srv, "command_run", json!({ "id": "edit.undo" })));
    assert_eq!(u["changed"], false, "a reset leaves no undo history");
    let ev = body(&call(&mut srv, "ui_events", json!({ "since": cursor })));
    assert_eq!(ev["events"][0]["event"], "reset");
}

/// Headless pictures of the motion: an animated GIF inline, or a file when asked or too big.
#[test]
fn animation_preview_makes_a_gif() {
    let mut srv = server("preview");
    key_a_rise(&mut srv);
    let r = call(
        &mut srv,
        "animation_preview",
        json!({ "from": 0, "to": 10, "fps": 10, "width": 64, "height": 48, "view": "front" }),
    );
    assert_eq!(r["content"][0]["type"], "image", "{r}");
    assert_eq!(r["content"][0]["mimeType"], "image/gif");
    let gif = ac_cli::render::unbase64(r["content"][0]["data"].as_str().unwrap()).unwrap();
    assert_eq!(&gif[..6], b"GIF89a");
    let info = body(&r);
    // 30 fps clip at 10 pictures a second: frames 0, 3, 6, 9
    assert_eq!(info["frames"], 4);
    assert_eq!(info["fps"], 10.0);
    // the playhead is where it was
    assert_eq!(body(&call(&mut srv, "editor_state", json!({})))["frame"], 10.0);
    // too big for inline: a file, and the reason
    let r = call(
        &mut srv,
        "animation_preview",
        json!({ "width": 64, "height": 48, "inline_max_kb": 0 }),
    );
    let info = body(&r);
    assert_eq!(r["content"].as_array().unwrap().len(), 1);
    let path = PathBuf::from(info["gif"].as_str().unwrap());
    assert_eq!(&std::fs::read(&path).unwrap()[..6], b"GIF89a");
    assert!(info["notes"][0].as_str().unwrap().contains("inline_max_kb"));
    let _ = std::fs::remove_file(path);
    // refused requests
    for bad in [
        json!({ "from": 5, "to": 1 }),
        json!({ "width": 0 }),
        json!({ "mp4": "out.mp4" }),
    ] {
        assert_eq!(
            call(&mut srv, "animation_preview", bad.clone())["isError"],
            true,
            "{bad}"
        );
    }
}

/// Live: requests carry ids, a late reply to an earlier request is skipped, and progress lines
/// become MCP progress notifications.
#[test]
fn live_requests_carry_ids_and_progress() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_app = seen.clone();
    std::thread::spawn(move || {
        let (s, _) = listener.accept().unwrap();
        let mut out = s.try_clone().unwrap();
        for line in BufReader::new(s).lines() {
            let msg: Value = serde_json::from_str(&line.unwrap()).unwrap();
            seen_app.lock().unwrap().push(msg.clone());
            let id = msg["id"].clone();
            // a stale reply first (to some earlier request), then progress, then the answer
            writeln!(out, "{}", json!({ "id": -1, "ok": "stale" })).unwrap();
            if msg["progress"] == true {
                for p in [1, 2] {
                    writeln!(out, "{}", json!({ "id": id, "progress": { "elapsed": p } })).unwrap();
                }
            }
            writeln!(out, "{}", json!({ "id": id, "ok": { "ran": msg["command"] } })).unwrap();
        }
    });
    let registry = ac_cli::session(&repo().join("assets"), &scratch("live-registry")).unwrap();
    let live = ac_cli::mcp::Live::connect(port).unwrap().with_timeout(7);
    let mut srv = ac_cli::mcp::Server::live(live, &registry);
    let notes = Arc::new(Mutex::new(Vec::new()));
    let sink = notes.clone();
    srv.set_notifier(Box::new(move |v| sink.lock().unwrap().push(v.clone())));
    let r = srv
        .handle(
            &json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
            "name": "command_run", "arguments": { "id": "ai.fill" }, "_meta": { "progressToken": "t1" } } }),
        )
        .unwrap();
    assert_eq!(body(&r["result"])["ran"], "ai.fill", "{r}");
    let notes = notes.lock().unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0]["method"], "notifications/progress");
    assert_eq!(notes[0]["params"]["progressToken"], "t1");
    // without a token: no progress asked for, still the right reply
    let r = call(
        &mut srv,
        "command_run",
        json!({ "id": "frame.set", "params": { "frame": 1 } }),
    );
    assert_eq!(body(&r)["ran"], "frame.set");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0]["id"], 1);
    assert_eq!(seen[0]["timeout"], 7);
    assert_eq!(seen[0]["progress"], true);
    assert_eq!(seen[1]["id"], 2);
    assert!(seen[1].get("progress").is_none());
}

/// `command_run {dry_run: true}` runs through edit.dry_run: the result, the change summary and the
/// undo step it would make, and the document as it was.
#[test]
fn a_dry_run_changes_nothing() {
    let mut srv = server("dry-run");
    let r = call(
        &mut srv,
        "command_run",
        json!({ "id": "pose.set_root", "params": { "root": [0, 0, 2] }, "dry_run": true }),
    );
    assert_eq!(r["isError"], false, "{r}");
    let b = body(&r);
    assert_eq!(b["id"], "pose.set_root");
    assert_eq!(b["undo_label"], "Set root offset");
    assert!(b["changed"].is_object(), "{b}");
    let root = body(&call(
        &mut srv,
        "command_run",
        json!({ "id": "bone.world", "params": { "bone": "root" } }),
    ));
    assert!(
        (root["position"][2].as_f64().unwrap() - 2.0).abs() > 0.5,
        "{root}"
    );
    let u = body(&call(&mut srv, "command_run", json!({ "id": "edit.undo" })));
    assert_eq!(u["changed"], false, "a dry run makes no undo step");
    // a command that writes files is refused, with the fix
    let w = call(
        &mut srv,
        "command_run",
        json!({ "id": "clip.export", "params": { "path": "x.glb" }, "dry_run": true }),
    );
    assert_eq!(w["isError"], true);
    assert_eq!(w["structuredContent"]["kind"], "refused", "{w}");
    assert!(w["structuredContent"]["hint"].is_string(), "{w}");
}

/// A refusal is structured: kind, command and the suggestions (or hint) as structuredContent and
/// beside the message in the text.
#[test]
fn refusals_are_structured() {
    let mut srv = server("refusals");
    let r = call(&mut srv, "command_run", json!({ "id": "key.sett" }));
    assert_eq!(r["isError"], true);
    let s = &r["structuredContent"];
    assert_eq!(s["kind"], "unknown_command");
    assert_eq!(s["suggestions"], json!(["key.set"]), "{r}");
    let b = body(&r);
    assert!(b["error"].as_str().unwrap().contains("key.set"), "{b}");
    assert_eq!(b["suggestions"], json!(["key.set"]));
    // a refusal with its fix
    let r = call(&mut srv, "command_run", json!({ "id": "clip.metrics" }));
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(r["structuredContent"]["kind"], "refused");
    assert_eq!(r["structuredContent"]["command"], "clip.metrics");
    // bad parameters name the parameter
    let r = call(
        &mut srv,
        "command_run",
        json!({ "id": "frame.set", "params": { "fram": 1 } }),
    );
    assert_eq!(r["structuredContent"]["kind"], "params", "{r}");
    assert!(
        r["structuredContent"]["message"]
            .as_str()
            .unwrap()
            .contains("frame"),
        "{r}"
    );
}

/// The schemas follow each parameter's form (a list is an array of its kind, JSON an object or
/// array as its doc says), and outputs list the required ones.
#[test]
fn schemas_follow_forms_and_outputs() {
    let mut srv = server("forms");
    let trail = body(&call(
        &mut srv,
        "describe_command",
        json!({ "id": "trail.points" }),
    ));
    let points = &trail["output_schema"]["properties"]["points"];
    assert_eq!(points["type"], "array", "{trail}");
    assert_eq!(points["items"]["maxItems"], 3);
    assert!(
        trail["output_schema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("points"))
    );
    let batch = body(&call(&mut srv, "describe_command", json!({ "id": "edit.batch" })));
    assert_eq!(batch["input_schema"]["properties"]["commands"]["type"], "array");
    assert_eq!(batch["undo_label"], "per run");
    let dry = body(&call(
        &mut srv,
        "describe_command",
        json!({ "id": "edit.dry_run" }),
    ));
    assert_eq!(dry["input_schema"]["properties"]["params"]["type"], "object");
    let export = body(&call(
        &mut srv,
        "describe_command",
        json!({ "id": "clip.export" }),
    ));
    assert_eq!(export["writes"], true);
    // the compact list reads kinds with their form
    let l = body(&call(
        &mut srv,
        "command_list",
        json!({ "query": "trail.points", "limit": 1 }),
    ));
    let out = l[0]["outputs"].as_array().unwrap();
    let pts = out.iter().find(|o| o["name"] == "points").unwrap();
    assert_eq!(pts["kind"], "list of [x, y, z]");
}

/// `initialize` and `session_reset` carry the session's own account (session.info): rig and limit
/// hashes; the headless reset is the session.reset command.
#[test]
fn determinism_names_the_session() {
    let mut srv = server("determinism");
    let init = rpc(&mut srv, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    let s = &init["result"]["determinism"]["session"];
    assert!(s["rig_hash"].is_string(), "{s}");
    assert!(s["limits_hash"].is_string(), "{s}");
    assert_eq!(s["deterministic"], true);
    let _ = call(
        &mut srv,
        "command_run",
        json!({ "id": "pose.set_root", "params": { "root": [0, 0, 1] } }),
    );
    let r = body(&call(&mut srv, "session_reset", json!({})));
    assert_eq!(r["result"]["reset"], true, "{r}");
    assert_eq!(r["result"]["dropped"], 1);
    assert_eq!(r["determinism"]["session"]["history"], 0);
}

/// fix_foot_sliding measures with clip.metrics and clip.critique.
#[test]
fn the_foot_sliding_prompt_measures_with_metrics() {
    let mut srv = server("prompt");
    let p = rpc(
        &mut srv,
        "prompts/get",
        json!({ "name": "fix_foot_sliding", "arguments": {} }),
    );
    let text = p["result"]["messages"][0]["content"]["text"].as_str().unwrap();
    for id in [
        "clip.metrics",
        "clip.critique",
        "feet.plant",
        "dry_run",
        "command_batch",
    ] {
        assert!(text.contains(id), "{id}: {text}");
    }
    assert!(!text.contains("If command_list"), "{text}");
}

/// Live: a refusal the app sends with its `detail` (CoreError::to_json) comes back structured.
#[test]
fn live_refusals_keep_their_detail() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (s, _) = listener.accept().unwrap();
        let mut out = s.try_clone().unwrap();
        for line in BufReader::new(s).lines() {
            let msg: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let reply = json!({ "id": msg["id"], "error": "key.sett: unknown",
                "detail": { "kind": "unknown_command", "message": "unknown command \"key.sett\"", "suggestions": ["key.set"] } });
            writeln!(out, "{reply}").unwrap();
        }
    });
    let registry = ac_cli::session(&repo().join("assets"), &scratch("live-detail")).unwrap();
    let mut srv = ac_cli::mcp::Server::live(ac_cli::mcp::Live::connect(port).unwrap(), &registry);
    let r = call(&mut srv, "command_run", json!({ "id": "key.sett" }));
    assert_eq!(r["isError"], true);
    assert_eq!(r["structuredContent"]["suggestions"], json!(["key.set"]), "{r}");
}
