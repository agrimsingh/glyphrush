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
fn eval_manifest_can_assert_page_image_artifact_count() {
    let dir = temp_dir("eval-page-image-artifact-count-pass");
    let pdf_path = dir.join("image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_full_page_image_and_text("Image-backed native text"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "image.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "image_artifact_count": 1
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.image_artifact_count"]["expected"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.image_artifact_count"]["actual"],
        1
    );
}

#[test]
fn eval_manifest_fails_when_page_image_artifact_count_regresses() {
    let dir = temp_dir("eval-page-image-artifact-count-fail");
    let pdf_path = dir.join("image.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_full_page_image_and_text("Image-backed native text"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "image.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "image_artifact_count": 0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.image_artifact_count"]["expected"],
        0
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.image_artifact_count"]["actual"],
        1
    );
}

#[test]
fn eval_manifest_reports_route_reason_count_checks() {
    let dir = temp_dir("eval-route-reason-counts-pass");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "route_reason_counts": {
                  "high_image_coverage_without_native_text": 1
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["route_reason_counts"]["expected"],
        serde_json::json!({
          "high_image_coverage_without_native_text": 1
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["route_reason_counts"]["actual"],
        serde_json::json!({
          "high_image_coverage_without_native_text": 1
        })
    );
}

#[test]
fn eval_manifest_fails_when_route_reason_counts_regress() {
    let dir = temp_dir("eval-route-reason-counts-fail");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "route_reason_counts": {
                  "high_image_coverage_without_native_text": 0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["route_reason_counts"]["expected"],
        serde_json::json!({
          "high_image_coverage_without_native_text": 0
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["route_reason_counts"]["actual"],
        serde_json::json!({
          "high_image_coverage_without_native_text": 1
        })
    );
}

#[test]
fn eval_manifest_reports_text_recall_scores() {
    let dir = temp_dir("eval-text-recall-pass");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Hello Eval Harness")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "text_recall": {
                  "expected": "Hello Eval Harness",
                  "min_word_recall": 1.0,
                  "min_char_recall": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(
        json["documents"][0]["checks"]["text_recall"]["actual"]["word_recall"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["text_recall"]["actual"]["char_recall"],
        1.0
    );
}

#[test]
fn eval_manifest_reports_span_bbox_scores() {
    let dir = temp_dir("eval-span-bbox-pass");
    let pdf_path = dir.join("positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td (First line) Tj 0 -24 Td (Second line) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "positioned.pdf",
              "expect": {
                "span_bbox": [
                  {
                    "page": 0,
                    "text": "First line",
                    "provenance": "native",
                    "min_x0": 71.0,
                    "max_x0": 73.0,
                    "min_y0": 71.0,
                    "max_y0": 73.0,
                    "min_x1": 137.0,
                    "max_x1": 139.0,
                    "min_y1": 83.0,
                    "max_y1": 85.0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
        "--span-geometry",
    ]);
    let check = &json["documents"][0]["checks"]["span_bbox.000000"];

    assert_eq!(json["passed"], true);
    assert_eq!(check["passed"], true);
    assert_eq!(check["actual"]["matched"], true);
    assert_eq!(check["actual"]["span"]["text"], "First line");
    assert_eq!(check["actual"]["span"]["bbox"]["x0"], 72.0);
    assert_eq!(check["actual"]["bound_failures"], Value::Array(vec![]));
}

#[test]
fn eval_manifest_fails_when_text_recall_is_below_threshold() {
    let dir = temp_dir("eval-text-recall-fail");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Hello Eval Harness")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "text_recall": {
                  "expected": "Hello Missing Harness",
                  "min_word_recall": 1.0,
                  "min_char_recall": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["text_recall"]["expected"]["min_word_recall"],
        1.0
    );
    assert!(
        json["documents"][0]["checks"]["text_recall"]["actual"]["word_recall"]
            .as_f64()
            .unwrap()
            < 1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["text_recall"]["actual"]["missing_words"],
        Value::Array(vec![Value::String("missing".to_string())])
    );
}

#[test]
fn eval_manifest_reports_reading_order_score() {
    let dir = temp_dir("eval-reading-order-pass");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Alpha Beta Gamma")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "reading_order": {
                  "expected_sequence": ["Alpha", "Beta", "Gamma"],
                  "min_score": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["score"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["matched"],
        Value::Array(vec![
            serde_json::json!({"snippet": "Alpha", "position": 0}),
            serde_json::json!({"snippet": "Beta", "position": 6}),
            serde_json::json!({"snippet": "Gamma", "position": 11}),
        ])
    );
}

#[test]
fn eval_manifest_uses_span_geometry_layout_for_two_column_reading_order() {
    let dir = temp_dir("eval-two-column-reading-order");
    let pdf_path = dir.join("two-column.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 700 Td (Left column starts) Tj ET \
             BT /F1 12 Tf 330 700 Td (Right column starts) Tj ET \
             BT /F1 12 Tf 72 680 Td (Left column continues) Tj ET \
             BT /F1 12 Tf 330 680 Td (Right column continues) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "two-column.pdf",
              "expect": {
                "reading_order": {
                  "expected_sequence": [
                    "Left column starts",
                    "Left column continues",
                    "Right column starts",
                    "Right column continues"
                  ],
                  "min_score": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
        "--span-geometry",
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["score"],
        1.0
    );
}

#[test]
fn eval_manifest_uses_span_geometry_layout_for_full_width_heading_before_two_columns() {
    let dir = temp_dir("eval-heading-two-column-reading-order");
    let pdf_path = dir.join("heading-two-column.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 730 Td (FULL WIDTH TITLE) Tj ET \
             BT /F1 12 Tf 72 700 Td (Left column starts) Tj ET \
             BT /F1 12 Tf 330 700 Td (Right column starts) Tj ET \
             BT /F1 12 Tf 72 680 Td (Left column continues) Tj ET \
             BT /F1 12 Tf 330 680 Td (Right column continues) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "heading-two-column.pdf",
              "expect": {
                "reading_order": {
                  "expected_sequence": [
                    "FULL WIDTH TITLE",
                    "Left column starts",
                    "Left column continues",
                    "Right column starts",
                    "Right column continues"
                  ],
                  "min_score": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
        "--span-geometry",
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["score"],
        1.0
    );
}

#[test]
fn eval_manifest_fails_when_reading_order_score_is_below_threshold() {
    let dir = temp_dir("eval-reading-order-fail");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Alpha Beta Gamma")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "reading_order": {
                  "expected_sequence": ["Alpha", "Gamma", "Beta"],
                  "min_score": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["score"]
            .as_f64()
            .unwrap()
            < 1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["inversion_count"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["reading_order"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_reports_ocr_required_classification_scores() {
    let dir = temp_dir("eval-ocr-classification-pass");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "ocr_required_classification": {
                  "expected_pages": [0],
                  "min_precision": 1.0,
                  "min_recall": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(
        json["documents"][0]["checks"]["ocr_required_classification"]["actual"]["precision"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["ocr_required_classification"]["actual"]["recall"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["ocr_required_classification"]["actual"]["actual_pages"],
        Value::Array(vec![Value::from(0)])
    );
}

#[test]
fn eval_manifest_fails_when_ocr_required_recall_is_below_threshold() {
    let dir = temp_dir("eval-ocr-classification-fail");
    let pdf_path = dir.join("native.pdf");
    fs::write(&pdf_path, minimal_pdf("Native text only")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "native.pdf",
              "expect": {
                "ocr_required_classification": {
                  "expected_pages": [0],
                  "min_precision": 1.0,
                  "min_recall": 1.0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["ocr_required_classification"]["actual"]["recall"],
        0.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["ocr_required_classification"]["actual"]["false_negative_pages"],
        Value::Array(vec![Value::from(0)])
    );
}

#[test]
fn eval_manifest_reports_quality_flag_classification_scores() {
    let dir = temp_dir("eval-quality-flag-classification-pass");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "quality_flag_classification": [
                  {
                    "flag": "low_confidence_text",
                    "expected_pages": [0],
                    "min_precision": 1.0,
                    "min_recall": 1.0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_classification.low_confidence_text"]["actual"]
            ["precision"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_classification.low_confidence_text"]["actual"]
            ["recall"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_classification.low_confidence_text"]["actual"]
            ["actual_pages"],
        Value::Array(vec![Value::from(0)])
    );
}

#[test]
fn eval_manifest_fails_when_quality_flag_recall_is_below_threshold() {
    let dir = temp_dir("eval-quality-flag-classification-fail");
    let pdf_path = dir.join("native.pdf");
    fs::write(&pdf_path, minimal_pdf("Native text only")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "native.pdf",
              "expect": {
                "quality_flag_classification": [
                  {
                    "flag": "low_confidence_text",
                    "expected_pages": [0],
                    "min_precision": 1.0,
                    "min_recall": 1.0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_classification.low_confidence_text"]["actual"]
            ["recall"],
        0.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_classification.low_confidence_text"]["actual"]
            ["false_negative_pages"],
        Value::Array(vec![Value::from(0)])
    );
}

#[test]
fn eval_manifest_reports_zero_silent_failures_when_flags_are_expected() {
    let dir = temp_dir("eval-silent-failures-pass");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "silent_failures": {
                  "max_count": 0
                },
                "pages": [
                  {
                    "index": 0,
                    "required_flags": ["requires_ocr", "low_confidence_text"]
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["silent_failures"]["actual"]["count"],
        0
    );
    assert_eq!(
        json["documents"][0]["checks"]["silent_failures"]["actual"]["pages"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_fails_when_quality_flags_are_not_expected() {
    let dir = temp_dir("eval-silent-failures-fail");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "silent_failures": {
                  "max_count": 0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["silent_failures"]["actual"]["count"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["silent_failures"]["actual"]["pages"],
        serde_json::json!([
            {
                "page": 0,
                "flags": ["requires_ocr", "low_confidence_text"],
                "empty_text_output": true
            }
        ])
    );
}

#[test]
fn eval_manifest_fails_when_empty_text_output_page_is_not_expected() {
    let dir = temp_dir("eval-silent-empty-text-fail");
    let pdf_path = dir.join("blank.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "blank.pdf",
              "expect": {
                "silent_failures": {
                  "max_count": 0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["silent_failures"]["actual"]["pages"],
        serde_json::json!([
            {
                "page": 0,
                "flags": [],
                "empty_text_output": true
            }
        ])
    );
}

#[test]
fn eval_manifest_allows_expected_empty_text_output_pages() {
    let dir = temp_dir("eval-silent-empty-text-pass");
    let pdf_path = dir.join("blank.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "blank.pdf",
              "expect": {
                "silent_failures": {
                  "max_count": 0
                },
                "pages": [
                  {
                    "index": 0,
                    "empty_text_output": true
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["silent_failures"]["actual"]["pages"],
        Value::Array(vec![])
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.empty_text_output"]["actual"],
        true
    );
}

#[test]
fn eval_manifest_reports_table_structure_scores() {
    let dir = temp_dir("eval-table-structure-pass");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value |) Tj T* (| A | 1 |) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "table.pdf",
              "expect": {
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["Part", "Value"], ["A", "1"]],
                    "min_row_recall": 1.0,
                    "min_cell_recall": 1.0,
                    "min_cell_f1": 1.0,
                    "baseline": true
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["table_structure.page_000000"]["actual"]["row_recall"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["table_structure.page_000000"]["actual"]["cell_f1"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["table_structure.page_000000"]["actual"]["extracted_rows"],
        serde_json::json!([["Part", "Value"], ["A", "1"]])
    );
}

#[test]
fn eval_manifest_scores_multiple_table_structure_expectations_on_same_page() {
    let dir = temp_dir("eval-multiple-table-structures-same-page");
    let pdf_path = dir.join("tables.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| First | Value |) Tj T* (| A | 1 |) Tj T* (| Second | Value |) Tj T* (| B | 2 |) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "tables.pdf",
              "expect": {
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["First", "Value"], ["A", "1"]],
                    "min_row_recall": 1.0,
                    "min_cell_recall": 1.0,
                    "min_cell_f1": 1.0
                  },
                  {
                    "page": 0,
                    "expected_rows": [["Second", "Value"], ["B", "2"]],
                    "min_row_recall": 1.0,
                    "min_cell_recall": 1.0,
                    "min_cell_f1": 1.0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    let checks = json["documents"][0]["checks"]
        .as_object()
        .expect("checks object");

    assert_eq!(json["passed"], true);
    assert_eq!(
        checks["table_structure.page_000000.expectation_000000"]["actual"]["extracted_rows"],
        serde_json::json!([["First", "Value"], ["A", "1"]])
    );
    assert_eq!(
        checks["table_structure.page_000000.expectation_000001"]["actual"]["extracted_rows"],
        serde_json::json!([["Second", "Value"], ["B", "2"]])
    );
    assert_eq!(
        checks["table_structure.page_000000.expectation_000001"]["actual"]["cell_f1"],
        1.0
    );
}

#[test]
fn eval_manifest_preserves_empty_table_cells_in_structure_scores() {
    let dir = temp_dir("eval-table-empty-cells");
    let pdf_path = dir.join("table-empty-cells.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value | Note |) Tj T* (| A | | missing value |) Tj T* (| B | 2 | |) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "table-empty-cells.pdf",
              "expect": {
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [
                      ["Part", "Value", "Note"],
                      ["A", "", "missing value"],
                      ["B", "2", ""]
                    ],
                    "min_cell_recall": 1.0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    let actual = &json["documents"][0]["checks"]["table_structure.page_000000"]["actual"];

    assert_eq!(json["passed"], true);
    assert_eq!(
        actual["extracted_rows"],
        serde_json::json!([
            ["Part", "Value", "Note"],
            ["A", "", "missing value"],
            ["B", "2", ""]
        ])
    );
    assert_eq!(actual["cell_recall"], 1.0);
}

#[test]
fn eval_manifest_recovers_whitespace_rows_from_ruled_table_signal() {
    let dir = temp_dir("eval-ruled-table-structure");
    let pdf_path = dir.join("ruled-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "ruled-table.pdf",
              "expect": {
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["Part", "Value"], ["A", "1"], ["B", "2"]],
                    "min_row_recall": 1.0,
                    "min_cell_recall": 1.0,
                    "min_cell_f1": 1.0
                  }
                ],
                "pages": [
                  {
                    "index": 0,
                    "route": "needs_fallback",
                    "required_flags": ["table_uncertain"],
                    "required_reasons": ["table_line_density"]
                  }
                ],
                "silent_failures": {
                  "max_count": 0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    let table_check = &json["documents"][0]["checks"]["table_structure.page_000000"];

    assert_eq!(json["passed"], true);
    assert_eq!(
        table_check["actual"]["extracted_rows"],
        serde_json::json!([["Part", "Value"], ["A", "1"], ["B", "2"]])
    );
    assert_eq!(table_check["actual"]["cell_f1"], 1.0);
}

#[test]
fn eval_manifest_recovers_positioned_table_rows_with_span_geometry() {
    let dir = temp_dir("eval-positioned-table-structure");
    let pdf_path = dir.join("positioned-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_positioned_ruled_table()).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "positioned-table.pdf",
              "expect": {
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["Part", "Value"], ["A", "1"], ["B", "2"]],
                    "min_row_recall": 1.0,
                    "min_cell_recall": 1.0,
                    "min_cell_f1": 1.0
                  }
                ],
                "pages": [
                  {
                    "index": 0,
                    "route": "needs_fallback",
                    "required_flags": ["table_uncertain"],
                    "required_reasons": ["table_line_density"]
                  }
                ],
                "silent_failures": {
                  "max_count": 0
                }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
        "--span-geometry",
    ]);
    let table_check = &json["documents"][0]["checks"]["table_structure.page_000000"];

    assert_eq!(json["passed"], true);
    assert_eq!(
        table_check["actual"]["extracted_rows"],
        serde_json::json!([["Part", "Value"], ["A", "1"], ["B", "2"]])
    );
    assert_eq!(table_check["actual"]["cell_f1"], 1.0);
}

#[test]
fn eval_manifest_fails_when_table_cell_recall_is_below_threshold() {
    let dir = temp_dir("eval-table-structure-fail");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value |) Tj T* (| A | 1 |) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "table.pdf",
              "expect": {
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["Part", "Value"], ["B", "2"]],
                    "min_cell_recall": 1.0
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert!(
        json["documents"][0]["checks"]["table_structure.page_000000"]["actual"]["cell_recall"]
            .as_f64()
            .unwrap()
            < 1.0
    );
    assert_eq!(
        json["documents"][0]["checks"]["table_structure.page_000000"]["actual"]["missing_cells"],
        serde_json::json!([
            {"row": 1, "column": 0, "text": "B"},
            {"row": 1, "column": 1, "text": "2"}
        ])
    );
}

#[test]
fn eval_manifest_exits_nonzero_when_a_quality_gate_fails() {
    let dir = temp_dir("eval-fail");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("One page only")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "page_count": 2
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(json["documents"][0]["checks"]["page_count"]["expected"], 2);
    assert_eq!(json["documents"][0]["checks"]["page_count"]["actual"], 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("eval failed"));
}

#[test]
fn eval_manifest_can_assert_page_route_and_required_quality_flags() {
    let dir = temp_dir("eval-page-pass");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "page_count": 1,
                "pages": [
	                  {
	                    "index": 0,
	                    "route": "ocr_fallback",
	                    "required_flags": ["requires_ocr", "low_confidence_text"],
	                    "required_reasons": ["high_image_coverage_without_native_text"]
	                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.route"]["actual"],
        "ocr_fallback"
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.required_flags"]["actual"]["missing"],
        Value::Array(vec![])
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.required_reasons"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_can_assert_page_layout_block_counts() {
    let dir = temp_dir("eval-page-layout-block-counts");
    let pdf_path = dir.join("repeated-margins.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 12 Tf 72 768 Td (DATASHEET HEADER) Tj ET \
             BT /F1 12 Tf 72 650 Td (First page body) Tj ET \
             BT /F1 12 Tf 72 24 Td (CONFIDENTIAL FOOTER) Tj ET",
            "BT /F1 12 Tf 72 768 Td (DATASHEET HEADER) Tj ET \
             BT /F1 12 Tf 72 650 Td (Second page body) Tj ET \
             BT /F1 12 Tf 72 24 Td (CONFIDENTIAL FOOTER) Tj ET",
        ]),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "repeated-margins.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "layout_block_counts": {
                      "block_count": 3,
                      "paragraph_blocks": 1,
                      "header_blocks": 1,
                      "footer_blocks": 1
                    }
                  },
                  {
                    "index": 1,
                    "layout_block_counts": {
                      "block_count": 3,
                      "paragraph_blocks": 1,
                      "header_blocks": 1,
                      "footer_blocks": 1
                    }
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
        "--span-geometry",
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.layout_block_counts"]["actual"],
        serde_json::json!({
          "block_count": 3,
          "paragraph_blocks": 1,
          "heading_blocks": 0,
          "list_blocks": 0,
          "table_blocks": 0,
          "figure_blocks": 0,
          "header_blocks": 1,
          "footer_blocks": 1
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000001.layout_block_counts"]["actual"],
        serde_json::json!({
          "block_count": 3,
          "paragraph_blocks": 1,
          "heading_blocks": 0,
          "list_blocks": 0,
          "table_blocks": 0,
          "figure_blocks": 0,
          "header_blocks": 1,
          "footer_blocks": 1
        })
    );
}

#[test]
fn eval_manifest_layout_block_counts_allow_unasserted_table_detail_fields() {
    let dir = temp_dir("eval-layout-counts-table-details-subset");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value |) Tj T* (| A | 1 |) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "table.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "layout_block_counts": {
                      "block_count": 1,
                      "table_blocks": 1
                    }
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.layout_block_counts"]["actual"]["table_rows"],
        2
    );
}

#[test]
fn eval_manifest_fails_when_page_route_regresses() {
    let dir = temp_dir("eval-page-fail");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "route": "native_fast_path"
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.route"]["expected"],
        "native_fast_path"
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.route"]["actual"],
        "ocr_fallback"
    );
}
