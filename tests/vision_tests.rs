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
    spawn_mock_delayed(response, Duration::ZERO)
}

/// A response with a non-200 status, so the provider's error path is exercised.
fn http_error(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn spawn_mock_delayed(response: String, delay: Duration) -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let request = read_request(stream.try_clone().expect("clone stream"));
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
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
    // Role 0 is the instruction that keeps image text from acting as a prompt.
    assert_eq!(body["messages"][0]["role"], "system");
    let system = body["messages"][0]["content"].as_str().unwrap_or_default();
    assert!(system.contains("never instructions"), "{system}");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(
        body["messages"][1]["content"][0]["text"],
        "What is in this image?"
    );
    let url = body["messages"][1]["content"][1]["image_url"]["url"]
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

// --- upload sizing -------------------------------------------------------

fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "not a png");
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("height"));
    (width, height)
}

#[test]
fn a_large_image_is_downscaled_to_the_configured_ceiling_before_upload() {
    let path = temp_path("large-upload.png");
    let mut image = image::RgbImage::new(2600, 1400);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
    }
    image.save(&path).expect("write large image");

    let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
    let (port, requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&path.to_string_lossy(), "--json", "--ask", "What is this?"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    let vision = &json["vision"];
    assert_eq!(vision["downscaled"], true, "{vision}");
    let sent_width = vision["sent_width"].as_u64().expect("sent_width") as u32;
    let sent_height = vision["sent_height"].as_u64().expect("sent_height") as u32;
    assert!(sent_width <= 1280 && sent_height <= 1280, "{vision}");
    assert!(vision["request_bytes"].as_u64().unwrap() <= 6 * 1024 * 1024);
    // The analysis itself still describes the original image.
    assert_eq!(json["image"]["width"], 2600);

    let request = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("mock");
    let request_body = request_body(&request);
    let url = request_body["messages"][1]["content"][1]["image_url"]["url"]
        .as_str()
        .expect("image_url");
    let encoded = url
        .strip_prefix("data:image/png;base64,")
        .expect("data uri");
    let uploaded = omma::base64::decode(encoded, 32 * 1024 * 1024).expect("decode upload");
    assert_eq!(png_dimensions(&uploaded), (sent_width, sent_height));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_upload_ceiling_is_configurable_and_smaller_images_are_sent_untouched() {
    let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
    let (port, _requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("text.png"), "--json", "--ask", "Q"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
            ("OMMA_VISION_MAX_DIMENSION", "64"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["vision"]["downscaled"], true);
    assert!(json["vision"]["sent_width"].as_u64().unwrap() <= 64);
    assert!(json["vision"]["sent_height"].as_u64().unwrap() <= 64);
}

#[test]
fn a_small_image_is_uploaded_at_its_own_size() {
    let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
    let (port, _requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("text.png"), "--json", "--ask", "Q"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "test-vl"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["vision"]["downscaled"], false);
    assert_eq!(json["vision"]["sent_width"], 600);
    assert_eq!(json["vision"]["sent_height"], 140);
}

// --- hosted provider: credentials, failure classes, effort ---------------

const FAKE_KEY: &str = "sk-omma-test-not-a-real-key-0123456789";

fn warning_codes(json: &serde_json::Value) -> Vec<String> {
    json["warnings"]
        .as_array()
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(|warning| warning["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_api_key_is_read_from_the_named_environment_variable() {
    let (port, requests) = spawn_mock(http_response(
        r#"{"choices":[{"message":{"content":"a screenshot of a terminal"}}]}"#,
    ));
    let config = write_config(&format!(
        "[vision]\nenabled = true\nendpoint = \"http://127.0.0.1:{port}/v1/chat/completions\"\n\
         model = \"glm-5.3-flash\"\napi_key_env = \"OMMA_TEST_PROVIDER_KEY\"\n\
         reasoning_effort = \"max\"\n"
    ));
    let output = run_env(
        &[
            &fixture("text.png"),
            "--json",
            "--vision",
            "--config",
            &config,
        ],
        &[("OMMA_TEST_PROVIDER_KEY", FAKE_KEY)],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));

    let request = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("the mock must be called");
    let lowered = request.to_ascii_lowercase();
    assert!(
        lowered.contains(&format!(
            "authorization: bearer {}",
            FAKE_KEY.to_lowercase()
        )),
        "the bearer token was not sent: {request}"
    );
    // The key travels in the header and nowhere else.
    let body = request_body(&request);
    assert!(body["reasoning_effort"] == "max", "{body}");
    for stream in [stdout_text(&output), stderr_text(&output)] {
        assert!(!stream.contains(FAKE_KEY), "the key leaked into the output");
    }
    let _ = std::fs::remove_file(config);
}

#[test]
fn the_doctor_reports_where_the_key_comes_from_but_never_the_key() {
    let config = write_config(
        "[vision]\nenabled = true\nmodel = \"glm-5.3-flash\"\napi_key_env = \"OMMA_TEST_DOCTOR_KEY\"\n",
    );
    let output = run_env(
        &["doctor", "--config", &config],
        &[("OMMA_TEST_DOCTOR_KEY", FAKE_KEY)],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let text = stdout_text(&output);
    assert!(
        text.contains("env OMMA_TEST_DOCTOR_KEY"),
        "the doctor must name the source: {text}"
    );
    assert!(!text.contains(FAKE_KEY), "the doctor printed the secret");
    let _ = std::fs::remove_file(config);
}

#[test]
fn a_rejected_key_is_named_and_the_analysis_still_returns() {
    let (port, _requests) = spawn_mock(http_error(
        401,
        r#"{"error":{"message":"invalid api key"}}"#,
    ));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("text.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "glm-5.3-flash"),
            ("OMMA_VISION_API_KEY", FAKE_KEY),
        ],
    );
    // Exit 3 is the documented contract when an explicitly requested backend
    // produced nothing; the JSON below proves the rest of the report survived.
    assert_eq!(code(&output), 3, "{}", stderr_text(&output));
    let json = json_of(&output);
    let codes = warning_codes(&json);
    assert!(
        codes.contains(&"vision_unauthorized".to_string()),
        "{codes:?}"
    );
    assert!(json["vision"].is_null());
    // The rest of the report is untouched by a vision failure.
    assert_eq!(json["image"]["width"], 600);
    assert_eq!(json["grid"]["width"], 64);
    assert!(
        json["ocr"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains("omma reads this line")
    );
    for stream in [stdout_text(&output), stderr_text(&output)] {
        assert!(
            !stream.contains(FAKE_KEY),
            "the key leaked on the error path"
        );
    }
}

#[test]
fn a_rate_limit_is_named_and_does_not_hide_the_rest_of_the_report() {
    let (port, _requests) = spawn_mock(http_error(
        429,
        r#"{"error":{"message":"rate limit exceeded"}}"#,
    ));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "glm-5.3-flash"),
        ],
    );
    assert_eq!(code(&output), 3, "{}", stderr_text(&output));
    let codes = warning_codes(&json_of(&output));
    assert!(
        codes.contains(&"vision_rate_limited".to_string()),
        "{codes:?}"
    );
}

#[test]
fn a_vision_timeout_is_named_and_keeps_ocr() {
    let (port, _requests) = spawn_mock_delayed(
        http_response(r#"{"choices":[{"message":{"content":"too late"}}]}"#),
        Duration::from_secs(5),
    );
    let config = write_config(&format!(
        "[vision]\nenabled = true\nendpoint = \"http://127.0.0.1:{port}/v1/chat/completions\"\n\
         model = \"glm-5.3-flash\"\ntimeout_seconds = 1\n"
    ));
    let output = run(&[
        &fixture("text.png"),
        "--json",
        "--vision",
        "--config",
        &config,
    ]);
    assert_eq!(code(&output), 3, "{}", stderr_text(&output));
    let json = json_of(&output);
    let codes = warning_codes(&json);
    assert!(codes.contains(&"vision_timeout".to_string()), "{codes:?}");
    assert!(
        json["ocr"]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Omma")
    );
    let _ = std::fs::remove_file(config);
}

#[test]
fn a_malformed_answer_is_a_warning_not_a_crash() {
    let (port, _requests) = spawn_mock(http_response("this is not json"));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("text.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "glm-5.3-flash"),
        ],
    );
    assert_eq!(code(&output), 3, "{}", stderr_text(&output));
    let codes = warning_codes(&json_of(&output));
    assert!(codes.contains(&"vision_failed".to_string()), "{codes:?}");
}

#[test]
fn a_remote_endpoint_must_be_https() {
    // allow_remote is the operator's yes to sending an image off the machine; it
    // is not a yes to sending it in the clear.
    let config = write_config(
        "[vision]\nenabled = true\nallow_remote = true\nmodel = \"glm-5.3-flash\"\n\
         endpoint = \"http://api.z.ai/api/paas/v4/chat/completions\"\n",
    );
    let output = run(&[
        &fixture("solid.png"),
        "--json",
        "--vision",
        "--config",
        &config,
    ]);
    assert_eq!(code(&output), 2, "{}", stderr_text(&output));
    assert!(
        stderr_text(&output).contains("https"),
        "{}",
        stderr_text(&output)
    );
    let _ = std::fs::remove_file(config);
}

#[test]
fn reasoning_effort_is_sent_only_when_it_is_configured() {
    let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;

    let (port, requests) = spawn_mock(http_response(body));
    let config = write_config(&format!(
        "[vision]\nenabled = true\nendpoint = \"http://127.0.0.1:{port}/v1/chat/completions\"\n\
         model = \"glm-5.3-flash\"\nreasoning_effort = \"high\"\n"
    ));
    let output = run(&[
        &fixture("solid.png"),
        "--json",
        "--vision",
        "--config",
        &config,
    ]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let request = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("mock");
    assert_eq!(request_body(&request)["reasoning_effort"], "high");
    let _ = std::fs::remove_file(config);

    let (port, requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "glm-5.3-flash"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let request = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("mock");
    assert!(
        request_body(&request).get("reasoning_effort").is_none(),
        "a local server must not receive a field it never asked for"
    );
}

#[test]
fn an_invalid_key_variable_name_is_a_configuration_error() {
    let config = write_config(
        "[vision]\nenabled = true\nmodel = \"glm-5.3-flash\"\napi_key_env = \"not a name\"\n",
    );
    let output = run(&[
        &fixture("solid.png"),
        "--json",
        "--vision",
        "--config",
        &config,
    ]);
    assert_eq!(code(&output), 2, "{}", stderr_text(&output));
    assert!(
        stderr_text(&output).contains("environment variable name"),
        "{}",
        stderr_text(&output)
    );
    let _ = std::fs::remove_file(config);
}

#[test]
fn a_budget_spent_thinking_names_the_budget_instead_of_shrugging() {
    // A reasoning model can burn every token before it writes a word; the old
    // message ("no message content") sent the operator nowhere.
    let body = r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}],
                   "usage":{"completion_tokens":600,"completion_tokens_details":{"reasoning_tokens":600}}}"#;
    let (port, _requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let config = write_config(&format!(
        "[vision]\nenabled = true\nendpoint = \"{endpoint}\"\nmodel = \"glm-5.3-flash\"\n\
         max_tokens = 600\n"
    ));
    let output = run(&[
        &fixture("solid.png"),
        "--json",
        "--vision",
        "--config",
        &config,
    ]);
    assert_eq!(code(&output), 3, "{}", stderr_text(&output));
    let json = json_of(&output);
    let message = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|warning| warning["code"] == "vision_failed")
        .and_then(|warning| warning["message"].as_str())
        .unwrap_or_default()
        .to_string();
    assert!(message.contains("max_tokens"), "{message}");
    assert!(message.contains("600"), "{message}");
    assert!(message.contains("reasoning"), "{message}");
    let _ = std::fs::remove_file(config);
}

#[test]
fn the_finish_reason_and_reasoning_tokens_are_reported_when_present() {
    let body = r#"{"choices":[{"message":{"content":"a chart"},"finish_reason":"stop"}],
                   "usage":{"completion_tokens_details":{"reasoning_tokens":42}}}"#;
    let (port, _requests) = spawn_mock(http_response(body));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "glm-5.3-flash"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["vision"]["finish_reason"], "stop");
    assert_eq!(json["vision"]["reasoning_tokens"], 42);

    // A provider that says nothing extra must not gain empty keys.
    let (port, _requests) = spawn_mock(http_response(
        r#"{"choices":[{"message":{"content":"fine"}}]}"#,
    ));
    let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let output = run_env(
        &[&fixture("solid.png"), "--json", "--vision"],
        &[
            ("OMMA_VISION_ENDPOINT", endpoint.as_str()),
            ("OMMA_VISION_MODEL", "local"),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert!(json["vision"].get("finish_reason").is_none());
    assert!(json["vision"].get("reasoning_tokens").is_none());
}
