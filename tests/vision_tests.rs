//! Vision provider: request construction, untrusted marking, loopback policy,
//! and refusal to follow a redirect off the machine. A local mock server stands
//! in for a real model.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use common::*;

fn http_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Accept one connection, capture the request, answer with `response`.
fn spawn_mock(response: String) -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let request = read_request(stream.try_clone().expect("clone stream"));
            answer_into(stream, &response);
            let _ = sender.send(request);
        }
    });
    (port, receiver)
}

fn read_request(stream: TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let length: usize = head
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().to_string())
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    format!("{head}{}", String::from_utf8_lossy(&body))
}

fn answer_into(stream: TcpStream, response: &str) {
    let mut stream = stream;
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Split a captured HTTP request into header block and parsed JSON body.
fn request_body(request: &str) -> serde_json::Value {
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_else(|| panic!("request has no body: {request}"));
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON ({err}): {body}"))
}

#[test]
fn a_local_endpoint_is_asked_with_a_data_uri_and_marked_untrusted() {
    let body = r#"{"choices":[{"message":{"content":"A white box with black text."}}]}"#;
    let (port, requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");

    let output = run_env(
        &[
            &fixture("text.png"),
            "--json",
            "--ask",
            "What is in this image?",
        ],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(
        json["vision"]["description"],
        "A white box with black text."
    );
    assert_eq!(json["vision"]["untrusted"], true);
    assert_eq!(json["vision"]["provider"], "openai-compatible");
    assert_eq!(json["vision"]["model"], "test-vl");
    assert_eq!(json["vision"]["question"], "What is in this image?");
    assert!(json["vision"]["duration_ms"].as_u64().is_some());

    let request = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("the mock must have been called");
    assert!(
        request.starts_with("POST /v1/chat/completions"),
        "{request}"
    );
    let body = request_body(&request);
    assert_eq!(body["model"], "test-vl");
    assert_eq!(body["max_tokens"], 400);
    assert_eq!(
        body["messages"][0]["content"][0]["text"],
        "What is in this image?"
    );
    let url = body["messages"][0]["content"][1]["image_url"]["url"]
        .as_str()
        .expect("image_url");
    assert!(
        url.starts_with("data:image/png;base64,"),
        "expected a png data URI, got {}",
        &url[..url.len().min(40)]
    );
    assert!(
        !request.contains("Authorization"),
        "no key was configured, so no header should be sent"
    );
}

#[test]
fn an_api_key_is_sent_only_when_configured() {
    let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
    let (port, requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
            ("OMMA_VISION_API_KEY", "sk-test-123"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["vision"]["description"], "ok");
    assert_eq!(json["vision"]["question"], omma::cli::DEFAULT_QUESTION);
    let request = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("called");
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-test-123"),
        "{request}"
    );
    // The key must never surface in the report.
    assert!(!stdout_text(&output).contains("sk-test-123"));
    assert!(!stderr_text(&output).contains("sk-test-123"));
}

#[test]
fn a_remote_endpoint_is_refused_without_explicit_opt_in() {
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            (
                "OMMA_VISION_ENDPOINT",
                "http://example.com/v1/chat/completions",
            ),
            ("OMMA_VISION_MODEL", "some-model"),
        ],
    );
    assert_eq!(code(&output), 2, "a remote upload needs allow_remote");
    let stderr = stderr_text(&output);
    assert!(stderr.contains("loopback"), "unexpected error: {stderr}");
}

#[test]
fn an_endpoint_with_embedded_credentials_is_refused() {
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", "http://user:pass@127.0.0.1:9/x"),
            ("OMMA_VISION_MODEL", "m"),
        ],
    );
    assert_eq!(code(&output), 2);
    assert!(stderr_text(&output).contains("credentials"));
}

#[test]
fn a_redirect_is_not_followed() {
    // The redirecting mock and the would-be destination.
    let destination = TcpListener::bind("127.0.0.1:0").expect("bind destination");
    let destination_port = destination.local_addr().unwrap().port();
    destination.set_nonblocking(true).expect("nonblocking");

    let body = "";
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{destination_port}/elsewhere\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (port, _requests) = spawn_mock(response);
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");

    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(
        code(&output),
        3,
        "a redirect is a backend failure, not a success"
    );
    let json = json_of(&output);
    assert!(json["vision"].is_null());
    let warnings = json["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning["code"] == "vision_failed"),
        "expected vision_failed, got {warnings:?}"
    );
    assert!(
        destination.accept().is_err(),
        "omma must not connect to the redirect target"
    );
}

#[test]
fn an_unreachable_endpoint_warns_and_still_returns_the_analysis() {
    // Reserve a port, then release it: nothing is listening there.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().unwrap().port()
    };
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("text.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(code(&output), 3);
    let json = json_of(&output);
    assert!(json["vision"].is_null());
    assert!(json["colors"].is_object(), "other subsystems still ran");
    assert!(
        json["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| { warning["code"] == "vision_failed" })
    );
}

#[test]
fn no_vision_request_is_made_when_vision_is_not_asked_for() {
    let (port, requests) = spawn_mock(http_response("{}"));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("solid.png"), "--json"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(code(&output), 0);
    assert!(json_of(&output)["vision"].is_null());
    assert!(
        requests.recv_timeout(Duration::from_millis(300)).is_err(),
        "no image may be sent anywhere without an explicit request"
    );
}

#[test]
fn base64_encoding_matches_the_standard_alphabet() {
    assert_eq!(omma::vision::encode_base64(b""), "");
    assert_eq!(omma::vision::encode_base64(b"f"), "Zg==");
    assert_eq!(omma::vision::encode_base64(b"fo"), "Zm8=");
    assert_eq!(omma::vision::encode_base64(b"foo"), "Zm9v");
    assert_eq!(omma::vision::encode_base64(b"foobar"), "Zm9vYmFy");
}

// --- regression: loopback detection -------------------------------------

#[test]
fn a_hostname_that_only_looks_like_loopback_is_refused() {
    for endpoint in [
        "http://127.0.0.1.attacker.example:11434/v1/chat/completions",
        "http://127.example.com/v1/chat/completions",
        "http://localhost.attacker.example/v1/chat/completions",
    ] {
        let config = write_config(&format!(
            "[vision]\nenabled = true\nendpoint = \"{endpoint}\"\nmodel = \"m\"\n"
        ));
        let output = run(&[
            &fixture("text.png"),
            "--json",
            "--vision",
            "--config",
            &config,
        ]);
        assert_eq!(
            code(&output),
            2,
            "{endpoint} must be refused before any request: {}",
            stderr_text(&output)
        );
        assert!(
            stderr_text(&output).contains("loopback"),
            "{endpoint}: {}",
            stderr_text(&output)
        );
        let _ = std::fs::remove_file(config);
    }
}

#[test]
fn loopback_means_a_loopback_literal_and_nothing_else() {
    for allowed in [
        "localhost",
        "LOCALHOST",
        "localhost:11434",
        "127.0.0.1",
        "127.0.0.1:8080",
        "127.1.2.3",
        "[::1]:80",
    ] {
        assert!(
            omma::vision::is_loopback(allowed),
            "{allowed} should be loopback"
        );
    }
    for refused in [
        "127.0.0.1.attacker.example",
        "127.example.com",
        "127.0.0.1x",
        "127",
        "0.0.0.0",
        "10.1.2.3",
        "example.com",
        "localhost.attacker.example",
        "[fe80::1]",
    ] {
        assert!(
            !omma::vision::is_loopback(refused),
            "{refused} must not be loopback"
        );
    }
}

#[test]
fn a_control_character_in_the_answer_never_reaches_the_terminal() {
    let body = r#"{"choices":[{"message":{"content":"before\u001b[2Jafter"}}]}"#;
    let (port, _requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("text.png"), "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let stdout = stdout_text(&output);
    assert!(stdout.contains("before") && stdout.contains("after"));
    assert!(
        !stdout.contains('\u{1b}'),
        "an escape sequence from the model reached the terminal"
    );
}
