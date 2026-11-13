//! OCR: recognition, both segmentation passes, the upscale path, TSV hierarchy
//! and graceful behaviour without the backend. Skipped loudly when Tesseract is
//! absent.

mod common;

use common::*;
use omma::analysis::ocr;

#[test]
fn a_region_needs_real_text_at_real_confidence() {
    use omma::analysis::layout::is_text_region;
    // Credible: two or more alphanumeric characters above the threshold.
    assert!(is_text_region("Save", Some(80.0), 40.0));
    assert!(is_text_region("OK", Some(40.0), 40.0));
    // Noise shapes and single glyphs are not regions.
    assert!(!is_text_region("|l", Some(90.0), 40.0));
    assert!(!is_text_region("A", Some(77.0), 40.0));
    assert!(!is_text_region("»", Some(95.0), 40.0));
    // Real text below the confidence floor is not a region either.
    assert!(!is_text_region("Hello", Some(21.0), 40.0));
    assert!(!is_text_region("Hello", None, 40.0));
}

#[test]
fn normalize_text_strips_trailing_space_and_blank_lines() {
    assert_eq!(ocr::normalize_text("a  \n\n b\t\n"), "a\n b");
    assert_eq!(ocr::normalize_text(""), "");
}

#[test]
fn tsv_hierarchy_is_read_from_its_own_columns() {
    let raw = "\
level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
1\t1\t0\t0\t0\t0\t0\t0\t200\t100\t-1\t
2\t1\t1\t0\t0\t0\t10\t10\t180\t30\t-1\t
3\t1\t1\t1\t0\t0\t10\t10\t180\t12\t-1\t
4\t1\t1\t1\t1\t0\t10\t10\t180\t12\t-1\t
5\t1\t1\t1\t1\t1\t10\t10\t80\t12\t95.5\tHello
5\t1\t1\t1\t1\t2\t100\t10\t90\t12\t88.0\tworld
2\t1\t2\t0\t0\t0\t10\t40\t100\t12\t-1\t
3\t1\t2\t1\t0\t0\t10\t40\t100\t12\t-1\t
4\t1\t2\t1\t1\t0\t10\t40\t100\t12\t-1\t
5\t1\t2\t1\t1\t1\t10\t40\t100\t12\t91.0\tSecond
";
    let rows = ocr::parse_tsv(raw);
    assert_eq!(rows.len(), 10);
    let words: Vec<_> = rows.iter().filter(|row| row.level == 5).collect();
    assert_eq!(words.len(), 3);
    assert_eq!(words[0].text, "Hello");
    assert_eq!(words[0].block, 1);
    assert_eq!(words[0].line, 1);
    assert_eq!(words[2].block, 2);
    assert!(words[0].confidence > 95.0);
}

#[test]
fn text_is_read_with_word_boxes_and_structure() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    let output = run(&[&fixture("text.png"), "--json"]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    let ocr = &json["ocr"];
    assert_eq!(ocr["available"], true);
    assert_eq!(ocr["untrusted"], true);
    assert_eq!(ocr["engine"], "tesseract");
    assert!(ocr["language"].as_str().unwrap().contains("eng"));
    let text = ocr["text"].as_str().unwrap().to_lowercase();
    assert!(
        text.contains("omma"),
        "expected the fixture text, got {text:?}"
    );
    assert!(text.contains("line"));

    let words = ocr["words"].as_array().unwrap();
    assert!(!words.is_empty());
    let top_word = &words[0];
    assert!(!top_word["bbox"]["width"].as_u64().unwrap().eq(&0));
    assert!(top_word["confidence"].as_f64().unwrap() > 50.0);

    assert!(!ocr["lines"].as_array().unwrap().is_empty());
    assert!(!ocr["paragraphs"].as_array().unwrap().is_empty());
    assert!(!ocr["blocks"].as_array().unwrap().is_empty());
    let psm = ocr["psm"].as_u64().unwrap();
    assert!(
        psm == 6 || psm == 11,
        "psm should be one of the passes, got {psm}"
    );
    assert!(ocr["confidence"].as_f64().unwrap() > 50.0);
}

#[test]
fn small_crops_are_upscaled_and_boxes_stay_in_the_analyzed_frame() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    let output = run(&[&fixture("text_small.png"), "--json"]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    let ocr = &json["ocr"];
    assert_eq!(ocr["upscaled"], 4, "height < 120 must trigger the 4x path");
    let height = json["image"]["analyzed_height"].as_u64().unwrap() as f64;
    let width = json["image"]["analyzed_width"].as_u64().unwrap() as f64;
    for word in ocr["words"].as_array().unwrap() {
        let bbox = &word["bbox"];
        assert!(bbox["y"].as_u64().unwrap() as f64 <= height);
        assert!(
            (bbox["y"].as_u64().unwrap() + bbox["height"].as_u64().unwrap()) as f64 <= height * 1.1
        );
        assert!(bbox["x"].as_u64().unwrap() as f64 <= width);
        assert!(
            bbox["normalized"]["height"].as_f64().unwrap() <= 1.0,
            "boxes must be normalized against the analyzed frame"
        );
    }
}

#[test]
fn missing_language_pack_warns_instead_of_pretending() {
    if !ocr_available() {
        eprintln!("skipping: tesseract is not installed");
        return;
    }
    let output = run(&[&fixture("text.png"), "--json", "--language", "tlh"]);
    assert_eq!(code(&output), 0);
    let json = json_of(&output);
    let warnings = json["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning["code"] == "ocr_language_missing"),
        "expected a missing-language warning, got {warnings:?}"
    );
    assert!(json["ocr"].is_null());
    // The rest of the analysis still ran.
    assert!(json["colors"].is_object());
}

#[test]
fn multiple_languages_are_accepted_when_installed() {
    let Some(backend) = omma::analysis::ocr::backend() else {
        eprintln!("skipping: tesseract is not installed");
        return;
    };
    // Which packs exist differs per platform and install method, so the test
    // asks the backend instead of assuming `osd` or any second language.
    let languages: Vec<String> = backend
        .languages
        .iter()
        .filter(|name| !name.starts_with('-') && !name.is_empty())
        .cloned()
        .collect();
    if languages.len() < 2 {
        eprintln!("skipping: only one language pack installed ({languages:?})");
        return;
    }
    let requested = format!("{}+{}", languages[0], languages[1]);
    let output = run(&[&fixture("text.png"), "--json", "--language", &requested]);
    assert_eq!(code(&output), 0, "{}", stderr_text(&output));
    let json = json_of(&output);
    assert_eq!(json["ocr"]["language"], requested);
}

/// A backend that is a wrapper script starts children of its own. Killing only
/// the script leaves them holding the output pipes, so the timeout used to hang
/// instead of firing: the readers waited on a `sleep` nobody had killed.
#[cfg(unix)]
#[test]
fn a_wrapper_backend_times_out_instead_of_hanging() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = temp_path("fake-backend");
    std::fs::create_dir_all(&dir).expect("cannot create the fake backend directory");
    let script = dir.join("tesseract");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         for a in \"$@\"; do\n\
           case \"$a\" in\n\
             --version) echo 'tesseract 5.5.4'; echo '  eng'; exit 0;;\n\
             --list-langs) echo 'List of available languages'; echo 'eng'; exit 0;;\n\
           esac\n\
         done\n\
         sleep 30\n",
    )
    .expect("cannot write the fake backend");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("cannot make the fake backend executable");

    let config = write_config("[ocr]\ntimeout_seconds = 1\n");
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let started = Instant::now();
    let output = run_env(
        &[&fixture("text.png"), "--json", "--config", &config],
        &[("PATH", &path)],
    );
    let elapsed = started.elapsed();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(code(&output), 0, "stderr: {}", stderr_text(&output));
    assert!(
        elapsed < Duration::from_secs(25),
        "a hung backend must not hang the tool; took {elapsed:?}"
    );
    let report = json_of(&output);
    let codes: Vec<&str> = report["warnings"]
        .as_array()
        .map(|list| list.iter().filter_map(|w| w["code"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        codes.contains(&"ocr_timeout"),
        "expected an ocr_timeout warning, got {codes:?}"
    );
}
