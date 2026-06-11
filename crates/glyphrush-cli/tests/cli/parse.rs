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
fn parse_pdfium_emits_positioned_native_spans_when_span_geometry_requested() {
    let dir = temp_dir("parse-pdfium-positioned-spans");
    let pdf_path = dir.join("positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td (First line) Tj 0 -24 Td (Second line) Tj ET",
        ),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "pdfium",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let page = &json["pages"][0];
    let spans = page["native_spans"].as_array().unwrap();

    assert_eq!(page["signals"]["span_geometry_capped"], false);
    assert!(
        spans.len() >= 2,
        "PDFium should expose line-level text segments, spans: {spans:?}"
    );
    assert_eq!(spans[0]["text"], "First line");
    assert_eq!(spans[1]["text"], "Second line");
    assert!(spans[0]["bbox"]["x0"].as_f64().unwrap() >= 60.0);
    assert!(spans[0]["bbox"]["x0"].as_f64().unwrap() <= 90.0);
    assert!(spans[0]["bbox"]["x1"].as_f64().unwrap() > spans[0]["bbox"]["x0"].as_f64().unwrap());
    assert!(spans[0]["bbox"]["y0"].as_f64().unwrap() < spans[1]["bbox"]["y0"].as_f64().unwrap());
    assert!(spans[0]["bbox"]["y1"].as_f64().unwrap() < 120.0);
    assert_ne!(spans[0]["bbox"]["x1"], 612.0);
    assert_ne!(spans[0]["bbox"]["y1"], 792.0);
}

#[cfg(feature = "pdfium")]
#[test]
fn parse_pdfium_span_geometry_handles_large_native_text_pages() {
    let dir = temp_dir("parse-pdfium-large-positioned-spans");
    let pdf_path = dir.join("large-positioned.pdf");
    let mut stream = String::new();
    for row in 0..72 {
        let y = 740 - row * 8;
        stream.push_str(&format!(
            "BT /F1 8 Tf 72 {y} Td (Left column row {row:03} contextual language model filler) Tj ET\n"
        ));
        stream.push_str(&format!(
            "BT /F1 8 Tf 330 {y} Td (Right column row {row:03} masked objective filler) Tj ET\n"
        ));
    }
    fs::write(&pdf_path, minimal_pdf_with_stream(&stream)).unwrap();

    let json = glyphrush(&[
        "--backend",
        "pdfium",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let page = &json["pages"][0];
    let spans = page["native_spans"].as_array().unwrap();

    assert!(
        page["signals"]["native_text_bytes"].as_u64().unwrap() > 4096,
        "test must exceed the lopdf text-positioning cap: {}",
        page["signals"]
    );
    assert_eq!(page["signals"]["span_geometry_capped"], false);
    assert!(
        spans.len() >= 100,
        "PDFium should expose segment geometry for large native-text pages, got {} spans",
        spans.len()
    );
    assert!(
        spans[0]["text"]
            .as_str()
            .unwrap()
            .contains("Left column row 000")
    );
    assert!(
        spans[1]["text"]
            .as_str()
            .unwrap()
            .contains("Right column row 000")
    );
    assert_ne!(spans[0]["bbox"]["x1"], 612.0);
}

#[cfg(feature = "pdfium")]
#[test]
fn parse_pdfium_ocr_command_rendered_image_invokes_adapter_only_for_ocr_pages() {
    let dir = temp_dir("pdfium-rendered-image-ocr");
    let native_path = dir.join("native.pdf");
    let scan_path = dir.join("scan.pdf");
    let log_path = dir.join("rendered-ocr.log");
    fs::write(
        &native_path,
        minimal_pdf("Native PDFium rendered OCR bypass"),
    )
    .unwrap();
    fs::write(&scan_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();
    let command = write_rendered_ocr_command_script("pdfium-rendered-ocr-command", &log_path);

    let native = glyphrush(&[
        "--backend",
        "pdfium",
        "parse",
        native_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-command",
        command.to_str().unwrap(),
        "--ocr-command-input",
        "rendered-image",
    ]);
    let scan = glyphrush(&[
        "--backend",
        "pdfium",
        "parse",
        scan_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-command",
        command.to_str().unwrap(),
        "--ocr-command-input",
        "rendered-image",
    ]);

    assert_eq!(native["global_diagnostics"]["ocr_required_pages"], 0);
    assert_eq!(native["global_diagnostics"]["ocr_applied_pages"], 0);
    assert_eq!(scan["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(scan["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(
        scan["pages"][0]["ocr_spans"][0]["text"],
        "Rendered OCR text page 0"
    );
    assert!(
        scan["pages"][0]["timings"]["render_us"].as_u64().unwrap() > 0,
        "scan page timings: {}",
        scan["pages"][0]["timings"]
    );
    assert!(scan["pages"][0]["timings"]["ocr_us"].as_u64().unwrap() > 0);

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
        "temporary rendered image should be removed after OCR command returns"
    );
}

#[cfg(feature = "pdfium")]
#[test]
fn parse_pdfium_ocr_http_rendered_image_invokes_adapter_only_for_ocr_pages() {
    let dir = temp_dir("pdfium-rendered-image-http-ocr");
    let native_path = dir.join("native.pdf");
    let scan_path = dir.join("scan.pdf");
    fs::write(
        &native_path,
        minimal_pdf("Native PDFium rendered HTTP OCR bypass"),
    )
    .unwrap();
    fs::write(&scan_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();

    let native = glyphrush(&[
        "--backend",
        "pdfium",
        "parse",
        native_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-http-url",
        "http://127.0.0.1:1/ocr",
        "--ocr-command-input",
        "rendered-image",
    ]);
    let (ocr_url, request_rx, server) =
        start_rendered_ocr_http_server("Rendered HTTP OCR text page 0");
    let scan = glyphrush(&[
        "--backend",
        "pdfium",
        "parse",
        scan_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-http-url",
        &ocr_url,
        "--ocr-command-input",
        "rendered-image",
    ]);

    let observed = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("HTTP OCR server should receive one rendered-image request");
    server
        .join()
        .expect("rendered-image HTTP OCR server should finish");

    assert_eq!(native["global_diagnostics"]["ocr_required_pages"], 0);
    assert_eq!(native["global_diagnostics"]["ocr_applied_pages"], 0);
    assert_eq!(scan["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(scan["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(
        scan["pages"][0]["ocr_spans"][0]["text"],
        "Rendered HTTP OCR text page 0"
    );
    assert!(
        scan["pages"][0]["timings"]["render_us"].as_u64().unwrap() > 0,
        "scan page timings: {}",
        scan["pages"][0]["timings"]
    );
    assert!(scan["pages"][0]["timings"]["ocr_us"].as_u64().unwrap() > 0);

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
        "temporary rendered image should be removed after HTTP OCR returns"
    );
}

#[test]
fn parse_lopdf_rejects_rendered_image_ocr_command_without_render_backend() {
    let dir = temp_dir("lopdf-rendered-image-ocr-reject");
    let pdf_path = dir.join("scan.pdf");
    let log_path = dir.join("rendered-ocr.log");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();
    let command = write_rendered_ocr_command_script("lopdf-rendered-ocr-command", &log_path);

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--ocr-command",
            command.to_str().unwrap(),
            "--ocr-command-input",
            "rendered-image",
        ])
        .output()
        .expect("run glyphrush lopdf parse with rendered-image OCR command");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("rendered-image OCR command input requires a rendering backend"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !log_path.exists(),
        "OCR command should not be invoked when the backend cannot render"
    );
}

#[cfg(feature = "pdfium")]
#[test]
fn parse_json_emits_structured_artifact_with_native_text() {
    let pdf_path = write_test_pdf("parse", "Hello Glyphrush");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(json["metadata"]["parser_name"], "glyphrush");
    assert_eq!(
        json["metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["metadata"]["backend"], "lopdf");
    assert_eq!(json["metadata"]["backend_version"], "lopdf-adapter-v0");
    assert_eq!(
        json["metadata"]["source_size_bytes"],
        fs::metadata(&pdf_path).unwrap().len()
    );
    assert_eq!(json["pages"].as_array().unwrap().len(), 1);
    assert_eq!(json["global_diagnostics"]["fallback_pages"], 0);
    assert_eq!(json["pages"][0]["signals"]["page_index"], 0);
    assert!(
        json["pages"][0]["signals"]["native_text_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(json["pages"][0]["signals"]["image_area_ratio"], 0.0);
    assert!(
        json["pages"][0]["native_spans"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Hello Glyphrush")
    );
    assert_eq!(json["pages"][0]["quality"]["flags"], Value::Array(vec![]));
    assert!(
        json["pages"][0]["timings"]["native_extract_us"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn parse_json_jobs_preserve_page_order_and_report_worker_count() {
    let dir = temp_dir("parse-page-jobs");
    let pdf_path = dir.join("multi.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (First parse page) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Second parse page) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Third parse page) Tj ET",
        ]),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--jobs",
        "2",
    ]);

    assert_eq!(json["global_diagnostics"]["worker_count"], 2);
    assert_eq!(json["pages"].as_array().unwrap().len(), 3);
    assert_eq!(json["pages"][0]["page_index"], 0);
    assert_eq!(json["pages"][1]["page_index"], 1);
    assert_eq!(json["pages"][2]["page_index"], 2);
    assert!(
        json["pages"][0]["native_spans"][0]["text"]
            .as_str()
            .unwrap()
            .contains("First parse page")
    );
    assert!(
        json["pages"][1]["native_spans"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Second parse page")
    );
    assert!(
        json["pages"][2]["native_spans"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Third parse page")
    );
}

#[test]
fn parse_json_reports_table_signal_timing_for_ruled_table() {
    let dir = temp_dir("parse-ruled-table-timing");
    let pdf_path = dir.join("ruled-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(json["pages"][0]["route"]["run_table_recovery"], true);
    assert_eq!(
        json["pages"][0]["quality"]["flags"],
        Value::Array(vec![Value::String("table_uncertain".to_string())])
    );
    assert!(
        json["pages"][0]["timings"]["table_us"].as_u64().unwrap() > 0,
        "timings: {}",
        json["pages"][0]["timings"]
    );
}

#[test]
fn parse_json_exposes_image_xobject_metadata_without_rendering_pixels() {
    let dir = temp_dir("parse-image-artifacts");
    let pdf_path = dir.join("image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_full_page_image_and_text("Image-backed native text"),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let images = json["pages"][0]["image_artifacts"]
        .as_array()
        .expect("image_artifacts is an array");

    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["image_id"], "p000000:im000000");
    assert_eq!(images[0]["source_name"], "Im1");
    assert_eq!(images[0]["bbox"]["x0"], 0.0);
    assert_eq!(images[0]["bbox"]["y0"], 0.0);
    assert_eq!(images[0]["bbox"]["x1"], 612.0);
    assert_eq!(images[0]["bbox"]["y1"], 792.0);
    assert!(
        images[0]["area_ratio"].as_f64().unwrap() >= 0.95,
        "image artifact: {}",
        images[0]
    );
}

#[test]
fn parse_json_uses_nested_form_image_geometry_for_area_ratio() {
    let dir = temp_dir("parse-small-form-image-artifacts");
    let pdf_path = dir.join("small-form-image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_small_form_wrapped_image_and_text("Small form image text"),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let page = &json["pages"][0];
    let images = page["image_artifacts"]
        .as_array()
        .expect("image_artifacts is an array");

    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["source_name"], "Fm1");
    assert!(
        images[0]["area_ratio"].as_f64().unwrap() <= 0.02,
        "image artifact: {}",
        images[0]
    );
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() <= 0.02,
        "signals: {}",
        page["signals"]
    );
    assert_eq!(page["route"]["route"], "native_fast_path");
}

#[test]
fn parse_json_uses_unioned_image_coverage_for_overlapping_images() {
    let dir = temp_dir("parse-overlapping-image-artifacts");
    let pdf_path = dir.join("overlapping-images.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_overlapping_half_page_images_and_text("Repeated logo"),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let page = &json["pages"][0];
    let images = page["image_artifacts"]
        .as_array()
        .expect("image_artifacts is an array");

    assert_eq!(images.len(), 2);
    assert!(
        images[0]["area_ratio"].as_f64().unwrap() >= 0.49,
        "image artifact: {}",
        images[0]
    );
    assert!(
        images[1]["area_ratio"].as_f64().unwrap() >= 0.49,
        "image artifact: {}",
        images[1]
    );
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() <= 0.55,
        "signals: {}",
        page["signals"]
    );
    assert_eq!(page["route"]["route"], "native_fast_path");
}

#[test]
fn parse_json_uses_crop_box_origin_for_image_coverage() {
    let dir = temp_dir("parse-cropbox-image-artifacts");
    let pdf_path = dir.join("cropbox-image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_nonzero_crop_box_full_page_image_and_sparse_text("Sparse text"),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let page = &json["pages"][0];

    assert_eq!(page["signals"]["dimensions"]["width"], 306.0);
    assert_eq!(page["signals"]["dimensions"]["height"], 396.0);
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        page["signals"]
    );
    assert_eq!(page["route"]["route"], "ocr_fallback");
    assert_eq!(
        page["route"]["reasons"],
        serde_json::json!(["high_image_coverage_with_sparse_native_text"])
    );
}

#[test]
fn parse_json_preserves_off_crop_image_without_counting_coverage() {
    let dir = temp_dir("parse-off-crop-image-artifacts");
    let pdf_path = dir.join("off-crop-image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_nonzero_crop_box_off_crop_image_and_text("Visible native text"),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let page = &json["pages"][0];
    let images = page["image_artifacts"]
        .as_array()
        .expect("image_artifacts is an array");

    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["source_name"], "Im1");
    assert_eq!(images[0]["area_ratio"], 0.0);
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() <= 0.01,
        "signals: {}",
        page["signals"]
    );
}

#[test]
fn parse_json_detects_full_page_inline_image_for_ocr_routing() {
    let dir = temp_dir("parse-inline-image-artifacts");
    let pdf_path = dir.join("inline-image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("q 612 0 0 792 0 0 cm BI /W 1 /H 1 /CS /Gray /BPC 8 ID\n0\nEI Q"),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let page = &json["pages"][0];
    let images = page["image_artifacts"]
        .as_array()
        .expect("image_artifacts is an array");

    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["source_name"], "inline");
    assert!(
        images[0]["area_ratio"].as_f64().unwrap() >= 0.95,
        "image artifact: {}",
        images[0]
    );
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        page["signals"]
    );
    assert_eq!(page["route"]["route"], "ocr_fallback");
    assert_eq!(
        page["route"]["reasons"],
        serde_json::json!(["high_image_coverage_without_native_text"])
    );
}

#[test]
fn parse_json_preserves_skipped_inline_image_as_visible_artifact() {
    let dir = temp_dir("parse-skipped-inline-image-artifacts");
    let pdf_path = dir.join("skipped-inline-image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "q 612 0 0 792 0 0 cm BI /W 1 /H 1 /CS /ICCBased /BPC 8 ID\n0\nEI Q BT /F1 24 Tf 72 720 Td (Overlay text) Tj ET",
        ),
    )
    .unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let page = &json["pages"][0];
    let images = page["image_artifacts"]
        .as_array()
        .expect("image_artifacts is an array");

    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["source_name"], "inline");
    assert!(
        images[0]["area_ratio"].as_f64().unwrap() >= 0.95,
        "image artifact: {}",
        images[0]
    );
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        page["signals"]
    );
    assert_eq!(page["route"]["route"], "ocr_fallback");
    assert_eq!(
        page["route"]["reasons"],
        serde_json::json!(["high_image_coverage_with_sparse_native_text"])
    );
}

#[test]
fn parse_json_uses_page_wide_native_span_by_default_for_hot_path() {
    let dir = temp_dir("parse-default-spans");
    let pdf_path = dir.join("positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td (First line) Tj 0 -24 Td (Second line) Tj ET",
        ),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();

    assert_eq!(spans.len(), 1);
    assert!(spans[0]["text"].as_str().unwrap().contains("First line"));
    assert!(spans[0]["text"].as_str().unwrap().contains("Second line"));
    assert_eq!(spans[0]["bbox"]["x0"].as_f64().unwrap(), 0.0);
    assert_eq!(spans[0]["bbox"]["x1"].as_f64().unwrap(), 612.0);
}

#[test]
fn parse_json_emits_positioned_native_spans_when_span_geometry_requested() {
    let dir = temp_dir("parse-positioned-spans");
    let pdf_path = dir.join("positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td (First line) Tj 0 -24 Td (Second line) Tj ET",
        ),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["text"], "First line");
    assert_eq!(spans[1]["text"], "Second line");
    assert_eq!(spans[0]["bbox"]["x0"].as_f64().unwrap(), 72.0);
    assert!(spans[0]["bbox"]["x1"].as_f64().unwrap() > 72.0);
    assert!(spans[0]["bbox"]["y1"].as_f64().unwrap() > spans[0]["bbox"]["y0"].as_f64().unwrap());
    assert!(spans[1]["bbox"]["y0"].as_f64().unwrap() > spans[0]["bbox"]["y0"].as_f64().unwrap());
}

#[test]
fn parse_json_applies_content_matrix_to_positioned_spans() {
    let dir = temp_dir("parse-transformed-positioned-spans");
    let pdf_path = dir.join("transformed.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("q 1 0 0 1 100 50 cm BT /F1 12 Tf 0 700 Td (Shifted text) Tj ET Q"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["text"], "Shifted text");
    assert_eq!(spans[0]["bbox"]["x0"].as_f64().unwrap(), 100.0);
    assert_eq!(spans[0]["bbox"]["y0"].as_f64().unwrap(), 42.0);
    assert!(spans[0]["bbox"]["x1"].as_f64().unwrap() > 100.0);
    assert!(spans[0]["bbox"]["y1"].as_f64().unwrap() > 42.0);
}

#[test]
fn parse_json_uses_crop_box_origin_for_positioned_span_bbox() {
    let dir = temp_dir("parse-cropbox-positioned-spans");
    let pdf_path = dir.join("cropbox-positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_nonzero_crop_box_stream("BT /F1 12 Tf 120 450 Td (Crop text) Tj ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let page = &json["pages"][0];
    let spans = page["native_spans"].as_array().unwrap();
    let bbox = &spans[0]["bbox"];

    assert_eq!(page["signals"]["dimensions"]["width"], 306.0);
    assert_eq!(page["signals"]["dimensions"]["height"], 396.0);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["text"], "Crop text");
    assert_eq!(bbox["x0"].as_f64().unwrap(), 20.0);
    assert_eq!(bbox["y0"].as_f64().unwrap(), 46.0);
    assert!(bbox["x1"].as_f64().unwrap() > 20.0, "bbox: {}", bbox);
    assert_eq!(bbox["y1"].as_f64().unwrap(), 58.0);
}

#[test]
fn parse_json_applies_text_matrix_scale_to_positioned_spans() {
    let dir = temp_dir("parse-scaled-positioned-spans");
    let pdf_path = dir.join("scaled.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 2 0 0 2 100 350 Tm (Scaled text) Tj ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();
    let bbox = &spans[0]["bbox"];

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["text"], "Scaled text");
    assert_eq!(bbox["x0"].as_f64().unwrap(), 100.0);
    assert_eq!(bbox["y0"].as_f64().unwrap(), 442.0);
    assert!(bbox["x1"].as_f64().unwrap() > 240.0, "bbox: {}", bbox);
    assert!(bbox["y1"].as_f64().unwrap() > 464.0, "bbox: {}", bbox);
}

#[test]
fn parse_json_applies_tj_spacing_to_positioned_spans() {
    let dir = temp_dir("parse-tj-positioned-spans");
    let pdf_path = dir.join("tj.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 72 720 Td [(A) -1000 (B)] TJ ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["text"], "A");
    assert_eq!(spans[1]["text"], "B");
    assert_eq!(spans[0]["bbox"]["x0"].as_f64().unwrap(), 72.0);
    assert!(
        spans[1]["bbox"]["x0"].as_f64().unwrap() >= 90.0,
        "spans: {}",
        serde_json::json!(spans)
    );
}

#[test]
fn parse_json_applies_character_spacing_to_positioned_span_width() {
    let dir = temp_dir("parse-character-spacing-spans");
    let pdf_path = dir.join("char-spacing.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 10 Tc 72 720 Td (AB) Tj ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();
    let bbox = &spans[0]["bbox"];

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["text"], "AB");
    assert_eq!(bbox["x0"].as_f64().unwrap(), 72.0);
    assert!(bbox["x1"].as_f64().unwrap() >= 95.0, "bbox: {}", bbox);
}

#[test]
fn parse_json_preserves_text_state_set_before_text_object() {
    let dir = temp_dir("parse-pre-bt-text-state-spans");
    let pdf_path = dir.join("pre-bt-text-state.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("/F1 24 Tf BT 72 720 Td (Large) Tj ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();
    let bbox = &spans[0]["bbox"];

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["text"], "Large");
    assert_eq!(bbox["y0"].as_f64().unwrap(), 72.0);
    assert_eq!(bbox["y1"].as_f64().unwrap(), 96.0);
}

#[test]
fn parse_json_applies_text_rise_to_positioned_span_bbox() {
    let dir = temp_dir("parse-text-rise-spans");
    let pdf_path = dir.join("text-rise.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 72 720 Td (Base) Tj 12 Ts (Up) Tj ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();
    let raised_bbox = &spans[1]["bbox"];

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["text"], "Base");
    assert_eq!(spans[1]["text"], "Up");
    assert_eq!(spans[0]["bbox"]["y0"].as_f64().unwrap(), 72.0);
    assert_eq!(raised_bbox["y0"].as_f64().unwrap(), 60.0);
    assert_eq!(raised_bbox["y1"].as_f64().unwrap(), 72.0);
}

#[test]
fn parse_json_applies_text_leading_to_positioned_line_spans() {
    let dir = temp_dir("parse-leading-positioned-spans");
    let pdf_path = dir.join("leading.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 24 TL 72 720 Td (First) Tj T* (Second) Tj ET"),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["text"], "First");
    assert_eq!(spans[1]["text"], "Second");
    assert_eq!(spans[0]["bbox"]["y0"].as_f64().unwrap(), 72.0);
    assert_eq!(spans[1]["bbox"]["y0"].as_f64().unwrap(), 96.0);
}

#[test]
fn parse_json_applies_single_quote_text_showing_to_next_line() {
    let dir = temp_dir("parse-single-quote-line-spans");
    let pdf_path = dir.join("single-quote-line.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(r#"BT /F1 12 Tf 24 TL 72 720 Td (First) Tj (Second) ' ET"#),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["text"], "First");
    assert_eq!(spans[1]["text"], "Second");
    assert_eq!(spans[0]["bbox"]["y0"].as_f64().unwrap(), 72.0);
    assert_eq!(spans[1]["bbox"]["x0"].as_f64().unwrap(), 72.0);
    assert_eq!(spans[1]["bbox"]["y0"].as_f64().unwrap(), 96.0);
}

#[test]
fn parse_json_applies_double_quote_spacing_to_positioned_span_width() {
    let dir = temp_dir("parse-double-quote-spacing-spans");
    let pdf_path = dir.join("double-quote-spacing.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(r#"BT /F1 12 Tf 24 TL 72 720 Td (First) Tj 20 10 (A B) " ET"#),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let spans = json["pages"][0]["native_spans"].as_array().unwrap();
    let bbox = &spans[1]["bbox"];

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["text"], "First");
    assert_eq!(spans[1]["text"], "A B");
    assert_eq!(bbox["x0"].as_f64().unwrap(), 72.0);
    assert_eq!(bbox["y0"].as_f64().unwrap(), 96.0);
    assert!(bbox["x1"].as_f64().unwrap() >= 125.0, "bbox: {}", bbox);
}

#[test]
fn parse_json_flags_overlapping_positioned_spans_as_layout_uncertain() {
    let dir = temp_dir("parse-overlapping-spans");
    let pdf_path = dir.join("overlap.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td (Shadow text) Tj ET BT /F1 12 Tf 72 720 Td (Shadow text) Tj ET",
        ),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let page = &json["pages"][0];

    assert_eq!(page["route"]["route"], "needs_fallback");
    assert_eq!(
        page["route"]["flags"],
        serde_json::json!(["layout_uncertain"])
    );
    assert_eq!(
        page["route"]["reasons"],
        serde_json::json!(["bbox_overlap"])
    );
    assert!(
        page["signals"]["bbox_overlap_ratio"].as_f64().unwrap() >= 0.45,
        "signals: {}",
        page["signals"]
    );
}

#[test]
fn parse_json_skips_positioned_spans_for_large_content_streams() {
    let dir = temp_dir("parse-large-content-spans");
    let pdf_path = dir.join("large.pdf");
    let mut stream = String::from("BT /F1 12 Tf 72 720 Td (Large content text) Tj ET\n");
    stream.push_str(&"0 0 m 1 1 l S\n".repeat(6_000));
    fs::write(&pdf_path, minimal_pdf_with_stream(&stream)).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let span = &json["pages"][0]["native_spans"][0];

    assert_eq!(
        json["pages"][0]["native_spans"].as_array().unwrap().len(),
        1
    );
    assert_eq!(span["bbox"]["x0"].as_f64().unwrap(), 0.0);
    assert_eq!(span["bbox"]["x1"].as_f64().unwrap(), 612.0);
    assert!(
        span["text"]
            .as_str()
            .unwrap()
            .contains("Large content text")
    );
    assert_eq!(json["pages"][0]["route"]["route"], "unsupported");
    assert_eq!(
        json["pages"][0]["route"]["flags"],
        Value::Array(vec![Value::String("unsupported_feature".to_string())])
    );
    assert_eq!(
        json["pages"][0]["route"]["reasons"],
        Value::Array(vec![Value::String("span_geometry_capped".to_string())])
    );
    assert_eq!(
        json["global_diagnostics"]["warnings"],
        serde_json::json!(["p000000: unsupported_feature: span_geometry_capped"])
    );
}

#[test]
fn parse_json_caps_positioned_spans_for_rotated_pages() {
    let dir = temp_dir("parse-rotated-positioned-spans");
    let pdf_path = dir.join("rotated.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_inherited_rotation("Rotated geometry", 90),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let page = &json["pages"][0];
    let span = &page["native_spans"][0];

    assert_eq!(page["native_spans"].as_array().unwrap().len(), 1);
    assert_eq!(span["bbox"]["x0"].as_f64().unwrap(), 0.0);
    assert_eq!(span["bbox"]["x1"].as_f64().unwrap(), 612.0);
    assert!(span["text"].as_str().unwrap().contains("Rotated geometry"));
    assert_eq!(page["signals"]["rotation_degrees"], 90);
    assert_eq!(page["signals"]["span_geometry_capped"], true);
    assert_eq!(page["route"]["route"], "unsupported");
    assert_eq!(
        page["route"]["flags"],
        serde_json::json!(["layout_uncertain", "unsupported_feature"])
    );
    assert_eq!(
        page["route"]["reasons"],
        serde_json::json!(["rotated_page", "span_geometry_capped"])
    );
    assert_eq!(
        json["global_diagnostics"]["warnings"],
        serde_json::json!(["p000000: unsupported_feature: span_geometry_capped"])
    );
}

#[test]
fn parse_json_flags_widget_annotations_as_unsupported_feature() {
    let dir = temp_dir("parse-widget-annotation");
    let pdf_path = dir.join("form.pdf");
    fs::write(&pdf_path, minimal_pdf_with_widget_annotation("Form text")).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(json["pages"][0]["route"]["route"], "unsupported");
    assert_eq!(
        json["pages"][0]["quality"]["flags"],
        Value::Array(vec![Value::String("unsupported_feature".to_string())])
    );
    assert_eq!(
        json["pages"][0]["route"]["reasons"],
        Value::Array(vec![Value::String("annotation_or_form".to_string())])
    );
    assert_eq!(
        json["global_diagnostics"]["warnings"],
        serde_json::json!(["p000000: unsupported_feature: annotation_or_form"])
    );
}

#[test]
fn parse_json_flags_non_widget_annotations_as_unsupported_feature() {
    let dir = temp_dir("parse-text-annotation");
    let pdf_path = dir.join("annotated.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_text_annotation("Annotated text"),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(json["pages"][0]["signals"]["annotation_count"], 1);
    assert_eq!(json["pages"][0]["route"]["route"], "unsupported");
    assert_eq!(
        json["pages"][0]["quality"]["flags"],
        Value::Array(vec![Value::String("unsupported_feature".to_string())])
    );
    assert_eq!(
        json["pages"][0]["route"]["reasons"],
        Value::Array(vec![Value::String("annotation_or_form".to_string())])
    );
    assert_eq!(
        json["global_diagnostics"]["warnings"],
        serde_json::json!(["p000000: unsupported_feature: annotation_or_form"])
    );
}

#[test]
fn parse_json_flags_catalog_acroform_fields_as_unsupported_feature() {
    let dir = temp_dir("parse-catalog-acroform");
    let pdf_path = dir.join("catalog-form.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_catalog_acroform("Catalog form text"),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(json["pages"][0]["route"]["route"], "unsupported");
    assert_eq!(
        json["pages"][0]["quality"]["flags"],
        Value::Array(vec![Value::String("unsupported_feature".to_string())])
    );
    assert_eq!(
        json["pages"][0]["route"]["reasons"],
        Value::Array(vec![Value::String("annotation_or_form".to_string())])
    );
    assert_eq!(
        json["global_diagnostics"]["warnings"],
        serde_json::json!(["p000000: unsupported_feature: annotation_or_form"])
    );
}

#[test]
fn parse_text_emits_warnings_to_stderr_for_capped_span_geometry() {
    let dir = temp_dir("parse-text-span-geometry-warning");
    let pdf_path = dir.join("large.pdf");
    let mut stream = String::from("BT /F1 12 Tf 72 720 Td (Large content text) Tj ET\n");
    stream.push_str(&"0 0 m 1 1 l S\n".repeat(6_000));
    fs::write(&pdf_path, minimal_pdf_with_stream(&stream)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "text",
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush parse text with capped span geometry");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Large content text")
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("warning: p000000: unsupported_feature: span_geometry_capped")
    );
}

#[test]
fn parse_markdown_emits_text_from_layout_blocks() {
    let pdf_path = write_test_pdf("parse-markdown", "Hello Markdown");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "markdown",
        ])
        .output()
        .expect("run glyphrush parse markdown");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let markdown = String::from_utf8(output.stdout).expect("markdown output is utf8");
    assert!(markdown.contains("Hello Markdown"));
    assert!(!markdown.trim_start().starts_with('{'));
}

#[test]
fn parse_markdown_renders_pipe_table_blocks_as_markdown_tables() {
    let dir = temp_dir("parse-markdown-table");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 18 TL 72 720 Td (| Part | Value |) Tj T* (| A | 1 |) Tj T* (| B | 2 |) Tj ET",
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "markdown",
        ])
        .output()
        .expect("run glyphrush parse markdown table");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let markdown = String::from_utf8(output.stdout).expect("markdown output is utf8");

    assert!(markdown.contains("| Part | Value |"));
    assert!(markdown.contains("| --- | --- |"));
    assert!(markdown.contains("| A | 1 |"));
    assert!(markdown.contains("| B | 2 |"));
}

#[test]
fn parse_markdown_renders_whitespace_table_blocks_as_markdown_tables() {
    let dir = temp_dir("parse-markdown-whitespace-table");
    let pdf_path = dir.join("ruled-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "markdown",
        ])
        .output()
        .expect("run glyphrush parse markdown whitespace table");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let markdown = String::from_utf8(output.stdout).expect("markdown output is utf8");

    assert!(markdown.contains("| Part | Value |"));
    assert!(markdown.contains("| --- | --- |"));
    assert!(markdown.contains("| A | 1 |"));
    assert!(markdown.contains("| B | 2 |"));
}

#[test]
fn parse_json_preserves_empty_cells_for_aligned_whitespace_table_blocks() {
    let dir = temp_dir("parse-json-aligned-whitespace-table");
    let pdf_path = dir.join("aligned-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_aligned_whitespace_ruled_table()).unwrap();

    let json = glyphrush(&["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
    let blocks = json["pages"][0]["layout_blocks"].as_array().unwrap();
    let table_block = blocks
        .iter()
        .find(|block| block["kind"] == "table")
        .expect("aligned whitespace table block");

    assert_eq!(table_block["table"]["rows"][0]["cells"][0]["text"], "Part");
    assert_eq!(table_block["table"]["rows"][0]["cells"][1]["text"], "Value");
    assert_eq!(table_block["table"]["rows"][0]["cells"][2]["text"], "Note");
    assert_eq!(table_block["table"]["rows"][1]["cells"][0]["text"], "A");
    assert_eq!(table_block["table"]["rows"][1]["cells"][1]["text"], "");
    assert_eq!(
        table_block["table"]["rows"][1]["cells"][2]["text"],
        "missing value"
    );
    assert_eq!(table_block["table"]["rows"][2]["cells"][0]["text"], "B");
    assert_eq!(table_block["table"]["rows"][2]["cells"][1]["text"], "2");
    assert_eq!(table_block["table"]["rows"][2]["cells"][2]["text"], "");
}

#[test]
fn parse_json_emits_structured_cells_for_positioned_table_blocks() {
    let dir = temp_dir("parse-json-positioned-table-cells");
    let pdf_path = dir.join("positioned-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_positioned_ruled_table()).unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let blocks = json["pages"][0]["layout_blocks"].as_array().unwrap();
    let table_block = blocks
        .iter()
        .find(|block| block["kind"] == "table")
        .expect("positioned table block");

    assert_eq!(table_block["text"], "Part Value\nA 1\nB 2");
    assert_eq!(table_block["table"]["rows"][0]["cells"][0]["text"], "Part");
    assert_eq!(table_block["table"]["rows"][0]["cells"][1]["text"], "Value");
    assert_eq!(table_block["table"]["rows"][1]["cells"][0]["text"], "A");
    assert_eq!(table_block["table"]["rows"][2]["cells"][1]["text"], "2");
    assert_eq!(
        table_block["table"]["rows"][0]["cells"][0]["bbox"]["x0"],
        84.0
    );
    assert_eq!(
        table_block["table"]["rows"][0]["cells"][0]["bbox"]["y0"],
        218.0
    );
    assert_eq!(
        table_block["table"]["rows"][0]["cells"][1]["bbox"]["x0"],
        228.0
    );
}

#[test]
fn parse_json_preserves_empty_cells_for_positioned_table_blocks() {
    let dir = temp_dir("parse-json-positioned-empty-table-cells");
    let pdf_path = dir.join("positioned-empty-table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_positioned_ruled_table_empty_cells(),
    )
    .unwrap();

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
    ]);
    let blocks = json["pages"][0]["layout_blocks"].as_array().unwrap();
    let table_block = blocks
        .iter()
        .find(|block| block["kind"] == "table")
        .expect("positioned table block");

    assert_eq!(table_block["table"]["rows"][0]["cells"][0]["text"], "Part");
    assert_eq!(table_block["table"]["rows"][0]["cells"][1]["text"], "Value");
    assert_eq!(table_block["table"]["rows"][0]["cells"][2]["text"], "Note");
    assert_eq!(table_block["table"]["rows"][1]["cells"][0]["text"], "A");
    assert_eq!(table_block["table"]["rows"][1]["cells"][1]["text"], "");
    assert_eq!(
        table_block["table"]["rows"][1]["cells"][2]["text"],
        "missing value"
    );
    assert_eq!(table_block["table"]["rows"][2]["cells"][0]["text"], "B");
    assert_eq!(table_block["table"]["rows"][2]["cells"][1]["text"], "2");
    assert_eq!(table_block["table"]["rows"][2]["cells"][2]["text"], "");
    assert_eq!(
        table_block["table"]["rows"][1]["cells"][1].get("bbox"),
        None
    );
    assert_eq!(
        table_block["table"]["rows"][2]["cells"][2].get("bbox"),
        None
    );
}

#[test]
fn parse_text_emits_warnings_to_stderr_for_incomplete_ocr_pages() {
    let dir = temp_dir("parse-text-warning");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "text",
        ])
        .output()
        .expect("run glyphrush parse text without ocr sidecar");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("warning: p000000: requires_ocr_without_ocr_output")
    );
}

#[test]
fn parse_markdown_emits_warnings_to_stderr_for_incomplete_ocr_pages() {
    let dir = temp_dir("parse-markdown-warning");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "markdown",
        ])
        .output()
        .expect("run glyphrush parse markdown without ocr sidecar");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("warning: p000000: requires_ocr_without_ocr_output")
    );
}

#[test]
fn parse_without_ocr_sidecar_warns_when_ocr_is_required() {
    let dir = temp_dir("parse-ocr-missing-warning");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(json["pages"][0]["route"]["route"], "ocr_fallback");
    assert_eq!(json["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(json["global_diagnostics"]["ocr_applied_pages"], 0);
    assert_eq!(
        json["global_diagnostics"]["warnings"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
}

#[test]
fn parse_with_ocr_sidecar_populates_ocr_span_for_required_page() {
    let dir = temp_dir("parse-ocr-sidecar");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(sidecar_dir.join("scan.p000000.txt"), "Sidecar OCR text").unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
    ]);

    assert_eq!(json["pages"][0]["route"]["route"], "ocr_fallback");
    assert_eq!(json["pages"][0]["ocr_spans"][0]["text"], "Sidecar OCR text");
    assert_eq!(json["pages"][0]["ocr_spans"][0]["provenance"], "ocr");
    assert_eq!(json["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(json["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(json["global_diagnostics"]["warnings"], Value::Array(vec![]));
    assert!(json["pages"][0]["timings"]["ocr_us"].as_u64().unwrap() > 0);
}

#[test]
fn parse_with_ocr_sidecar_escalates_image_backed_broken_native_text() {
    let dir = temp_dir("parse-ocr-broken-native");
    let pdf_path = dir.join("broken.pdf");
    let broken_native = format!(
        "{}{}",
        "\u{fffd}".repeat(80),
        " native fallback text".repeat(8)
    );
    fs::write(
        &pdf_path,
        minimal_pdf_with_full_page_image_and_text(&broken_native),
    )
    .unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(
        sidecar_dir.join("broken.p000000.txt"),
        "OCR RECOVERED\n\nRecovered OCR paragraph.",
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
    ]);
    let page = &json["pages"][0];

    assert_eq!(page["route"]["route"], "ocr_fallback");
    assert_eq!(page["route"]["run_ocr"], true);
    assert!(
        page["signals"]["broken_encoding_ratio"].as_f64().unwrap() >= 0.20,
        "signals: {}",
        page["signals"]
    );
    assert!(
        page["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        page["signals"]
    );
    assert_eq!(
        page["route"]["reasons"],
        serde_json::json!([
            "image_text_overlay",
            "broken_encoding",
            "broken_encoding_with_image_coverage"
        ])
    );
    assert_eq!(
        page["quality"]["flags"],
        serde_json::json!([
            "layout_uncertain",
            "broken_encoding",
            "low_confidence_text",
            "requires_ocr"
        ])
    );
    assert_eq!(
        page["ocr_spans"][0]["text"],
        "OCR RECOVERED\n\nRecovered OCR paragraph."
    );
    assert_eq!(page["layout_blocks"][0]["text"], "OCR RECOVERED");
    assert_eq!(page["layout_blocks"][1]["text"], "Recovered OCR paragraph.");
    assert_eq!(json["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(json["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(json["global_diagnostics"]["warnings"], Value::Array(vec![]));
}

#[test]
fn parse_with_ocr_command_invokes_adapter_only_for_ocr_pages() {
    let dir = temp_dir("parse-ocr-command");
    let native_path = dir.join("native.pdf");
    let scan_path = dir.join("scan.pdf");
    let log_path = dir.join("ocr.log");
    fs::write(&native_path, minimal_pdf("Native OCR command bypass")).unwrap();
    fs::write(&scan_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_ocr_command_script("ocr-command-adapter", &log_path);

    let native = glyphrush(&[
        "parse",
        native_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-command",
        command.to_str().unwrap(),
    ]);
    let scan = glyphrush(&[
        "parse",
        scan_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-command",
        command.to_str().unwrap(),
    ]);

    let log = fs::read_to_string(&log_path).expect("read ocr command log");

    assert_eq!(native["global_diagnostics"]["ocr_required_pages"], 0);
    assert_eq!(native["global_diagnostics"]["ocr_applied_pages"], 0);
    assert_eq!(native["pages"][0]["ocr_spans"], Value::Array(vec![]));
    assert_eq!(scan["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(scan["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(
        scan["pages"][0]["ocr_spans"][0]["text"],
        "Command OCR text page 0"
    );
    assert_eq!(scan["global_diagnostics"]["warnings"], Value::Array(vec![]));
    assert_eq!(log.lines().count(), 1);
    assert!(log.contains(scan_path.to_str().unwrap()));
    assert!(log.contains(":0"));
}

#[test]
fn parse_with_ocr_http_invokes_adapter_only_for_ocr_pages() {
    let dir = temp_dir("parse-ocr-http");
    let native_path = dir.join("native.pdf");
    let scan_path = dir.join("scan.pdf");
    fs::write(&native_path, minimal_pdf("Native OCR HTTP bypass")).unwrap();
    fs::write(&scan_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let (ocr_url, request_rx, server) = start_ocr_http_server("HTTP OCR text page 0");

    let native = glyphrush(&[
        "parse",
        native_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-http-url",
        &ocr_url,
    ]);
    let scan = glyphrush(&[
        "parse",
        scan_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-http-url",
        &ocr_url,
    ]);

    let request = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("HTTP OCR server should receive one OCR-routed request");
    server.join().expect("HTTP OCR server should finish");

    assert_eq!(native["global_diagnostics"]["ocr_required_pages"], 0);
    assert_eq!(native["global_diagnostics"]["ocr_applied_pages"], 0);
    assert_eq!(native["pages"][0]["ocr_spans"], Value::Array(vec![]));
    assert_eq!(scan["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(scan["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(
        scan["pages"][0]["ocr_spans"][0]["text"],
        "HTTP OCR text page 0"
    );
    assert_eq!(scan["global_diagnostics"]["warnings"], Value::Array(vec![]));
    assert!(request.starts_with("POST /ocr HTTP/1.1"));
    assert!(request.contains("\"page_index\":0"));
    assert!(request.contains(scan_path.to_str().unwrap()));
}

#[test]
fn parse_with_ocr_http_accepts_json_text_response() {
    let dir = temp_dir("parse-ocr-http-json");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let (ocr_url, request_rx, server) = start_ocr_http_server_with_response(
        "application/json",
        r#"{"text":"JSON OCR text page 0"}"#,
    );

    let json = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-http-url",
        &ocr_url,
    ]);

    let request = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("HTTP OCR server should receive OCR-routed request");
    server.join().expect("HTTP OCR server should finish");

    assert_eq!(json["global_diagnostics"]["ocr_required_pages"], 1);
    assert_eq!(json["global_diagnostics"]["ocr_applied_pages"], 1);
    assert_eq!(
        json["pages"][0]["ocr_spans"][0]["text"],
        "JSON OCR text page 0"
    );
    assert_eq!(json["global_diagnostics"]["warnings"], Value::Array(vec![]));
    assert!(request.contains("\"page_index\":0"));
    assert!(request.contains(pdf_path.to_str().unwrap()));
}

#[test]
fn parse_with_ocr_command_times_out_slow_adapter() {
    let dir = temp_dir("parse-ocr-command-timeout");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_baseline_script("ocr-command-timeout", "sleep 2\nprintf 'late OCR'");

    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--ocr-command",
            command.to_str().unwrap(),
            "--ocr-timeout-ms",
            "50",
        ])
        .output()
        .expect("run glyphrush parse with slow OCR command");
    let elapsed = start.elapsed();

    assert!(!output.status.success());
    assert!(
        elapsed.as_millis() < 1_000,
        "OCR timeout took {} ms",
        elapsed.as_millis()
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("OCR command timed out after 50 ms"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_with_cache_dir_reports_miss_then_hit_for_same_pdf() {
    let dir = temp_dir("parse-cache");
    let pdf_path = dir.join("cache.pdf");
    fs::write(&pdf_path, minimal_pdf("Cache me")).unwrap();
    let cache_dir = dir.join("cache");

    let first = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let second = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(first["global_diagnostics"]["cache_status"], "miss");
    assert_eq!(second["global_diagnostics"]["cache_status"], "hit");
    assert_eq!(
        first["global_diagnostics"]["cache_key"],
        second["global_diagnostics"]["cache_key"]
    );
    assert_eq!(
        first["pages"][0]["native_spans"][0]["text"],
        second["pages"][0]["native_spans"][0]["text"]
    );
    assert!(
        first["pages"][0]["timings"]["native_extract_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(second["pages"][0]["timings"]["native_extract_us"], 0);
    assert_eq!(second["pages"][0]["timings"]["layout_us"], 0);
    assert_eq!(second["pages"][0]["timings"]["table_us"], 0);
    assert!(
        first["global_diagnostics"]["total_stage_time_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(second["global_diagnostics"]["total_stage_time_us"], 0);
    let cache_files = fs::read_dir(&cache_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(cache_files.len(), 1);
    let snapshot: Value =
        serde_json::from_slice(&fs::read(&cache_files[0]).unwrap()).expect("cache snapshot json");
    assert_eq!(snapshot["snapshot_version"], "glyphrush-cache-snapshot-v1");
    assert_eq!(snapshot["cache_schema"], "glyphrush-cache-v43");
    assert_eq!(
        snapshot["cache_key"],
        first["global_diagnostics"]["cache_key"]
    );
    assert_eq!(snapshot["parser_name"], "glyphrush");
    assert_eq!(snapshot["backend"], "lopdf");
    assert_eq!(
        snapshot["document_fingerprint"],
        first["document_fingerprint"]
    );
    assert_eq!(
        snapshot["artifact"]["document_fingerprint"],
        first["document_fingerprint"]
    );
}

#[test]
fn parse_cache_hit_refreshes_source_modified_metadata() {
    let dir = temp_dir("parse-cache-source-modified");
    let pdf_path = dir.join("cache.pdf");
    let pdf_bytes = minimal_pdf("Cache modified metadata");
    fs::write(&pdf_path, &pdf_bytes).unwrap();
    let cache_dir = dir.join("cache");

    let first = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let first_modified_ms = source_modified_unix_ms(&pdf_path);
    assert_eq!(first["global_diagnostics"]["cache_status"], "miss");
    assert_eq!(
        first["metadata"]["source_modified_unix_ms"],
        first_modified_ms
    );

    let second_modified_ms = rewrite_until_modified_ms_changes(&pdf_path, &pdf_bytes);
    let second = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(second["global_diagnostics"]["cache_status"], "hit");
    assert_eq!(
        first["global_diagnostics"]["cache_key"],
        second["global_diagnostics"]["cache_key"]
    );
    assert_eq!(
        second["metadata"]["source_modified_unix_ms"],
        second_modified_ms
    );
}

#[test]
fn parse_ignores_corrupt_cache_snapshot_and_rebuilds_miss() {
    let dir = temp_dir("parse-cache-corrupt-snapshot");
    let pdf_path = dir.join("cache.pdf");
    fs::write(&pdf_path, minimal_pdf("Cache corruption recovery")).unwrap();
    let cache_dir = dir.join("cache");

    let first = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let cache_key = first["global_diagnostics"]["cache_key"].as_str().unwrap();
    let cache_path = cache_dir.join(format!("{cache_key}.json"));
    fs::write(&cache_path, b"{ this is not valid json").unwrap();

    let second = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(second["global_diagnostics"]["cache_status"], "miss");
    assert_eq!(second["global_diagnostics"]["cache_key"], cache_key);
    assert!(
        second["pages"][0]["native_spans"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Cache corruption recovery")
    );
    let warnings = second["global_diagnostics"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|warning| warning.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.starts_with("cache_snapshot_ignored:")),
        "warnings: {warnings:?}"
    );

    let snapshot: Value = serde_json::from_slice(&fs::read(&cache_path).unwrap())
        .expect("rebuilt cache snapshot json");
    assert_eq!(snapshot["cache_key"], cache_key);
    assert_eq!(
        snapshot["artifact"]["document_fingerprint"],
        second["document_fingerprint"]
    );
}
