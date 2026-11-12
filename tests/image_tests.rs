//! Decoding, format detection, transparency, animation and resource limits.

mod common;

use common::*;

fn analyze(image: &str, extra: &[&str]) -> serde_json::Value {
    let mut args = vec![image, "--json"];
    args.extend_from_slice(extra);
    let output = run(&args);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    json_of(&output)
}

#[test]
fn a_solid_colour_quantizes_exactly() {
    let json = analyze(&fixture("solid.png"), &[]);
    let dominant = &json["colors"]["dominant"][0];
    // #cc3311 with the top 5 bits kept is #c83010.
    assert_eq!(dominant["hex"], "#c83010");
    assert_eq!(dominant["rgb"], serde_json::json!([200, 48, 16]));
    assert!((dominant["percentage"].as_f64().unwrap() - 100.0).abs() < 0.01);
    assert_eq!(json["colors"]["sample"]["transparent_samples"], 0);
}

#[test]
fn transparent_pixels_do_not_distort_the_palette() {
    let json = analyze(&fixture("alpha.png"), &[]);
    let sample = &json["colors"]["sample"];
    assert!(
        sample["transparent_samples"].as_u64().unwrap() > 0,
        "the fixture has a transparent half"
    );
    let dominant = &json["colors"]["dominant"][0];
    let hex = dominant["hex"].as_str().unwrap();
    // The opaque half is pure red; black must not have leaked in from the
    // transparent side.
    assert_ne!(hex, "#000000", "transparent pixels must not become black");
    assert!(
        hex.starts_with("#f"),
        "expected a red dominant colour, got {hex}"
    );

    let cells = json["grid"]["cells"].as_array().unwrap();
    let nulls = cells
        .iter()
        .flat_map(|row| row.as_array().unwrap())
        .filter(|cell| cell.is_null())
        .count();
    assert!(nulls > 0, "fully transparent grid cells must be null");
}

#[test]
fn gif_animation_is_detected_and_png_is_not_detectable() {
    let gif = analyze(&fixture("anim.gif"), &[]);
    assert_eq!(gif["image"]["animated"], true);
    let png = analyze(&fixture("solid.png"), &[]);
    assert!(png["image"]["animated"].is_null());
}

#[test]
fn format_comes_from_content_not_the_extension() {
    let source = fixture("text.png");
    let target = temp_path("mislabelled.bmp");
    std::fs::copy(&source, &target).expect("copy");
    let target_text = target.to_string_lossy().into_owned();
    let json = analyze(&target_text, &["--no-ocr"]);
    assert_eq!(json["image"]["format"], "png");
    assert_eq!(json["image"]["width"], 600);
    let _ = std::fs::remove_file(&target);
}

#[test]
fn pixel_limit_is_enforced_with_a_clear_error() {
    // solid.png is 120x80 = 9600 pixels.
    let config = write_config("[general]\nmax_pixels = 1000\n");
    let output = run(&[&fixture("solid.png"), "--json", "--config", &config]);
    assert_eq!(code(&output), 1);
    let stderr = stderr_text(&output);
    assert!(stderr.contains("too large"), "unexpected error: {stderr}");
    let _ = std::fs::remove_file(config);
}

#[test]
fn dimension_limit_is_enforced() {
    let config = write_config("[general]\nmax_dimension = 64\n");
    let output = run(&[&fixture("solid.png"), "--json", "--config", &config]);
    assert_eq!(code(&output), 1);
    let _ = std::fs::remove_file(config);
}

#[test]
fn empty_input_is_rejected() {
    let path = temp_path("empty.png");
    std::fs::write(&path, b"").expect("write");
    let path_text = path.to_string_lossy().into_owned();
    let output = run(&[&path_text]);
    assert_eq!(code(&output), 1);
    assert!(stderr_text(&output).contains("empty"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn palette_size_and_grid_geometry_follow_configuration() {
    let config = write_config(
        "[colors]\nmax_colors = 2\ngrid_width = 8\ngrid_height = 4\nquantization_bits = 4\n",
    );
    let json = analyze(&fixture("text.png"), &["--config", &config]);
    assert!(json["colors"]["dominant"].as_array().unwrap().len() <= 2);
    assert_eq!(json["grid"]["width"], 8);
    assert_eq!(json["grid"]["height"], 4);
    assert_eq!(json["grid"]["quantization_bits"], 4);
    assert_eq!(json["grid"]["cells"].as_array().unwrap().len(), 4);
    assert_eq!(json["grid"]["cells"][0].as_array().unwrap().len(), 8);
    assert_eq!(json["layout"]["grid"]["width"], 8);
    let _ = std::fs::remove_file(config);
}

#[test]
fn an_oversized_image_is_described_with_its_real_dimensions() {
    let path = temp_path("oversized.png");
    let pixels = image::RgbaImage::from_pixel(20_000, 100, image::Rgba([255, 0, 0, 255]));
    pixels.save(&path).expect("write oversized image");
    let path_text = path.to_string_lossy().into_owned();
    let output = run(&[&path_text]);
    assert_eq!(code(&output), 1);
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("20000x100"),
        "the error must name the image's real size, got: {stderr}"
    );
    assert!(
        stderr.contains("16384 px per side"),
        "the error must name the limit, got: {stderr}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_directory_is_an_input_error_not_a_usage_error() {
    let dir = temp_path("a-directory");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let dir_text = dir.to_string_lossy().into_owned();
    let output = run(&[&dir_text]);
    assert_eq!(code(&output), 1, "{}", stderr_text(&output));
    assert!(stderr_text(&output).contains("not a regular file"));
    let _ = std::fs::remove_dir_all(&dir);
}
