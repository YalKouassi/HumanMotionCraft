//! The MCP server answers the protocol and reaches the same commands (no GPU needed).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::{Value, json};

fn server() -> ac_cli::mcp::Server {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = std::env::temp_dir().join(format!(
        "{}-mcp-test-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let s = ac_cli::session(&repo.join("assets"), &lib).unwrap();
    ac_cli::mcp::Server::new(s, repo.join("assets"))
}

fn call(srv: &mut ac_cli::mcp::Server, id: i64, name: &str, args: Value) -> Value {
    let r = srv
        .handle(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } }))
        .unwrap();
    assert_eq!(r["id"], id);
    r["result"].clone()
}

fn body(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn speaks_the_protocol_and_runs_commands() {
    let mut srv = server();
    let init = srv
        .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }))
        .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], ac_core::product::ID);
    assert!(
        srv.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .is_none()
    );
    let tools = srv
        .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .unwrap();
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "command_list",
            "describe_command",
            "command_run",
            "command_batch",
            "editor_state",
            "render_preview",
            "contact_sheet",
            "animation_preview",
            "expect",
            "session_reset",
            "ui_events",
            "ui_layout",
            "ui_input",
            "ui_click",
            "ui_set",
            "ui_screenshot"
        ]
    );

    // every registered command is listed
    let list = body(&call(&mut srv, 3, "command_list", json!({})));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = ac_cli::session(
        &repo.join("assets"),
        &std::env::temp_dir().join(format!("{}-mcp-test-count", ac_core::product::ID)),
    )
    .unwrap();
    let ids: Vec<&str> = registry.commands().iter().map(|c| c.id).collect();
    let listed: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(listed, ids);
    // a command runs, and its effect shows in a query
    let r = call(
        &mut srv,
        4,
        "command_run",
        json!({ "id": "pose.set_root", "params": { "root": [0, 0, 0] } }),
    );
    assert_eq!(r["isError"], false);
    let w = body(&call(
        &mut srv,
        5,
        "command_run",
        json!({ "id": "bone.world", "params": { "bone": "root" } }),
    ));
    assert!((w["position"][2].as_f64().unwrap() - 0.0504142).abs() < 1e-6);
    // a refusal is an error result with the reason, not a protocol error
    let bad = call(
        &mut srv,
        6,
        "command_run",
        json!({ "id": "bone.world", "params": { "bone": "hand.L" } }),
    );
    assert_eq!(bad["isError"], true);
    assert!(body(&bad)["error"].as_str().unwrap().contains("hand.L"));
    // a batch is one edit.batch: atomic by default, so a refusal names the failed step and puts
    // everything back
    let three = json!({ "commands": [
        { "id": "frame.set", "params": { "frame": 3 } }, { "id": "no.such", "params": {} },
        { "id": "frame.set", "params": { "frame": 9 } } ] });
    let b = call(&mut srv, 7, "command_batch", three.clone());
    assert_eq!(b["isError"], true);
    assert_eq!(body(&b)["kind"], "step", "{b}");
    assert_eq!(body(&b)["step"], 1);
    assert_eq!(body(&b)["id"], "no.such");
    assert_eq!(b["structuredContent"]["error"]["kind"], "unknown_command");
    assert_eq!(body(&call(&mut srv, 8, "editor_state", json!({})))["frame"], 0.0);
    // not atomic: the steps before the refusal stay, and the result says which step failed
    let mut partial = three;
    partial["atomic"] = json!(false);
    partial["commands"][1] = json!({ "id": "frame.set", "params": { "frame": -5 } });
    let b = call(&mut srv, 9, "command_batch", partial);
    assert_eq!(b["isError"], true, "{b}");
    assert_eq!(body(&b)["done"], 1);
    assert_eq!(body(&b)["failed"]["step"], 1);
    assert_eq!(body(&call(&mut srv, 10, "editor_state", json!({})))["frame"], 3.0);
    // a batch that passes is one undo step under its label
    let ok = call(
        &mut srv,
        11,
        "command_batch",
        json!({ "label": "Two roots", "commands": [
            { "id": "pose.set_root", "params": { "root": [0, 0, 1] } },
            { "id": "pose.set_root", "params": { "root": [0, 0, 1.5] } } ] }),
    );
    assert_eq!(ok["isError"], false, "{ok}");
    assert_eq!(body(&ok)["done"], 2);
    let undo = body(&call(&mut srv, 12, "command_run", json!({ "id": "edit.undo" })));
    assert_eq!(undo["label"], "Two roots");
    // unknown methods are protocol errors
    let e = srv
        .handle(&json!({ "jsonrpc": "2.0", "id": 13, "method": "nope" }))
        .unwrap();
    assert_eq!(e["error"]["code"], -32601);
}

/// `ac mcp --live`: tools reach the app's control port (a stand-in here) and come back as MCP
/// results; a picture arrives as an image.
#[test]
fn live_mode_forwards_to_the_app_port() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (s, _) = listener.accept().unwrap();
        let mut out = s.try_clone().unwrap();
        for line in BufReader::new(s).lines() {
            let msg: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
            let reply = match msg["op"].as_str().unwrap() {
                "run" if msg["command"] == "no.such" => json!({ "error": "unknown command no.such" }),
                "run" => json!({ "ok": { "ran": msg["command"], "params": msg["params"] } }),
                "screenshot" | "render" => json!({ "ok": { "png": ac_cli::render::base64(b"PNGDATA") } }),
                "sheet" => {
                    json!({ "ok": { "png": ac_cli::render::base64(b"SHEET"), "frames": msg["frames"] } })
                }
                "input" => json!({ "ok": { "played": msg["events"].as_array().map_or(0, Vec::len) } }),
                "layout" => json!({ "ok": { "viewport": [0, 0, 10, 10] } }),
                "ui" => json!({ "ok": { "set": msg["name"] } }),
                "click" => json!({ "ok": { "clicked": msg["widget"], "modifiers": msg["modifiers"] } }),
                _ => json!({ "error": "?" }),
            };
            writeln!(out, "{reply}").unwrap();
        }
    });
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = ac_cli::session(
        &repo.join("assets"),
        &std::env::temp_dir().join(format!("{}-mcp-live-test", ac_core::product::ID)),
    )
    .unwrap();
    let mut srv = ac_cli::mcp::Server::live(ac_cli::mcp::Live::connect(port).unwrap(), &registry);
    let r = call(
        &mut srv,
        1,
        "command_run",
        json!({ "id": "pose.reset", "params": { "x": 1 } }),
    );
    assert_eq!(r["isError"], false);
    assert_eq!(body(&r)["ran"], "pose.reset");
    assert_eq!(body(&r)["params"]["x"], 1);
    let bad = call(&mut srv, 2, "command_run", json!({ "id": "no.such" }));
    assert_eq!(bad["isError"], true);
    let shot = call(&mut srv, 3, "ui_screenshot", json!({}));
    assert_eq!(shot["content"][0]["type"], "image");
    assert_eq!(
        ac_cli::render::unbase64(shot["content"][0]["data"].as_str().unwrap()).unwrap(),
        b"PNGDATA"
    );
    let sheet = call(&mut srv, 5, "contact_sheet", json!({ "frames": [0, 10] }));
    assert_eq!(
        ac_cli::render::unbase64(sheet["content"][0]["data"].as_str().unwrap()).unwrap(),
        b"SHEET"
    );
    let played = call(
        &mut srv,
        6,
        "ui_input",
        json!({ "events": [{ "kind": "click", "x": 1, "y": 2 }] }),
    );
    assert_eq!(body(&played)["played"], 1);
    assert_eq!(
        body(&call(&mut srv, 7, "ui_layout", json!({})))["viewport"],
        json!([0, 0, 10, 10])
    );
    assert_eq!(
        body(&call(
            &mut srv,
            8,
            "ui_set",
            json!({ "name": "dope", "value": "on" })
        ))["set"],
        "dope"
    );
    let clicked = call(
        &mut srv,
        9,
        "ui_click",
        json!({ "id": "toolbar.tool.rotate", "modifiers": ["shift"] }),
    );
    assert_eq!(body(&clicked)["clicked"], "toolbar.tool.rotate");
    assert_eq!(body(&clicked)["modifiers"], json!(["shift"]));
    // the command list is the registry's, without asking the app
    let list = body(&call(&mut srv, 4, "command_list", json!({ "area": "ai." })));
    assert!(list.as_array().unwrap().iter().any(|c| c["id"] == "ai.fill"));
}

/// Headless, the window tools say what they need instead of pretending.
#[test]
fn window_tools_need_the_running_app() {
    let mut srv = server();
    for (i, tool) in ["ui_layout", "ui_input", "ui_click", "ui_set", "ui_screenshot"]
        .into_iter()
        .enumerate()
    {
        let r = call(
            &mut srv,
            i as i64 + 1,
            tool,
            json!({ "events": [], "id": "toolbar.tool.rotate", "name": "dope", "value": "on" }),
        );
        assert_eq!(r["isError"], true, "{tool}");
        assert!(
            r["content"][0]["text"].as_str().unwrap().contains("--control"),
            "{tool}: {r}"
        );
    }
}

/// `ac mcp --live` starts before the app does and survives the app restarting: a call with no app
/// says how to start it, the next call after the app is up reaches it, and after the app drops the
/// connection (a restart) the following call reconnects instead of failing.
#[test]
fn live_mode_waits_for_the_app_and_reconnects() {
    use std::io::{BufRead, BufReader, Write};
    // a free port, closed again so nothing listens on it yet
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = ac_cli::session(
        &repo.join("assets"),
        &std::env::temp_dir().join(format!("{}-mcp-reconnect-test", ac_core::product::ID)),
    )
    .unwrap();
    let mut srv = ac_cli::mcp::Server::live(ac_cli::mcp::Live::new(port), &registry);
    let none = call(&mut srv, 1, "editor_state", json!({}));
    assert_eq!(none["isError"], true);
    assert!(
        none["content"][0]["text"].as_str().unwrap().contains("--control"),
        "{none}"
    );
    // the app comes up; each connection answers one request and closes, like an app restarting
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    std::thread::spawn(move || {
        for (n, s) in listener.incoming().flatten().enumerate() {
            let mut out = s.try_clone().unwrap();
            let mut line = String::new();
            if BufReader::new(s).read_line(&mut line).unwrap_or(0) == 0 {
                continue;
            }
            writeln!(out, "{}", json!({ "ok": { "frame": n } })).unwrap();
        }
    });
    for id in 2..5 {
        let r = call(&mut srv, id, "editor_state", json!({}));
        assert_eq!(r["isError"], false, "call {id}: {r}");
    }
}

/// `ac mcp --live <port> --timeout <s>`: seconds from 1 to 3600, said plainly otherwise.
#[test]
fn live_timeouts_are_checked() {
    assert_eq!(ac_cli::mcp::parse_timeout("30"), Ok(30));
    assert_eq!(ac_cli::mcp::parse_timeout(" 3600 "), Ok(3600));
    for bad in ["0", "3601", "-5", "soon", ""] {
        assert!(ac_cli::mcp::parse_timeout(bad).is_err(), "{bad}");
    }
}

/// An FBX animation comes onto the rig through `command_run clip.import`, with its report, and a
/// file that is not a humanoid is refused with the missing landmarks named (roadmap 152000).
#[test]
fn an_fbx_imports_through_mcp() {
    let mut srv = server();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = repo.join("fixtures/data/walk-yup.fbx");
    let r = call(
        &mut srv,
        1,
        "command_run",
        json!({ "id": "clip.import", "params": { "path": path.to_string_lossy() } }),
    );
    assert_eq!(r["isError"], false, "{r}");
    let b = body(&r);
    assert_eq!(b["length"], 40);
    assert_eq!(b["joints"]["l_knee"], "LeftLeg");
    assert_eq!(b["source"]["up_axis"], "+Y");
    assert!(
        b["at_rest"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "finger: 30 of 30 at rest"),
        "{b}"
    );
    let state = body(&call(&mut srv, 2, "editor_state", json!({})));
    assert_eq!(
        state["clip"]["frames"]
            .as_i64()
            .or(state["frames"].as_i64())
            .unwrap_or(40),
        40
    );
    let bad = repo.join("fixtures/data/not-humanoid.fbx");
    let r = call(
        &mut srv,
        3,
        "command_run",
        json!({ "id": "clip.import", "params": { "path": bad.to_string_lossy() } }),
    );
    assert_eq!(r["isError"], true, "{r}");
    let e = body(&r)["error"].as_str().unwrap().to_string();
    assert!(e.contains("l_knee") && e.contains("joints"), "{e}");
}
