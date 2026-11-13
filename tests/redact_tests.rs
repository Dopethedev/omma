//! Redaction: rectangles painted out before any subsystem reads the pixels.
//!
//! The contract is stronger than cropping — a redacted area must be gone from
//! OCR, colours, layout and the vision upload at once, because the point is that
//! the data is not in the report either.

mod common;

use common::*;

#[test]
fn the_covered_text_is_absent_from_the_report() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    let clean = run(&[&fixture("text.png"), "--json"]);
    assert_eq!(code(&clean), 0, "stderr: {}", stderr_text(&clean));
    let clean_text = json_of(&clean)["ocr"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        clean_text.contains("Omma"),
        "fixture should read; got {clean_text:?}"
    );

    // Cover the whole frame: nothing can be recognised through opaque paint.
    let hidden = run(&[&fixture("text.png"), "--json", "--redact", "0,0,600,140"]);
    assert_eq!(code(&hidden), 0, "stderr: {}", stderr_text(&hidden));
    let report = json_of(&hidden);
    let hidden_text = report["ocr"]["text"].as_str().unwrap_or_default();
    assert!(
        !hidden_text.contains("Omma"),
        "redacted text survived: {hidden_text:?}"
    );
}

#[test]
fn every_box_is_recorded_so_the_report_says_what_was_hidden() {
    let output = run(&[
        &fixture("text.png"),
        "--json",
        "--redact",
        "10,20,30,40",
        "--redact",
        "100,0,50,20",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr_text(&output));
    let report = json_of(&output);
    let boxes = report["redactions"].as_array().expect("redactions array");
    assert_eq!(boxes.len(), 2);
    assert_eq!(boxes[0]["x"], 10);
    assert_eq!(boxes[0]["y"], 20);
    assert_eq!(boxes[0]["width"], 30);
    assert_eq!(boxes[0]["height"], 40);
    assert_eq!(boxes[0]["source_width"], 600);
    assert_eq!(boxes[0]["source_height"], 140);
}

#[test]
fn a_box_that_runs_past_the_edge_is_clamped() {
    let output = run(&[
        &fixture("text.png"),
        "--json",
        "--redact",
        "580,120,999,999",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr_text(&output));
    let report = json_of(&output);
    let box_ = &report["redactions"][0];
    assert_eq!(box_["width"], 20, "clamped to the remaining columns");
    assert_eq!(box_["height"], 20, "clamped to the remaining rows");
}

#[test]
fn useless_boxes_are_refused_with_a_reason() {
    let zero = run(&[&fixture("text.png"), "--redact", "10,10,0,5"]);
    assert_eq!(code(&zero), 2);
    assert!(
        stderr_text(&zero).contains("greater than 0"),
        "stderr: {}",
        stderr_text(&zero)
    );

    let outside = run(&[&fixture("text.png"), "--redact", "900,900,10,10"]);
    assert_eq!(code(&outside), 2);
    assert!(
        stderr_text(&outside).contains("outside"),
        "stderr: {}",
        stderr_text(&outside)
    );
}

#[test]
fn no_boxes_means_no_field_in_the_json() {
    let output = run(&[&fixture("text.png"), "--json"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr_text(&output));
    let report = json_of(&output);
    assert!(
        report.get("redactions").is_none(),
        "an empty redaction list should be omitted, not serialised"
    );
}
