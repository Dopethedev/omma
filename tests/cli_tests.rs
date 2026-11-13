//! CLI behaviour: both documented syntaxes, stdin, regions, output routing and
//! the exit-code contract.

mod common;

use common::*;

#[test]
fn legacy_syntax_and_subcommand_agree() {
    let image = fixture("text.png");
    let legacy = run(&[&image, "--json"]);
    let modern = run(&["analyze", &image, "--json"]);
    assert_eq!(code(&legacy), 0, "{}", stderr_text(&legacy));
    assert_eq!(code(&modern), 0, "{}", stderr_text(&modern));

    let legacy = json_of(&legacy);
    let modern = json_of(&modern);
    assert_eq!(legacy["schema_version"], "1.0");
    assert_eq!(legacy["image"]["width"], modern["image"]["width"]);
    assert_eq!(legacy["image"]["height"], modern["image"]["height"]);
    assert_eq!(legacy["colors"]["dominant"], modern["colors"]["dominant"]);
}

#[test]
fn json_flag_is_an_alias_for_format_json() {
    let image = fixture("text.png");
    let aliased = run(&[&image, "--json"]);
    let explicit = run(&[&image, "--format", "json"]);
    assert_eq!(code(&aliased), 0);
    assert_eq!(code(&explicit), 0);
    let aliased = json_of(&aliased);
    let explicit = json_of(&explicit);
    assert_eq!(aliased["grid"], explicit["grid"]);
}

#[test]
fn unknown_format_is_a_usage_error() {
    let image = fixture("text.png");
    let output = run(&[&image, "--format", "yaml"]);
    assert_eq!(code(&output), 2);
    assert!(stderr_text(&output).contains("unknown --format"));
}

#[test]
fn text_mode_is_human_readable() {
    let image = fixture("text.png");
    let output = run(&[&image]);
    assert_eq!(code(&output), 0);
    let text = stdout_text(&output);
    assert!(text.contains("== text.png"), "missing header: {text}");
    assert!(text.contains("dominant colours"));
    assert!(text.contains("colour grid 64x36"));
}

#[test]
fn json_mode_puts_only_json_on_stdout() {
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--pretty"]);
    assert_eq!(code(&output), 0);
    let stdout = stdout_text(&output);
    assert!(stdout.trim_start().starts_with('{'));
    assert!(stdout.trim_end().ends_with('}'));
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be one JSON value");
}

#[test]
fn stdin_input_is_analyzed() {
    let bytes = std::fs::read(fixture("text.png")).expect("fixture");
    let output = run_stdin(&["-", "--json"], &bytes);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["image"]["filename"], "stdin");
    assert_eq!(json["image"]["format"], "png");
}

#[test]
fn output_file_receives_the_result_and_stdout_stays_empty() {
    let image = fixture("text.png");
    let target = temp_path("result.json");
    let target_text = target.to_string_lossy().into_owned();
    let output = run(&[&image, "--json", "--output", &target_text]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty with --output"
    );
    let written = std::fs::read_to_string(&target).expect("output file");
    let json: serde_json::Value = serde_json::from_str(&written).expect("valid json");
    assert_eq!(json["image"]["width"], 600);
    let _ = std::fs::remove_file(&target);
}

#[test]
fn missing_file_is_an_input_error() {
    let output = run(&["/definitely/not/here.png"]);
    assert_eq!(code(&output), 1);
    assert!(stderr_text(&output).contains("no such file"));
}

#[test]
fn invalid_region_is_a_usage_error() {
    let image = fixture("solid.png");
    for region in ["1,2,3", "a,b,c,d", "0,0,99999,10", "10,10,0,5"] {
        let output = run(&[&image, "--json", "--region", region]);
        assert_eq!(code(&output), 2, "region {region} should be rejected");
    }
}

#[test]
fn region_reports_offsets_and_crop_relative_coordinates() {
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--region", "20,10,300,100"]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["region"]["x"], 20);
    assert_eq!(json["region"]["width"], 300);
    assert_eq!(json["region"]["source_width"], 600);
    assert_eq!(json["image"]["analyzed_width"], 300);
    assert_eq!(json["image"]["width"], 600);
    assert_eq!(json["grid"]["width"], 64);
}

#[test]
fn no_ocr_skips_text_but_keeps_colours() {
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--no-ocr"]);
    assert_eq!(code(&output), 0);
    let json = json_of(&output);
    assert!(json["ocr"].is_null(), "ocr must be null when disabled");
    assert!(json["colors"].is_object());
    assert!(json["grid"].is_object());
}

#[test]
fn ocr_only_selection_skips_other_subsystems() {
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--ocr"]);
    assert_eq!(code(&output), 0);
    let json = json_of(&output);
    assert!(json["colors"].is_null());
    assert!(json["grid"].is_null());
    assert!(json["layout"].is_null());
}

#[test]
fn colours_only_selection_skips_ocr() {
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--colors"]);
    assert_eq!(code(&output), 0);
    let json = json_of(&output);
    assert!(json["ocr"].is_null());
    assert!(json["layout"].is_null());
    assert!(json["grid"].is_object());
}

#[test]
fn max_colors_is_respected_and_validated() {
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--max-colors", "2"]);
    assert_eq!(code(&output), 0);
    assert!(
        json_of(&output)["colors"]["dominant"]
            .as_array()
            .unwrap()
            .len()
            <= 2
    );

    let invalid = run(&[&image, "--json", "--max-colors", "0"]);
    assert_eq!(code(&invalid), 2);
}

#[test]
fn config_file_overrides_defaults() {
    let config = write_config("[colors]\nmax_colors = 3\n");
    let image = fixture("text.png");
    let output = run(&[&image, "--json", "--config", &config]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let dominant = json_of(&output)["colors"]["dominant"]
        .as_array()
        .unwrap()
        .len();
    assert!(dominant <= 3);
    let _ = std::fs::remove_file(config);
}

#[test]
fn unknown_config_key_is_a_configuration_error() {
    let config = write_config("[general]\nnot_a_setting = 1\n");
    let image = fixture("solid.png");
    let output = run(&[&image, "--json", "--config", &config]);
    assert_eq!(code(&output), 2);
    assert!(stderr_text(&output).contains("invalid configuration"));
    let _ = std::fs::remove_file(config);
}

#[test]
fn missing_explicit_config_is_a_configuration_error() {
    let output = run(&["--config", "/nonexistent/omma.toml", &fixture("solid.png")]);
    assert_eq!(code(&output), 2);
}

#[test]
fn ocr_requested_without_tesseract_exits_three() {
    let image = fixture("text.png");
    let output = run_env(
        &[&image, "--json", "--ocr"],
        &[("PATH", "/nonexistent-path-for-omma-test")],
    );
    assert_eq!(
        code(&output),
        3,
        "explicitly requested OCR with no backend must exit 3: {}",
        stderr_text(&output)
    );
    let json = json_of(&output);
    assert!(json["ocr"].is_null());
    assert!(
        json["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "ocr_unavailable")
    );
}

#[test]
fn default_run_without_tesseract_still_succeeds() {
    let image = fixture("text.png");
    let output = run_env(
        &[&image, "--json"],
        &[("PATH", "/nonexistent-path-for-omma-test")],
    );
    assert_eq!(code(&output), 0);
    let json = json_of(&output);
    assert!(json["ocr"].is_null());
    assert!(json["colors"].is_object());
}

#[test]
fn doctor_reports_backends_and_limits() {
    let output = run(&["doctor"]);
    assert_eq!(code(&output), 0);
    let text = stdout_text(&output);
    assert!(text.contains("omma 0.1.0"));
    assert!(text.contains("tesseract") || text.contains("tesseract not found"));
    assert!(text.contains("limits:"));
    assert!(text.contains("mcp: stdio transport"));
}

#[test]
fn svg_input_is_rejected_as_unsupported() {
    let path = temp_path("drawing.svg");
    std::fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").expect("write svg");
    let path_text = path.to_string_lossy().into_owned();
    let output = run(&[&path_text, "--json"]);
    assert_eq!(code(&output), 1);
    assert!(stderr_text(&output).contains("unsupported image format"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn non_image_bytes_are_rejected() {
    let path = temp_path("notes.txt");
    std::fs::write(&path, "this is not an image").expect("write text");
    let path_text = path.to_string_lossy().into_owned();
    let output = run(&[&path_text]);
    assert_eq!(code(&output), 1);
    assert!(stderr_text(&output).contains("unsupported image format"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn json_and_format_text_contradict_each_other() {
    // `--json` is documented as an alias for `--format json`; silently emitting a
    // human report to a caller that asked for JSON is a trap.
    let output = run(&[&fixture("solid.png"), "--json", "--format", "text"]);
    assert_eq!(code(&output), 2, "{}", stderr_text(&output));
    assert!(
        stderr_text(&output).contains("contradict"),
        "{}",
        stderr_text(&output)
    );
    assert!(stdout_text(&output).is_empty());
}

#[test]
fn a_frightening_filename_is_printed_safely() {
    if cfg!(windows) {
        // Windows forbids control characters in file names, so the vector is
        // Linux/macOS only.
        eprintln!("skipping: Windows rejects control characters in file names");
        return;
    }
    let tricky = temp_path("escape-\u{1b}[2J-name.png");
    std::fs::copy(fixture("text.png"), &tricky).expect("copy fixture");
    let name = tricky.to_string_lossy().into_owned();
    let output = run(&[&name, "--no-ocr"]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    assert!(
        !stdout_text(&output).contains('\u{1b}'),
        "an escape sequence in the file name reached the terminal"
    );
    let _ = std::fs::remove_file(&tricky);
}
