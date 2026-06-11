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

#[test]
fn debug_page_explains_classifier_decision_for_a_page() {
    let pdf_path = write_test_pdf("debug", "Hello Glyphrush");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert_eq!(json["backend"], "lopdf");
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
    assert_eq!(json["page_index"], 0);
    assert_eq!(json["decision"]["route"], "native_fast_path");
    assert_eq!(json["decision"]["run_ocr"], false);
    assert!(json["signals"]["native_text_bytes"].as_u64().unwrap() > 0);
    assert_eq!(json["quality"]["flags"], Value::Array(vec![]));
    assert_eq!(json["quality"]["text_confidence"], 90);
    assert_eq!(json["text_output"]["empty"], false);
    assert!(json["text_output"]["word_count"].as_u64().unwrap() >= 2);
    assert_eq!(json["layout"]["paragraph_blocks"], 1);
    assert_eq!(json["layout"]["block_count"], 1);
}

#[test]
fn debug_page_reports_selected_page_artifact_identity_and_timings() {
    let pdf_path = write_test_pdf("debug-artifact-timings", "Timed Glyphrush page");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert!(
        json["artifact_id"].as_str().unwrap().contains(":p000000:"),
        "artifact_id should identify the selected page artifact: {}",
        json["artifact_id"]
    );
    assert_eq!(json["page_fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(json["dimensions"]["width"].as_f64().unwrap(), 612.0);
    assert_eq!(json["dimensions"]["height"].as_f64().unwrap(), 792.0);
    assert!(json["timings"]["open_us"].as_u64().unwrap() > 0);
    assert!(json["timings"]["classify_us"].as_u64().unwrap() > 0);
    assert!(json["timings"]["native_extract_us"].as_u64().unwrap() > 0);
    assert!(json["timings"]["layout_us"].as_u64().unwrap() > 0);
    assert!(json["timings"]["table_us"].as_u64().unwrap() > 0);
}

#[test]
fn debug_page_extracts_only_requested_page() {
    let dir = temp_dir("debug-page-selective");
    let pdf_path = dir.join("two-pages.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (First debug page) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Second page should not be extracted) Tj ET",
        ]),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert_eq!(json["document_page_count"], 2);
    assert_eq!(json["extracted_page_count"], 1);
    assert_eq!(json["page_index"], 0);
    assert!(json["signals"]["native_text_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn debug_page_detects_inherited_page_tree_rotation() {
    let dir = temp_dir("debug-inherited-rotation");
    let pdf_path = dir.join("rotated.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_inherited_rotation("Inherited rotation", 90),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert_eq!(json["signals"]["rotation_degrees"], 90);
    assert_eq!(json["decision"]["route"], "needs_fallback");
    assert_eq!(json["decision"]["run_heavy_layout"], true);
    assert_eq!(
        json["decision"]["flags"],
        Value::Array(vec![Value::String("layout_uncertain".to_string())])
    );
    assert_eq!(
        json["decision"]["reasons"],
        Value::Array(vec![Value::String("rotated_page".to_string())])
    );
}

#[test]
fn debug_page_uses_inherited_page_tree_media_box() {
    let dir = temp_dir("debug-inherited-mediabox");
    let pdf_path = dir.join("inherited-mediabox.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_inherited_media_box("Inherited dimensions", 300.0, 400.0),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert_eq!(json["signals"]["dimensions"]["width"], 300.0);
    assert_eq!(json["signals"]["dimensions"]["height"], 400.0);
}

#[test]
fn debug_page_prefers_page_crop_box_for_effective_dimensions() {
    let dir = temp_dir("debug-page-cropbox");
    let pdf_path = dir.join("cropbox.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_page_crop_box("CropBox dimensions", 612.0, 792.0, 306.0, 396.0),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert_eq!(json["signals"]["dimensions"]["width"], 306.0);
    assert_eq!(json["signals"]["dimensions"]["height"], 396.0);
}

#[test]
fn debug_page_escalates_full_page_image_with_sparse_native_text() {
    let dir = temp_dir("debug-hybrid-image");
    let pdf_path = dir.join("hybrid.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_full_page_image_and_text("Hybrid native text"),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert!(json["signals"]["native_text_bytes"].as_u64().unwrap() > 0);
    assert!(
        json["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        json["signals"]
    );
    assert_eq!(json["image_artifacts"][0]["image_id"], "p000000:im000000");
    assert_eq!(json["image_artifacts"][0]["source_name"], "Im1");
    assert!(
        json["image_artifacts"][0]["area_ratio"].as_f64().unwrap() >= 0.95,
        "image artifact: {}",
        json["image_artifacts"][0]
    );
    assert_eq!(json["decision"]["route"], "ocr_fallback");
    assert_eq!(json["decision"]["run_ocr"], true);
    assert_eq!(
        json["decision"]["flags"],
        Value::Array(vec![
            Value::String("requires_ocr".to_string()),
            Value::String("low_confidence_text".to_string())
        ])
    );
    assert_eq!(
        json["quality"]["flags"],
        Value::Array(vec![
            Value::String("requires_ocr".to_string()),
            Value::String("low_confidence_text".to_string())
        ])
    );
    assert_eq!(json["quality"]["text_confidence"], 25);
    assert_eq!(json["text_output"]["empty"], false);
    assert_eq!(
        json["decision"]["reasons"],
        Value::Array(vec![Value::String(
            "high_image_coverage_with_sparse_native_text".to_string()
        )])
    );
}

#[test]
fn debug_page_accepts_ocr_sidecar_for_page_fallback_investigation() {
    let dir = temp_dir("debug-ocr-sidecar");
    let pdf_path = dir.join("scan-debug.pdf");
    let sidecar_dir = dir.join("ocr");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(
        sidecar_dir.join("scan-debug.p000000.txt"),
        "OCR sidecar words",
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
    ]);

    assert_eq!(json["decision"]["route"], "ocr_fallback");
    assert_eq!(json["decision"]["run_ocr"], true);
    assert_eq!(
        json["quality"]["flags"],
        Value::Array(vec![
            Value::String("requires_ocr".to_string()),
            Value::String("low_confidence_text".to_string())
        ])
    );
    assert_eq!(json["text_output"]["empty"], false);
    assert_eq!(json["text_output"]["word_count"], 3);
    assert_eq!(json["warnings"], Value::Array(vec![]));
}

#[test]
fn debug_page_escalates_form_wrapped_image_with_sparse_native_text() {
    let dir = temp_dir("debug-form-image");
    let pdf_path = dir.join("form-image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_form_wrapped_full_page_image_and_text("Form image text"),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert!(
        json["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        json["signals"]
    );
    assert_eq!(json["image_artifacts"][0]["source_name"], "Fm1");
    assert!(
        json["image_artifacts"][0]["area_ratio"].as_f64().unwrap() >= 0.95,
        "image artifact: {}",
        json["image_artifacts"][0]
    );
    assert_eq!(json["decision"]["route"], "ocr_fallback");
    assert_eq!(
        json["decision"]["reasons"],
        Value::Array(vec![Value::String(
            "high_image_coverage_with_sparse_native_text".to_string()
        )])
    );
}

#[test]
fn debug_page_flags_image_heavy_page_with_substantial_native_text_for_layout_review() {
    let dir = temp_dir("debug-hybrid-native");
    let pdf_path = dir.join("hybrid-native.pdf");
    let text = "Native text layer with enough document content to trust the fast path. \
        This synthetic hybrid page represents searchable PDFs that include a full-page \
        image background but also carry a substantial native text layer for extraction.";
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text(text)).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert!(json["signals"]["native_text_bytes"].as_u64().unwrap() >= 128);
    assert!(
        json["signals"]["image_area_ratio"].as_f64().unwrap() >= 0.95,
        "signals: {}",
        json["signals"]
    );
    assert_eq!(json["decision"]["route"], "needs_fallback");
    assert_eq!(json["decision"]["run_ocr"], false);
    assert_eq!(json["decision"]["run_heavy_layout"], true);
    assert_eq!(
        json["decision"]["flags"],
        Value::Array(vec![Value::String("layout_uncertain".to_string())])
    );
    assert_eq!(
        json["decision"]["reasons"],
        Value::Array(vec![Value::String("image_text_overlay".to_string())])
    );
}

#[test]
fn debug_page_flags_ruled_table_geometry_without_ocr() {
    let dir = temp_dir("debug-ruled-table");
    let pdf_path = dir.join("ruled-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

    assert!(
        json["signals"]["table_line_density"].as_f64().unwrap() >= 0.25,
        "signals: {}",
        json["signals"]
    );
    assert_eq!(json["decision"]["route"], "needs_fallback");
    assert_eq!(json["decision"]["run_ocr"], false);
    assert_eq!(json["decision"]["run_table_recovery"], true);
    assert_eq!(json["layout"]["table_blocks"], 1);
    assert_eq!(json["layout"]["table_rows"], 3);
    assert_eq!(json["layout"]["table_cells"], 6);
    assert_eq!(json["layout"]["table_cells_with_bbox"], 0);
    assert_eq!(
        json["decision"]["flags"],
        Value::Array(vec![Value::String("table_uncertain".to_string())])
    );
}
