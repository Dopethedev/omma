//! The JSON contract: shape, types, null rules and determinism.

mod common;

use common::*;
use serde_json::Value;

fn report(image: &str, extra: &[&str]) -> Value {
    let mut args = vec![image, "--json"];
    args.extend_from_slice(extra);
    let output = run(&args);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    json_of(&output)
}

#[test]
fn top_level_shape_is_stable() {
    let json = report(&fixture("text.png"), &[]);
    for key in [
        "schema_version",
        "tool",
        "image",
        "region",
        "ocr",
        "colors",
        "grid",
        "layout",
        "vision",
        "warnings",
        "processing",
    ] {
        assert!(json.get(key).is_some(), "missing top-level key {key}");
    }
    assert_eq!(json["tool"]["name"], "omma");
    assert!(json["tool"]["version"].is_string());
    assert_eq!(json["schema_version"], "1.0");
}

#[test]
fn image_metadata_is_measured_and_typed() {
    let json = report(&fixture("text.png"), &[]);
    let image = &json["image"];
    assert_eq!(image["format"], "png");
    assert_eq!(image["width"], 600);
    assert_eq!(image["height"], 140);
    assert_eq!(image["pixel_count"], 600 * 140);
    assert!(image["file_size_bytes"].as_u64().unwrap() > 0);
    assert!((image["aspect_ratio"].as_f64().unwrap() - 600.0 / 140.0).abs() < 0.001);
    assert!(
        image["animated"].is_null(),
        "png frame count is not detectable"
    );
}

#[test]
fn untrusted_markers_are_present_where_required() {
    let json = report(&fixture("text.png"), &[]);
    assert_eq!(json["ocr"]["untrusted"], true);
    // Vision is off here, so there is nothing to mark.
    assert!(json["vision"].is_null());
}

#[test]
fn null_rules_hold_for_disabled_subsystems() {
    let image = fixture("text.png");
    let json = report(&image, &["--colors"]);
    assert!(json["ocr"].is_null());
    assert!(json["layout"].is_null());
    assert!(json["vision"].is_null());
    assert!(json["region"].is_null());
    assert!(json["colors"].is_object());
}

#[test]
fn percentages_and_statistics_are_in_range() {
    let json = report(&fixture("text.png"), &[]);
    let dominant = json["colors"]["dominant"].as_array().unwrap();
    assert!(!dominant.is_empty());
    let mut sum = 0.0;
    for swatch in dominant {
        let percentage = swatch["percentage"].as_f64().unwrap();
        assert!((0.0..=100.0).contains(&percentage));
        sum += percentage;
        assert_eq!(swatch["rgb"].as_array().unwrap().len(), 3);
        let hex = swatch["hex"].as_str().unwrap();
        assert_eq!(hex.len(), 7);
        assert!(hex.starts_with('#'));
    }
    assert!(sum <= 100.01, "percentages cannot exceed 100");
    let brightness = json["colors"]["brightness"].as_f64().unwrap();
    assert!((0.0..=1.0).contains(&brightness));
    let saturation = json["colors"]["saturation"].as_f64().unwrap();
    assert!((0.0..=1.0).contains(&saturation));
}

#[test]
fn grid_geometry_matches_its_declaration() {
    let json = report(&fixture("text.png"), &[]);
    let grid = &json["grid"];
    assert_eq!(grid["width"], 64);
    assert_eq!(grid["height"], 36);
    assert_eq!(grid["quantization_bits"], 5);
    let rows = grid["cells"].as_array().unwrap();
    assert_eq!(rows.len(), 36);
    for row in rows {
        let cells = row.as_array().unwrap();
        assert_eq!(cells.len(), 64);
        for cell in cells {
            if let Some(hex) = cell.as_str() {
                assert_eq!(hex.len(), 7, "cell {hex} is not a hex colour");
            }
        }
    }
}

#[test]
fn normalized_coordinates_stay_within_the_unit_square() {
    let json = report(&fixture("text.png"), &[]);
    let words = json["ocr"]["words"].as_array().unwrap();
    assert!(!words.is_empty());
    for word in words {
        let normalized = &word["bbox"]["normalized"];
        for key in ["x", "y", "width", "height"] {
            let value = normalized[key].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&value), "{key} out of range: {value}");
        }
    }
}

#[test]
fn output_is_deterministic_apart_from_timings() {
    let image = fixture("text.png");
    let first = report(&image, &[]);
    let second = report(&image, &[]);
    let strip = |mut value: Value| -> Value {
        value["processing"] = Value::Null;
        value["ocr"]["duration_ms"] = Value::Null;
        value
    };
    assert_eq!(strip(first), strip(second), "two runs must agree");
}

#[test]
fn layout_regions_are_empty_not_fabricated_when_ocr_is_off() {
    let image = fixture("text.png");
    let json = report(&image, &["--no-ocr"]);
    let regions = json["layout"]["regions"].as_array().unwrap();
    assert!(
        regions.is_empty(),
        "no OCR means no text regions, never invented ones"
    );
    assert_eq!(json["layout"]["grid"]["included"], true);
}

#[test]
fn text_noise_is_flagged_and_not_promoted_to_regions() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    // A threshold no real word can meet: every block is measured, none is a region.
    let config = write_config("[ocr]\nmin_confidence = 100.0\n");
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--config", &config]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);

    // Measurements survive...
    assert!(!json["ocr"]["blocks"].as_array().unwrap().is_empty());
    assert!(!json["ocr"]["text"].as_str().unwrap().is_empty());
    // ...but nothing becomes a region and both warnings explain why.
    assert!(json["layout"]["regions"].as_array().unwrap().is_empty());
    let codes: Vec<&str> = json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|warning| warning["code"].as_str())
        .collect();
    assert!(codes.contains(&"ocr_low_confidence"), "got {codes:?}");
    assert!(
        codes.contains(&"layout_low_confidence_blocks"),
        "got {codes:?}"
    );
    let _ = std::fs::remove_file(config);
}

#[test]
fn block_and_region_confidences_are_reported() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    let json = report(&fixture("text.png"), &[]);
    for block in json["ocr"]["blocks"].as_array().unwrap() {
        let confidence = block["confidence"].as_f64().expect("block confidence");
        assert!((0.0..=100.0).contains(&confidence));
    }
    for region in json["layout"]["regions"].as_array().unwrap() {
        let confidence = region["confidence"].as_f64().expect("region confidence");
        assert!(
            confidence >= 40.0,
            "regions below the threshold must be filtered"
        );
    }
}

#[test]
fn layout_regions_carry_position_labels_and_reading_order() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    let json = report(&fixture("text.png"), &[]);
    let regions = json["layout"]["regions"].as_array().unwrap();
    assert!(!regions.is_empty());
    let labels = [
        "top-left",
        "top-center",
        "top-right",
        "middle-left",
        "center",
        "middle-right",
        "bottom-left",
        "bottom-center",
        "bottom-right",
    ];
    for (index, region) in regions.iter().enumerate() {
        assert_eq!(region["type"], "text");
        assert_eq!(region["reading_order"], index as u64);
        let position = region["position"].as_str().unwrap();
        assert!(labels.contains(&position), "unknown position {position}");
        assert!(region["content"].as_str().is_some());
    }
}

#[test]
fn compact_and_pretty_json_describe_the_same_report() {
    let image = fixture("solid.png");
    let compact = run(&[&image, "--json"]);
    let pretty = run(&[&image, "--json", "--pretty"]);
    let compact: Value = serde_json::from_slice(&compact.stdout).unwrap();
    let pretty_value: Value = serde_json::from_slice(&pretty.stdout).unwrap();
    let mut compact = compact;
    let mut pretty_value = pretty_value;
    compact["processing"] = Value::Null;
    pretty_value["processing"] = Value::Null;
    compact["ocr"]["duration_ms"] = Value::Null;
    pretty_value["ocr"]["duration_ms"] = Value::Null;
    assert_eq!(compact, pretty_value);
    assert!(pretty.stdout.len() > compact.to_string().len());
}
