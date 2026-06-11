#![allow(unused_imports)]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use super::harness::*;
use glyphrush_core::sha256_hex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[cfg(feature = "pdfium")]
#[test]
fn ocr_check_pdfium_rendered_image_command_preflights_rendered_page() {
    let dir = temp_dir("ocr-check-pdfium-rendered-image");
    let pdf_path = dir.join("scan.pdf");
    let log_path = dir.join("rendered-ocr.log");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();
    let command = write_rendered_ocr_command_script("ocr-check-pdfium-rendered-image", &log_path);

    let json = glyphrush(&[
        "--backend",
        "pdfium",
        "ocr-check",
        pdf_path.to_str().unwrap(),
        "--page-index",
        "0",
        "--ocr-command",
        command.to_str().unwrap(),
        "--ocr-command-input",
        "rendered-image",
        "--strict",
    ]);

    assert_eq!(json["adapter"], "ocr_command_rendered_image");
    assert_eq!(json["passed"], true);
    assert_eq!(json["success"], true);
    assert_eq!(json["exit_status"], 0);
    assert_eq!(json["timed_out"], false);
    assert_eq!(json["empty_output"], false);
    assert!(json["render_us"].as_u64().unwrap() > 0, "json: {json}");
    assert!(json["wall_us"].as_u64().unwrap() > 0, "json: {json}");
    assert_eq!(json["stdout_word_count"], 5);
    assert_eq!(json["error_kind"], Value::Null);

    let log = fs::read_to_string(&log_path).expect("read rendered OCR command log");
    let lines = log.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "log: {log}");
    let columns = lines[0].split('\t').collect::<Vec<_>>();
    assert_eq!(columns.len(), 4, "log line: {}", lines[0]);
    assert!(
        columns[0].ends_with(".ppm"),
        "rendered path: {}",
        columns[0]
    );
    assert_eq!(columns[1], "0");
    assert_eq!(columns[2], "P6");
    assert!(columns[3].parse::<usize>().unwrap() > 32);
    assert!(
        !PathBuf::from(columns[0]).exists(),
        "temporary rendered image should be removed after ocr-check returns"
    );
}

#[cfg(feature = "pdfium")]
#[test]
fn ocr_check_pdfium_rendered_image_http_preflights_rendered_page() {
    let dir = temp_dir("ocr-check-pdfium-rendered-image-http");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();
    let (ocr_url, request_rx, server) =
        start_rendered_ocr_http_server("Rendered HTTP check OCR text");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "pdfium",
            "ocr-check",
            pdf_path.to_str().unwrap(),
            "--page-index",
            "0",
            "--ocr-http-url",
            &ocr_url,
            "--ocr-command-input",
            "rendered-image",
            "--strict",
        ])
        .output()
        .expect("run glyphrush pdfium ocr-check with rendered-image HTTP adapter");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("HTTP OCR server should receive rendered-image preflight request");
    server
        .join()
        .expect("rendered-image HTTP OCR server should finish");
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

    assert_eq!(json["adapter"], "ocr_http_rendered_image");
    assert_eq!(json["passed"], true);
    assert_eq!(json["success"], true);
    assert_eq!(json["exit_status"], 200);
    assert_eq!(json["timed_out"], false);
    assert_eq!(json["empty_output"], false);
    assert!(json["render_us"].as_u64().unwrap() > 0, "json: {json}");
    assert!(json["wall_us"].as_u64().unwrap() > 0, "json: {json}");
    assert_eq!(json["stdout_word_count"], 5);
    assert_eq!(json["error_kind"], Value::Null);
    assert!(observed.request.starts_with("POST /ocr HTTP/1.1"));
    assert!(observed.request.contains("\"page_index\":0"));
    assert!(!observed.request.contains("\"pdf_path\""));
    let rendered_path = observed
        .rendered_image_path
        .as_deref()
        .expect("request should include rendered_image_path");
    assert!(
        rendered_path.ends_with(".ppm"),
        "rendered path: {rendered_path}"
    );
    assert!(
        observed.image_existed,
        "rendered image should exist during HTTP request"
    );
    assert_eq!(observed.header.as_deref(), Some("P6"));
    assert!(observed.bytes.unwrap_or_default() > 32);
    assert!(
        !PathBuf::from(rendered_path).exists(),
        "temporary rendered image should be removed after HTTP OCR check returns"
    );
}

#[test]
fn ocr_check_command_smoke_reports_nonempty_output() {
    let dir = temp_dir("ocr-check-command-success");
    let pdf_path = dir.join("scan.pdf");
    let log_path = dir.join("ocr.log");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_ocr_command_script("ocr-check-command-adapter", &log_path);

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "ocr-check",
        pdf_path.to_str().unwrap(),
        "--page-index",
        "0",
        "--ocr-command",
        command.to_str().unwrap(),
        "--strict",
    ]);

    assert_eq!(json["report_version"], "glyphrush-ocr-check-report-v1");
    assert_eq!(json["adapter"], "ocr_command");
    assert_eq!(json["passed"], true);
    assert_eq!(json["success"], true);
    assert_eq!(json["exit_status"], 0);
    assert_eq!(json["timed_out"], false);
    assert_eq!(json["empty_output"], false);
    assert_eq!(json["output_bytes"], 23);
    assert_eq!(json["stdout_word_count"], 5);
    assert_eq!(json["stderr_bytes"], 0);
    assert_eq!(json["error_kind"], Value::Null);
    assert_eq!(
        json["stdout_sha256"].as_str().unwrap().len(),
        64,
        "json: {json}"
    );
    let log = fs::read_to_string(&log_path).expect("read ocr check log");
    assert!(log.contains(pdf_path.to_str().unwrap()));
    assert!(log.contains(":0"));
}

#[test]
fn ocr_check_http_smoke_reports_nonempty_output() {
    let dir = temp_dir("ocr-check-http-success");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let (ocr_url, request_rx, server) = start_ocr_http_server("HTTP check OCR text");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "ocr-check",
            pdf_path.to_str().unwrap(),
            "--page-index",
            "0",
            "--ocr-http-url",
            &ocr_url,
            "--strict",
        ])
        .output()
        .expect("run glyphrush ocr-check with HTTP adapter");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let request = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("HTTP OCR server should receive preflight request");
    server.join().expect("HTTP OCR server should finish");
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

    assert_eq!(json["report_version"], "glyphrush-ocr-check-report-v1");
    assert_eq!(json["adapter"], "ocr_http");
    assert_eq!(json["passed"], true);
    assert_eq!(json["success"], true);
    assert_eq!(json["exit_status"], 200);
    assert_eq!(json["timed_out"], false);
    assert_eq!(json["empty_output"], false);
    assert_eq!(json["output_bytes"], 19);
    assert_eq!(json["stdout_word_count"], 4);
    assert_eq!(json["error_kind"], Value::Null);
    assert!(request.starts_with("POST /ocr HTTP/1.1"));
    assert!(request.contains("\"page_index\":0"));
    assert!(request.contains(pdf_path.to_str().unwrap()));
}

#[test]
fn ocr_check_http_rejects_json_without_text_field() {
    let dir = temp_dir("ocr-check-http-json-missing-text");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let (ocr_url, request_rx, server) =
        start_ocr_http_server_with_response("application/json", r#"{"result":"missing text"}"#);

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "ocr-check",
            pdf_path.to_str().unwrap(),
            "--page-index",
            "0",
            "--ocr-http-url",
            &ocr_url,
            "--strict",
        ])
        .output()
        .expect("run glyphrush ocr-check with malformed HTTP JSON adapter");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let request = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("HTTP OCR server should receive preflight request");
    server.join().expect("HTTP OCR server should finish");
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

    assert_eq!(json["passed"], false);
    assert_eq!(json["success"], true);
    assert_eq!(json["empty_output"], false);
    assert_eq!(json["error_kind"], "http_response_decode_failed");
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("missing text field")
    );
    assert!(request.contains("\"page_index\":0"));
}

#[test]
fn ocr_check_rejects_rendered_image_command_input_without_render_preflight() {
    let dir = temp_dir("ocr-check-rendered-image-reject");
    let pdf_path = dir.join("scan.pdf");
    let log_path = dir.join("ocr.log");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_rendered_ocr_command_script("ocr-check-rendered-image", &log_path);

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "ocr-check",
        pdf_path.to_str().unwrap(),
        "--page-index",
        "0",
        "--ocr-command",
        command.to_str().unwrap(),
        "--ocr-command-input",
        "rendered-image",
    ]);
    assert!(!output.status.success());

    assert_eq!(json["adapter"], "ocr_command_rendered_image");
    assert_eq!(json["success"], false);
    assert_eq!(json["passed"], false);
    assert_eq!(json["error_kind"], "render_backend_required");
    assert_eq!(
        json["error"],
        "rendered-image OCR command input requires a rendering backend"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("rendered-image OCR command input requires a rendering backend"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !log_path.exists(),
        "ocr-check should not invoke a rendered-image command without a render preflight"
    );
}

#[test]
fn ocr_check_strict_rejects_empty_command_output_after_writing_json() {
    let dir = temp_dir("ocr-check-empty-command");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_baseline_script("ocr-check-empty", "true");

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "ocr-check",
        pdf_path.to_str().unwrap(),
        "--page-index",
        "0",
        "--ocr-command",
        command.to_str().unwrap(),
        "--strict",
    ]);
    assert!(!output.status.success());

    assert_eq!(json["adapter"], "ocr_command");
    assert_eq!(json["success"], true);
    assert_eq!(json["passed"], false);
    assert_eq!(json["empty_output"], true);
    assert_eq!(json["error_kind"], "empty_output");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("ocr-check strict failed"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ocr_check_classifies_tesseract_language_data_failure_as_missing_dependency() {
    let dir = temp_dir("ocr-check-missing-tessdata");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_baseline_script(
        "ocr-check-missing-tessdata",
        "printf 'Error opening data file /tmp/tessdata/eng.traineddata\\nFailed loading language '\\''eng'\\''\\nTesseract couldn'\\''t load any languages!\\n' >&2\nexit 1",
    );

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "ocr-check",
        pdf_path.to_str().unwrap(),
        "--page-index",
        "0",
        "--ocr-command",
        command.to_str().unwrap(),
        "--strict",
    ]);
    assert!(!output.status.success());

    assert_eq!(json["adapter"], "ocr_command");
    assert_eq!(json["success"], false);
    assert_eq!(json["passed"], false);
    assert_eq!(json["exit_status"], 1);
    assert_eq!(json["error_kind"], "missing_dependency");
    assert!(
        json["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("eng.traineddata"),
        "json: {json}"
    );
}
