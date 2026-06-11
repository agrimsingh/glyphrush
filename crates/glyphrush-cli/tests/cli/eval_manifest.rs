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
fn eval_manifest_passes_when_counts_and_required_text_match() {
    let dir = temp_dir("eval-pass");
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
                "page_count": 1,
                "fallback_pages": 0,
                "ocr_required_pages": 0,
                "ocr_applied_pages": 0,
                "required_text": ["Hello Eval Harness"]
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
    let manifest_sha256 = format!("{:x}", Sha256::digest(fs::read(&manifest_path).unwrap()));

    assert_eq!(json["report_version"], "glyphrush-eval-report-v1");
    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["run_metadata"]["parser_name"], "glyphrush");
    assert_eq!(
        json["run_metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["run_metadata"]["backend"], "lopdf");
    assert_eq!(json["run_metadata"]["backend_version"], "lopdf-adapter-v0");
    assert_eq!(json["manifest_sha256"], manifest_sha256);
    assert_eq!(
        json["corpus_fingerprint"],
        expected_corpus_fingerprint(&json)
    );
    assert_eq!(json["corpus_fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(json["passed"], true);
    assert_eq!(json["quality_passed"], true);
    assert_eq!(json["quality_failed"], false);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(json["documents"][0]["path"], "sample.pdf");
    assert_eq!(json["documents"][0]["metadata"]["parser_name"], "glyphrush");
    assert_eq!(
        json["documents"][0]["metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["documents"][0]["metadata"]["backend"], "lopdf");
    assert_eq!(
        json["documents"][0]["metadata"]["backend_version"],
        "lopdf-adapter-v0"
    );
    assert_eq!(
        json["documents"][0]["metadata"]["source_size_bytes"],
        fs::metadata(&pdf_path).unwrap().len()
    );
    assert_eq!(json["documents"][0]["checks"]["page_count"]["actual"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_passes_when_required_text_anchor_differs_only_by_whitespace() {
    let dir = temp_dir("eval-pass-squashed-anchor");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Hello world anchor")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "required_text": ["Helloworld anchor"]
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
    assert_eq!(json["quality_passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_required_text_uses_layout_reflowed_text() {
    let dir = temp_dir("eval-required-text-layout-reflow");
    let pdf_path = dir.join("fragmented.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 72 720 Td 24 TL (AP735) Tj T* (4) Tj ET"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "fragmented.pdf",
              "expect": {
                "required_text": ["AP7354"]
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
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_required_text_matches_across_layout_line_breaks() {
    let dir = temp_dir("eval-required-text-normalized-linebreaks");
    let pdf_path = dir.join("wrapped.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (Alpha Beta) Tj T* (Gamma Delta) Tj ET",
        ),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "wrapped.pdf",
              "expect": {
                "required_text": ["Alpha Beta Gamma Delta"],
                "pages": [
                  {
                    "index": 0,
                    "required_text": ["Alpha Beta Gamma Delta"]
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
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_page_required_text_is_page_local() {
    let dir = temp_dir("eval-page-required-text");
    let pdf_path = dir.join("two-pages.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (Only on first page) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Second page content) Tj ET",
        ]),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "two-pages.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 1,
                    "required_text": ["Only on first page"]
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with page-local required text");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["page_000001.required_text"]["actual"]["missing"],
        serde_json::json!(["Only on first page"])
    );
}

#[test]
fn eval_manifest_fails_when_page_fingerprint_regresses() {
    let dir = temp_dir("eval-page-fingerprint-regression");
    let pdf_path = dir.join("fingerprinted.pdf");
    fs::write(&pdf_path, minimal_pdf("Page fingerprint text")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "fingerprinted.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "page_fingerprint": "0000000000000000000000000000000000000000000000000000000000000000"
                  }
                ]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with page fingerprint mismatch");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["page_000000.page_fingerprint"]["passed"],
        false
    );
}

#[test]
fn eval_manifest_reports_category_counts_and_document_categories() {
    let dir = temp_dir("eval-category-counts");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean category text")).unwrap();
    fs::write(
        dir.join("unlabeled.pdf"),
        minimal_pdf("Unlabeled category text"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "page_count": 1,
                "required_text": ["Clean category text"]
              }
            },
            {
              "path": "unlabeled.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Unlabeled category text"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&["eval", manifest_path.to_str().unwrap()]);

    assert_eq!(
        json["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "uncategorized": 1
        })
    );
    assert_eq!(json["documents"][0]["category"], "clean_digital");
    assert_eq!(json["documents"][1]["category"], Value::Null);
    assert_eq!(json["quality_passed"], true);
    assert_eq!(json["failed_checks"], 0);
}

#[test]
fn eval_manifest_category_filter_runs_only_matching_documents() {
    let dir = temp_dir("eval-category-filter");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean filter text")).unwrap();
    fs::write(dir.join("scan.pdf"), minimal_pdf("Scan filter text")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "page_count": 1,
                "required_text": ["Clean filter text"]
              }
            },
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "page_count": 1,
                "required_text": ["missing scan text"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "eval",
        manifest_path.to_str().unwrap(),
        "--category",
        "clean_digital",
    ]);

    assert_eq!(json["document_count"], 1);
    assert_eq!(json["documents"][0]["path"], "clean.pdf");
    assert_eq!(json["documents"][0]["category"], "clean_digital");
    assert_eq!(
        json["category_counts"],
        serde_json::json!({"clean_digital": 1})
    );
    assert_eq!(json["quality_passed"], true);
    assert_eq!(json["failed_checks"], 0);
}

#[test]
fn eval_manifest_category_filter_accepts_comma_separated_category_set() {
    let dir = temp_dir("eval-category-set-filter");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean set filter text")).unwrap();
    fs::write(dir.join("table.pdf"), minimal_pdf("Table set filter text")).unwrap();
    fs::write(dir.join("scan.pdf"), minimal_pdf("Scan set filter text")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "page_count": 1,
                "required_text": ["Clean set filter text"]
              }
            },
            {
              "path": "table.pdf",
              "category": "tables",
              "expect": {
                "page_count": 1,
                "required_text": ["Table set filter text"]
              }
            },
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "page_count": 1,
                "required_text": ["missing scan set text"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "eval",
        manifest_path.to_str().unwrap(),
        "--category",
        "clean_digital,tables",
    ]);

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["documents"][0]["path"], "clean.pdf");
    assert_eq!(json["documents"][1]["path"], "table.pdf");
    assert_eq!(
        json["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "tables": 1
        })
    );
    assert_eq!(json["quality_passed"], true);
    assert_eq!(json["failed_checks"], 0);
}

#[test]
fn eval_manifest_category_preset_filters_native_text_surface() {
    let dir = temp_dir("eval-category-preset-native-text");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean Preset Eval")).unwrap();
    fs::write(dir.join("table.pdf"), minimal_pdf("Table Preset Eval")).unwrap();
    fs::write(dir.join("scan.pdf"), minimal_pdf("Scan Preset Eval")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "page_count": 1,
                "required_text": ["Clean Preset Eval"]
              }
            },
            {
              "path": "table.pdf",
              "category": "tables",
              "expect": {
                "page_count": 1,
                "required_text": ["Table Preset Eval"]
              }
            },
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "page_count": 1,
                "required_text": ["missing scanned preset eval text"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "eval",
        manifest_path.to_str().unwrap(),
        "--category-preset",
        "glyphrush-v0-native-text",
    ]);

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["documents"][0]["path"], "clean.pdf");
    assert_eq!(json["documents"][1]["path"], "table.pdf");
    assert_eq!(
        json["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "tables": 1
        })
    );
    assert!(json["category_counts"]["scanned"].is_null());
    assert_eq!(json["quality_passed"], true);
    assert_eq!(json["failed_checks"], 0);
}

#[test]
fn eval_manifest_category_filter_rejects_empty_selection_without_vacuous_pass() {
    let dir = temp_dir("eval-category-empty-selection");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean filter text")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "page_count": 1,
                "required_text": ["Clean filter text"]
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
        "--category",
        "scanned",
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["quality_passed"], false);
    assert_eq!(json["document_count"], 0);
    assert_eq!(json["category_counts"], serde_json::json!({}));
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(json["failure_samples"][0]["check"], "document_count");
    assert_eq!(
        json["failure_samples"][0]["expected"],
        serde_json::json!({"min": 1})
    );
    assert_eq!(json["failure_samples"][0]["actual"], 0);
}

#[test]
fn eval_manifest_rejects_empty_document_set_without_vacuous_pass() {
    let dir = temp_dir("eval-empty-document-set");
    let manifest_path = dir.join("corpus.json");
    fs::write(&manifest_path, r#"{ "documents": [] }"#).unwrap();

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "eval",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

    assert_eq!(json["passed"], false);
    assert_eq!(json["quality_passed"], false);
    assert_eq!(json["document_count"], 0);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(json["failure_samples"][0]["check"], "document_count");
    assert_eq!(
        json["failure_samples"][0]["expected"],
        serde_json::json!({"min": 1})
    );
    assert_eq!(json["failure_samples"][0]["actual"], 0);
}

#[test]
fn eval_manifest_reports_category_quality_summaries() {
    let dir = temp_dir("eval-category-summaries");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean summary text")).unwrap();
    fs::write(
        dir.join("scanned.pdf"),
        minimal_pdf("Scanned summary text without the required anchor"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "page_count": 1,
                "required_text": ["Clean summary text"]
              }
            },
            {
              "path": "scanned.pdf",
              "category": "scanned",
              "expect": {
                "page_count": 1,
                "required_text": ["missing scanned anchor"]
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

    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["category_summaries"]["clean_digital"],
        serde_json::json!({
            "document_count": 1,
            "page_count": 1,
            "passed_documents": 1,
            "failed_documents": 0,
            "failed_checks": 0,
            "quality_passed": true,
            "quality_failed": false
        })
    );
    assert_eq!(
        json["category_summaries"]["scanned"],
        serde_json::json!({
            "document_count": 1,
            "page_count": 1,
            "passed_documents": 0,
            "failed_documents": 1,
            "failed_checks": 1,
            "quality_passed": false,
            "quality_failed": true
        })
    );
}

#[test]
fn eval_manifest_fails_when_required_categories_are_missing() {
    let dir = temp_dir("eval-required-category-coverage");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean coverage text")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "required_categories": ["clean_digital", "scanned"],
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean coverage text"]
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

    assert_eq!(json["quality_passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["category_coverage"],
        serde_json::json!({
            "required": ["clean_digital", "scanned"],
            "present": ["clean_digital"],
            "missing": ["scanned"],
            "passed": false
        })
    );
    assert_eq!(json["failure_samples"][0]["check"], "required_categories");
    assert_eq!(
        json["failure_samples"][0]["actual"]["missing"],
        serde_json::json!(["scanned"])
    );
}

#[test]
fn eval_manifest_fails_when_min_category_counts_are_not_met() {
    let dir = temp_dir("eval-min-category-counts");
    fs::write(
        dir.join("datasheet.pdf"),
        minimal_pdf("Datasheet coverage text"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "min_category_counts": {
            "datasheet": 2,
            "scanned": 1
          },
          "documents": [
            {
              "path": "datasheet.pdf",
              "category": "datasheet",
              "expect": {
                "required_text": ["Datasheet coverage text"]
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

    assert_eq!(json["quality_passed"], false);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["category_coverage"]["min_category_counts"],
        serde_json::json!({
            "datasheet": 2,
            "scanned": 1
        })
    );
    assert_eq!(
        json["category_coverage"]["under_minimum"],
        serde_json::json!({
            "datasheet": {
                "required": 2,
                "actual": 1
            },
            "scanned": {
                "required": 1,
                "actual": 0
            }
        })
    );
    assert_eq!(json["failure_samples"][0]["check"], "min_category_counts");
}

#[test]
fn eval_manifest_reports_run_configuration_options() {
    let dir = temp_dir("eval-run-configuration");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Hello Eval Config")).unwrap();
    let ocr_dir = dir.join("ocr");
    fs::create_dir(&ocr_dir).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Hello Eval Config"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "eval",
        manifest_path.to_str().unwrap(),
        "--span-geometry",
        "--ocr-sidecar",
        ocr_dir.to_str().unwrap(),
        "--ocr-timeout-ms",
        "1234",
    ]);

    assert_eq!(json["run_configuration"]["span_geometry"], true);
    assert_eq!(json["run_configuration"]["ocr_sidecar"], true);
    assert_eq!(json["run_configuration"]["ocr_command"], false);
    assert_eq!(json["run_configuration"]["ocr_command_input"], "pdf_page");
    assert_eq!(json["run_configuration"]["ocr_timeout_ms"], 1234);
    assert_eq!(json["quality_passed"], true);
    assert_eq!(json["failed_checks"], 0);
}

#[test]
fn eval_manifest_jobs_preserve_manifest_order_and_report_worker_count() {
    let dir = temp_dir("eval-jobs-order");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second eval jobs")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First eval jobs")).unwrap();
    fs::write(dir.join("c.pdf"), minimal_pdf("Third eval jobs")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "b.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Second eval jobs"]
              }
            },
            {
              "path": "a.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["First eval jobs"]
              }
            },
            {
              "path": "c.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Third eval jobs"]
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
        "--jobs",
        "2",
    ]);

    assert_eq!(json["worker_count"], 2);
    assert_eq!(json["document_count"], 3);
    assert_eq!(json["passed"], true);
    assert_eq!(json["failed_checks"], 0);
    assert_eq!(json["documents"][0]["path"], "b.pdf");
    assert_eq!(json["documents"][1]["path"], "a.pdf");
    assert_eq!(json["documents"][2]["path"], "c.pdf");
    assert_eq!(
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
    assert_eq!(
        json["documents"][1]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
    assert_eq!(
        json["documents"][2]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_uses_backend_specific_expectation_overrides() {
    let dir = temp_dir("eval-backend-specific-expectations");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Backend-specific manifest")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "route_counts": {
                  "native_fast_path": 0,
                  "needs_fallback": 0,
                  "ocr_fallback": 1,
                  "unsupported": 0
                }
              },
              "expect_by_backend": {
                "lopdf": {
                  "route_counts": {
                    "native_fast_path": 1,
                    "needs_fallback": 0,
                    "ocr_fallback": 0,
                    "unsupported": 0
                  }
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

    assert_eq!(json["quality_passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["route_counts"]["expected"],
        serde_json::json!({
            "native_fast_path": 1,
            "needs_fallback": 0,
            "ocr_fallback": 0,
            "unsupported": 0
        })
    );
}

#[test]
fn eval_manifest_with_cache_dir_reports_cache_hit_and_miss_counts() {
    let dir = temp_dir("eval-cache-counts");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second eval cache")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First eval cache")).unwrap();
    let cache_dir = dir.join("cache");
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "b.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Second eval cache"]
              }
            },
            {
              "path": "a.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["First eval cache"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let first = glyphrush(&[
        "eval",
        manifest_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let second = glyphrush(&[
        "eval",
        manifest_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(first["cache_hits"], 0);
    assert_eq!(first["cache_misses"], 2);
    assert_eq!(second["cache_hits"], 2);
    assert_eq!(second["cache_misses"], 0);
    assert_eq!(second["quality_passed"], true);
    assert_eq!(second["failed_checks"], 0);
    assert_eq!(second["documents"][0]["path"], "b.pdf");
    assert_eq!(second["documents"][1]["path"], "a.pdf");
    assert_eq!(first["documents"][0]["artifact_cache_status"], "miss");
    assert_eq!(first["documents"][1]["artifact_cache_status"], "miss");
    assert_eq!(second["documents"][0]["artifact_cache_status"], "hit");
    assert_eq!(second["documents"][1]["artifact_cache_status"], "hit");
}

#[test]
fn eval_manifest_reports_artifact_diagnostics_without_expected_checks() {
    let dir = temp_dir("eval-artifact-diagnostics");
    fs::write(
        dir.join("a-native.pdf"),
        minimal_pdf("Eval diagnostics native"),
    )
    .unwrap();
    fs::write(
        dir.join("b-scan.pdf"),
        minimal_pdf_with_full_page_image_and_text(""),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a-native.pdf",
              "expect": {
                "required_text": ["Eval diagnostics native"]
              }
            },
            {
              "path": "b-scan.pdf",
              "expect": {}
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&["eval", manifest_path.to_str().unwrap()]);

    assert_eq!(json["passed"], true);
    assert_eq!(json["page_count"], 2);
    assert_eq!(json["fallback_pages"], 1);
    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["ocr_applied_pages"], 0);
    assert_eq!(json["image_artifact_count"], 1);
    assert_eq!(json["image_artifact_pages"], 1);
    assert_eq!(json["warnings_count"], 1);
    assert_eq!(json["empty_text_output_pages"], 1);
    assert_eq!(json["route_counts"]["native_fast_path"], 1);
    assert_eq!(json["route_counts"]["ocr_fallback"], 1);
    assert_eq!(
        json["route_reason_counts"]["high_image_coverage_without_native_text"],
        1
    );
    assert_eq!(json["quality_flag_counts"]["requires_ocr"], 1);
    assert_eq!(json["quality_flag_counts"]["low_confidence_text"], 1);
    assert_eq!(json["documents"][0]["page_count"], 1);
    assert_eq!(json["documents"][0]["fallback_pages"], 0);
    assert_eq!(json["documents"][1]["page_count"], 1);
    assert_eq!(json["documents"][1]["fallback_pages"], 1);
    assert_eq!(json["documents"][1]["ocr_required_pages"], 1);
    assert_eq!(json["documents"][1]["warnings_count"], 1);
    assert_eq!(json["documents"][1]["empty_text_output_pages"], 1);
    assert_eq!(
        json["documents"][1]["route_reason_counts"]["high_image_coverage_without_native_text"],
        1
    );
}

#[test]
fn eval_manifest_fails_when_source_provenance_regresses() {
    let dir = temp_dir("eval-source-provenance-fail");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Source provenance")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "document_fingerprint": "not-the-current-document",
              "source_size_bytes": 1,
              "source_modified_unix_ms": 1
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
    let actual_fingerprint = sha256_hex(fs::read(&pdf_path).unwrap());

    assert_eq!(json["passed"], false);
    assert_eq!(json["failed_checks"], 3);
    assert_eq!(
        json["documents"][0]["checks"]["document_fingerprint"]["expected"],
        "not-the-current-document"
    );
    assert_eq!(
        json["documents"][0]["checks"]["document_fingerprint"]["actual"],
        actual_fingerprint
    );
    assert_eq!(
        json["documents"][0]["checks"]["source_size_bytes"]["expected"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["source_size_bytes"]["actual"],
        fs::metadata(&pdf_path).unwrap().len()
    );
    assert_eq!(
        json["documents"][0]["checks"]["source_modified_unix_ms"]["expected"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["source_modified_unix_ms"]["actual"],
        source_modified_unix_ms(&pdf_path)
    );
    assert_eq!(json["failure_samples"][0]["check"], "document_fingerprint");
}

#[test]
fn eval_manifest_reports_required_warning_checks() {
    let dir = temp_dir("eval-warning-pass");
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
                "warnings_count": 1,
                "required_warnings": ["p000000: requires_ocr_without_ocr_output"]
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
        json["documents"][0]["checks"]["warnings_count"]["actual"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["required_warnings"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn eval_manifest_fails_when_required_warning_is_missing() {
    let dir = temp_dir("eval-warning-fail");
    let pdf_path = dir.join("native.pdf");
    fs::write(&pdf_path, minimal_pdf("No OCR warning here")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "native.pdf",
              "expect": {
                "required_warnings": ["p000000: requires_ocr_without_ocr_output"]
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
    assert_eq!(json["quality_passed"], false);
    assert_eq!(json["quality_failed"], true);
    assert_eq!(json["failed_checks"], 1);
    assert_eq!(
        json["documents"][0]["checks"]["required_warnings"]["actual"]["missing"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
}

#[test]
fn eval_manifest_reports_route_count_checks() {
    let dir = temp_dir("eval-route-counts-pass");
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
                "route_counts": {
                  "native_fast_path": 0,
                  "needs_fallback": 0,
                  "ocr_fallback": 1,
                  "unsupported": 0
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
        json["documents"][0]["checks"]["route_counts"]["expected"],
        serde_json::json!({
          "native_fast_path": 0,
          "needs_fallback": 0,
          "ocr_fallback": 1,
          "unsupported": 0
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["route_counts"]["actual"],
        serde_json::json!({
          "native_fast_path": 0,
          "needs_fallback": 0,
          "ocr_fallback": 1,
          "unsupported": 0
        })
    );
}

#[test]
fn eval_manifest_fails_when_route_counts_regress() {
    let dir = temp_dir("eval-route-counts-fail");
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
                "route_counts": {
                  "native_fast_path": 1,
                  "needs_fallback": 0,
                  "ocr_fallback": 0,
                  "unsupported": 0
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
        json["documents"][0]["checks"]["route_counts"]["expected"],
        serde_json::json!({
          "native_fast_path": 1,
          "needs_fallback": 0,
          "ocr_fallback": 0,
          "unsupported": 0
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["route_counts"]["actual"],
        serde_json::json!({
          "native_fast_path": 0,
          "needs_fallback": 0,
          "ocr_fallback": 1,
          "unsupported": 0
        })
    );
}

#[test]
fn eval_manifest_reports_quality_flag_count_checks() {
    let dir = temp_dir("eval-quality-flag-counts-pass");
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
                "quality_flag_counts": {
                  "requires_ocr": 1,
                  "low_confidence_text": 1,
                  "broken_encoding": 0,
                  "layout_uncertain": 0,
                  "table_uncertain": 0,
                  "unsupported_feature": 0
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
        json["documents"][0]["checks"]["quality_flag_counts"]["expected"],
        serde_json::json!({
          "requires_ocr": 1,
          "low_confidence_text": 1,
          "broken_encoding": 0,
          "layout_uncertain": 0,
          "table_uncertain": 0,
          "unsupported_feature": 0
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_counts"]["actual"],
        serde_json::json!({
          "requires_ocr": 1,
          "low_confidence_text": 1,
          "broken_encoding": 0,
          "layout_uncertain": 0,
          "table_uncertain": 0,
          "unsupported_feature": 0
        })
    );
}

#[test]
fn eval_manifest_fails_when_quality_flag_counts_regress() {
    let dir = temp_dir("eval-quality-flag-counts-fail");
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
                "quality_flag_counts": {
                  "requires_ocr": 0,
                  "low_confidence_text": 0,
                  "broken_encoding": 0,
                  "layout_uncertain": 0,
                  "table_uncertain": 0,
                  "unsupported_feature": 0
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
        json["documents"][0]["checks"]["quality_flag_counts"]["expected"],
        serde_json::json!({
          "requires_ocr": 0,
          "low_confidence_text": 0,
          "broken_encoding": 0,
          "layout_uncertain": 0,
          "table_uncertain": 0,
          "unsupported_feature": 0
        })
    );
    assert_eq!(
        json["documents"][0]["checks"]["quality_flag_counts"]["actual"],
        serde_json::json!({
          "requires_ocr": 1,
          "low_confidence_text": 1,
          "broken_encoding": 0,
          "layout_uncertain": 0,
          "table_uncertain": 0,
          "unsupported_feature": 0
        })
    );
}

#[test]
fn eval_manifest_reports_image_artifact_count_checks() {
    let dir = temp_dir("eval-image-artifact-count-pass");
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
                "image_artifact_count": 1
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
        json["documents"][0]["checks"]["image_artifact_count"]["expected"],
        1
    );
    assert_eq!(
        json["documents"][0]["checks"]["image_artifact_count"]["actual"],
        1
    );
}

#[test]
fn eval_manifest_fails_when_image_artifact_count_regresses() {
    let dir = temp_dir("eval-image-artifact-count-fail");
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
                "image_artifact_count": 0
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
        json["documents"][0]["checks"]["image_artifact_count"]["expected"],
        0
    );
    assert_eq!(
        json["documents"][0]["checks"]["image_artifact_count"]["actual"],
        1
    );
}

#[test]
fn eval_required_text_can_match_empty_cells_from_structured_table_rows() {
    let dir = temp_dir("eval-structured-table-text");
    let pdf_path = dir.join("aligned-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_aligned_whitespace_ruled_table()).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "aligned-table.pdf",
              "expect": {
                "required_text": ["| A |  | missing value |", "| B | 2 |  |"]
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

    assert_eq!(json["quality_passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}
