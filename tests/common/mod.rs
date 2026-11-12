//! Shared helpers for the integration tests.

#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

pub fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_omma")
}

pub fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    path.to_string_lossy().into_owned()
}

pub fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "omma-test-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    path.push(unique);
    path
}

/// Run omma and capture everything.
pub fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("cannot run omma")
}

/// Run omma with extra environment variables.
pub fn run_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(binary());
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("cannot run omma")
}

pub fn run_stdin(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot spawn omma");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin)
        .expect("cannot write stdin");
    child.wait_with_output().expect("cannot wait for omma")
}

pub fn json_of(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not JSON ({err}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

/// Write a config file and return its path as a string.
pub fn write_config(body: &str) -> String {
    let path = temp_path("config.toml");
    std::fs::write(&path, body).expect("cannot write config");
    path.to_string_lossy().into_owned()
}

pub fn ocr_available() -> bool {
    omma::analysis::ocr::backend().is_some()
}
