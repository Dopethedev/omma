//! One optional provider: any OpenAI-compatible `/v1/chat/completions` endpoint
//! that accepts an `image_url` data URI.
//!
//! Ollama's `/v1`, llama.cpp's server, LM Studio and vLLM all speak this shape.
//! The engine never needs it: without a configured endpoint, nothing here runs
//! and no socket is opened.

use std::time::{Duration, Instant};

use crate::config::Vision;
use crate::error::{Error, Result};
use crate::report::VisionOut;

pub const PROVIDER: &str = "openai-compatible";

/// Reject a non-loopback endpoint unless it was explicitly allowed, and reject
/// URLs with embedded credentials — an upload target must be unambiguous.
pub fn check_endpoint(vision: &Vision) -> Result<()> {
    let raw = vision.endpoint.trim();
    if raw.is_empty() {
        return Err(Error::Config {
            path: "vision.endpoint".to_string(),
            message: "endpoint is empty".to_string(),
        });
    }
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Err(Error::Config {
            path: "vision.endpoint".to_string(),
            message: format!("{raw:?} has no scheme; expected http:// or https://"),
        });
    };
    if scheme != "http" && scheme != "https" {
        return Err(Error::Config {
            path: "vision.endpoint".to_string(),
            message: format!("unsupported scheme {scheme:?}"),
        });
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return Err(Error::Config {
            path: "vision.endpoint".to_string(),
            message: "endpoint has no host".to_string(),
        });
    }
    if authority.contains('@') {
        return Err(Error::Config {
            path: "vision.endpoint".to_string(),
            message: "endpoint must not embed credentials".to_string(),
        });
    }
    if !vision.allow_remote && !is_loopback(authority) {
        return Err(Error::Config {
            path: "vision.endpoint".to_string(),
            message: format!(
                "{authority} is not a loopback address; set vision.allow_remote = true \
                 (or OMMA_VISION_ALLOW_REMOTE=1) to send images off this machine"
            ),
        });
    }
    Ok(())
}

pub fn is_loopback(authority: &str) -> bool {
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        // IPv6 literal: [::1]:8080
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let host = host.to_ascii_lowercase();
    if host == "localhost" || host == "::1" {
        return true;
    }
    // A literal address in 127/8, nothing else. A prefix test accepted
    // "127.0.0.1.attacker.example", which resolves to a real remote host: the
    // image left the machine with allow_remote = false.
    host.parse::<std::net::Ipv4Addr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

/// Ask the configured endpoint one question about one image.
pub fn ask(image_png: &[u8], question: &str, vision: &Vision) -> Result<VisionOut> {
    if vision.model.trim().is_empty() {
        return Err(Error::Vision(
            "no vision model configured (vision.model / --model)".to_string(),
        ));
    }
    let started = Instant::now();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(vision.timeout_seconds)))
        // A local endpoint must not be able to bounce the upload elsewhere.
        .max_redirects(0)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let body = serde_json::json!({
        "model": vision.model,
        "max_tokens": vision.max_tokens,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": question },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", encode_base64(image_png))
                    }
                }
            ]
        }]
    });

    let request_body = serde_json::to_string(&body)
        .map_err(|err| Error::Vision(format!("cannot build request: {err}")))?;

    let mut request = agent
        .post(&vision.endpoint)
        .header("Content-Type", "application/json");
    if let Some(key) = &vision.api_key
        && !key.is_empty()
    {
        request = request.header("Authorization", format!("Bearer {key}"));
    }

    let response = match request.send(request_body.as_bytes()) {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(Error::Vision(format!(
                "{} returned HTTP {code}",
                vision.endpoint
            )));
        }
        Err(ureq::Error::Io(err)) => {
            return Err(Error::Vision(format!(
                "cannot reach {}: {err}",
                vision.endpoint
            )));
        }
        Err(ureq::Error::Timeout(_)) => {
            return Err(Error::Vision(format!(
                "{} timed out after {}s",
                vision.endpoint, vision.timeout_seconds
            )));
        }
        Err(err) => return Err(Error::Vision(format!("{err}"))),
    };

    let mut response = response;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|err| Error::Vision(format!("cannot read response: {err}")))?;
    let payload: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| Error::Vision(format!("response is not JSON: {err}")))?;

    if let Some(message) = payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
    {
        return Err(Error::Vision(format!("provider error: {message}")));
    }

    let answer = payload
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .map(content_text)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| Error::Vision("response contained no message content".to_string()))?;

    Ok(VisionOut {
        untrusted: true,
        enabled: true,
        provider: PROVIDER,
        endpoint: vision.endpoint.clone(),
        model: vision.model.clone(),
        question: question.to_string(),
        description: answer,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// Some servers return a string, others an array of typed parts.
fn content_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Standard base64 with padding; small enough not to justify a dependency.
pub fn encode_base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map(u32::from).unwrap_or(0);
        let b2 = chunk.get(2).copied().map(u32::from).unwrap_or(0);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}
