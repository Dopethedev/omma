//! MCP over stdio against the real binary: handshake, tool discovery, tool
//! calls, argument validation and the filesystem restriction. Every byte the
//! server writes to stdout must be a protocol frame.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use common::*;
use serde_json::{Value, json};

struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    seen: Vec<String>,
}

impl Session {
    fn start(extra: &[&str]) -> Session {
        let mut command = Command::new(binary());
        command.args(["mcp", "serve"]).args(extra);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("cannot start mcp server");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Session {
            child,
            stdin,
            lines: receiver,
            seen: Vec::new(),
        }
    }

    fn send(&mut self, message: Value) {
        let mut text = message.to_string();
        text.push('\n');
        self.stdin
            .write_all(text.as_bytes())
            .expect("cannot write to server");
        self.stdin.flush().expect("flush");
    }

    /// Read until the response with this id arrives.
    fn response(&mut self, id: u64) -> Value {
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(30))
                .unwrap_or_else(|_| panic!("no response for id {id}"));
            self.seen.push(line.clone());
            let value: Value = serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("stdout line is not JSON ({err}): {line}"));
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    fn initialize(&mut self) -> Value {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "omma-tests", "version": "0.0.0" }
            }
        }));
        let response = self.response(1);
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        response
    }

    fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }));
        self.response(id)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn content_json(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content in {response}"));
    serde_json::from_str(text).expect("tool content must be JSON")
}

#[test]
fn handshake_announces_tools_and_a_name() {
    let mut session = Session::start(&[]);
    let response = session.initialize();
    assert_eq!(response["result"]["serverInfo"]["name"], "omma");
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert!(
        response["result"]["instructions"]
            .as_str()
            .unwrap_or_default()
            .contains("untrusted")
    );
}

#[test]
fn tools_are_discoverable_with_schemas() {
    let mut session = Session::start(&[]);
    session.initialize();
    session.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let response = session.response(2);
    let tools = response["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"analyze_image"), "got {names:?}");
    assert!(names.contains(&"omma_capabilities"), "got {names:?}");

    let analyze = tools
        .iter()
        .find(|tool| tool["name"] == "analyze_image")
        .unwrap();
    assert_eq!(analyze["inputSchema"]["required"][0], "image_path");
    assert_eq!(analyze["annotations"]["readOnlyHint"], true);
    assert_eq!(analyze["annotations"]["idempotentHint"], true);
}

#[test]
fn capabilities_tool_reports_backends() {
    let mut session = Session::start(&[]);
    session.initialize();
    let response = session.call(3, "omma_capabilities", json!({}));
    assert_eq!(response["result"]["isError"], false);
    let value = content_json(&response);
    assert_eq!(value["tool"]["name"], "omma");
    assert_eq!(value["schema_version"], "1.0");
    assert!(value["ocr"]["engine"] == "tesseract");
    assert!(
        value["image_formats"]
            .as_array()
            .unwrap()
            .contains(&json!("png"))
    );
    assert!(value["limits"]["max_pixels"].as_u64().unwrap() > 0);
    assert_eq!(
        response["result"]["structuredContent"]["tool"]["name"],
        "omma"
    );
}

#[test]
fn analyze_image_returns_a_full_report() {
    let mut session = Session::start(&[]);
    session.initialize();
    let response = session.call(
        4,
        "analyze_image",
        json!({ "image_path": fixture("text.png"), "ocr": false }),
    );
    assert_eq!(response["result"]["isError"], false, "{response}");
    let value = content_json(&response);
    assert_eq!(value["schema_version"], "1.0");
    assert_eq!(value["image"]["width"], 600);
    assert!(value["ocr"].is_null());
    assert!(value["colors"].is_object());
    assert_eq!(value["grid"]["width"], 64);
}

#[test]
fn unknown_arguments_are_a_tool_error() {
    let mut session = Session::start(&[]);
    session.initialize();
    let response = session.call(
        5,
        "analyze_image",
        json!({ "image_path": fixture("solid.png"), "shell": "rm -rf /" }),
    );
    assert_eq!(response["result"]["isError"], true);
    let message = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("invalid arguments"), "{message}");
}

#[test]
fn a_missing_file_is_a_tool_error_not_a_crash() {
    let mut session = Session::start(&[]);
    session.initialize();
    let response = session.call(6, "analyze_image", json!({ "image_path": "/nope.png" }));
    assert_eq!(response["result"]["isError"], true);
    let message = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("no such file"), "{message}")
}

#[test]
fn allowed_directories_restrict_what_can_be_read() {
    let allowed = temp_path("allowed-root");
    std::fs::create_dir_all(&allowed).expect("mkdir");
    let config = write_config(&format!(
        "[mcp]\nallowed_directories = [\"{}\"]\n",
        allowed.display()
    ));
    let mut session = Session::start(&["--config", &config]);
    session.initialize();

    // The fixture lives outside the allowed root.
    let denied = session.call(
        7,
        "analyze_image",
        json!({ "image_path": fixture("solid.png") }),
    );
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    let message = denied["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        message.contains("outside the allowed directories"),
        "{message}"
    );

    // A file inside the root is accepted.
    let inside = allowed.join("copy.png");
    std::fs::copy(fixture("solid.png"), &inside).expect("copy");
    let allowed_response = session.call(
        8,
        "analyze_image",
        json!({ "image_path": inside.to_string_lossy(), "ocr": false }),
    );
    assert_eq!(
        allowed_response["result"]["isError"], false,
        "{allowed_response}"
    );

    let _ = std::fs::remove_dir_all(&allowed);
    let _ = std::fs::remove_file(config);
}

#[test]
fn stdout_carries_protocol_frames_only() {
    let mut session = Session::start(&[]);
    session.initialize();
    let _ = session.call(9, "omma_capabilities", json!({}));
    let response = session.call(10, "analyze_image", json!({ "image_path": "/nope.png" }));
    assert_eq!(response["result"]["isError"], true);
    assert!(!session.seen.is_empty());
    for line in &session.seen {
        let value: Value = serde_json::from_str(line).expect("every stdout line is a frame");
        assert_eq!(value["jsonrpc"], "2.0");
    }
}
