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
fn feature_parity_reports_liteparse_capability_gaps() {
    let json = glyphrush(&["--backend", "lopdf", "feature-parity"]);

    assert_eq!(json["report_version"], "glyphrush-feature-parity-report-v1");
    assert_eq!(json["comparison_target"], "liteparse");
    assert_eq!(json["selected_backend"], "lopdf");
    assert_eq!(
        json["run_metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["summary"]["target_capability_count"], 13);
    assert_eq!(json["summary"]["implemented"], 10);
    assert_eq!(json["summary"]["partial"], 1);
    assert_eq!(json["summary"]["planned"], 0);
    assert_eq!(json["summary"]["not_planned"], 2);
    assert_eq!(
        json["quality_policy"],
        "adaptive_fallback_no_silent_failure"
    );
    assert_eq!(
        json["speed_policy"],
        "quality_backed_speedup_claims_required"
    );
    assert_eq!(
        json["recommended_gate"],
        "bench --eval-manifest <manifest> --eval-category-preset glyphrush-v0-native-text --baseline-preset glyphrush-v0 --require-coverage-preset glyphrush-v0-native-text --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5"
    );
    assert_eq!(json["readiness"]["native_text_speed_race_ready"], true);
    assert_eq!(json["readiness"]["native_text_speed_claim_ready"], false);
    assert_eq!(
        json["readiness"]["native_text_speed_claim_blockers"],
        serde_json::json!(["missing_benchmark_evidence"])
    );
    assert_eq!(json["readiness"]["full_liteparse_drop_in_ready"], false);
    assert_eq!(json["readiness"]["glyphrush_product_parity_ready"], false);
    assert_eq!(
        json["readiness"]["native_text_speed_race_gate"],
        json["recommended_gate"]
    );
    assert_eq!(json["readiness"]["hot_path"]["capability_count"], 3);
    assert_eq!(json["readiness"]["hot_path"]["implemented"], 3);
    assert_eq!(json["readiness"]["hot_path"]["ready"], true);
    assert_eq!(
        json["readiness"]["liteparse_capabilities"]["implemented_or_partial"],
        11
    );
    assert_eq!(json["readiness"]["liteparse_capabilities"]["target"], 13);
    assert_eq!(
        json["readiness"]["remaining_partial"],
        serde_json::json!(["page_render_for_ocr"])
    );
    assert_eq!(
        json["readiness"]["remaining_planned"],
        serde_json::json!([])
    );
    assert_eq!(
        json["readiness"]["not_planned_by_design"],
        serde_json::json!(["mupdf_backend", "bundled_builtin_ocr"])
    );

    let capabilities = json["capabilities"].as_array().unwrap();
    assert_eq!(capabilities.len(), 13);

    let native_text = capability(capabilities, "native_text_extraction");
    assert_eq!(native_text["liteparse"], "pdfium_native_text");
    assert_eq!(native_text["glyphrush_status"], "implemented");
    assert_eq!(native_text["hot_path"], true);

    let benchmark = capability(capabilities, "quality_backed_benchmarking");
    assert_eq!(benchmark["glyphrush_status"], "implemented");
    assert_eq!(benchmark["glyphrush"], "strict_speedup_claim_gate");

    let span_geometry = capability(capabilities, "span_geometry_layout");
    assert_eq!(span_geometry["glyphrush_status"], "implemented");
    assert!(
        span_geometry["notes"]
            .as_str()
            .unwrap()
            .contains("clearly separated 2-5 column reading order")
    );
    assert!(
        span_geometry["notes"]
            .as_str()
            .unwrap()
            .contains("fragmented full-width heading rows")
    );
    assert!(
        span_geometry["notes"]
            .as_str()
            .unwrap()
            .contains("fragmented middle cross-column bands")
    );
    assert!(
        span_geometry["notes"]
            .as_str()
            .unwrap()
            .contains("fragmented short section separators")
    );
    assert!(
        span_geometry["notes"]
            .as_str()
            .unwrap()
            .contains("narrow academic gutters")
    );

    let cache = capability(capabilities, "artifact_cache_snapshots");
    assert_eq!(cache["glyphrush_status"], "implemented");
    assert_eq!(
        cache["glyphrush"],
        "cache_dir_snapshot_envelope_artifact_reuse"
    );

    let table_recovery = capability(capabilities, "table_recovery");
    assert_eq!(table_recovery["glyphrush_status"], "implemented");
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("aligned whitespace and positioned interior section rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("keeps positioned captions outside table grids")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("fixed-width wrapped descriptor fragments")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("key-value metadata rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("header-guided section rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("two-column descriptor/value rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("prefixed leading delimited/text-table captions outside table grids")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("trailing descriptor continuations")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("header-guided trailing blank cells")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("same-line fragmented positioned cells")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("first-column positioned section rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("fragmented first-column positioned section rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("same-column wrapped header rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("interior positioned condition/note rows")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("embedded pin/function tables")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("package pin-description tables")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("OMB-style budget projection tables")
    );
    assert!(
        table_recovery["notes"]
            .as_str()
            .unwrap()
            .contains("AWINIC parameter/test-condition electrical tables")
    );

    let ocr = capability(capabilities, "ocr");
    assert_eq!(ocr["liteparse"], "tesseract_or_http_ocr");
    assert_eq!(
        ocr["glyphrush"],
        "sidecar_command_http_or_tesseract_rendered_image_wrapper_invoked_page_selectively"
    );
    assert_eq!(ocr["glyphrush_status"], "implemented");
    assert_eq!(ocr["hot_path"], false);
    assert_eq!(
        ocr["quality_guard"],
        "page_selective_adapter_preflight_and_requires_ocr_flag"
    );

    let bindings = capability(capabilities, "python_node_bindings");
    assert_eq!(bindings["glyphrush_status"], "implemented");
    assert_eq!(
        bindings["glyphrush"],
        "thin_python_node_parse_inspect_debug_eval_bench_manifest_preflight_wrappers"
    );
    assert!(
        bindings["notes"]
            .as_str()
            .unwrap()
            .contains("text and markdown derived-output helpers")
    );

    let wasm = capability(capabilities, "wasm_bindings");
    assert_eq!(wasm["glyphrush_status"], "implemented");
    assert_eq!(
        wasm["glyphrush"],
        "wasm_parse_pdf_bytes_over_native_core_artifact"
    );

    let builtin_ocr = capability(capabilities, "bundled_builtin_ocr");
    assert_eq!(builtin_ocr["glyphrush_status"], "not_planned");
}

#[test]
fn feature_parity_can_require_quality_backed_liteparse_benchmark_evidence() {
    let dir = temp_dir("feature-parity-bench-evidence");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "quality": {
            "category_summaries": {
              "scanned": {
                "document_count": 1,
                "page_count": 3,
                "failed_checks": 0,
                "quality_passed": true
              },
              "clean_digital": {
                "document_count": 2,
                "page_count": 12,
                "failed_checks": 0,
                "quality_passed": true
              }
            }
          },
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 3.2,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            },
            {
              "baseline": "liteparse-no-ocr",
              "required_glyphrush_speedup": 1.5,
              "actual_glyphrush_speedup": 1.8,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
        "--require-speed-evidence",
    ]);

    assert_eq!(
        json["benchmark_evidence"]["report_path"],
        report_path.to_string_lossy().as_ref()
    );
    assert_eq!(
        json["benchmark_evidence"]["report_version"],
        "glyphrush-bench-report-v1"
    );
    assert_eq!(json["benchmark_evidence"]["backend"], "pdfium");
    assert_eq!(json["benchmark_evidence"]["quality_status"], "checked");
    assert_eq!(json["benchmark_evidence"]["required_claim_count"], 2);
    assert_eq!(json["benchmark_evidence"]["quality_backed_claim_count"], 2);
    assert_eq!(json["benchmark_evidence"]["claim_passed_count"], 2);
    assert_eq!(json["benchmark_evidence"]["evidence_passed"], true);
    assert_eq!(
        json["benchmark_evidence"]["quality_categories"],
        serde_json::json!([
            {
                "category": "clean_digital",
                "document_count": 2,
                "page_count": 12,
                "failed_checks": 0,
                "quality_passed": true
            },
            {
                "category": "scanned",
                "document_count": 1,
                "page_count": 3,
                "failed_checks": 0,
                "quality_passed": true
            }
        ])
    );
    assert_eq!(
        json["benchmark_evidence"]["coverage_requirement"],
        serde_json::json!({
            "preset": "glyphrush-v0",
            "required": false,
            "required_categories": [
                "clean_digital",
                "scanned",
                "hybrid",
                "academic_columns",
                "tables",
                "forms",
                "rotated",
                "weird_encoding",
                "large"
            ],
            "present_categories": ["clean_digital", "scanned"],
            "missing_categories": [
                "hybrid",
                "academic_columns",
                "tables",
                "forms",
                "rotated",
                "weird_encoding",
                "large"
            ],
            "passed": false
        })
    );
    assert_eq!(
        json["benchmark_evidence"]["missing_required_claims"],
        serde_json::json!([])
    );
    assert_eq!(json["readiness"]["native_text_speed_claim_ready"], false);
    assert_eq!(
        json["readiness"]["native_text_speed_claim_blockers"],
        serde_json::json!(["missing_coverage_preset"])
    );
}

#[test]
fn feature_parity_marks_liteparse_speed_claim_ready_with_quality_and_coverage_evidence() {
    let dir = temp_dir("feature-parity-bench-evidence-ready");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "quality": {
            "category_summaries": {
              "clean_digital": { "document_count": 1, "page_count": 2, "failed_checks": 0, "quality_passed": true },
              "scanned": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "hybrid": { "document_count": 1, "page_count": 3, "failed_checks": 0, "quality_passed": true },
              "academic_columns": { "document_count": 1, "page_count": 8, "failed_checks": 0, "quality_passed": true },
              "tables": { "document_count": 1, "page_count": 2, "failed_checks": 0, "quality_passed": true },
              "forms": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "rotated": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "weird_encoding": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "large": { "document_count": 1, "page_count": 50, "failed_checks": 0, "quality_passed": true }
            }
          },
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 3.2,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            },
            {
              "baseline": "liteparse-no-ocr",
              "required_glyphrush_speedup": 1.5,
              "actual_glyphrush_speedup": 1.8,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
        "--require-speed-evidence",
        "--require-coverage-preset",
        "glyphrush-v0",
    ]);

    assert_eq!(json["readiness"]["native_text_speed_race_ready"], true);
    assert_eq!(json["readiness"]["native_text_speed_claim_ready"], true);
    assert_eq!(
        json["readiness"]["native_text_speed_claim_blockers"],
        serde_json::json!([])
    );
    assert_eq!(
        json["benchmark_evidence"]["coverage_requirement"]["passed"],
        true
    );
}

#[test]
fn feature_parity_marks_native_text_speed_advantage_ready_when_baseline_quality_fails() {
    let dir = temp_dir("feature-parity-bench-speed-advantage");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "quality": {
            "category_summaries": {
              "clean_digital": { "document_count": 1, "page_count": 2, "failed_checks": 0, "quality_passed": true },
              "hybrid": { "document_count": 1, "page_count": 3, "failed_checks": 0, "quality_passed": true },
              "academic_columns": { "document_count": 1, "page_count": 8, "failed_checks": 0, "quality_passed": true },
              "tables": { "document_count": 1, "page_count": 2, "failed_checks": 0, "quality_passed": true },
              "forms": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "rotated": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "weird_encoding": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "large": { "document_count": 1, "page_count": 50, "failed_checks": 0, "quality_passed": true }
            }
          },
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 64.0,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": false,
              "glyphrush_quality_backed": true,
              "quality_backed": false,
              "quality_blocker": "baseline_quality_failed",
              "claim_passed": false,
              "status": "quality_failed"
            },
            {
              "baseline": "liteparse-no-ocr",
              "required_glyphrush_speedup": 1.5,
              "actual_glyphrush_speedup": 4.5,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": false,
              "glyphrush_quality_backed": true,
              "quality_backed": false,
              "quality_blocker": "baseline_quality_failed",
              "claim_passed": false,
              "status": "quality_failed"
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
        "--require-speed-advantage",
        "--require-coverage-preset",
        "glyphrush-v0-native-text",
    ]);

    assert_eq!(json["readiness"]["native_text_speed_claim_ready"], false);
    assert_eq!(
        json["readiness"]["native_text_speed_claim_blockers"],
        serde_json::json!(["missing_quality_backed_liteparse_claims"])
    );
    assert_eq!(json["readiness"]["native_text_speed_advantage_ready"], true);
    assert_eq!(
        json["readiness"]["native_text_speed_advantage_blockers"],
        serde_json::json!([])
    );
}

#[test]
fn feature_parity_speed_advantage_gate_fails_when_baseline_quality_is_unchecked() {
    let dir = temp_dir("feature-parity-bench-speed-advantage-unchecked");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "quality": {
            "category_summaries": {
              "clean_digital": { "document_count": 1, "page_count": 2, "failed_checks": 0, "quality_passed": true },
              "hybrid": { "document_count": 1, "page_count": 3, "failed_checks": 0, "quality_passed": true },
              "academic_columns": { "document_count": 1, "page_count": 8, "failed_checks": 0, "quality_passed": true },
              "tables": { "document_count": 1, "page_count": 2, "failed_checks": 0, "quality_passed": true },
              "forms": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "rotated": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "weird_encoding": { "document_count": 1, "page_count": 1, "failed_checks": 0, "quality_passed": true },
              "large": { "document_count": 1, "page_count": 50, "failed_checks": 0, "quality_passed": true }
            }
          },
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 64.0,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": false,
              "baseline_quality_passed": false,
              "glyphrush_quality_backed": true,
              "quality_backed": false,
              "quality_blocker": "baseline_quality_not_checked",
              "claim_passed": false,
              "status": "quality_not_checked"
            },
            {
              "baseline": "liteparse-no-ocr",
              "required_glyphrush_speedup": 1.5,
              "actual_glyphrush_speedup": 4.5,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": false,
              "baseline_quality_passed": false,
              "glyphrush_quality_backed": true,
              "quality_backed": false,
              "quality_blocker": "baseline_quality_not_checked",
              "claim_passed": false,
              "status": "quality_not_checked"
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "feature-parity",
            "--bench-report",
            report_path.to_str().unwrap(),
            "--require-speed-advantage",
            "--require-coverage-preset",
            "glyphrush-v0-native-text",
        ])
        .output()
        .expect("run glyphrush feature-parity with unchecked native-text speed advantage evidence");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity failure output is json");

    assert_eq!(
        json["readiness"]["native_text_speed_advantage_ready"],
        false
    );
    assert_eq!(
        json["readiness"]["native_text_speed_advantage_blockers"],
        serde_json::json!(["baseline_quality_not_checked"])
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("native-text speed advantage evidence")
    );
}

#[test]
fn feature_parity_speed_evidence_gate_fails_when_liteparse_claim_is_missing() {
    let dir = temp_dir("feature-parity-bench-evidence-missing");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 3.2,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "feature-parity",
            "--bench-report",
            report_path.to_str().unwrap(),
            "--require-speed-evidence",
        ])
        .output()
        .expect("run glyphrush feature-parity with incomplete benchmark evidence");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity failure output is json");

    assert_eq!(json["benchmark_evidence"]["evidence_passed"], false);
    assert_eq!(
        json["benchmark_evidence"]["missing_required_claims"],
        serde_json::json!(["liteparse-no-ocr"])
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("quality-backed LiteParse claims"));
}

#[test]
fn feature_parity_speed_evidence_rejects_claim_when_actual_speedup_misses_threshold() {
    let dir = temp_dir("feature-parity-bench-evidence-slow-actual");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 1.2,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            },
            {
              "baseline": "liteparse-no-ocr",
              "required_glyphrush_speedup": 1.5,
              "actual_glyphrush_speedup": 1.8,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "feature-parity",
            "--bench-report",
            report_path.to_str().unwrap(),
            "--require-speed-evidence",
        ])
        .output()
        .expect("run glyphrush feature-parity with inconsistent speed evidence");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity failure output is json");

    assert_eq!(json["benchmark_evidence"]["evidence_passed"], false);
    assert_eq!(
        json["benchmark_evidence"]["failed_required_claims"][0]["baseline"],
        "liteparse"
    );
    assert_eq!(
        json["benchmark_evidence"]["failed_required_claims"][0]["actual_glyphrush_speedup"],
        1.2
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("quality-backed LiteParse claims"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn feature_parity_preserves_speed_claim_quality_diagnostics_from_saved_bench_report() {
    let dir = temp_dir("feature-parity-bench-claim-quality-diagnostics");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 80.0,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": false,
              "baseline_quality_passed": false,
              "glyphrush_quality_backed": true,
              "quality_backed": false,
              "quality_blocker": "baseline_quality_not_checked",
              "claim_passed": false,
              "status": "quality_not_checked"
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
    ]);
    let claim = &json["benchmark_evidence"]["claims"][0];

    assert_eq!(claim["baseline"], "liteparse");
    assert_eq!(claim["speed_passed"], true);
    assert_eq!(claim["glyphrush_quality_checked"], true);
    assert_eq!(claim["glyphrush_quality_passed"], true);
    assert_eq!(claim["baseline_quality_checked"], false);
    assert_eq!(claim["baseline_quality_passed"], false);
    assert_eq!(claim["glyphrush_quality_backed"], true);
    assert_eq!(claim["quality_backed"], false);
    assert_eq!(claim["quality_blocker"], "baseline_quality_not_checked");
    assert_eq!(claim["claim_passed"], false);
    assert_eq!(claim["status"], "quality_not_checked");
}

#[test]
fn feature_parity_surfaces_baseline_quality_failures_from_saved_bench_report() {
    let dir = temp_dir("feature-parity-bench-baseline-quality-failures");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 64.0,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": false,
              "glyphrush_quality_backed": true,
              "quality_backed": false,
              "quality_blocker": "baseline_quality_failed",
              "claim_passed": false,
              "status": "quality_failed"
            }
          ],
          "baselines": [
            {
              "name": "liteparse",
              "target": "run-llama/liteparse",
              "quality_status": "checked",
              "quality_failed_documents": 2,
              "quality_failed_checks": 3,
              "quality_category_summaries": {
                "clean_digital": {
                  "document_count": 1,
                  "page_count": 4,
                  "failed_documents": 0,
                  "failed_checks": 0,
                  "quality_passed": true,
                  "quality_failed": false
                },
                "forms": {
                  "document_count": 1,
                  "page_count": 2,
                  "failed_documents": 1,
                  "failed_checks": 1,
                  "quality_passed": false,
                  "quality_failed": true
                },
                "large": {
                  "document_count": 1,
                  "page_count": 297,
                  "failed_documents": 1,
                  "failed_checks": 2,
                  "quality_passed": false,
                  "quality_failed": true
                }
              },
              "quality_failure_samples": [
                {
                  "path": "forms/irs-f1040-2025.pdf",
                  "failed_checks": 1,
                  "failed_check_types": ["required_text"]
                },
                {
                  "path": "large/nasa-systems-engineering-handbook.pdf",
                  "failed_checks": 2,
                  "failed_check_types": ["required_text", "reading_order"]
                }
              ]
            },
            {
              "name": "pymupdf",
              "target": "PyMuPDF",
              "quality_status": "checked",
              "quality_failed_documents": 0,
              "quality_failed_checks": 0,
              "quality_category_summaries": {},
              "quality_failure_samples": []
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
    ]);

    assert_eq!(
        json["benchmark_evidence"]["baseline_quality_failures"],
        serde_json::json!([
            {
                "baseline": "liteparse",
                "target": "run-llama/liteparse",
                "quality_status": "checked",
                "quality_failed_documents": 2,
                "quality_failed_checks": 3,
                "failed_categories": [
                    {
                        "category": "forms",
                        "document_count": 1,
                        "page_count": 2,
                        "failed_documents": 1,
                        "failed_checks": 1
                    },
                    {
                        "category": "large",
                        "document_count": 1,
                        "page_count": 297,
                        "failed_documents": 1,
                        "failed_checks": 2
                    }
                ],
                "failure_samples": [
                    {
                        "path": "forms/irs-f1040-2025.pdf",
                        "failed_checks": 1,
                        "failed_check_types": ["required_text"]
                    },
                    {
                        "path": "large/nasa-systems-engineering-handbook.pdf",
                        "failed_checks": 2,
                        "failed_check_types": ["required_text", "reading_order"]
                    }
                ]
            }
        ])
    );
}

#[test]
fn feature_parity_derives_speed_claim_quality_diagnostics_from_legacy_bench_report() {
    let dir = temp_dir("feature-parity-legacy-bench-claim-quality-diagnostics");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 80.0,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": false,
              "baseline_quality_passed": false,
              "quality_backed": false,
              "claim_passed": false,
              "status": "quality_not_checked"
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
    ]);
    let claim = &json["benchmark_evidence"]["claims"][0];

    assert_eq!(claim["baseline"], "liteparse");
    assert_eq!(claim["speed_passed"], true);
    assert_eq!(claim["glyphrush_quality_checked"], true);
    assert_eq!(claim["glyphrush_quality_passed"], true);
    assert_eq!(claim["baseline_quality_checked"], false);
    assert_eq!(claim["baseline_quality_passed"], false);
    assert_eq!(claim["glyphrush_quality_backed"], true);
    assert_eq!(claim["quality_backed"], false);
    assert_eq!(claim["quality_blocker"], "baseline_quality_not_checked");
    assert_eq!(claim["claim_passed"], false);
    assert_eq!(claim["status"], "quality_not_checked");
}

#[test]
fn feature_parity_derives_unchecked_baseline_quality_categories_from_legacy_bench_report() {
    let dir = temp_dir("feature-parity-legacy-unchecked-baseline-quality-categories");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "quality": {
            "documents": [
              {
                "path": "v0/clean_digital/clean.pdf",
                "category": "clean_digital",
                "document_fingerprint": "clean-fingerprint",
                "page_count": 2
              },
              {
                "path": "v0/scanned/scan.pdf",
                "category": "scanned",
                "document_fingerprint": "scan-fingerprint",
                "page_count": 6
              }
            ]
          },
          "documents": [
            {
              "path": "clean_digital/clean.pdf",
              "document_fingerprint": "clean-fingerprint",
              "page_count": 2,
              "baselines": [
                {
                  "name": "liteparse",
                  "quality_status": "checked",
                  "quality": {"passed": true}
                }
              ]
            },
            {
              "path": "scanned/scan.pdf",
              "document_fingerprint": "scan-fingerprint",
              "page_count": 6,
              "baselines": [
                {
                  "name": "liteparse",
                  "quality_status": "not_checked_no_expectations",
                  "quality": null
                },
                {
                  "name": "liteparse-no-ocr",
                  "quality_status": "not_checked_timed_out",
                  "quality": null
                }
              ]
            }
          ],
          "speedup_claims": []
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "feature-parity",
        "--bench-report",
        report_path.to_str().unwrap(),
    ]);

    assert_eq!(
        json["benchmark_evidence"]["baseline_quality_unchecked_categories"],
        serde_json::json!([
            {
                "baseline": "liteparse",
                "category": "scanned",
                "document_count": 1,
                "page_count": 6,
                "not_checked_no_expectations_documents": 1,
                "not_checked_timed_out_documents": 0,
                "not_checked_execution_failed_documents": 0
            },
            {
                "baseline": "liteparse-no-ocr",
                "category": "scanned",
                "document_count": 1,
                "page_count": 6,
                "not_checked_no_expectations_documents": 0,
                "not_checked_timed_out_documents": 1,
                "not_checked_execution_failed_documents": 0
            }
        ])
    );
}

#[test]
fn feature_parity_reports_invalid_saved_benchmark_before_failing_speed_evidence_gate() {
    let dir = temp_dir("feature-parity-invalid-bench-report");
    let report_path = dir.join("bench.json");
    fs::write(&report_path, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "feature-parity",
            "--bench-report",
            report_path.to_str().unwrap(),
            "--require-speed-evidence",
        ])
        .output()
        .expect("run glyphrush feature-parity with invalid benchmark report");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity failure output is json");

    assert_eq!(json["benchmark_evidence"]["report_valid"], false);
    assert_eq!(
        json["benchmark_evidence"]["report_error"]["kind"],
        "decode_error"
    );
    assert_eq!(
        json["benchmark_evidence"]["report_error"]["message"],
        "EOF while parsing a value at line 1 column 0"
    );
    assert_eq!(json["benchmark_evidence"]["evidence_passed"], false);
    assert_eq!(
        json["readiness"]["native_text_speed_claim_blockers"],
        serde_json::json!(["invalid_benchmark_report", "missing_coverage_preset"])
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("quality-backed LiteParse claims"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn feature_parity_coverage_preset_gate_fails_when_benchmark_categories_are_missing() {
    let dir = temp_dir("feature-parity-coverage-preset-missing");
    let report_path = dir.join("bench.json");
    fs::write(
        &report_path,
        r#"{
          "report_version": "glyphrush-bench-report-v1",
          "backend": "pdfium",
          "quality_status": "checked",
          "quality": {
            "category_summaries": {
              "clean_digital": {
                "document_count": 2,
                "page_count": 12,
                "failed_checks": 0,
                "quality_passed": true
              },
              "scanned": {
                "document_count": 1,
                "page_count": 3,
                "failed_checks": 0,
                "quality_passed": true
              }
            }
          },
          "speedup_claims": [
            {
              "baseline": "liteparse",
              "required_glyphrush_speedup": 2.0,
              "actual_glyphrush_speedup": 3.2,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            },
            {
              "baseline": "liteparse-no-ocr",
              "required_glyphrush_speedup": 1.5,
              "actual_glyphrush_speedup": 1.8,
              "speed_comparable": true,
              "speed_passed": true,
              "glyphrush_quality_checked": true,
              "glyphrush_quality_passed": true,
              "baseline_quality_checked": true,
              "baseline_quality_passed": true,
              "quality_backed": true,
              "claim_passed": true,
              "status": "passed"
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "feature-parity",
            "--bench-report",
            report_path.to_str().unwrap(),
            "--require-speed-evidence",
            "--require-coverage-preset",
            "glyphrush-v0",
        ])
        .output()
        .expect("run glyphrush feature-parity with missing coverage preset categories");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity failure output is json");

    assert_eq!(json["benchmark_evidence"]["evidence_passed"], true);
    assert_eq!(
        json["benchmark_evidence"]["coverage_requirement"],
        serde_json::json!({
            "preset": "glyphrush-v0",
            "required": true,
            "required_categories": [
                "clean_digital",
                "scanned",
                "hybrid",
                "academic_columns",
                "tables",
                "forms",
                "rotated",
                "weird_encoding",
                "large"
            ],
            "present_categories": ["clean_digital", "scanned"],
            "missing_categories": [
                "hybrid",
                "academic_columns",
                "tables",
                "forms",
                "rotated",
                "weird_encoding",
                "large"
            ],
            "passed": false
        })
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("coverage preset glyphrush-v0"));
}

#[cfg(feature = "pdfium")]
#[cfg(feature = "pdfium")]
#[test]
fn feature_parity_counts_pdfium_ocr_runtime_caps_and_cache_as_implemented() {
    let json = glyphrush(&["--backend", "pdfium", "feature-parity"]);

    assert_eq!(json["selected_backend"], "pdfium");
    assert_eq!(json["summary"]["implemented"], 11);
    assert_eq!(json["summary"]["partial"], 0);
    assert_eq!(json["summary"]["planned"], 0);
    assert_eq!(
        json["readiness"]["remaining_partial"],
        serde_json::json!([])
    );
    assert_eq!(
        json["readiness"]["remaining_planned"],
        serde_json::json!([])
    );

    let capabilities = json["capabilities"].as_array().unwrap();
    let page_render = capability(capabilities, "page_render_for_ocr");
    assert_eq!(page_render["glyphrush_status"], "implemented");
    assert_eq!(
        page_render["quality_guard"],
        "rendered_image_ocr_check_and_render_page_fallback_counts"
    );
    assert!(
        page_render["notes"]
            .as_str()
            .unwrap()
            .contains("PDFium renders only OCR-routed pages")
    );

    let ocr = capability(capabilities, "ocr");
    assert_eq!(ocr["glyphrush_status"], "implemented");
}

#[test]
fn ci_workflow_enables_pdfium_speed_path_verification() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let workflow =
        fs::read_to_string(repo_root.join(".github/workflows/ci.yml")).expect("read CI workflow");

    assert!(workflow.contains("GLYPHRUSH_VERIFY_PDFIUM: \"1\""));
    assert!(workflow.contains("bash scripts/verify.sh"));
}

#[cfg(feature = "pdfium")]
#[test]
fn liteparse_wrapper_uses_project_local_lit_install() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = temp_dir("baseline-local-lit");
    let pdf_path = root.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Local LiteParse")).unwrap();
    let lit = root
        .join(".glyphrush-baselines")
        .join("node_modules")
        .join(".bin")
        .join("lit");
    let tessdata = root.join(".glyphrush-baselines").join("tessdata");
    fs::create_dir_all(&tessdata).unwrap();
    fs::write(tessdata.join("eng.traineddata"), "fake tessdata").unwrap();
    write_executable(
        &lit,
        "#!/bin/sh\nprintf 'local lit:'\nprintf ' %s' \"$@\"\nprintf ' tessdata=%s\\n' \"${TESSDATA_PREFIX:-unset}\"\n",
    );

    let output = Command::new(workspace_root.join("tools/baselines/liteparse-text.sh"))
        .env("GLYPHRUSH_BASELINE_ROOT", &root)
        .env_remove("LITEPARSE_BIN")
        .arg(&pdf_path)
        .output()
        .expect("run liteparse wrapper with project-local lit");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("local lit: parse --format text --quiet"));
    assert!(stdout.contains(pdf_path.to_str().unwrap()));
    assert!(stdout.contains(&format!("tessdata={}", tessdata.display())));
}
