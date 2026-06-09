use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

#[cfg(feature = "pdfium")]
#[derive(Debug)]
struct RenderedOcrHttpObservation {
    request: String,
    rendered_image_path: Option<String>,
    image_existed: bool,
    header: Option<String>,
    bytes: Option<usize>,
}

#[test]
fn inspect_reports_pdf_page_count_and_fingerprint() {
    let pdf_path = write_test_pdf("inspect", "Hello Glyphrush");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "inspect", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush inspect");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("inspect output is json");
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
    assert_eq!(json["page_count"], 1);
    assert!(json["document_fingerprint"].as_str().unwrap().len() >= 12);
}

#[test]
fn inspect_accepts_explicit_backend_selection() {
    let pdf_path = write_test_pdf("inspect-backend", "Hello Backend");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "inspect", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush inspect with backend selection");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("inspect output is json");

    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["page_count"], 1);
}

#[test]
fn backend_check_reports_lopdf_and_pending_pdfium_mupdf_candidates() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "backend-check"])
        .output()
        .expect("run glyphrush backend-check");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");

    assert_eq!(json["report_version"], "glyphrush-backend-check-report-v1");
    assert_eq!(json["selected_backend"], "lopdf");
    assert_eq!(
        json["enabled_backend_count"],
        if cfg!(feature = "pdfium") { 2 } else { 1 }
    );
    assert_eq!(json["candidate_backend_count"], 3);
    assert_eq!(
        json["decision_gate"],
        "pdfium_mupdf_spike_required_before_backend_lock_in"
    );

    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 3);
    assert_eq!(backends[0]["name"], "lopdf");
    assert_eq!(backends[0]["status"], "enabled");
    assert_eq!(backends[0]["selected"], true);
    assert_eq!(backends[0]["version"], "lopdf-adapter-v0");
    assert_eq!(backends[0]["capabilities"]["open_pdf"], true);
    assert_eq!(backends[0]["capabilities"]["native_text"], true);
    assert_eq!(
        backends[0]["capabilities"]["span_geometry"],
        "bounded_simple_text"
    );
    assert_eq!(backends[0]["capabilities"]["image_metadata"], true);
    assert_eq!(backends[0]["capabilities"]["render_pages"], false);
    assert_eq!(backends[0]["capabilities"]["builtin_ocr"], false);
    assert_eq!(backends[1]["name"], "pdfium");
    assert_eq!(
        backends[1]["status"],
        if cfg!(feature = "pdfium") {
            "enabled"
        } else {
            "not_wired"
        }
    );
    assert_eq!(backends[1]["selected"], false);
    if cfg!(feature = "pdfium") {
        assert_eq!(backends[1]["capabilities"]["render_pages"], true);
        assert_eq!(backends[1]["capabilities"]["builtin_ocr"], false);
    }
    assert_eq!(backends[2]["name"], "mupdf");
    assert_eq!(backends[2]["status"], "not_wired");
    assert_eq!(backends[2]["selected"], false);
}

#[test]
fn backend_auto_selects_fastest_enabled_backend() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "auto", "backend-check"])
        .output()
        .expect("run glyphrush backend-check with auto backend");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");

    assert_eq!(
        json["selected_backend"],
        if cfg!(feature = "pdfium") {
            "pdfium"
        } else {
            "lopdf"
        }
    );
}

#[test]
fn default_backend_selects_fastest_enabled_backend() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["backend-check"])
        .output()
        .expect("run glyphrush backend-check with default backend");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");

    assert_eq!(
        json["selected_backend"],
        if cfg!(feature = "pdfium") {
            "pdfium"
        } else {
            "lopdf"
        }
    );
}

#[test]
fn feature_parity_reports_liteparse_capability_gaps() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "feature-parity"])
        .output()
        .expect("run glyphrush feature-parity");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity output is json");

    assert_eq!(json["report_version"], "glyphrush-feature-parity-report-v1");
    assert_eq!(json["comparison_target"], "liteparse");
    assert_eq!(json["selected_backend"], "lopdf");
    assert_eq!(
        json["run_metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["summary"]["target_capability_count"], 13);
    assert_eq!(json["summary"]["implemented"], 7);
    assert_eq!(json["summary"]["partial"], 3);
    assert_eq!(json["summary"]["planned"], 2);
    assert_eq!(json["summary"]["not_planned"], 1);
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
        "bench --eval-manifest <manifest> --baseline-preset glyphrush-v0 --require-coverage-preset glyphrush-v0 --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5"
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
        10
    );
    assert_eq!(json["readiness"]["liteparse_capabilities"]["target"], 13);
    assert_eq!(
        json["readiness"]["remaining_partial"],
        serde_json::json!([
            "span_geometry_layout",
            "page_render_for_ocr",
            "table_recovery"
        ])
    );
    assert_eq!(
        json["readiness"]["remaining_planned"],
        serde_json::json!(["wasm_bindings", "mupdf_backend"])
    );
    assert_eq!(
        json["readiness"]["not_planned_by_design"],
        serde_json::json!(["bundled_builtin_ocr"])
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
    assert_eq!(span_geometry["glyphrush_status"], "partial");
    assert!(
        span_geometry["notes"]
            .as_str()
            .unwrap()
            .contains("clearly separated 2-4 column reading order")
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

    let cache = capability(capabilities, "artifact_cache_snapshots");
    assert_eq!(cache["glyphrush_status"], "implemented");
    assert_eq!(
        cache["glyphrush"],
        "cache_dir_snapshot_envelope_artifact_reuse"
    );

    let table_recovery = capability(capabilities, "table_recovery");
    assert_eq!(table_recovery["glyphrush_status"], "partial");
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
            .contains("leading text-table captions outside table grids")
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
    assert_eq!(wasm["glyphrush_status"], "planned");
    assert_eq!(wasm["glyphrush"], "wasm_wrapper_planned_over_native_core");

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
        .expect("run glyphrush feature-parity with benchmark evidence");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity output is json");

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
        .expect("run glyphrush feature-parity with complete benchmark evidence");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity output is json");

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
#[test]
fn feature_parity_counts_pdfium_ocr_runtime_caps_and_cache_as_implemented() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "pdfium", "feature-parity"])
        .output()
        .expect("run glyphrush feature-parity with pdfium backend");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("feature-parity output is json");

    assert_eq!(json["selected_backend"], "pdfium");
    assert_eq!(json["summary"]["implemented"], 8);
    assert_eq!(json["summary"]["partial"], 2);
    assert_eq!(json["summary"]["planned"], 2);
    assert_eq!(
        json["readiness"]["remaining_partial"],
        serde_json::json!(["span_geometry_layout", "table_recovery"])
    );
    assert_eq!(
        json["readiness"]["remaining_planned"],
        serde_json::json!(["wasm_bindings", "mupdf_backend"])
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
fn liteparse_benchmark_gate_script_dry_run_uses_quality_backed_pdfium_command() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_JOBS", "3")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-gate.json",
        )
        .output()
        .expect("run bench-liteparse dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[0].contains("--pdf test/"));
    assert!(lines[0].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[0].contains("--strict"));
    assert!(lines[1].contains("cargo run -q --release -p glyphrush-cli"));
    assert!(lines[1].contains("--features pdfium"));
    assert!(lines[1].contains("--backend pdfium"));
    assert!(lines[1].contains("bench test/"));
    assert!(lines[1].contains("--eval-manifest test/corpus.datasheets.json"));
    assert!(lines[1].contains("--eval-category datasheet"));
    assert!(lines[1].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[1].contains("--require-baselines"));
    assert!(lines[1].contains("--require-baseline-quality"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
    assert!(lines[1].contains("--require-speedup-claim liteparse=2.0"));
    assert!(lines[1].contains("--require-speedup-claim liteparse-no-ocr=1.5"));
    assert!(lines[1].contains("--jobs 3"));
    assert!(lines[1].contains("> /tmp/glyphrush-liteparse-gate.json"));
    assert!(lines[2].contains("feature-parity"));
    assert!(lines[2].contains("--bench-report /tmp/glyphrush-liteparse-gate.json"));
    assert!(lines[2].contains("--require-speed-evidence"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_checks_saved_speed_report_without_coverage_gate() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-datasheet.json",
        )
        .output()
        .expect("run bench-liteparse dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[1].contains("bench test/"));
    assert!(lines[1].contains("> /tmp/glyphrush-liteparse-datasheet.json"));
    assert!(lines[2].contains("feature-parity"));
    assert!(lines[2].contains("--bench-report /tmp/glyphrush-liteparse-datasheet.json"));
    assert!(lines[2].contains("--require-speed-evidence"));
    assert!(!lines[2].contains("--require-coverage-preset"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_use_all_manifest_categories() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.full.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-full-coverage.json",
        )
        .output()
        .expect("run bench-liteparse dry run for all categories");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[1].contains("bench test/"));
    assert!(lines[1].contains("--eval-manifest test/corpus.full.json"));
    assert!(!lines[1].contains("--eval-category"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_defaults_v0_manifest_to_v0_pdf_root() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-v0-coverage.json",
        )
        .output()
        .expect("run bench-liteparse dry run for v0 manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(
        lines[0].contains("baseline-check"),
        "v0 gate should still preflight baseline wrapper availability:\n{stdout}"
    );
    assert!(
        !lines[0].contains("--pdf"),
        "v0 gate should not duplicate full baseline parsing before the benchmark:\n{stdout}"
    );
    assert!(
        !lines[0].contains("--baseline-timeout-ms"),
        "describe-only v0 preflight should not need the large corpus baseline timeout:\n{stdout}"
    );
    assert!(
        lines[1].contains("bench test/v0"),
        "benchmark should run only the v0 corpus root:\n{stdout}"
    );
    assert!(lines[1].contains("--eval-manifest test/corpus.v0.json"));
    assert!(!lines[1].contains("--eval-category"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
    assert!(lines[1].contains("--baseline-timeout-ms 900000"));
    assert!(lines[1].contains("2> >(tee"));
    assert!(lines[1].contains("/tmp/glyphrush-liteparse-v0-coverage.progress.log"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_override_progress_log() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env("GLYPHRUSH_BENCH_OUTPUT", "/tmp/glyphrush-liteparse-v0.json")
        .env(
            "GLYPHRUSH_BENCH_PROGRESS_LOG",
            "/tmp/custom-glyphrush-progress.log",
        )
        .output()
        .expect("run bench-liteparse dry run for custom progress log");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[1].contains("> /tmp/glyphrush-liteparse-v0.json"));
    assert!(lines[1].contains("2> >(tee"));
    assert!(lines[1].contains("/tmp/custom-glyphrush-progress.log"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_skip_preflight_for_long_v0_runs() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_PREFLIGHT", "none")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-v0-coverage.json",
        )
        .output()
        .expect("run bench-liteparse dry run with preflight disabled");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "dry-run output:\n{stdout}");
    assert!(
        !lines[0].contains("baseline-check"),
        "preflight=none should start with the real benchmark command:\n{stdout}"
    );
    assert!(lines[0].contains("bench test/v0"));
    assert!(lines[0].contains("--baseline-timeout-ms 900000"));
    assert!(lines[1].contains("feature-parity"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_probe_one_stalled_v0_pdf() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env(
            "GLYPHRUSH_BENCH_PROBE_PDF",
            "test/v0/academic_columns/acl-bert-naacl-2019.pdf",
        )
        .env("GLYPHRUSH_BENCH_OUTPUT", "/tmp/glyphrush-v0-probe.json")
        .output()
        .expect("run bench-liteparse probe dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "probe dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[0].contains("--pdf test/v0/academic_columns/acl-bert-naacl-2019.pdf"));
    assert!(lines[0].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[0].contains("--baseline-timeout-ms 60000"));
    assert!(lines[0].contains("--strict"));
    assert!(lines[0].contains("> /tmp/glyphrush-v0-probe.json"));
    assert!(
        !lines[0].contains("feature-parity"),
        "probe output should not ask feature-parity to read a baseline-check report:\n{stdout}"
    );
    assert!(
        !lines[0].contains("bench test/v0"),
        "probe output should avoid the full v0 benchmark command:\n{stdout}"
    );
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_probe_one_baseline() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env(
            "GLYPHRUSH_BENCH_PROBE_PDF",
            "test/v0/academic_columns/acl-bert-naacl-2019.pdf",
        )
        .env("GLYPHRUSH_BENCH_PROBE_BASELINE", "liteparse")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-v0-liteparse-probe.json",
        )
        .output()
        .expect("run bench-liteparse single-baseline probe dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "probe dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[0].contains("--pdf test/v0/academic_columns/acl-bert-naacl-2019.pdf"));
    assert!(lines[0].contains("--baseline liteparse=tools/baselines/liteparse-text.sh"));
    assert!(!lines[0].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[0].contains("--baseline-timeout-ms 60000"));
    assert!(lines[0].contains("> /tmp/glyphrush-v0-liteparse-probe.json"));
}

#[test]
fn verify_script_dry_run_exposes_opt_in_pdfium_speed_path_gate() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/verify.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_VERIFY_PDFIUM", "1")
        .output()
        .expect("run verify dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo fmt --all -- --check"));
    assert!(stdout.contains("cargo test --workspace"));
    assert!(stdout.contains("cargo clippy --workspace --all-targets -- -D warnings"));
    assert!(stdout.contains("cargo test -p glyphrush-cli --features pdfium"));
    assert!(
        stdout.contains("feature_parity_counts_pdfium_ocr_runtime_caps_and_cache_as_implemented")
    );
    assert!(
        stdout
            .contains("parse_pdfium_ocr_command_rendered_image_invokes_adapter_only_for_ocr_pages")
    );
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
fn backend_check_reports_feature_gated_pdfium_backend() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "pdfium", "backend-check"])
        .output()
        .expect("run glyphrush backend-check with pdfium backend");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");

    assert_eq!(json["selected_backend"], "pdfium");
    assert_eq!(json["enabled_backend_count"], 2);

    let backends = json["backends"].as_array().unwrap();
    let lopdf = backends
        .iter()
        .find(|backend| backend["name"] == "lopdf")
        .expect("lopdf backend candidate exists");
    assert_eq!(lopdf["version"], "lopdf-adapter-v0");

    let pdfium = backends
        .iter()
        .find(|backend| backend["name"] == "pdfium")
        .expect("pdfium backend candidate exists");
    assert_eq!(pdfium["status"], "enabled");
    assert_eq!(pdfium["selected"], true);
    assert_eq!(pdfium["version"], "pdfium-adapter-v1");
    assert_eq!(pdfium["capabilities"]["open_pdf"], true);
    assert_eq!(pdfium["capabilities"]["page_count"], true);
    assert_eq!(pdfium["capabilities"]["native_text"], true);
    assert_eq!(
        pdfium["capabilities"]["span_geometry"],
        "pdfium_text_segments"
    );
    assert_eq!(pdfium["capabilities"]["image_metadata"], true);
    assert_eq!(pdfium["capabilities"]["render_pages"], true);
    assert_eq!(pdfium["capabilities"]["builtin_ocr"], false);
}

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "pdfium",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush pdfium parse with span geometry");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");
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
fn pdfium_backend_flags_ruled_table_vector_paths() {
    let dir = temp_dir("pdfium-ruled-table");
    let pdf_path = dir.join("table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "pdfium",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush pdfium debug-page");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug-page output is json");

    assert!(
        json["signals"]["table_line_density"].as_f64().unwrap() >= 0.25,
        "signals: {}",
        json["signals"]
    );
    assert_eq!(json["decision"]["run_table_recovery"], true);
    assert_eq!(json["decision"]["reasons"][0], "table_line_density");
    assert!(
        json["quality"]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag == "table_uncertain")
    );
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

    let native = run_json([
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
    let scan = run_json([
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

    let native = run_json([
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
    let scan = run_json([
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
fn ocr_check_pdfium_rendered_image_command_preflights_rendered_page() {
    let dir = temp_dir("ocr-check-pdfium-rendered-image");
    let pdf_path = dir.join("scan.pdf");
    let log_path = dir.join("rendered-ocr.log");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("")).unwrap();
    let command = write_rendered_ocr_command_script("ocr-check-pdfium-rendered-image", &log_path);

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
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
        ])
        .output()
        .expect("run glyphrush pdfium ocr-check with rendered-image command");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

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
fn backend_check_smoke_pdf_reports_selected_backend_extraction_summary() {
    let pdf_path = write_test_pdf("backend-check-smoke", "Backend smoke text");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "backend-check",
            "--pdf",
            pdf_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush backend-check --pdf");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");
    let smoke = &json["smoke"];

    assert_eq!(smoke["backend"], "lopdf");
    assert_eq!(smoke["success"], true);
    assert_eq!(smoke["page_count"], 1);
    assert_eq!(smoke["extracted_page_count"], 1);
    assert_eq!(
        smoke["source_size_bytes"],
        fs::metadata(&pdf_path).unwrap().len()
    );
    assert!(smoke["document_fingerprint"].as_str().unwrap().len() >= 12);
    assert!(smoke["wall_us"].as_u64().unwrap() > 0);
    assert!(
        smoke["native_text_bytes"].as_u64().unwrap() >= "Backend smoke text".len() as u64,
        "smoke summary should include extracted native text bytes: {smoke}"
    );
    assert_eq!(smoke["ocr_required_pages"], 0);
    assert_eq!(smoke["image_artifact_count"], 0);
}

#[test]
fn backend_check_smoke_pdf_classifies_encrypted_input_without_losing_source_identity() {
    let dir = temp_dir("backend-check-encrypted");
    let pdf_path = dir.join("encrypted.pdf");
    fs::write(&pdf_path, minimal_encrypted_pdf("Encrypted smoke text")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "backend-check",
            "--pdf",
            pdf_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush backend-check --pdf encrypted");

    assert!(
        !output.status.success(),
        "encrypted smoke should fail the command after writing JSON"
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");
    let smoke = &json["smoke"];

    assert_eq!(smoke["mode"], "single_pdf");
    assert_eq!(smoke["success"], false);
    assert_eq!(smoke["error_kind"], "encrypted_pdf_requires_password");
    assert!(
        smoke["error"]
            .as_str()
            .unwrap()
            .contains("encrypted PDFs are not supported"),
        "error should explain encrypted unsupported input: {smoke}"
    );
    assert_eq!(
        smoke["source_size_bytes"],
        fs::metadata(&pdf_path).unwrap().len()
    );
    assert_eq!(smoke["document_fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(smoke["page_count"], Value::Null);
}

#[test]
fn backend_check_smoke_directory_reports_sorted_corpus_summary() {
    let dir = temp_dir("backend-check-smoke-dir");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second backend smoke")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First backend smoke")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "backend-check",
            "--pdf",
            dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush backend-check --pdf directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");
    let smoke = &json["smoke"];

    assert_eq!(smoke["mode"], "directory");
    assert_eq!(smoke["backend"], "lopdf");
    assert_eq!(smoke["success"], true);
    assert_eq!(smoke["document_count"], 2);
    assert_eq!(smoke["successful_documents"], 2);
    assert_eq!(smoke["failed_documents"], 0);
    assert_eq!(smoke["page_count"], 2);
    assert_eq!(smoke["extracted_page_count"], 2);
    assert!(
        smoke["native_text_bytes"].as_u64().unwrap() >= "First backend smoke".len() as u64,
        "directory smoke summary should aggregate native text bytes: {smoke}"
    );

    let documents = smoke["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 2);
    assert_eq!(documents[0]["path"], "a.pdf");
    assert_eq!(documents[0]["mode"], "single_pdf");
    assert_eq!(documents[0]["success"], true);
    assert_eq!(documents[0]["page_count"], 1);
    assert_eq!(documents[1]["path"], "b.pdf");
    assert_eq!(documents[1]["mode"], "single_pdf");
    assert_eq!(documents[1]["success"], true);
    assert_eq!(documents[1]["page_count"], 1);
}

#[test]
fn backend_check_smoke_directory_reports_failure_samples_with_error_kinds() {
    let dir = temp_dir("backend-check-smoke-dir-failures");
    fs::write(dir.join("a.pdf"), minimal_pdf("Passing backend smoke")).unwrap();
    fs::write(
        dir.join("b.pdf"),
        minimal_encrypted_pdf("Encrypted directory smoke"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "backend-check",
            "--pdf",
            dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush backend-check --pdf mixed directory");

    assert!(
        !output.status.success(),
        "mixed directory smoke should fail after writing JSON"
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");
    let smoke = &json["smoke"];

    assert_eq!(smoke["mode"], "directory");
    assert_eq!(smoke["success"], false);
    assert_eq!(smoke["document_count"], 2);
    assert_eq!(smoke["successful_documents"], 1);
    assert_eq!(smoke["failed_documents"], 1);
    assert_eq!(smoke["error"], "1 backend smoke document(s) failed");

    let failure_samples = smoke["failure_samples"].as_array().unwrap();
    assert_eq!(failure_samples.len(), 1);
    assert_eq!(failure_samples[0]["path"], "b.pdf");
    assert_eq!(
        failure_samples[0]["error_kind"],
        "encrypted_pdf_requires_password"
    );
    assert!(
        failure_samples[0]["error"]
            .as_str()
            .unwrap()
            .contains("encrypted PDFs are not supported"),
        "sample should preserve the concrete backend failure: {smoke}"
    );
}

#[test]
fn backend_check_smoke_directory_jobs_preserve_sorted_corpus_summary() {
    let dir = temp_dir("backend-check-smoke-dir-jobs");
    fs::write(dir.join("c.pdf"), minimal_pdf("Third backend smoke jobs")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First backend smoke jobs")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Second backend smoke jobs")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "backend-check",
            "--pdf",
            dir.to_str().unwrap(),
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush backend-check --pdf directory with jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("backend-check output is json");
    let smoke = &json["smoke"];

    assert_eq!(smoke["mode"], "directory");
    assert_eq!(smoke["success"], true);
    assert_eq!(smoke["worker_count"], 2);
    assert_eq!(smoke["document_count"], 3);
    assert_eq!(smoke["page_count"], 3);
    assert_eq!(smoke["extracted_page_count"], 3);
    assert_eq!(
        smoke["documents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|document| document["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["a.pdf", "b.pdf", "c.pdf"]
    );
}

#[test]
fn inspect_pages_reports_page_level_quality_triage() {
    let dir = temp_dir("inspect-pages");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("tiny")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            pdf_path.to_str().unwrap(),
            "--pages",
        ])
        .output()
        .expect("run glyphrush inspect --pages");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("inspect output is json");

    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["page_count"], 1);
    assert_eq!(json["warnings_count"], 1);
    assert_eq!(json["pages"].as_array().unwrap().len(), 1);
    assert_eq!(json["pages"][0]["page_index"], 0);
    assert!(
        json["pages"][0]["artifact_id"]
            .as_str()
            .unwrap()
            .contains(":p000000:"),
        "page summary should expose the selected artifact id: {}",
        json["pages"][0]["artifact_id"]
    );
    assert_eq!(
        json["pages"][0]["page_fingerprint"].as_str().unwrap().len(),
        64
    );
    assert_eq!(json["pages"][0]["route"], "ocr_fallback");
    assert_eq!(
        json["pages"][0]["quality_flags"],
        serde_json::json!(["requires_ocr", "low_confidence_text"])
    );
    assert_eq!(
        json["pages"][0]["reasons"],
        serde_json::json!(["high_image_coverage_with_sparse_native_text"])
    );
    assert_eq!(json["pages"][0]["native_span_count"], 1);
    assert!(
        json["pages"][0]["native_text_bytes"].as_u64().unwrap() > 0,
        "expected sparse native text bytes in page summary"
    );
    assert_eq!(json["pages"][0]["ocr_span_count"], 0);
    assert_eq!(json["pages"][0]["image_artifact_count"], 1);
    assert!(json["pages"][0]["timings"]["open_us"].as_u64().unwrap() > 0);
    assert!(json["pages"][0]["timings"]["classify_us"].as_u64().unwrap() > 0);
    assert!(
        json["pages"][0]["timings"]["native_extract_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(json["pages"][0]["timings"]["layout_us"].as_u64().unwrap() > 0);
    assert!(json["pages"][0]["timings"]["table_us"].as_u64().unwrap() > 0);
    assert_eq!(
        json["pages"][0]["warnings"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
}

#[test]
fn inspect_pages_reports_table_row_cell_triage() {
    let dir = temp_dir("inspect-pages-table-summary");
    let pdf_path = dir.join("ruled-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            pdf_path.to_str().unwrap(),
            "--pages",
        ])
        .output()
        .expect("run glyphrush inspect --pages on ruled table");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("inspect output is json");

    assert_eq!(json["pages"][0]["route"], "needs_fallback");
    assert_eq!(
        json["pages"][0]["reasons"],
        serde_json::json!(["table_line_density"])
    );
    assert_eq!(json["pages"][0]["layout"]["table_blocks"], 1);
    assert_eq!(json["pages"][0]["layout"]["table_rows"], 3);
    assert_eq!(json["pages"][0]["layout"]["table_cells"], 6);
    assert_eq!(json["pages"][0]["layout"]["table_cells_with_bbox"], 0);
}

#[test]
fn inspect_pages_jobs_report_worker_count_and_preserve_page_order() {
    let dir = temp_dir("inspect-pages-jobs");
    let pdf_path = dir.join("multi.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (First inspect jobs) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Second inspect jobs) Tj ET",
        ]),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            pdf_path.to_str().unwrap(),
            "--pages",
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush inspect --pages with jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("inspect output is json");

    assert_eq!(json["worker_count"], 2);
    assert_eq!(json["page_count"], 2);
    assert_eq!(json["pages"][0]["page_index"], 0);
    assert_eq!(json["pages"][1]["page_index"], 1);
    assert_eq!(json["pages"][0]["route"], "native_fast_path");
    assert_eq!(json["pages"][1]["route"], "native_fast_path");
}

#[test]
fn inspect_pages_with_cache_dir_reports_miss_then_hit() {
    let dir = temp_dir("inspect-pages-cache");
    let pdf_path = dir.join("cached.pdf");
    fs::write(&pdf_path, minimal_pdf("Inspect cache")).unwrap();
    let cache_dir = dir.join("cache");

    let first = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            pdf_path.to_str().unwrap(),
            "--pages",
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush inspect --pages with cache miss");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_json: Value = serde_json::from_slice(&first.stdout).expect("inspect output is json");

    let second = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            pdf_path.to_str().unwrap(),
            "--pages",
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush inspect --pages with cache hit");
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_json: Value =
        serde_json::from_slice(&second.stdout).expect("inspect output is json");

    assert_eq!(first_json["cache_status"], "miss");
    assert_eq!(second_json["cache_status"], "hit");
    assert_eq!(first_json["cache_key"], second_json["cache_key"]);
    assert_eq!(second_json["worker_count"], 1);
    assert_eq!(second_json["pages"][0]["route"], "native_fast_path");
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 1);
}

#[test]
fn manifest_generates_eval_manifest_for_single_pdf() {
    let dir = temp_dir("manifest-single");
    let pdf_path = dir.join("single.pdf");
    fs::write(&pdf_path, minimal_pdf("Manifest single")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest on one PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    let document_fingerprint = sha256_hex(fs::read(&pdf_path).unwrap());

    assert_eq!(json["manifest_version"], "glyphrush-eval-manifest-v1");
    assert_eq!(json["generator"]["parser_name"], "glyphrush");
    assert_eq!(
        json["generator"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["generator"]["backend"], "lopdf");
    assert_eq!(json["generator"]["backend_version"], "lopdf-adapter-v0");
    assert_eq!(json["generator"]["span_geometry"], false);
    assert_eq!(json["generator"]["ocr_sidecar"], false);
    assert_eq!(
        json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&json)
    );
    assert_eq!(json["documents"].as_array().unwrap().len(), 1);
    assert_eq!(json["documents"][0]["path"], "single.pdf");
    assert_eq!(
        json["documents"][0]["document_fingerprint"],
        document_fingerprint
    );
    assert_eq!(
        json["documents"][0]["source_size_bytes"],
        fs::metadata(&pdf_path).unwrap().len()
    );
    assert_eq!(
        json["documents"][0]["source_modified_unix_ms"],
        source_modified_unix_ms(&pdf_path)
    );
    assert_eq!(json["documents"][0]["expect"]["page_count"], 1);
    assert_eq!(
        json["documents"][0]["expect"]["route_counts"]["native_fast_path"],
        1
    );
    assert_eq!(
        json["documents"][0]["expect"]["quality_flag_counts"]["requires_ocr"],
        0
    );
    assert_eq!(
        json["documents"][0]["expect"]["ocr_required_classification"]["expected_pages"],
        serde_json::json!([])
    );
    assert_eq!(
        json["documents"][0]["expect"]["ocr_required_classification"]["min_precision"],
        1.0
    );
    assert_eq!(
        json["documents"][0]["expect"]["quality_flag_classification"],
        serde_json::json!([])
    );
    assert_eq!(
        json["documents"][0]["expect"]["pages"][0]["route"],
        "native_fast_path"
    );
    assert_eq!(
        json["documents"][0]["expect"]["pages"][0]["empty_text_output"],
        false
    );
    assert_eq!(
        json["documents"][0]["expect"]["silent_failures"]["max_count"],
        0
    );

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated manifest");
    assert!(
        eval_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&eval_output.stderr)
    );
}

#[test]
fn manifest_includes_page_layout_block_counts_for_eval_bootstrap() {
    let dir = temp_dir("manifest-layout-block-counts");
    let pdf_path = dir.join("layout.pdf");
    fs::write(&pdf_path, minimal_pdf("Manifest layout block")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest with layout block counts");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");

    assert_eq!(
        json["documents"][0]["expect"]["pages"][0]["layout_block_counts"],
        serde_json::json!({
          "block_count": 1,
          "paragraph_blocks": 1,
          "heading_blocks": 0,
          "list_blocks": 0,
          "table_blocks": 0,
          "figure_blocks": 0,
          "header_blocks": 0,
          "footer_blocks": 0
        })
    );
}

#[test]
fn manifest_includes_recovered_table_structure_for_eval_bootstrap() {
    let dir = temp_dir("manifest-table-structure");
    let pdf_path = dir.join("ruled-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest with recovered table structure");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");

    assert_eq!(
        json["documents"][0]["expect"]["table_structure"],
        serde_json::json!([
          {
            "page": 0,
            "expected_rows": [["Part", "Value"], ["A", "1"], ["B", "2"]],
            "min_row_recall": 1.0,
            "min_cell_recall": 1.0,
            "min_cell_f1": 1.0
          }
        ])
    );
}

#[test]
fn manifest_with_span_geometry_includes_bbox_samples_for_eval_bootstrap() {
    let dir = temp_dir("manifest-span-bbox");
    let pdf_path = dir.join("positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream("BT /F1 12 Tf 72 720 Td (Positioned sample text) Tj ET"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            pdf_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush manifest with span bbox samples");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(json["generator"]["span_geometry"], true);
    assert_eq!(
        json["documents"][0]["expect"]["span_bbox"],
        serde_json::json!([
          {
            "page": 0,
            "text": "Positioned sample text",
            "provenance": "native",
            "min_x0": 71.5,
            "max_x0": 72.5,
            "min_y0": 71.5,
            "max_y0": 72.5,
            "min_x1": 216.7,
            "max_x1": 217.7,
            "min_y1": 83.5,
            "max_y1": 84.5
          }
        ])
    );

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush eval on generated manifest with span bbox samples");
    assert!(
        eval_output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&eval_output.stdout),
        String::from_utf8_lossy(&eval_output.stderr)
    );
    let eval_json: Value =
        serde_json::from_slice(&eval_output.stdout).expect("eval output is json");
    assert_eq!(eval_json["quality_passed"], true);
    assert_eq!(eval_json["failed_checks"], 0);
    assert_eq!(
        eval_json["documents"][0]["checks"]["span_bbox.000000"]["actual"]["matched"],
        true
    );
}

#[test]
fn manifest_includes_page_identity_for_eval_bootstrap() {
    let dir = temp_dir("manifest-page-identity");
    let pdf_path = dir.join("identity.pdf");
    fs::write(&pdf_path, minimal_pdf("Manifest page identity")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest with page identity");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();
    let page = &json["documents"][0]["expect"]["pages"][0];

    assert!(
        page["artifact_id"].as_str().unwrap().contains(":p000000:"),
        "generated page expectation should pin artifact id: {}",
        page["artifact_id"]
    );
    assert_eq!(page["page_fingerprint"].as_str().unwrap().len(), 64);

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated manifest with page identity");
    assert!(
        eval_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&eval_output.stderr)
    );
}

#[test]
fn manifest_includes_page_required_text_for_eval_bootstrap() {
    let dir = temp_dir("manifest-page-required-text");
    let pdf_path = dir.join("anchors.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (First generated anchor) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Second generated anchor) Tj ET",
        ]),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest with page text anchors");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(
        json["documents"][0]["expect"]["pages"][0]["required_text"],
        serde_json::json!(["First generated anchor"])
    );
    assert_eq!(
        json["documents"][0]["expect"]["pages"][1]["required_text"],
        serde_json::json!(["Second generated anchor"])
    );

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated manifest with page text anchors");
    assert!(
        eval_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&eval_output.stderr)
    );
}

#[test]
fn manifest_page_required_text_prefers_substantive_anchor() {
    let dir = temp_dir("manifest-page-required-text-substantive");
    let pdf_path = dir.join("page-number.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 24 Tf 72 720 Td (1) Tj ET BT /F1 24 Tf 72 690 Td (Substantive generated anchor) Tj ET",
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest with page-number prefix");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");

    assert_eq!(
        json["documents"][0]["expect"]["pages"][0]["required_text"],
        serde_json::json!(["Substantive generated anchor"])
    );
}

#[test]
fn manifest_with_cache_dir_preserves_output_across_warm_runs() {
    let dir = temp_dir("manifest-cache-single");
    let pdf_path = dir.join("single.pdf");
    fs::write(&pdf_path, minimal_pdf("Manifest cache single")).unwrap();
    let cache_dir = dir.join("cache");

    let first = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            pdf_path.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush manifest with cache miss");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            pdf_path.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush manifest with cache hit");
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let first_json: Value = serde_json::from_slice(&first.stdout).expect("manifest output is json");
    let second_json: Value =
        serde_json::from_slice(&second.stdout).expect("manifest output is json");

    assert_eq!(first_json, second_json);
    assert_eq!(first_json["generator"]["ocr_sidecar"], false);
    assert_eq!(first_json["documents"][0]["path"], "single.pdf");
    assert_eq!(
        first_json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&first_json)
    );
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 1);
}

#[test]
fn manifest_generates_eval_manifest_for_directory_in_stable_order() {
    let dir = temp_dir("manifest-directory");
    fs::write(
        dir.join("b.pdf"),
        minimal_pdf_with_full_page_image_and_text("Hybrid native text"),
    )
    .unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("Native manifest")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", dir.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest on directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(json["manifest_version"], "glyphrush-eval-manifest-v1");
    assert_eq!(json["generator"]["parser_name"], "glyphrush");
    assert_eq!(json["generator"]["backend"], "lopdf");
    assert_eq!(
        json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&json)
    );
    assert_eq!(json["documents"].as_array().unwrap().len(), 2);
    assert_eq!(json["documents"][0]["path"], "a.pdf");
    assert_eq!(json["documents"][1]["path"], "b.pdf");
    assert_eq!(
        json["documents"][0]["document_fingerprint"],
        sha256_hex(fs::read(dir.join("a.pdf")).unwrap())
    );
    assert_eq!(
        json["documents"][1]["document_fingerprint"],
        sha256_hex(fs::read(dir.join("b.pdf")).unwrap())
    );
    assert_eq!(
        json["documents"][1]["expect"]["pages"][0]["required_flags"],
        serde_json::json!(["requires_ocr", "low_confidence_text"])
    );
    assert_eq!(
        json["documents"][1]["expect"]["pages"][0]["required_reasons"],
        serde_json::json!(["high_image_coverage_with_sparse_native_text"])
    );
    assert_eq!(
        json["documents"][1]["expect"]["pages"][0]["image_artifact_count"],
        1
    );
    assert_eq!(json["documents"][1]["expect"]["image_artifact_count"], 1);
    assert_eq!(json["documents"][1]["expect"]["warnings_count"], 1);
    assert_eq!(
        json["documents"][1]["expect"]["required_warnings"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
    assert_eq!(
        json["documents"][1]["expect"]["ocr_required_classification"]["expected_pages"],
        serde_json::json!([0])
    );
    assert_eq!(
        json["documents"][1]["expect"]["quality_flag_classification"],
        serde_json::json!([
          {
            "flag": "low_confidence_text",
            "expected_pages": [0],
            "min_precision": 1.0,
            "min_recall": 1.0
          }
        ])
    );

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated directory manifest");
    assert!(
        eval_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&eval_output.stderr)
    );
}

#[test]
fn manifest_category_stamps_generated_documents_for_eval_coverage() {
    let dir = temp_dir("manifest-category");
    fs::write(
        dir.join("b.pdf"),
        minimal_pdf("Second categorized manifest"),
    )
    .unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First categorized manifest")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--category",
            "datasheet",
        ])
        .output()
        .expect("run glyphrush manifest with category");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(json["documents"][0]["path"], "a.pdf");
    assert_eq!(json["documents"][0]["category"], "datasheet");
    assert_eq!(json["documents"][1]["path"], "b.pdf");
    assert_eq!(json["documents"][1]["category"], "datasheet");
    assert_eq!(
        json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&json)
    );

    let eval_json = run_json(["eval", manifest_path.to_str().unwrap()]);
    assert_eq!(
        eval_json["category_counts"],
        serde_json::json!({"datasheet": 2})
    );
    assert_eq!(
        eval_json["category_summaries"]["datasheet"]["document_count"],
        2
    );
    assert_eq!(eval_json["quality_passed"], true);
}

#[test]
fn manifest_category_from_path_uses_top_level_folders_for_coverage() {
    let dir = temp_dir("manifest-category-from-path");
    let clean_dir = dir.join("clean_digital");
    let scanned_dir = dir.join("scanned");
    fs::create_dir(&clean_dir).unwrap();
    fs::create_dir(&scanned_dir).unwrap();
    fs::write(
        clean_dir.join("b.pdf"),
        minimal_pdf("Clean folder manifest"),
    )
    .unwrap();
    fs::write(
        scanned_dir.join("a.pdf"),
        minimal_pdf("Scanned folder manifest"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--category-from-path",
        ])
        .output()
        .expect("run glyphrush manifest with category-from-path");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(json["documents"][0]["path"], "clean_digital/b.pdf");
    assert_eq!(json["documents"][0]["category"], "clean_digital");
    assert_eq!(json["documents"][1]["path"], "scanned/a.pdf");
    assert_eq!(json["documents"][1]["category"], "scanned");
    assert_eq!(
        json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&json)
    );

    let eval_json = run_json(["eval", manifest_path.to_str().unwrap()]);
    assert_eq!(
        eval_json["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "scanned": 1
        })
    );
    assert_eq!(eval_json["quality_passed"], true);
}

#[test]
fn manifest_required_categories_generate_coverage_gate() {
    let dir = temp_dir("manifest-required-categories");
    fs::write(dir.join("a.pdf"), minimal_pdf("Required category manifest")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--category",
            "datasheet",
            "--required-category",
            "scanned",
            "--required-category",
            "datasheet",
        ])
        .output()
        .expect("run glyphrush manifest with required categories");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(
        json["required_categories"],
        serde_json::json!(["datasheet", "scanned"])
    );
    assert_eq!(json["documents"][0]["category"], "datasheet");

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated manifest with coverage gate");
    assert!(!eval_output.status.success());
    let eval_json: Value =
        serde_json::from_slice(&eval_output.stdout).expect("eval output is json");

    assert_eq!(
        eval_json["category_coverage"]["missing"],
        serde_json::json!(["scanned"])
    );
    assert_eq!(eval_json["quality_passed"], false);
    assert_eq!(eval_json["failed_checks"], 1);
}

#[test]
fn manifest_min_category_counts_generate_coverage_gate() {
    let dir = temp_dir("manifest-min-category-counts");
    fs::write(dir.join("a.pdf"), minimal_pdf("Minimum category manifest")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--category",
            "datasheet",
            "--min-category-count",
            "datasheet=2",
            "--min-category-count",
            "scanned=1",
        ])
        .output()
        .expect("run glyphrush manifest with minimum category counts");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    assert_eq!(
        json["min_category_counts"],
        serde_json::json!({
            "datasheet": 2,
            "scanned": 1
        })
    );

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated manifest with minimum coverage gate");
    assert!(!eval_output.status.success());
    let eval_json: Value =
        serde_json::from_slice(&eval_output.stdout).expect("eval output is json");

    assert_eq!(
        eval_json["failure_samples"][0]["check"],
        "min_category_counts"
    );
    assert_eq!(
        eval_json["category_coverage"]["under_minimum"]["datasheet"],
        serde_json::json!({
            "required": 2,
            "actual": 1
        })
    );
    assert_eq!(
        eval_json["category_coverage"]["under_minimum"]["scanned"],
        serde_json::json!({
            "required": 1,
            "actual": 0
        })
    );
}

#[test]
fn manifest_coverage_preset_generates_glyphrush_v0_category_gate() {
    let dir = temp_dir("manifest-coverage-preset");
    fs::write(
        dir.join("clean.pdf"),
        minimal_pdf("Coverage preset manifest"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--category",
            "clean_digital",
            "--coverage-preset",
            "glyphrush-v0",
        ])
        .output()
        .expect("run glyphrush manifest with coverage preset");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("manifest output is json");
    let manifest_path = dir.join("corpus.generated.json");
    fs::write(&manifest_path, &output.stdout).unwrap();

    let expected_categories = serde_json::json!([
        "academic_columns",
        "clean_digital",
        "forms",
        "hybrid",
        "large",
        "rotated",
        "scanned",
        "tables",
        "weird_encoding"
    ]);
    assert_eq!(json["required_categories"], expected_categories);
    assert_eq!(
        json["min_category_counts"],
        serde_json::json!({
            "academic_columns": 1,
            "clean_digital": 1,
            "forms": 1,
            "hybrid": 1,
            "large": 1,
            "rotated": 1,
            "scanned": 1,
            "tables": 1,
            "weird_encoding": 1
        })
    );
    assert_eq!(json["documents"][0]["category"], "clean_digital");

    let eval_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on generated manifest with preset coverage gate");
    assert!(!eval_output.status.success());
    let eval_json: Value =
        serde_json::from_slice(&eval_output.stdout).expect("eval output is json");

    assert_eq!(
        eval_json["category_coverage"]["missing"],
        serde_json::json!([
            "academic_columns",
            "forms",
            "hybrid",
            "large",
            "rotated",
            "scanned",
            "tables",
            "weird_encoding"
        ])
    );
    assert_eq!(eval_json["quality_passed"], false);
}

#[test]
fn seed_datasheet_manifest_declares_category_coverage_gate() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let datasheet_documents = json["documents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|document| document["category"] == "datasheet")
        .count();

    assert_eq!(
        json["required_categories"],
        serde_json::json!(["datasheet"])
    );
    assert_eq!(
        json["min_category_counts"],
        serde_json::json!({ "datasheet": datasheet_documents })
    );
}

#[test]
fn seed_datasheet_manifest_pins_source_provenance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");

    for document in json["documents"].as_array().unwrap() {
        let path = document["path"].as_str().unwrap();
        let fingerprint = document["document_fingerprint"]
            .as_str()
            .unwrap_or_else(|| panic!("{path} is missing document_fingerprint"));
        assert_eq!(
            fingerprint.len(),
            64,
            "{path} fingerprint should be SHA-256 hex"
        );
        assert!(
            fingerprint
                .chars()
                .all(|character| character.is_ascii_hexdigit()),
            "{path} fingerprint should contain only hex digits"
        );
        assert!(
            document["source_size_bytes"].as_u64().unwrap_or_default() > 0,
            "{path} is missing source_size_bytes"
        );
    }
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_pin_function_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_FP6183-33X7.pdf")
        .expect("FP6183 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
          {
            "page": 2,
            "expected_rows": [
              ["Pin Name", "Pin No.", "Pin Function"],
              ["VOUT", "1", "The FP6183 is stable with an output capacitor 1µF or greater. The larger output capacitor will be required for application with larger load transients. The large output capacitor could reduce output noise, improve stability and PSRR."],
              ["GND", "2", "Common ground pin."],
              ["EN", "3", "Pull this pin high to enable IC, pull this pin low to shutdown IC. Floating this pin will be shutdown due to the built-in pull-low resistor."],
              ["VIN", "4", "Power is supplied to this device from this pin which is required an input filter capacitor. In general, the input capacitor in the range of 1µF to 10µF is sufficient."],
              ["Exposed pad", "EP", "The exposed pad must be soldered to a large PCB area and connected to GND for maximum power dissipation."]
            ],
            "min_row_recall": 1.0,
            "min_cell_recall": 1.0,
            "min_cell_f1": 1.0
          }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_bullet_leader_spec_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_FP6183-33X7.pdf")
        .expect("FP6183 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 3,
          "expected_rows": [
            ["Parameter", "Limit"],
            ["Input Voltage VIN", "-0.3V to +6.5V"],
            ["Output Voltage VOUT", "-0.3V to +6.5V"],
            ["EN Voltage VEN", "-0.3V to VIN +0.3V"],
            ["Power Dissipation @ TA=25°C & TJ=125°C (PD) UTDFN-4L (1.0mmx1.0mm)", "0.5W"],
            ["Package Thermal Resistance (θJA) (Note 3) UTDFN-4L (1.0mmx1.0mm)", "195°C/W"],
            ["Package Thermal Resistance (θJC) UTDFN-4L (1.0mmx1.0mm)", "65°C/W"],
            ["Lead Temperature (Soldering, 10sec.)", "+260°C"],
            ["Junction Temperature (TJ)", "-40°C to +150°C"],
            ["Storage Temperature (TSTG)", "-65°C to +150°C"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_parameter_symbol_conditions_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_FP6183-33X7.pdf")
        .expect("FP6183 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 4,
          "expected_rows": [
            ["Current Limit", "ILIMIT", "", "", "320", "", "mA"],
            ["Current Foldback", "ICFB", "RLoad=1Ω", "", "100", "", "mA"],
            ["Output Discharge Resistance", "RDIS", "VEN=0V", "", "60", "", "Ω"],
            ["EN Pin Current", "IEN", "VEN=2.5V", "", "0.3", "", "uA"],
            ["Thermal Shutdown Threshold (Note 7)", "TSD", "", "", "160", "", "ºC"],
            ["Thermal Shutdown Threshold Hysteresis (Note 7)", "TSD", "", "", "30", "", "ºC"],
            ["EN Pin Threshold", "VEN(ON)", "Start-up", "", "1.0", "", "V"],
            ["", "VEN(OFF)", "Shutdown", "", "0.4", "", "V"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_pin_number_name_function_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_APL5324BI-TRG.pdf")
        .expect("APL5324 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(
        table_structure
            .iter()
            .any(|expectation| expectation
                == &serde_json::json!(
          {
            "page": 6,
            "expected_rows": [
              ["Pin No.", "Name", "Function"],
              ["1", "VIN", "Voltage supply input pin."],
              ["2", "GND", "Ground pin."],
              ["3", "SHDN", "Shutdown control pin, logic high: enable; logic low: shutdown."],
              ["4", "SET", "Connect this pin to an external resistor divider to adjust output voltage."],
              ["5", "VOUT", "Regulator output pin."]
            ],
            "min_row_recall": 1.0,
            "min_cell_recall": 1.0,
            "min_cell_f1": 1.0
          }
                ))
    );
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_fragmented_symbol_rating_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_APL5324BI-TRG.pdf")
        .expect("APL5324 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 1,
          "expected_rows": [
            ["Symbol", "Parameter", "Rating", "Unit"],
            ["VIN", "Supply Voltage (VIN to GND)", "-0.3 ~ 6.5", "V"],
            ["VSHDN", "SHDN Input Voltage (SHDN to GND)", "-0.3 ~ 6.5", "V"],
            ["PD", "Power Dissipation Internally Limited", "", "W"],
            ["TJ", "Junction Temperature", "-40 ~ 150", "oC"],
            ["TSTG", "Storage Temperature", "-65 ~ 150", "oC"],
            ["TSDR", "Maximum Lead Soldering Temperature, 10 Seconds", "260", "oC"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_electrical_characteristics_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_APL5324BI-TRG.pdf")
        .expect("APL5324 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 2,
          "expected_rows": [
            ["Symbol", "Parameter", "Test Conditions", "Min.", "Typ.", "Max.", "Unit"],
            ["VIN", "Input Voltage", "", "2.7", "-", "6", "V"],
            ["VOUT", "Output Voltage Range", "", "0.8", "-", "5.5", "V"],
            ["IQ", "Quiescent Current", "IOUT =10mA ~300mA", "-", "135", "160", "mA"],
            ["VREF", "Reference Voltage", "Measured on SET, VIN=3V, IOUT=10mA", "-", "0.8", "-", "V"],
            ["", "Output Voltage Accuracy", "IOUT=10mA", "-2", "-", "+2", "%"],
            ["REGLINE", "Line Regulation", "DVOUT%/DVIN, IOUT=10mA", "-0.06", "-", "+0.06", "%/V"],
            ["REGLOAD", "Load Regulation", "DVOUT%/DIOUT", "-0.2", "-", "+0.2", "%/A"],
            ["VDROP", "Dropout Voltage", "VOUT = 2.5V, IOUT = 300mA", "-", "500", "650", "mV"],
            ["", "", "VOUT = 3.3V, IOUT = 300mA", "-", "300", "400", "mV"],
            ["PSRR", "Power Supply Ripple Rejection Ratio", "f = 10kHz, IOUT = 300mA", "-", "45", "-", "dB"],
            ["", "Noise", "f = 80Hz to 100kHz, IOUT = 300mA", "-", "160", "-", "mVRMS"],
            ["ILIMIT", "Current Limit", "", "450", "550", "-", "mA"],
            ["ISHORT", "Foldback Current", "VOUT = 0V", "-", "80", "-", "mA"],
            ["", "SHDN Input Voltage High", "", "1.6", "-", "-", "V"],
            ["", "SHDN Input Voltage Low", "", "-", "-", "0.4", "V"],
            ["", "VOUT Discharge MOSFET RDS(ON)", "SHDN = Low", "-", "60", "-", "W"],
            ["", "Shutdown VIN Supply Current", "SHDN = Low, VIN = 6V", "-", "0.1", "1", "mA"],
            ["", "SHDN Pull Low Resistance", "", "-", "3", "-", "MW"],
            ["", "Over Temperature Threshold", "", "-", "160", "-", "oC"],
            ["", "Over Temperature Hysteresis", "", "-", "40", "-", "oC"],
            ["", "SET Input Bias Current", "VSET=0.8V", "-100", "-", "100", "nA"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_reflow_profile_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_APL5324BI-TRG.pdf")
        .expect("APL5324 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 14,
          "expected_rows": [
            ["Profile Feature", "Sn-Pb Eutectic Assembly", "Pb-Free Assembly"],
            ["Preheat & Soak", "", ""],
            ["Temperature min (Tsmin)", "100 °C", "150 °C"],
            ["Temperature max (Tsmax)", "150 °C", "200 °C"],
            ["Time (Tsmin to Tsmax) (ts)", "60-120 seconds", "60-120 seconds"],
            ["Average ramp-up rate (Tsmax to TP)", "3 °C/second max.", "3°C/second max."],
            ["Liquidous temperature (TL)", "183 °C", "217 °C"],
            ["Time at liquidous (tL)", "60-150 seconds", "60-150 seconds"],
            ["Peak package body Temperature (Tp)*", "See Classification Temp in table 1", "See Classification Temp in table 2"],
            ["Time (tP)** within 5°C of the specified classification temperature (Tc)", "20** seconds", "30** seconds"],
            ["Average ramp-down rate (Tp to Tsmax)", "6 °C/second max.", "6 °C/second max."],
            ["Time 25°C to peak temperature", "6 minutes max.", "8 minutes max."]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_classification_temperature_tables() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_APL5324BI-TRG.pdf")
        .expect("APL5324 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 14,
          "expected_rows": [
            ["Package Thickness", "Volume mm3 <350", "Volume mm3 ³350"],
            ["<2.5 mm", "235 °C", "220 °C"],
            ["³2.5 mm", "220 °C", "220 °C"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 14,
          "expected_rows": [
            ["Package Thickness", "Volume mm3 <350", "Volume mm3 350-2000", "Volume mm3 >2000"],
            ["<1.6 mm", "260 °C", "260 °C", "260 °C"],
            ["1.6 mm – 2.5 mm", "260 °C", "250 °C", "245 °C"],
            ["³2.5 mm", "250 °C", "245 °C", "245 °C"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_package_pin_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_AP7354D-15W5-7.pdf")
        .expect("AP7354 datasheet expectation exists");

    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(
        table_structure
            .iter()
            .any(|expectation| expectation
                == &serde_json::json!(
          {
            "page": 1,
            "expected_rows": [
              ["SOT25", "SOT23", "X2-DFN1010-4 (Type B)", "Pin Name", "Function"],
              ["3", "—", "3", "EN", "Chip Enable — This should be driven either high or low and must not be floating. Driving EN high enables regulator output, while pulling it low places regulator into shutdown mode."],
              ["2", "3", "2", "GND", "Ground"],
              ["5", "2", "1", "VOUT", "Output Voltage"],
              ["1", "1", "4", "VIN", "Power Input"],
              ["—", "—", "Center Pad", "—", "No connection or ground. Note: Chip Ground must be through GND pin."]
            ],
            "min_row_recall": 1.0,
            "min_cell_recall": 1.0,
            "min_cell_f1": 1.0
          }
                ))
    );
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_part_number_ordering_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_AP7354D-15W5-7.pdf")
        .expect("AP7354 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    let expectation = table_structure
        .iter()
        .find(|expectation| expectation["page"] == 13)
        .expect("AP7354 page 13 ordering table expectation exists");
    let expected_rows = expectation["expected_rows"]
        .as_array()
        .expect("expected rows are recorded");

    assert_eq!(expected_rows.len(), 21);
    assert_eq!(
        expected_rows.first().unwrap(),
        &serde_json::json!(["Part Number", "VOUT", "Package", "Identification Code"])
    );
    assert_eq!(
        expected_rows.get(1).unwrap(),
        &serde_json::json!(["AP7354-11FS4-7", "1.1V", "X2-DFN1010-4 (Type B)", "A8M"])
    );
    assert_eq!(
        expected_rows.last().unwrap(),
        &serde_json::json!(["AP7354D-45FS4-7", "4.5V", "X2-DFN1010-4 (Type B)", "A9J"])
    );
    assert_eq!(expectation["min_row_recall"], 1.0);
    assert_eq!(expectation["min_cell_recall"], 1.0);
    assert_eq!(expectation["min_cell_f1"], 1.0);
}

#[test]
fn seed_datasheet_manifest_rejects_pdfium_description_prose_table_false_positive() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_AW37030D180DNR.pdf")
        .expect("AW37030D180 datasheet expectation exists");

    let pages = document["expect_by_backend"]["pdfium"]["pages"]
        .as_array()
        .expect("pdfium page expectations");
    assert!(pages.iter().any(|page| page
        == &serde_json::json!(
        {
          "index": 0,
          "layout_block_counts": {
            "block_count": 6,
            "paragraph_blocks": 6,
            "heading_blocks": 0,
            "list_blocks": 0,
            "table_blocks": 0,
            "figure_blocks": 0,
            "header_blocks": 0,
            "footer_blocks": 0
          }
        }
              )));
}

#[test]
fn seed_datasheet_manifest_tracks_pdfium_awinic_electrical_table() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join("test/corpus.datasheets.json");
    let json: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read seed datasheet manifest"))
            .expect("seed datasheet manifest is json");
    let documents = json["documents"].as_array().unwrap();
    let document = documents
        .iter()
        .find(|document| document["path"] == "LDO_AW37030D180DNR.pdf")
        .expect("AW37030D180 datasheet expectation exists");
    let table_structure = document["expect_by_backend"]["pdfium"]["table_structure"]
        .as_array()
        .expect("pdfium table structure expectations");

    assert!(table_structure.iter().any(|expectation| expectation
        == &serde_json::json!(
        {
          "page": 5,
          "expected_rows": [
            ["Parameter", "Test Condition", "Min.", "Typ.", "Max.", "Unit"],
            ["VIN Input Voltage Range", "", "1.4", "", "5.5", "V"],
            ["VOUT_ACC Output Voltage Accuracy", "TA=25°C", "-1.3", "", "1.3", "%"],
            ["", "-40°C ≤TA≤85°C", "-2", "", "2", "%"],
            ["LOADReg Load Regulation", "1mA≤IOUT≤300mA", "", "1", "40", "mV"],
            ["LINEReg Line Regulation", "VOUT(SET)+0.5V≤VIN ≤5.5V", "", "1", "5", "mV"],
            ["Vdropout Dropout Voltage", "IOUT=300mA VOUT(SET)=1.8V", "", "310", "", "mV"],
            ["", "IOUT=300mA VOUT(SET)=3.3V", "", "158", "", "mV"],
            ["ISD Shutdown Current", "VCE<0.4V", "", "0.1", "1", "A"],
            ["IQ Quiescent Current", "IOUT=0mA", "", "50", "80", "A"],
            ["VCEH CE Input Voltage “H”", "-40°C ≤TA≤85°C", "", "1", "", "V"],
            ["VCEL CE Input Voltage “L”", "-40°C ≤TA≤85°C", "", "0.4", "", "V"],
            ["PSRR Power Supply Ripple Rejection", "IOUT=30mA, f=1kHz VOUT(SET)=1.8V", "", "90", "", "dB"],
            ["VN Output Voltage Noise", "IOUT=30mA BW=10Hz to 100kHz VOUT(SET)=1.8V", "", "33", "", "Vrms"],
            ["", "IOUT=30mA BW=10Hz to 100kHz VOUT(SET)=3.3V", "", "46", "", "Vrms"],
            ["ICL Output Current Limit", "VOUT=90%*VOUT(SET)", "", "300", "", "mA"],
            ["ISC Short Current Limit", "VOUT<10%*VOUT(SET)", "", "120", "", "mA"],
            ["VTC Output Voltage Temperature Coefficient", "-40°C ≤TA≤85°C", "", "±40", "", "ppm/°C"],
            ["RDISC Auto Discharge Resistance", "VIN=4V, VCE<0.4V, VOUT=2.8V", "", "130", "", "Ω"],
            ["ICE CE Pull Down Current", "", "", "140", "", "nA"],
            ["TSDH Thermal Shutdown Threshold", "Temperature Rising", "", "150", "", "°C"],
            ["TSDL Thermal Shutdown Reset Threshold", "Temperature Falling", "", "130", "", "°C"]
          ],
          "min_row_recall": 1.0,
          "min_cell_recall": 1.0,
          "min_cell_f1": 1.0
        }
              )));
}

#[test]
fn manifest_directory_jobs_preserve_stable_output() {
    let dir = temp_dir("manifest-directory-jobs");
    fs::write(dir.join("c.pdf"), minimal_pdf("Third manifest jobs")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First manifest jobs")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Second manifest jobs")).unwrap();

    let serial_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "manifest", dir.to_str().unwrap()])
        .output()
        .expect("run glyphrush manifest serially");
    assert!(
        serial_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&serial_output.stderr)
    );

    let parallel_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush manifest with jobs");
    assert!(
        parallel_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&parallel_output.stderr)
    );

    let serial_json: Value =
        serde_json::from_slice(&serial_output.stdout).expect("serial manifest output is json");
    let parallel_json: Value =
        serde_json::from_slice(&parallel_output.stdout).expect("parallel manifest output is json");

    assert_eq!(serial_json, parallel_json);
    assert_eq!(parallel_json["documents"][0]["path"], "a.pdf");
    assert_eq!(parallel_json["documents"][1]["path"], "b.pdf");
    assert_eq!(parallel_json["documents"][2]["path"], "c.pdf");
    assert_eq!(
        parallel_json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&parallel_json)
    );
}

#[test]
fn manifest_directory_with_cache_dir_preserves_stable_output() {
    let dir = temp_dir("manifest-directory-cache");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second manifest cache")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First manifest cache")).unwrap();
    let cache_dir = dir.join("cache");

    let serial_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush manifest directory with cache miss");
    assert!(
        serial_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&serial_output.stderr)
    );

    let parallel_output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "manifest",
            dir.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush manifest directory with cache hit and jobs");
    assert!(
        parallel_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&parallel_output.stderr)
    );

    let serial_json: Value =
        serde_json::from_slice(&serial_output.stdout).expect("manifest output is json");
    let parallel_json: Value =
        serde_json::from_slice(&parallel_output.stdout).expect("manifest output is json");

    assert_eq!(serial_json, parallel_json);
    assert_eq!(parallel_json["documents"][0]["path"], "a.pdf");
    assert_eq!(parallel_json["documents"][1]["path"], "b.pdf");
    assert_eq!(
        parallel_json["corpus_fingerprint"],
        expected_generated_manifest_corpus_fingerprint(&parallel_json)
    );
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 2);
}

#[test]
fn parse_json_emits_structured_artifact_with_native_text() {
    let pdf_path = write_test_pdf("parse", "Hello Glyphrush");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush parse with page jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse on ruled table");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush parse");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let json = run_json([
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush parse");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");
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

    let json = run_json([
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse on widget annotation");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse on text annotation");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse on catalog AcroForm");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let json = run_json(["parse", pdf_path.to_str().unwrap(), "--format", "json"]);
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval on structured blank-cell table");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

    assert_eq!(json["quality_passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn parse_json_emits_structured_cells_for_positioned_table_blocks() {
    let dir = temp_dir("parse-json-positioned-table-cells");
    let pdf_path = dir.join("positioned-table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_positioned_ruled_table()).unwrap();

    let json = run_json([
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

    let json = run_json([
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run glyphrush parse without ocr sidecar");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--ocr-sidecar",
            sidecar_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush parse with ocr sidecar");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "parse",
            pdf_path.to_str().unwrap(),
            "--format",
            "json",
            "--ocr-sidecar",
            sidecar_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush parse with broken native text and ocr sidecar");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output is json");
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

    let native = run_json([
        "parse",
        native_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-command",
        command.to_str().unwrap(),
    ]);
    let scan = run_json([
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

    let native = run_json([
        "parse",
        native_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-http-url",
        &ocr_url,
    ]);
    let scan = run_json([
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

    let json = run_json([
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
fn ocr_check_command_smoke_reports_nonempty_output() {
    let dir = temp_dir("ocr-check-command-success");
    let pdf_path = dir.join("scan.pdf");
    let log_path = dir.join("ocr.log");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let command = write_ocr_command_script("ocr-check-command-adapter", &log_path);

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "ocr-check",
            pdf_path.to_str().unwrap(),
            "--page-index",
            "0",
            "--ocr-command",
            command.to_str().unwrap(),
            "--strict",
        ])
        .output()
        .expect("run glyphrush ocr-check with command adapter");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
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
        ])
        .output()
        .expect("run glyphrush ocr-check with rendered-image input");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "ocr-check",
            pdf_path.to_str().unwrap(),
            "--page-index",
            "0",
            "--ocr-command",
            command.to_str().unwrap(),
            "--strict",
        ])
        .output()
        .expect("run glyphrush strict ocr-check with empty command output");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "ocr-check",
            pdf_path.to_str().unwrap(),
            "--page-index",
            "0",
            "--ocr-command",
            command.to_str().unwrap(),
            "--strict",
        ])
        .output()
        .expect("run glyphrush strict ocr-check with missing tessdata");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("ocr-check output is json");

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

#[test]
fn parse_with_cache_dir_reports_miss_then_hit_for_same_pdf() {
    let dir = temp_dir("parse-cache");
    let pdf_path = dir.join("cache.pdf");
    fs::write(&pdf_path, minimal_pdf("Cache me")).unwrap();
    let cache_dir = dir.join("cache");

    let first = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let second = run_json([
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
    assert_eq!(snapshot["cache_schema"], "glyphrush-cache-v40");
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

    let first = run_json([
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
    let second = run_json([
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

    let first = run_json([
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

    let second = run_json([
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

#[test]
fn cache_key_does_not_reuse_prior_schema_artifacts() {
    let dir = temp_dir("parse-cache-schema-version");
    let pdf_path = dir.join("cache-schema.pdf");
    fs::write(&pdf_path, minimal_pdf("Cache schema")).unwrap();
    let cache_dir = dir.join("cache");

    let json = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let fingerprint = json["document_fingerprint"].as_str().unwrap();
    let old_v1_key = sha256_hex(format!(
        "glyphrush-cache-v1:lopdf:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v2_key = sha256_hex(format!(
        "glyphrush-cache-v2:lopdf:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v3_key = sha256_hex(format!(
        "glyphrush-cache-v3:lopdf:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v4_key = sha256_hex(format!(
        "glyphrush-cache-v4:lopdf:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v5_key = sha256_hex(format!(
        "glyphrush-cache-v5:lopdf:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v6_key = sha256_hex(format!(
        "glyphrush-cache-v6:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v7_key = sha256_hex(format!(
        "glyphrush-cache-v7:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v8_key = sha256_hex(format!(
        "glyphrush-cache-v8:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v9_key = sha256_hex(format!(
        "glyphrush-cache-v9:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false"
    ));
    let old_v10_key = sha256_hex(format!(
        "glyphrush-cache-v10:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v11_key = sha256_hex(format!(
        "glyphrush-cache-v11:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v12_key = sha256_hex(format!(
        "glyphrush-cache-v12:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v13_key = sha256_hex(format!(
        "glyphrush-cache-v13:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v14_key = sha256_hex(format!(
        "glyphrush-cache-v14:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v15_key = sha256_hex(format!(
        "glyphrush-cache-v15:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v16_key = sha256_hex(format!(
        "glyphrush-cache-v16:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v17_key = sha256_hex(format!(
        "glyphrush-cache-v17:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v18_key = sha256_hex(format!(
        "glyphrush-cache-v18:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v19_key = sha256_hex(format!(
        "glyphrush-cache-v19:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v20_key = sha256_hex(format!(
        "glyphrush-cache-v20:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v21_key = sha256_hex(format!(
        "glyphrush-cache-v21:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v22_key = sha256_hex(format!(
        "glyphrush-cache-v22:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v23_key = sha256_hex(format!(
        "glyphrush-cache-v23:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v24_key = sha256_hex(format!(
        "glyphrush-cache-v24:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v25_key = sha256_hex(format!(
        "glyphrush-cache-v25:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v26_key = sha256_hex(format!(
        "glyphrush-cache-v26:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v27_key = sha256_hex(format!(
        "glyphrush-cache-v27:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v28_key = sha256_hex(format!(
        "glyphrush-cache-v28:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v29_key = sha256_hex(format!(
        "glyphrush-cache-v29:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v30_key = sha256_hex(format!(
        "glyphrush-cache-v30:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v31_key = sha256_hex(format!(
        "glyphrush-cache-v31:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v32_key = sha256_hex(format!(
        "glyphrush-cache-v32:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v33_key = sha256_hex(format!(
        "glyphrush-cache-v33:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v34_key = sha256_hex(format!(
        "glyphrush-cache-v34:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v35_key = sha256_hex(format!(
        "glyphrush-cache-v35:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v36_key = sha256_hex(format!(
        "glyphrush-cache-v36:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v37_key = sha256_hex(format!(
        "glyphrush-cache-v37:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v38_key = sha256_hex(format!(
        "glyphrush-cache-v38:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v39_key = sha256_hex(format!(
        "glyphrush-cache-v39:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let expected_current_key = sha256_hex(format!(
        "glyphrush-cache-v40:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));

    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v1_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v2_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v3_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v4_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v5_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v6_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v7_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v8_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v9_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v10_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v11_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v12_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v13_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v14_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v15_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v16_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v17_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v18_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v19_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v20_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v21_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v22_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v23_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v24_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v25_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v26_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v27_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v28_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v29_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v30_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v31_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v32_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v33_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v34_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v35_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v36_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v37_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v38_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v39_key
    );
    assert_eq!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        expected_current_key
    );
}

#[test]
fn cache_key_changes_when_ocr_sidecar_text_changes() {
    let dir = temp_dir("parse-cache-sidecar");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    let sidecar_path = sidecar_dir.join("scan.p000000.txt");
    fs::write(&sidecar_path, "First OCR text").unwrap();
    let cache_dir = dir.join("cache");

    let first = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    fs::write(&sidecar_path, "Second OCR text").unwrap();
    let second = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(first["global_diagnostics"]["cache_status"], "miss");
    assert_eq!(second["global_diagnostics"]["cache_status"], "miss");
    assert_ne!(
        first["global_diagnostics"]["cache_key"],
        second["global_diagnostics"]["cache_key"]
    );
    assert_eq!(first["pages"][0]["ocr_spans"][0]["text"], "First OCR text");
    assert_eq!(
        second["pages"][0]["ocr_spans"][0]["text"],
        "Second OCR text"
    );
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 2);
}

#[test]
fn cache_key_ignores_unrelated_ocr_sidecar_files() {
    let dir = temp_dir("parse-cache-sidecar-unrelated");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(sidecar_dir.join("scan.p000000.txt"), "Scan OCR text").unwrap();
    let cache_dir = dir.join("cache");

    let first = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    fs::write(
        sidecar_dir.join("other-document.p000000.txt"),
        "Unrelated OCR text",
    )
    .unwrap();
    let second = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(first["global_diagnostics"]["cache_status"], "miss");
    assert_eq!(second["global_diagnostics"]["cache_status"], "hit");
    assert_eq!(
        first["global_diagnostics"]["cache_key"],
        second["global_diagnostics"]["cache_key"]
    );
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 1);
}

#[test]
fn cache_key_changes_when_span_geometry_option_changes() {
    let dir = temp_dir("parse-cache-span-geometry");
    let pdf_path = dir.join("positioned.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td (First line) Tj 0 -24 Td (Second line) Tj ET",
        ),
    )
    .unwrap();
    let cache_dir = dir.join("cache");

    let default = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let geometry = run_json([
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--span-geometry",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(default["global_diagnostics"]["cache_status"], "miss");
    assert_eq!(geometry["global_diagnostics"]["cache_status"], "miss");
    assert_ne!(
        default["global_diagnostics"]["cache_key"],
        geometry["global_diagnostics"]["cache_key"]
    );
    assert_eq!(
        default["pages"][0]["native_spans"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        geometry["pages"][0]["native_spans"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 2);
}

#[test]
fn bench_reports_timing_and_fallback_counts() {
    let pdf_path = write_test_pdf("bench", "Hello Glyphrush");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "bench", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush bench");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["report_version"], "glyphrush-bench-report-v1");
    assert_eq!(json["quality_status"], "not_checked_no_eval_manifest");
    assert!(json.get("quality").is_none());
    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["run_metadata"]["parser_name"], "glyphrush");
    assert_eq!(
        json["run_metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["run_metadata"]["backend"], "lopdf");
    assert_eq!(json["run_metadata"]["backend_version"], "lopdf-adapter-v0");
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
    assert_eq!(json["page_count"], 1);
    assert_eq!(json["fallback_pages"], 0);
    assert_eq!(json["ocr_required_pages"], 0);
    assert_eq!(json["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["ocr_requested_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["heavy_layout_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["table_recovery_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["render_pages"], 0);
    assert!(json["text_output_bytes"].as_u64().unwrap() > 0);
    assert_eq!(json["text_output_line_count"], 1);
    assert_eq!(json["text_output_word_count"], 2);
    assert_eq!(json["empty_text_output"], false);
    assert!(json["allocated_bytes"].as_u64().unwrap() > 0);
    assert!(json["allocated_bytes_per_page"].as_f64().unwrap() > 0.0);
    assert_eq!(json["route_counts"]["native_fast_path"], 1);
    assert_eq!(json["route_counts"]["needs_fallback"], 0);
    assert_eq!(json["route_counts"]["ocr_fallback"], 0);
    assert_eq!(json["route_counts"]["unsupported"], 0);
    assert!(
        json["route_latency_us"]["native_fast_path"]["p50_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        json["route_latency_us"]["native_fast_path"]["p95_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(json["route_latency_us"]["ocr_fallback"]["p50_us"], 0);
    assert_eq!(json["route_reason_counts"], serde_json::json!({}));
    assert!(json["wall_us"].as_u64().unwrap() > 0);
    assert!(json["artifact_bytes"].as_u64().unwrap() > 0);
    assert!(json["peak_rss_bytes"].as_u64().unwrap() > 0);
    assert!(json["stage_timings_us"]["open_us"].as_u64().unwrap() > 0);
    assert!(
        json["stage_timings_us"]["native_extract_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(json["stage_timings_us"]["classify_us"].as_u64().unwrap() > 0);
    assert!(json["stage_timings_us"]["layout_us"].as_u64().unwrap() > 0);
    assert!(json["stage_timings_us"]["total_us"].as_u64().unwrap() > 0);
    assert!(json["page_latency_us"]["p50_us"].as_u64().unwrap() > 0);
    assert!(json["page_latency_us"]["p95_us"].as_u64().unwrap() > 0);
}

#[test]
fn bench_require_quality_rejects_speed_only_reports_after_writing_json() {
    let pdf_path = write_test_pdf("bench-require-quality", "Speed only");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--require-quality",
        ])
        .output()
        .expect("run glyphrush bench requiring quality");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    assert_eq!(json["quality_status"], "not_checked_no_eval_manifest");
    assert!(json.get("quality").is_none());
    assert_eq!(json["requirements"]["require_quality"], true);
    assert_eq!(json["requirements"]["require_baselines"], false);
    assert_eq!(json["requirements"]["require_baseline_quality"], false);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench quality required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_single_pdf_jobs_preserve_page_order_for_quality_checks() {
    let dir = temp_dir("bench-single-page-jobs");
    let pdf_path = dir.join("multi.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (First page) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Second page) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Third page) Tj ET",
        ]),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "multi.pdf",
              "expect": {
                "page_count": 3,
                "reading_order": {
                  "expected_sequence": ["First page", "Second page", "Third page"],
                  "min_score": 1.0
                }
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--jobs",
            "2",
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with page jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["worker_count"], 2);
    assert_eq!(json["page_count"], 3);
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["quality"]["failed_checks"], 0);
    assert_eq!(json["quality"]["failure_samples"], Value::Array(vec![]));
    assert_eq!(
        json["quality"]["documents"][0]["checks"]["reading_order"]["actual"]["score"],
        1.0
    );
}

#[test]
fn bench_runs_named_external_baseline_and_reports_metrics() {
    let pdf_path = write_test_pdf("bench-baseline", "Hello Baseline");
    let baseline = write_baseline_script("baseline-ok", "printf 'baseline output for %s' \"$1\"");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baselines = json["baselines"].as_array().unwrap();

    assert_eq!(baselines.len(), 1);
    assert_eq!(baselines[0]["name"], "mock");
    assert_eq!(baselines[0]["success"], true);
    assert_eq!(baselines[0]["exit_status"], 0);
    assert_eq!(
        baselines[0]["quality_status"],
        "not_checked_no_expectations"
    );
    assert_eq!(baselines[0]["quality"], Value::Null);
    assert_eq!(baselines[0]["timed_out"], false);
    assert_eq!(baselines[0]["timeout_ms"], 120000);
    assert!(baselines[0]["wall_us"].as_u64().unwrap() > 0);
    assert!(baselines[0]["output_bytes"].as_u64().unwrap() > 0);
    assert_eq!(baselines[0]["stderr_bytes"], 0);
    assert_eq!(
        baselines[0]["comparison"]["glyphrush_wall_us"],
        json["wall_us"]
    );
    assert_eq!(
        baselines[0]["comparison"]["baseline_wall_us"],
        baselines[0]["wall_us"]
    );
    assert_eq!(
        baselines[0]["comparison"]["glyphrush_text_output_bytes"],
        json["text_output_bytes"]
    );
    assert_eq!(
        baselines[0]["comparison"]["baseline_output_bytes"],
        baselines[0]["output_bytes"]
    );
    assert!(
        baselines[0]["comparison"]["glyphrush_speedup"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(
        baselines[0]["comparison"]["baseline_to_glyphrush_output_bytes"]
            .as_f64()
            .unwrap()
            > 0.0
    );
}

#[test]
fn bench_reports_external_baseline_progress_to_stderr() {
    let pdf_path = write_test_pdf("bench-baseline-progress", "Hello Baseline Progress");
    let baseline = write_baseline_script(
        "baseline-progress",
        "sleep 0.1\nprintf 'baseline progress output for %s' \"$1\"",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with baseline progress");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("glyphrush: external baseline start"),
        "bench should expose progress before long external baseline runs:\n{stderr}"
    );
    assert!(
        stderr.contains("baseline=mock"),
        "progress should identify the external baseline:\n{stderr}"
    );
    assert!(
        stderr.contains(pdf_path.to_str().unwrap()),
        "progress should identify the current PDF:\n{stderr}"
    );
}

#[test]
fn bench_require_speedup_rejects_slow_glyphrush_after_writing_json() {
    let pdf_path = write_test_pdf("bench-require-speedup", "Require speedup");
    let baseline = write_baseline_script("baseline-fast", "printf 'fast baseline'");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--require-speedup",
            "mock=1000000.0",
        ])
        .output()
        .expect("run glyphrush bench requiring speedup");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(
        json["requirements"]["require_speedups"],
        serde_json::json!([
            {
                "baseline": "mock",
                "min_glyphrush_speedup": 1000000.0
            }
        ])
    );
    assert_eq!(baseline["name"], "mock");
    assert_eq!(baseline["success"], true);
    assert!(
        baseline["comparison"]["speed_comparable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        baseline["comparison"]["glyphrush_speedup"]
            .as_f64()
            .unwrap()
            < 1000000.0
    );
    let claim = &json["speedup_claims"][0];
    assert_eq!(claim["baseline"], "mock");
    assert_eq!(claim["required_glyphrush_speedup"], 1000000.0);
    assert!(
        claim["actual_glyphrush_speedup"].as_f64().unwrap() < 1000000.0,
        "claim should preserve measured speedup: {claim:?}"
    );
    assert_eq!(claim["speed_comparable"], true);
    assert_eq!(claim["speed_passed"], false);
    assert_eq!(claim["quality_backed"], false);
    assert_eq!(claim["claim_passed"], false);
    assert_eq!(claim["status"], "speedup_failed");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench speedup required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_require_speedup_claim_rejects_speed_only_report_after_writing_json() {
    let pdf_path = write_test_pdf("bench-require-speedup-claim", "Claim speed only");
    let baseline = write_baseline_script("baseline-claim-speed-only", "printf 'speed only'");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--require-speedup-claim",
            "mock=0.000001",
        ])
        .output()
        .expect("run glyphrush bench requiring speedup claim");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let claim = &json["speedup_claims"][0];

    assert_eq!(
        json["requirements"]["require_speedup_claims"],
        serde_json::json!([
            {
                "baseline": "mock",
                "min_glyphrush_speedup": 0.000001
            }
        ])
    );
    assert_eq!(claim["baseline"], "mock");
    assert_eq!(claim["speed_comparable"], true);
    assert_eq!(claim["speed_passed"], true);
    assert_eq!(claim["glyphrush_quality_checked"], false);
    assert_eq!(claim["baseline_quality_checked"], false);
    assert_eq!(claim["quality_backed"], false);
    assert_eq!(claim["claim_passed"], false);
    assert_eq!(claim["status"], "quality_not_checked");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench speedup claim required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_includes_external_baseline_description_when_available() {
    let pdf_path = write_test_pdf("bench-baseline-describe", "Hello Baseline Describe");
    let baseline = write_baseline_script(
        "baseline-describe",
        "if [ \"${1:-}\" = \"--describe\" ]; then printf '{\"target\":\"MockParse\",\"ocr\":\"none\"}'; exit 0; fi\nprintf 'baseline output'",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with describing baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(baseline["name"], "mock");
    assert_eq!(baseline["success"], true);
    assert_eq!(baseline["description"]["target"], "MockParse");
    assert_eq!(baseline["description"]["ocr"], "none");
    assert_eq!(baseline["description_status"]["success"], true);
    assert_eq!(baseline["description_status"]["valid_json_object"], true);
    assert_eq!(baseline["description_status"]["error"], Value::Null);
}

#[test]
fn bench_reports_external_baseline_description_probe_failures() {
    let pdf_path = write_test_pdf(
        "bench-baseline-describe-failure",
        "Hello Baseline Describe Failure",
    );
    let baseline = write_baseline_script(
        "baseline-describe-failure",
        "if [ \"${1:-}\" = \"--describe\" ]; then printf 'lit missing' >&2; exit 127; fi\nprintf 'baseline output'",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with failed describing baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(baseline["success"], true);
    assert_eq!(baseline["description"], Value::Null);
    assert_eq!(baseline["description_status"]["success"], false);
    assert_eq!(
        baseline["description_status"]["error_kind"],
        "missing_dependency"
    );
    assert!(
        baseline["description_status"]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("lit missing")
    );
}

#[test]
fn bench_directory_summary_reports_external_baseline_description_probe_status() {
    let dir = temp_dir("bench-dir-baseline-describe-failure");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second describe failure")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First describe failure")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-describe-failure",
        "if [ \"${1:-}\" = \"--describe\" ]; then printf 'not json'; exit 0; fi\nprintf 'baseline output'",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench directory with failed describing baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["description"], Value::Null);
    assert_eq!(
        baseline_summary["description_status"]["error_kind"],
        "invalid_describe_output"
    );
    assert_eq!(
        json["documents"][0]["baselines"][0]["description_status"]["error_kind"],
        "invalid_describe_output"
    );
    assert_eq!(
        json["documents"][1]["baselines"][0]["description_status"]["error_kind"],
        "invalid_describe_output"
    );
}

#[test]
fn bench_directory_baseline_summary_exposes_comparison_target() {
    let dir = temp_dir("bench-dir-baseline-target");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second target")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First target")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-target",
        "if [ \"${1:-}\" = \"--describe\" ]; then printf '{\"target\":\"MockParse\",\"kind\":\"text-baseline-wrapper\"}'; exit 0; fi\nprintf 'baseline target output'",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench directory with describing baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["target"], "MockParse");
    assert_eq!(json["documents"][0]["baselines"][0]["target"], "MockParse");
    assert_eq!(json["documents"][1]["baselines"][0]["target"], "MockParse");
}

#[test]
fn bench_reports_external_baseline_output_digest_and_text_stats() {
    let pdf_path = write_test_pdf("bench-baseline-stats", "Hello Baseline Stats");
    let baseline = write_baseline_script("baseline-stats", "printf 'alpha beta\\ncharlie'");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with baseline stats");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(baseline["success"], true);
    assert_eq!(baseline["empty_output"], false);
    assert_eq!(baseline["stdout_line_count"], 2);
    assert_eq!(baseline["stdout_word_count"], 3);
    assert_eq!(baseline["stdout_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn baseline_check_rejects_empty_baseline_list_without_vacuous_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .arg("baseline-check")
        .output()
        .expect("run glyphrush baseline-check without baselines");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["report_version"], "glyphrush-baseline-check-report-v1");
    assert_eq!(json["baseline_count"], 0);
    assert_eq!(json["describe_success_count"], 0);
    assert_eq!(json["all_described"], false);
    assert!(json["baselines"].as_array().unwrap().is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("baseline-check requires at least one --baseline")
    );
}

#[test]
fn bench_reports_failed_external_baseline_without_hiding_glyphrush_metrics() {
    let pdf_path = write_test_pdf("bench-baseline-fail", "Hello Failed Baseline");
    let baseline = write_baseline_script("baseline-fail", "printf 'bad baseline' >&2\nexit 7");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("broken={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with failing baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["page_count"], 1);
    assert_eq!(json["fallback_pages"], 0);
    assert_eq!(json["baselines"][0]["name"], "broken");
    assert_eq!(json["baselines"][0]["success"], false);
    assert_eq!(json["baselines"][0]["exit_status"], 7);
    assert_eq!(json["baselines"][0]["error_kind"], "execution_failed");
    assert_eq!(json["baselines"][0]["output_bytes"], 0);
    assert!(json["baselines"][0]["stderr_bytes"].as_u64().unwrap() > 0);
    assert!(
        json["baselines"][0]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("bad baseline")
    );
    assert_eq!(
        json["baselines"][0]["comparison"]["speed_comparable"],
        false
    );
    assert_eq!(json["baselines"][0]["comparison"]["glyphrush_speedup"], 0.0);
    assert_eq!(json["baselines"][0]["comparison"]["baseline_speedup"], 0.0);
}

#[test]
fn bench_reports_missing_dependency_external_baseline_kind() {
    let pdf_path = write_test_pdf(
        "bench-baseline-missing-dependency",
        "Hello Missing Dependency",
    );
    let baseline = write_baseline_script(
        "baseline-missing-dependency",
        "printf 'parser dependency missing' >&2\nexit 127",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("missing={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with missing dependency baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(baseline["success"], false);
    assert_eq!(baseline["exit_status"], 127);
    assert_eq!(baseline["error_kind"], "missing_dependency");
    assert!(
        baseline["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing")
    );
    assert_eq!(baseline["comparison"]["speed_comparable"], false);
}

#[test]
fn bench_require_baselines_rejects_failed_baseline_after_writing_json() {
    let pdf_path = write_test_pdf("bench-require-baseline-fail", "Hello Failed Baseline");
    let baseline = write_baseline_script(
        "require-baseline-fail",
        "printf 'strict baseline failed' >&2\nexit 7",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("broken={}", baseline.display()),
            "--require-baselines",
        ])
        .output()
        .expect("run glyphrush bench requiring baselines");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["page_count"], 1);
    assert_eq!(json["baselines"][0]["name"], "broken");
    assert_eq!(json["baselines"][0]["success"], false);
    assert_eq!(json["baselines"][0]["exit_status"], 7);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench baselines required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_does_not_quality_score_failed_external_baseline() {
    let dir = temp_dir("bench-baseline-quality-fail");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Failed Baseline Quality")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "required_text": ["Failed Baseline Quality"]
              }
            }
          ]
        }"#,
    )
    .unwrap();
    let baseline = write_baseline_script("baseline-quality-exec-fail", "exit 7");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--baseline",
            &format!("broken={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench with failing baseline and quality manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(baseline["success"], false);
    assert_eq!(baseline["quality_status"], "not_checked_execution_failed");
    assert_eq!(baseline["quality"], Value::Null);
}

#[test]
fn bench_reports_timed_out_external_baseline_without_hanging() {
    let pdf_path = write_test_pdf("bench-baseline-timeout", "Hello Timed Baseline");
    let manifest_path = pdf_path.parent().unwrap().join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "required_text": ["Hello Timed Baseline"]
              }
            }
          ]
        }"#,
    )
    .unwrap();
    let baseline = write_baseline_script(
        "baseline-timeout",
        "if [ \"${1:-}\" = \"--describe\" ]; then exit 0; fi\nsleep 2\nprintf 'late output'",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("slow={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--baseline-timeout-ms",
            "50",
        ])
        .output()
        .expect("run glyphrush bench with timed-out baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let baseline = &json["baselines"][0];

    assert_eq!(json["page_count"], 1);
    assert_eq!(baseline["name"], "slow");
    assert_eq!(baseline["success"], false);
    assert_eq!(baseline["timed_out"], true);
    assert_eq!(baseline["error_kind"], "timeout");
    assert_eq!(baseline["quality_status"], "not_checked_timed_out");
    assert_eq!(baseline["quality"], Value::Null);
    assert_eq!(baseline["timeout_ms"], 50);
    assert!(
        baseline["wall_us"].as_u64().unwrap() < 1_000_000,
        "baseline wall_us: {}",
        baseline["wall_us"]
    );
    assert!(
        baseline["error"]
            .as_str()
            .unwrap()
            .contains("timed out after 50 ms")
    );
    assert_eq!(baseline["comparison"]["speed_comparable"], false);
    assert_eq!(baseline["comparison"]["glyphrush_speedup"], 0.0);
    assert_eq!(baseline["comparison"]["baseline_speedup"], 0.0);
}

#[test]
fn bench_with_eval_manifest_embeds_quality_summary() {
    let dir = temp_dir("bench-eval-pass");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench Quality")).unwrap();
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
                "required_text": ["Bench Quality"]
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--span-geometry",
            "--ocr-sidecar",
            ocr_dir.to_str().unwrap(),
            "--ocr-timeout-ms",
            "4321",
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with eval manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["report_version"], "glyphrush-bench-report-v1");
    assert_eq!(json["quality_status"], "checked");
    assert!(json["wall_us"].as_u64().unwrap() > 0);
    assert_eq!(json["run_configuration"]["span_geometry"], true);
    assert_eq!(json["run_configuration"]["ocr_sidecar"], true);
    assert_eq!(json["run_configuration"]["ocr_command"], false);
    assert_eq!(json["run_configuration"]["ocr_command_input"], "pdf_page");
    assert_eq!(json["run_configuration"]["ocr_timeout_ms"], 4321);
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(
        json["quality"]["report_version"],
        "glyphrush-eval-report-v1"
    );
    assert_eq!(json["quality"]["run_configuration"]["span_geometry"], true);
    assert_eq!(json["quality"]["run_configuration"]["ocr_sidecar"], true);
    assert_eq!(json["quality"]["run_configuration"]["ocr_command"], false);
    assert_eq!(
        json["quality"]["run_configuration"]["ocr_command_input"],
        "pdf_page"
    );
    assert_eq!(json["quality"]["run_configuration"]["ocr_timeout_ms"], 4321);
    assert_eq!(json["quality"]["failed_checks"], 0);
    assert_eq!(json["quality"]["document_count"], 1);
    assert_eq!(
        json["quality"]["documents"][0]["checks"]["page_count"]["actual"],
        1
    );
}

#[test]
fn bench_with_eval_manifest_selects_matching_document_from_full_corpus_manifest() {
    let dir = temp_dir("bench-eval-single-from-corpus");
    let pdf_path = dir.join("a.pdf");
    fs::write(&pdf_path, minimal_pdf("Only this document is benchmarked")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Unrelated corpus document")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Only this document is benchmarked"]
              }
            },
            {
              "path": "b.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Unrelated corpus document"]
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with one PDF and full corpus manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["quality"]["failed_checks"], 0);
    assert_eq!(json["quality"]["document_count"], 1);
    assert_eq!(json["quality"]["documents"][0]["path"], "a.pdf");
    assert_eq!(
        json["quality"]["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn bench_with_eval_manifest_scores_quality_from_bench_artifact() {
    let dir = temp_dir("bench-eval-reuses-bench-artifact");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench Artifact Quality")).unwrap();
    let cache_dir = dir.join("cache");
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "page_count": 1,
                "required_text": ["Bench Artifact Quality"]
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with eval manifest and cache");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["cache_status"], "miss");
    assert_eq!(
        json["quality"]["documents"][0]["artifact_cache_status"],
        "miss"
    );
    assert_eq!(json["quality"]["passed"], true);
}

#[test]
fn bench_with_eval_manifest_scores_baseline_output_quality() {
    let dir = temp_dir("bench-eval-baseline-quality");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench Quality Alpha Beta")).unwrap();
    let good_baseline =
        write_baseline_script("baseline-quality-good", "printf 'Bench Quality Alpha Beta'");
    let bad_baseline = write_baseline_script("baseline-quality-bad", "printf 'Unrelated output'");
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "required_text": ["Bench Quality"],
                "text_recall": {
                  "expected": "Alpha Beta",
                  "min_word_recall": 1.0,
                  "min_char_recall": 1.0
                }
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("good={}", good_baseline.display()),
            "--baseline",
            &format!("bad={}", bad_baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with eval manifest and baseline quality");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][1]["quality_status"], "checked");
    let good = &json["baselines"][0]["quality"];
    let bad = &json["baselines"][1]["quality"];

    assert_eq!(good["passed"], true);
    assert_eq!(good["failed_checks"], 0);
    assert_eq!(good["required_text"]["missing"], Value::Array(vec![]));
    assert_eq!(good["text_recall"]["word_recall"], 1.0);
    assert_eq!(bad["passed"], false);
    assert_eq!(bad["failed_checks"], 2);
    assert_eq!(
        bad["required_text"]["missing"],
        serde_json::json!(["Bench Quality"])
    );
    assert_eq!(bad["text_recall"]["word_recall"], 0.0);
}

#[test]
fn bench_require_baseline_quality_rejects_failed_baseline_quality_after_writing_json() {
    let dir = temp_dir("bench-require-baseline-quality");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench Quality Alpha Beta")).unwrap();
    let bad_baseline =
        write_baseline_script("require-baseline-quality-bad", "printf 'Unrelated output'");
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "required_text": ["Bench Quality"]
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("bad={}", bad_baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--require-baseline-quality",
        ])
        .output()
        .expect("run glyphrush bench requiring baseline quality");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][0]["quality"]["passed"], false);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench baseline quality required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_with_eval_manifest_scores_baseline_page_required_text() {
    let dir = temp_dir("bench-eval-baseline-page-required-text");
    let pdf_path = dir.join("sample.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_streams(&[
            "BT /F1 24 Tf 72 720 Td (Page One Anchor) Tj ET",
            "BT /F1 24 Tf 72 720 Td (Page Two Anchor) Tj ET",
        ]),
    )
    .unwrap();
    let baseline = write_baseline_script(
        "baseline-page-required-text",
        "printf 'Page One Anchor only'",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "pages": [
                  {
                    "index": 0,
                    "required_text": ["Page One Anchor"]
                  },
                  {
                    "index": 1,
                    "required_text": ["Page Two Anchor"]
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with page-required baseline quality");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(
        json["baselines"][0]["quality"]["required_text"]["expected"],
        serde_json::json!(["Page One Anchor", "Page Two Anchor"])
    );
    assert_eq!(
        json["baselines"][0]["quality"]["required_text"]["missing"],
        serde_json::json!(["Page Two Anchor"])
    );
}

#[test]
fn bench_with_eval_manifest_scores_baseline_reading_order() {
    let dir = temp_dir("bench-eval-baseline-reading-order");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Alpha Beta Gamma")).unwrap();
    let ordered_baseline =
        write_baseline_script("baseline-order-good", "printf 'Alpha Beta Gamma'");
    let inverted_baseline =
        write_baseline_script("baseline-order-bad", "printf 'Gamma Beta Alpha'");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("ordered={}", ordered_baseline.display()),
            "--baseline",
            &format!("inverted={}", inverted_baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with eval manifest and baseline reading order");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let ordered = &json["baselines"][0]["quality"];
    let inverted = &json["baselines"][1]["quality"];

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(ordered["passed"], true);
    assert_eq!(ordered["failed_checks"], 0);
    assert_eq!(ordered["reading_order"]["passed"], true);
    assert_eq!(ordered["reading_order"]["score"], 1.0);
    assert_eq!(inverted["passed"], false);
    assert_eq!(inverted["failed_checks"], 1);
    assert_eq!(inverted["reading_order"]["passed"], false);
    assert_eq!(inverted["reading_order"]["score"], 0.0);
    assert_eq!(inverted["reading_order"]["inversion_count"], 3);
}

#[test]
fn bench_with_eval_manifest_scores_baseline_table_structure() {
    let dir = temp_dir("bench-eval-baseline-table-structure");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value |) Tj T* (| A | 1 |) Tj ET",
        ),
    )
    .unwrap();
    let table_baseline = write_baseline_script(
        "baseline-table-good",
        "printf '| Part | Value |\\n| --- | --- |\\n| A | 1 |'",
    );
    let wrong_table_baseline =
        write_baseline_script("baseline-table-bad", "printf 'Part\\tValue\\nB\\t2'");
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
                    "min_cell_f1": 1.0
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("table={}", table_baseline.display()),
            "--baseline",
            &format!("wrong={}", wrong_table_baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with eval manifest and baseline table structure");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");
    let table = &json["baselines"][0]["quality"];
    let wrong = &json["baselines"][1]["quality"];

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(table["passed"], true);
    assert_eq!(table["failed_checks"], 0);
    assert_eq!(table["table_structure"][0]["passed"], true);
    assert_eq!(table["table_structure"][0]["cell_recall"], 1.0);
    assert_eq!(wrong["passed"], false);
    assert_eq!(wrong["failed_checks"], 1);
    assert_eq!(wrong["table_structure"][0]["passed"], false);
    assert!(wrong["table_structure"][0]["cell_recall"].as_f64().unwrap() < 1.0);
    assert_eq!(
        wrong["table_structure"][0]["missing_cells"],
        serde_json::json!([
            {"row": 1, "column": 0, "text": "A"},
            {"row": 1, "column": 1, "text": "1"}
        ])
    );
}

#[test]
fn bench_with_ocr_sidecar_reports_applied_ocr_pages() {
    let dir = temp_dir("bench-ocr-sidecar");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(sidecar_dir.join("scan.p000000.txt"), "Sidecar OCR text").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--ocr-sidecar",
            sidecar_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with ocr sidecar");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["fallback_pages"], 1);
    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["ocr_applied_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["ocr_requested_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["ocr_applied_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["heavy_layout_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["table_recovery_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["render_pages"], 0);
    assert!(json["stage_timings_us"]["ocr_us"].as_u64().unwrap() > 0);
}

#[test]
fn bench_reports_fallback_action_counts_for_table_recovery() {
    let dir = temp_dir("bench-table-action-counts");
    let pdf_path = dir.join("table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "bench", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush bench on table-like PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["fallback_pages"], 1);
    assert_eq!(json["quality_flag_counts"]["table_uncertain"], 1);
    assert_eq!(json["fallback_action_counts"]["ocr_requested_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["heavy_layout_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["table_recovery_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["render_pages"], 0);
}

#[test]
fn bench_reports_warnings_for_incomplete_ocr_pages() {
    let dir = temp_dir("bench-warning");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "bench", pdf_path.to_str().unwrap()])
        .output()
        .expect("run glyphrush bench without ocr sidecar");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["ocr_requested_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["render_pages"], 0);
    assert_eq!(json["text_output_bytes"], 0);
    assert_eq!(json["text_output_word_count"], 0);
    assert_eq!(json["empty_text_output"], true);
    assert_eq!(json["quality_flag_counts"]["requires_ocr"], 1);
    assert_eq!(json["quality_flag_counts"]["low_confidence_text"], 1);
    assert_eq!(json["quality_flag_counts"]["broken_encoding"], 0);
    assert_eq!(json["quality_flag_counts"]["layout_uncertain"], 0);
    assert_eq!(json["quality_flag_counts"]["table_uncertain"], 0);
    assert_eq!(json["quality_flag_counts"]["unsupported_feature"], 0);
    assert_eq!(json["warnings_count"], 1);
    assert_eq!(
        json["warnings"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
}

#[test]
fn bench_with_eval_manifest_uses_ocr_sidecar_for_quality_checks() {
    let dir = temp_dir("bench-eval-ocr-sidecar");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(sidecar_dir.join("scan.p000000.txt"), "Sidecar OCR text").unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "expect": {
                "ocr_required_pages": 1,
                "ocr_applied_pages": 1,
                "required_text": ["Sidecar OCR text"],
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--ocr-sidecar",
            sidecar_dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with ocr sidecar and eval manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    let manifest_sha256 = format!("{:x}", Sha256::digest(fs::read(&manifest_path).unwrap()));

    assert_eq!(json["ocr_applied_pages"], 1);
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["quality"]["manifest_sha256"], manifest_sha256);
    assert_eq!(json["quality"]["failed_checks"], 0);
    assert_eq!(
        json["quality"]["documents"][0]["checks"]["required_text"]["actual"]["missing"],
        Value::Array(vec![])
    );
}

#[test]
fn bench_with_eval_manifest_exits_nonzero_when_quality_fails() {
    let dir = temp_dir("bench-eval-fail");
    let pdf_path = dir.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench Quality")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "sample.pdf",
              "expect": {
                "required_text": ["Missing Quality"]
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
            "bench",
            pdf_path.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with failing eval manifest");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert!(json["wall_us"].as_u64().unwrap() > 0);
    assert_eq!(json["quality"]["passed"], false);
    assert_eq!(json["quality"]["failed_checks"], 1);
    assert_eq!(
        json["quality"]["failure_samples"],
        serde_json::json!([
            {
                "path": "sample.pdf",
                "check": "required_text",
                "expected": ["Missing Quality"],
                "actual": {
                    "missing": ["Missing Quality"]
                }
            }
        ])
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("bench quality failed"));
}

#[test]
fn bench_with_eval_manifest_reports_silent_failure_summary() {
    let dir = temp_dir("bench-eval-silent-failure-summary");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench with silent-failure eval manifest");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["quality_status"], "checked");
    assert_eq!(json["quality"]["failed_checks"], 1);
    assert_eq!(json["silent_failure_count"], 1);
    assert_eq!(
        json["silent_failure_pages"],
        serde_json::json!([
            {
                "path": "scan.pdf",
                "page": 0,
                "flags": ["requires_ocr", "low_confidence_text"],
                "empty_text_output": true
            }
        ])
    );
}

#[test]
fn bench_directory_with_eval_manifest_embeds_quality_summary() {
    let dir = temp_dir("bench-dir-eval-pass");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second Quality")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First Quality")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "required_text": ["First Quality"]
              }
            },
            {
              "path": "b.pdf",
              "expect": {
                "required_text": ["Second Quality"]
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
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush directory bench with eval manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["quality_status"], "checked");
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["quality"]["failed_checks"], 0);
    assert_eq!(json["quality"]["document_count"], 2);
    assert_eq!(json["documents"][0]["path"], "a.pdf");
}

#[test]
fn bench_directory_with_eval_manifest_reports_silent_failure_summary() {
    let dir = temp_dir("bench-dir-eval-silent-failure-summary");
    fs::write(dir.join("a.pdf"), minimal_pdf("Clean Directory Bench")).unwrap();
    fs::write(
        dir.join("b.pdf"),
        minimal_pdf_with_stream("0 0 m 10 10 l S"),
    )
    .unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "silent_failures": {
                  "max_count": 0
                }
              }
            },
            {
              "path": "b.pdf",
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush directory bench with silent-failure eval manifest");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["quality_status"], "checked");
    assert_eq!(json["quality"]["failed_checks"], 1);
    assert_eq!(json["silent_failure_count"], 1);
    assert_eq!(
        json["silent_failure_pages"],
        serde_json::json!([
            {
                "path": "b.pdf",
                "page": 0,
                "flags": ["requires_ocr", "low_confidence_text"],
                "empty_text_output": true
            }
        ])
    );
}

#[test]
fn bench_directory_with_eval_manifest_reports_benchmark_category_summaries() {
    let dir = temp_dir("bench-dir-category-summary");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second Category Bench")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First Category Bench")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["First Category Bench"]
              }
            },
            {
              "path": "b.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["Second Category Bench"]
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
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush directory bench with category manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let clean_summary = &json["category_summaries"]["clean_digital"];
    let scanned_summary = &json["category_summaries"]["scanned"];

    assert_eq!(clean_summary["document_count"], 1);
    assert_eq!(clean_summary["page_count"], 1);
    assert!(clean_summary["wall_us"].as_u64().unwrap() > 0);
    assert!(clean_summary["pages_per_sec"].as_f64().unwrap() > 0.0);
    assert_eq!(clean_summary["fallback_pages"], 0);
    assert_eq!(clean_summary["ocr_required_pages"], 0);
    assert_eq!(clean_summary["route_counts"]["native_fast_path"], 1);
    assert_eq!(clean_summary["quality_passed"], true);
    assert_eq!(clean_summary["quality_failed"], false);
    assert_eq!(clean_summary["failed_checks"], 0);

    assert_eq!(scanned_summary["document_count"], 1);
    assert_eq!(scanned_summary["page_count"], 1);
    assert!(scanned_summary["wall_us"].as_u64().unwrap() > 0);
    assert!(scanned_summary["pages_per_sec"].as_f64().unwrap() > 0.0);
    assert_eq!(scanned_summary["quality_passed"], true);
    assert_eq!(scanned_summary["failed_checks"], 0);
}

#[test]
fn bench_require_coverage_preset_rejects_incomplete_speed_corpus_after_writing_json() {
    let dir = temp_dir("bench-require-coverage-preset");
    fs::write(
        dir.join("clean.pdf"),
        minimal_pdf("Clean coverage bench text"),
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
                "required_text": ["Clean coverage bench text"]
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
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--require-coverage-preset",
            "glyphrush-v0",
        ])
        .output()
        .expect("run glyphrush bench with coverage preset gate");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["quality_status"], "checked");
    assert_eq!(
        json["requirements"]["require_coverage_preset"],
        "glyphrush-v0"
    );
    assert_eq!(json["quality"]["quality_passed"], false);
    assert_eq!(json["quality"]["failed_checks"], 1);
    assert_eq!(
        json["quality"]["category_coverage"],
        serde_json::json!({
            "required": [
                "academic_columns",
                "clean_digital",
                "forms",
                "hybrid",
                "large",
                "rotated",
                "scanned",
                "tables",
                "weird_encoding",
            ],
            "present": ["clean_digital"],
            "missing": [
                "academic_columns",
                "forms",
                "hybrid",
                "large",
                "rotated",
                "scanned",
                "tables",
                "weird_encoding",
            ],
            "passed": false
        })
    );
    assert_eq!(
        json["quality"]["failure_samples"][0]["check"],
        "required_categories"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("coverage preset glyphrush-v0"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_directory_with_eval_category_filters_quality_manifest() {
    let dir = temp_dir("bench-dir-eval-category-filter");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean Bench Filter")).unwrap();
    fs::write(dir.join("scan.pdf"), minimal_pdf("Scan Bench Filter")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean Bench Filter"]
              }
            },
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["missing scanned bench text"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = run_json([
        "bench",
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--eval-category",
        "clean_digital",
    ]);

    assert_eq!(json["quality"]["document_count"], 1);
    assert_eq!(json["quality"]["documents"][0]["path"], "clean.pdf");
    assert_eq!(
        json["quality"]["category_counts"],
        serde_json::json!({"clean_digital": 1})
    );
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["quality"]["failed_checks"], 0);
    assert_eq!(json["category_summaries"].as_object().unwrap().len(), 1);
    assert_eq!(
        json["category_summaries"]["clean_digital"]["document_count"],
        1
    );
    assert!(json["category_summaries"]["scanned"].is_null());
}

#[test]
fn bench_directory_with_eval_category_rejects_empty_selection_after_writing_json() {
    let dir = temp_dir("bench-dir-eval-empty-category");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean Bench Empty")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean Bench Empty"]
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
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--eval-category",
            "scanned",
        ])
        .output()
        .expect("run glyphrush bench with empty eval category");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["report_version"], "glyphrush-bench-report-v1");
    assert_eq!(json["document_count"], 1);
    assert_eq!(json["quality_status"], "checked");
    assert_eq!(json["quality"]["document_count"], 0);
    assert_eq!(json["quality"]["passed"], false);
    assert_eq!(json["quality"]["quality_passed"], false);
    assert_eq!(json["quality"]["failed_checks"], 1);
    assert_eq!(
        json["quality"]["failure_samples"][0]["check"],
        "document_count"
    );
    assert_eq!(
        json["quality"]["failure_samples"][0]["expected"],
        serde_json::json!({"min": 1})
    );
    assert_eq!(json["quality"]["failure_samples"][0]["actual"], 0);
    assert!(json.get("category_summaries").is_none());
    assert!(String::from_utf8_lossy(&output.stderr).contains("bench quality failed"));
}

#[test]
fn bench_directory_with_eval_manifest_rejects_manifest_documents_outside_corpus() {
    let dir = temp_dir("bench-dir-eval-strict");
    fs::write(dir.join("a.pdf"), minimal_pdf("First Quality")).unwrap();
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "required_text": ["First Quality"]
              }
            },
            {
              "path": "missing.pdf",
              "expect": {
                "required_text": ["Missing Quality"]
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
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush directory bench with extra manifest document");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("was not part of this benchmark"));
}

#[test]
fn bench_with_cache_dir_reports_miss_then_hit() {
    let dir = temp_dir("bench-cache");
    let pdf_path = dir.join("bench-cache.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench cache")).unwrap();
    let cache_dir = dir.join("cache");

    let first = run_json([
        "bench",
        pdf_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let second = run_json([
        "bench",
        pdf_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);

    assert_eq!(first["cache_status"], "miss");
    assert_eq!(second["cache_status"], "hit");
    assert_eq!(first["cache_key"], second["cache_key"]);
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 1);
}

#[test]
fn bench_cache_probe_reports_cold_miss_and_warm_hit_in_one_run() {
    let dir = temp_dir("bench-cache-probe");
    let pdf_path = dir.join("bench-cache-probe.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let cache_dir = dir.join("cache");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--cache-probe",
        ])
        .output()
        .expect("run glyphrush bench cache probe");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["cache_status"], "miss");
    assert_eq!(json["cache_probe"]["cold"]["cache_status"], "miss");
    assert_eq!(json["cache_probe"]["warm"]["cache_status"], "hit");
    assert_eq!(json["cache_probe"]["cache_key_match"], true);
    assert_eq!(
        json["cache_probe"]["cold"]["route_counts"]["ocr_fallback"],
        1
    );
    assert_eq!(
        json["cache_probe"]["warm"]["route_counts"]["ocr_fallback"],
        1
    );
    assert_eq!(
        json["cache_probe"]["cold"]["route_reason_counts"]["high_image_coverage_without_native_text"],
        1
    );
    assert_eq!(
        json["cache_probe"]["warm"]["route_reason_counts"]["high_image_coverage_without_native_text"],
        1
    );
    assert_eq!(
        json["cache_probe"]["cold"]["quality_flag_counts"]["requires_ocr"],
        1
    );
    assert_eq!(
        json["cache_probe"]["warm"]["quality_flag_counts"]["requires_ocr"],
        1
    );
    assert_eq!(
        json["cache_probe"]["cold"]["fallback_action_counts"]["ocr_requested_pages"],
        1
    );
    assert_eq!(
        json["cache_probe"]["warm"]["fallback_action_counts"]["ocr_requested_pages"],
        1
    );
    assert_eq!(
        json["cache_probe"]["cold"]["fallback_action_counts"]["ocr_applied_pages"],
        0
    );
    assert_eq!(
        json["cache_probe"]["warm"]["fallback_action_counts"]["ocr_applied_pages"],
        0
    );
    assert_eq!(
        json["cache_probe"]["cold"]["fallback_action_counts"]["render_pages"],
        0
    );
    assert_eq!(
        json["cache_probe"]["warm"]["fallback_action_counts"]["render_pages"],
        0
    );
    assert_eq!(json["cache_probe"]["cold"]["warnings_count"], 1);
    assert_eq!(json["cache_probe"]["warm"]["warnings_count"], 1);
    assert_eq!(json["cache_probe"]["cold"]["text_output_bytes"], 0);
    assert_eq!(json["cache_probe"]["warm"]["text_output_bytes"], 0);
    assert_eq!(json["cache_probe"]["cold"]["empty_text_output"], true);
    assert_eq!(json["cache_probe"]["warm"]["empty_text_output"], true);
    assert!(
        json["cache_probe"]["cold"]["allocated_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        json["cache_probe"]["warm"]["allocated_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        json["cache_probe"]["cold"]["allocated_bytes_per_page"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(
        json["cache_probe"]["warm"]["allocated_bytes_per_page"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(json["cache_probe"]["cold"]["wall_us"].as_u64().unwrap() > 0);
    assert!(json["cache_probe"]["warm"]["wall_us"].as_u64().unwrap() > 0);
    assert!(
        json["cache_probe"]["cold"]["route_latency_us"]["ocr_fallback"]["p50_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        json["cache_probe"]["warm"]["route_latency_us"]["ocr_fallback"]["p50_us"],
        0
    );
    assert!(json["cache_probe"]["warm_speedup"].as_f64().unwrap() >= 0.0);
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 1);
}

#[test]
fn bench_directory_reports_sorted_documents_and_aggregate_counts() {
    let dir = temp_dir("bench-dir");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second")).unwrap();
    fs::write(dir.join("ignore.txt"), "not a pdf").unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First")).unwrap();
    let ocr_dir = dir.join("ocr");
    fs::create_dir(&ocr_dir).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--span-geometry",
            "--ocr-sidecar",
            ocr_dir.to_str().unwrap(),
            "--ocr-timeout-ms",
            "5678",
        ])
        .output()
        .expect("run glyphrush bench on directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["report_version"], "glyphrush-bench-report-v1");
    assert_eq!(json["quality_status"], "not_checked_no_eval_manifest");
    assert!(json.get("quality").is_none());
    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["run_metadata"]["parser_name"], "glyphrush");
    assert_eq!(
        json["run_metadata"]["parser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(json["run_metadata"]["backend"], "lopdf");
    assert_eq!(json["run_metadata"]["backend_version"], "lopdf-adapter-v0");
    assert_eq!(json["run_configuration"]["span_geometry"], true);
    assert_eq!(json["run_configuration"]["ocr_sidecar"], true);
    assert_eq!(json["run_configuration"]["ocr_command"], false);
    assert_eq!(json["run_configuration"]["ocr_command_input"], "pdf_page");
    assert_eq!(json["run_configuration"]["ocr_timeout_ms"], 5678);
    assert_eq!(json["document_count"], 2);
    assert_eq!(json["page_count"], 2);
    assert_eq!(
        json["corpus_fingerprint"],
        expected_corpus_fingerprint(&json)
    );
    assert_eq!(json["corpus_fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(json["fallback_pages"], 0);
    assert_eq!(json["ocr_pages"], 0);
    assert_eq!(json["ocr_required_pages"], 0);
    assert_eq!(json["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["ocr_requested_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["heavy_layout_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["table_recovery_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["render_pages"], 0);
    assert_eq!(json["text_output_word_count"], 2);
    assert_eq!(json["empty_text_output_documents"], 0);
    assert_eq!(json["empty_text_output_pages"], 0);
    assert_eq!(json["documents"][0]["path"].as_str().unwrap(), "a.pdf");
    assert_eq!(json["documents"][1]["path"].as_str().unwrap(), "b.pdf");
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
        fs::metadata(dir.join("a.pdf")).unwrap().len()
    );
    assert_eq!(
        json["documents"][0]["fallback_action_counts"]["ocr_requested_pages"],
        0
    );
    assert_eq!(
        json["documents"][0]["fallback_action_counts"]["table_recovery_pages"],
        0
    );
    assert_eq!(json["documents"][0]["text_output_word_count"], 1);
    assert_eq!(json["documents"][0]["empty_text_output"], false);
    assert!(json["wall_us"].as_u64().unwrap() > 0);
    assert!(json["artifact_bytes"].as_u64().unwrap() > 0);
    assert!(json["allocated_bytes"].as_u64().unwrap() > 0);
    assert!(json["allocated_bytes_per_page"].as_f64().unwrap() > 0.0);
    assert!(json["peak_rss_bytes"].as_u64().unwrap() > 0);
    assert!(json["documents"][0]["artifact_bytes"].as_u64().unwrap() > 0);
    assert!(json["documents"][0]["allocated_bytes"].as_u64().unwrap() > 0);
    assert!(
        json["documents"][0]["allocated_bytes_per_page"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(json["documents"][0]["peak_rss_bytes"].as_u64().unwrap() > 0);
    assert!(json["stage_timings_us"]["total_us"].as_u64().unwrap() > 0);
    assert!(json["page_latency_us"]["p50_us"].as_u64().unwrap() > 0);
    assert!(
        json["documents"][0]["stage_timings_us"]["total_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        json["documents"][0]["page_latency_us"]["p95_us"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn bench_directory_recursively_matches_category_manifest_paths() {
    let dir = temp_dir("bench-dir-recursive-category-paths");
    let clean_dir = dir.join("clean_digital");
    let scanned_dir = dir.join("scanned");
    fs::create_dir(&clean_dir).unwrap();
    fs::create_dir(&scanned_dir).unwrap();
    fs::write(
        clean_dir.join("clean.pdf"),
        minimal_pdf("Clean nested bench"),
    )
    .unwrap();
    fs::write(
        scanned_dir.join("scan.pdf"),
        minimal_pdf("Scanned nested bench"),
    )
    .unwrap();

    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean_digital/clean.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean nested bench"]
              }
            },
            {
              "path": "scanned/scan.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["Scanned nested bench"]
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
            "bench",
            dir.to_str().unwrap(),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench on nested category corpus");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["documents"][0]["path"], "clean_digital/clean.pdf");
    assert_eq!(json["documents"][1]["path"], "scanned/scan.pdf");
    assert_eq!(
        json["quality"]["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "scanned": 1
        })
    );
    assert_eq!(json["quality"]["quality_passed"], true);
}

#[test]
fn bench_directory_jobs_preserve_stable_document_order() {
    let dir = temp_dir("bench-dir-jobs");
    fs::write(dir.join("c.pdf"), minimal_pdf("Third")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Second")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush bench on directory with parallel jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["worker_count"], 2);
    assert_eq!(json["document_count"], 3);
    assert_eq!(json["page_count"], 3);
    assert_eq!(json["documents"][0]["path"], "a.pdf");
    assert_eq!(json["documents"][1]["path"], "b.pdf");
    assert_eq!(json["documents"][2]["path"], "c.pdf");
    assert_eq!(
        json["corpus_fingerprint"],
        expected_corpus_fingerprint(&json)
    );
    assert_eq!(json["route_counts"]["native_fast_path"], 3);
}

#[test]
fn bench_directory_parallel_wall_time_uses_worker_chunk_parser_time() {
    let dir = temp_dir("bench-dir-parallel-wall-time");
    fs::write(
        dir.join("b.pdf"),
        minimal_pdf_with_stream("0 0 m 10 10 l S"),
    )
    .unwrap();
    fs::write(
        dir.join("a.pdf"),
        minimal_pdf_with_stream("0 0 m 10 10 l S"),
    )
    .unwrap();
    let command = write_baseline_script(
        "bench-dir-parallel-wall-time-ocr",
        "sleep 0.4\nprintf 'Parallel OCR page %s' \"$2\"",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--jobs",
            "2",
            "--ocr-command",
            command.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench on directory with parallel OCR jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    let wall_us = json["wall_us"].as_u64().unwrap();
    let summed_document_wall_us = json["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|document| document["wall_us"].as_u64().unwrap())
        .sum::<u64>();

    assert_eq!(json["worker_count"], 2);
    assert_eq!(json["document_count"], 2);
    assert_eq!(json["ocr_applied_pages"], 2);
    assert!(
        wall_us.saturating_mul(4) < summed_document_wall_us.saturating_mul(3),
        "parallel corpus wall_us should account for worker chunks, not sum document parser times: wall_us={wall_us}, summed_document_wall_us={summed_document_wall_us}"
    );
}

#[test]
fn bench_directory_wall_time_excludes_external_baseline_time() {
    let dir = temp_dir("bench-dir-wall-excludes-baseline");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline delay")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First baseline delay")).unwrap();
    let baseline = write_baseline_script("bench-dir-slow-baseline", "sleep 0.6\nprintf baseline");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--jobs",
            "2",
            "--baseline",
            &format!("slow={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench on directory with slow external baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    let wall_us = json["wall_us"].as_u64().unwrap();
    let summed_document_wall_us = json["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|document| document["wall_us"].as_u64().unwrap())
        .sum::<u64>();

    assert_eq!(json["worker_count"], 2);
    assert!(
        json["baselines"][0]["wall_us"].as_u64().unwrap() >= 1_000_000,
        "baseline summary should include the deliberate sleep cost"
    );
    assert!(
        wall_us < summed_document_wall_us.saturating_add(300_000),
        "glyphrush corpus wall_us should exclude external baseline time: wall_us={wall_us}, summed_document_wall_us={summed_document_wall_us}"
    );
}

#[test]
fn bench_directory_reports_image_artifact_counts() {
    let dir = temp_dir("bench-dir-image-artifacts");
    fs::write(dir.join("a-native.pdf"), minimal_pdf("Native only")).unwrap();
    fs::write(
        dir.join("b-image.pdf"),
        minimal_pdf_with_full_page_image_and_text("Image-backed text"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "bench", dir.to_str().unwrap()])
        .output()
        .expect("run glyphrush bench directory with image artifacts");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["image_artifact_count"], 1);
    assert_eq!(json["image_artifact_pages"], 1);
    assert_eq!(json["documents"][0]["image_artifact_count"], 0);
    assert_eq!(json["documents"][0]["image_artifact_pages"], 0);
    assert_eq!(json["documents"][1]["image_artifact_count"], 1);
    assert_eq!(json["documents"][1]["image_artifact_pages"], 1);
}

#[test]
fn bench_directory_counts_empty_text_pages_inside_non_empty_documents() {
    let dir = temp_dir("bench-dir-empty-page");
    fs::write(
        dir.join("mixed.pdf"),
        minimal_pdf_with_streams(&["BT /F1 24 Tf 72 720 Td (First) Tj ET", "0 0 m 10 10 l S"]),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "bench", dir.to_str().unwrap()])
        .output()
        .expect("run glyphrush bench on mixed text directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["document_count"], 1);
    assert_eq!(json["page_count"], 2);
    assert_eq!(json["text_output_word_count"], 1);
    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["empty_text_output_documents"], 0);
    assert_eq!(json["empty_text_output_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["ocr_requested_pages"], 1);
    assert_eq!(json["fallback_action_counts"]["ocr_applied_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["table_recovery_pages"], 0);
    assert_eq!(json["fallback_action_counts"]["render_pages"], 0);
    assert_eq!(json["documents"][0]["empty_text_output"], false);
}

#[test]
fn bench_directory_reports_warning_summary_for_incomplete_ocr_pages() {
    let dir = temp_dir("bench-dir-warnings");
    fs::write(dir.join("a.pdf"), minimal_pdf("Native document")).unwrap();
    fs::write(
        dir.join("b.pdf"),
        minimal_pdf_with_stream("0 0 m 10 10 l S"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "bench", dir.to_str().unwrap()])
        .output()
        .expect("run glyphrush bench directory with missing OCR");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["ocr_applied_pages"], 0);
    assert_eq!(json["route_counts"]["native_fast_path"], 1);
    assert_eq!(json["route_counts"]["needs_fallback"], 0);
    assert_eq!(json["route_counts"]["ocr_fallback"], 1);
    assert_eq!(json["route_counts"]["unsupported"], 0);
    assert!(
        json["route_latency_us"]["native_fast_path"]["p50_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        json["route_latency_us"]["ocr_fallback"]["p50_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        json["route_reason_counts"]["high_image_coverage_without_native_text"],
        1
    );
    assert_eq!(json["quality_flag_counts"]["requires_ocr"], 1);
    assert_eq!(json["quality_flag_counts"]["low_confidence_text"], 1);
    assert_eq!(json["quality_flag_counts"]["broken_encoding"], 0);
    assert_eq!(json["quality_flag_counts"]["layout_uncertain"], 0);
    assert_eq!(json["quality_flag_counts"]["table_uncertain"], 0);
    assert_eq!(json["quality_flag_counts"]["unsupported_feature"], 0);
    assert_eq!(json["warnings_count"], 1);
    assert_eq!(json["warning_samples"][0]["path"], "b.pdf");
    assert_eq!(
        json["warning_samples"][0]["warning"],
        "p000000: requires_ocr_without_ocr_output"
    );
    assert_eq!(json["documents"][0]["warnings_count"], 0);
    assert_eq!(json["documents"][0]["route_counts"]["native_fast_path"], 1);
    assert_eq!(json["documents"][0]["route_counts"]["ocr_fallback"], 0);
    assert!(
        json["documents"][0]["route_latency_us"]["native_fast_path"]["p50_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        json["documents"][0]["route_latency_us"]["ocr_fallback"]["p50_us"],
        0
    );
    assert_eq!(
        json["documents"][0]["route_reason_counts"],
        serde_json::json!({})
    );
    assert_eq!(
        json["documents"][0]["quality_flag_counts"]["requires_ocr"],
        0
    );
    assert_eq!(json["documents"][0]["warnings"], Value::Array(vec![]));
    assert_eq!(json["documents"][1]["warnings_count"], 1);
    assert_eq!(json["documents"][1]["route_counts"]["native_fast_path"], 0);
    assert_eq!(json["documents"][1]["route_counts"]["ocr_fallback"], 1);
    assert_eq!(
        json["documents"][1]["route_latency_us"]["native_fast_path"]["p50_us"],
        0
    );
    assert!(
        json["documents"][1]["route_latency_us"]["ocr_fallback"]["p50_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        json["documents"][1]["route_reason_counts"]["high_image_coverage_without_native_text"],
        1
    );
    assert_eq!(
        json["documents"][1]["quality_flag_counts"]["requires_ocr"],
        1
    );
    assert_eq!(
        json["documents"][1]["quality_flag_counts"]["low_confidence_text"],
        1
    );
    assert_eq!(
        json["documents"][1]["warnings"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
}

#[test]
fn bench_directory_cache_probe_aggregates_cold_and_warm_runs() {
    let dir = temp_dir("bench-dir-cache-probe");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second cache probe")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First cache probe")).unwrap();
    let cache_dir = dir.join("cache");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--cache-probe",
        ])
        .output()
        .expect("run glyphrush bench directory cache probe");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["cache_probe"]["cold_cache_misses"], 2);
    assert_eq!(json["cache_probe"]["warm_cache_hits"], 2);
    assert!(json["cache_probe"]["cold_wall_us"].as_u64().unwrap() > 0);
    assert!(json["cache_probe"]["warm_wall_us"].as_u64().unwrap() > 0);
    assert!(
        json["cache_probe"]["cold_stage_timings_us"]["total_us"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(json["cache_probe"]["warm_stage_timings_us"]["total_us"], 0);
    assert_eq!(
        json["cache_probe"]["cold_fallback_action_counts"]["ocr_requested_pages"],
        0
    );
    assert_eq!(
        json["cache_probe"]["warm_fallback_action_counts"]["ocr_requested_pages"],
        0
    );
    assert_eq!(
        json["cache_probe"]["cold_fallback_action_counts"]["table_recovery_pages"],
        0
    );
    assert_eq!(
        json["cache_probe"]["warm_fallback_action_counts"]["table_recovery_pages"],
        0
    );
    assert_eq!(
        json["documents"][0]["cache_probe"]["cold"]["cache_status"],
        "miss"
    );
    assert_eq!(
        json["documents"][0]["cache_probe"]["warm"]["cache_status"],
        "hit"
    );
    assert_eq!(fs::read_dir(cache_dir).unwrap().count(), 2);
}

#[test]
fn bench_directory_aggregates_external_baseline_metrics() {
    let dir = temp_dir("bench-dir-baseline");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First baseline")).unwrap();
    let baseline = write_baseline_script("baseline-dir-ok", "printf 'baseline %s' \"$1\"");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench directory with baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["baselines"][0]["name"], "mock");
    assert_eq!(json["baselines"][0]["document_count"], 2);
    assert_eq!(json["baselines"][0]["successful_documents"], 2);
    assert_eq!(json["baselines"][0]["failed_documents"], 0);
    assert_eq!(
        json["baselines"][0]["quality_status"],
        "not_checked_no_expectations"
    );
    assert_eq!(json["baselines"][0]["quality_documents"], 0);
    assert_eq!(json["baselines"][0]["quality_unchecked_documents"], 2);
    assert!(json["baselines"][0]["output_bytes"].as_u64().unwrap() > 0);
    assert_eq!(
        json["baselines"][0]["comparison"]["glyphrush_wall_us"],
        json["wall_us"]
    );
    assert_eq!(
        json["baselines"][0]["comparison"]["baseline_wall_us"],
        json["baselines"][0]["wall_us"]
    );
    assert_eq!(
        json["baselines"][0]["comparison"]["glyphrush_text_output_bytes"],
        json["text_output_bytes"]
    );
    assert_eq!(
        json["baselines"][0]["comparison"]["baseline_output_bytes"],
        json["baselines"][0]["output_bytes"]
    );
    assert!(
        json["baselines"][0]["comparison"]["glyphrush_speedup"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(json["documents"][0]["path"], "a.pdf");
    assert_eq!(json["documents"][0]["baselines"][0]["name"], "mock");
    assert_eq!(json["documents"][1]["path"], "b.pdf");
    assert_eq!(json["documents"][1]["baselines"][0]["success"], true);
}

#[test]
fn bench_directory_with_eval_manifest_aggregates_baseline_quality() {
    let dir = temp_dir("bench-dir-baseline-quality");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second Quality")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First Quality")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-quality",
        "case \"$1\" in *a.pdf) printf 'First Quality';; *) printf 'Wrong Quality';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "required_text": ["First Quality"]
              }
            },
            {
              "path": "b.pdf",
              "expect": {
                "required_text": ["Second Quality"]
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
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench directory with baseline quality");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(baseline_summary["quality_status"], "checked");
    assert_eq!(baseline_summary["quality_documents"], 2);
    assert_eq!(baseline_summary["quality_unchecked_documents"], 0);
    assert_eq!(baseline_summary["quality_passed_documents"], 1);
    assert_eq!(baseline_summary["quality_failed_documents"], 1);
    assert_eq!(baseline_summary["quality_pass_rate"], 0.5);
    assert_eq!(
        json["documents"][0]["baselines"][0]["quality"]["passed"],
        true
    );
    assert_eq!(
        json["documents"][1]["baselines"][0]["quality"]["passed"],
        false
    );
}

#[test]
fn bench_directory_require_baseline_quality_rejects_quality_failures_after_writing_json() {
    let dir = temp_dir("bench-dir-require-baseline-quality");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second Quality")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First Quality")).unwrap();
    let baseline = write_baseline_script(
        "require-baseline-dir-quality",
        "case \"$1\" in *a.pdf) printf 'First Quality';; *) printf 'Wrong Quality';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "required_text": ["First Quality"]
              }
            },
            {
              "path": "b.pdf",
              "expect": {
                "required_text": ["Second Quality"]
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
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--require-baseline-quality",
        ])
        .output()
        .expect("run glyphrush bench directory requiring baseline quality");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(baseline_summary["quality_status"], "checked");
    assert_eq!(baseline_summary["quality_documents"], 2);
    assert_eq!(baseline_summary["quality_failed_documents"], 1);
    assert_eq!(json["requirements"]["require_quality"], false);
    assert_eq!(json["requirements"]["require_baselines"], false);
    assert_eq!(json["requirements"]["require_baseline_quality"], true);
    assert_eq!(
        baseline_summary["quality_failure_samples"][0]["path"],
        "b.pdf"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench baseline quality required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_directory_with_eval_manifest_aggregates_baseline_quality_by_category() {
    let dir = temp_dir("bench-dir-baseline-quality-category");
    fs::write(dir.join("a.pdf"), minimal_pdf("Clean Baseline Category")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Scanned Baseline Category")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-quality-category",
        "case \"$1\" in *a.pdf) printf 'Clean Baseline Category';; *) printf 'Wrong Baseline Category';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean Baseline Category"]
              }
            },
            {
              "path": "b.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["Scanned Baseline Category"]
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
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench directory with baseline category quality");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["quality_documents"], 2);
    assert_eq!(baseline_summary["quality_failed_documents"], 1);
    assert_eq!(
        baseline_summary["quality_category_summaries"]["clean_digital"],
        serde_json::json!({
            "document_count": 1,
            "page_count": 1,
            "passed_documents": 1,
            "failed_documents": 0,
            "failed_checks": 0,
            "quality_pass_rate": 1.0,
            "quality_passed": true,
            "quality_failed": false
        })
    );
    assert_eq!(
        baseline_summary["quality_category_summaries"]["scanned"],
        serde_json::json!({
            "document_count": 1,
            "page_count": 1,
            "passed_documents": 0,
            "failed_documents": 1,
            "failed_checks": 1,
            "quality_pass_rate": 0.0,
            "quality_passed": false,
            "quality_failed": true
        })
    );
}

#[test]
fn bench_eval_category_filters_baseline_quality_expectations() {
    let dir = temp_dir("bench-dir-baseline-quality-category-filter");
    fs::write(dir.join("a.pdf"), minimal_pdf("Clean Filtered Baseline")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Scanned Filtered Baseline")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-quality-category-filter",
        "case \"$1\" in *a.pdf) printf 'Clean Filtered Baseline';; *) printf 'Wrong Filtered Baseline';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean Filtered Baseline"]
              }
            },
            {
              "path": "b.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["Scanned Filtered Baseline"]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = run_json([
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--eval-category",
        "clean_digital",
    ]);
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["quality_status"], "partially_checked");
    assert_eq!(baseline_summary["quality_documents"], 1);
    assert_eq!(baseline_summary["quality_unchecked_documents"], 1);
    assert_eq!(baseline_summary["quality_passed_documents"], 1);
    assert_eq!(baseline_summary["quality_failed_documents"], 0);
    assert_eq!(
        baseline_summary["quality_category_summaries"],
        serde_json::json!({
            "clean_digital": {
                "document_count": 1,
                "page_count": 1,
                "passed_documents": 1,
                "failed_documents": 0,
                "failed_checks": 0,
                "quality_pass_rate": 1.0,
                "quality_passed": true,
                "quality_failed": false
            }
        })
    );
    assert_eq!(
        json["documents"][0]["baselines"][0]["quality"]["category"],
        "clean_digital"
    );
    assert!(json["documents"][1]["baselines"][0]["quality"].is_null());
}

#[test]
fn bench_directory_baseline_quality_summary_counts_failed_check_types() {
    let dir = temp_dir("bench-dir-baseline-quality-check-types");
    fs::write(
        dir.join("b.pdf"),
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| First | Second |) Tj T* (| A | B |) Tj ET",
        ),
    )
    .unwrap();
    fs::write(
        dir.join("a.pdf"),
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| First | Second |) Tj T* (| A | B |) Tj ET",
        ),
    )
    .unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-quality-check-types",
        "case \"$1\" in *a.pdf) printf '| First | Second |\\n| A | B |';; *) printf '| Second | First |\\n| A | C |';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "required_text": ["| First | Second |"],
                "text_recall": {
                  "expected": "First Second A B",
                  "min_word_recall": 1.0,
                  "min_char_recall": 1.0
                },
                "reading_order": {
                  "expected_sequence": ["First", "Second"],
                  "min_score": 1.0
                },
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["First", "Second"], ["A", "B"]],
                    "min_cell_recall": 1.0
                  }
                ]
              }
            },
            {
              "path": "b.pdf",
              "expect": {
                "required_text": ["| First | Second |"],
                "text_recall": {
                  "expected": "First Second A B",
                  "min_word_recall": 1.0,
                  "min_char_recall": 1.0
                },
                "reading_order": {
                  "expected_sequence": ["First", "Second"],
                  "min_score": 1.0
                },
                "table_structure": [
                  {
                    "page": 0,
                    "expected_rows": [["First", "Second"], ["A", "B"]],
                    "min_cell_recall": 1.0
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
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush bench directory with baseline quality check-type counters");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["quality_documents"], 2);
    assert_eq!(baseline_summary["quality_failed_documents"], 1);
    assert_eq!(baseline_summary["quality_failed_checks"], 4);
    assert_eq!(
        baseline_summary["quality_required_text_failed_documents"],
        1
    );
    assert_eq!(baseline_summary["quality_text_recall_failed_documents"], 1);
    assert_eq!(
        baseline_summary["quality_reading_order_failed_documents"],
        1
    );
    assert_eq!(
        baseline_summary["quality_table_structure_failed_documents"],
        1
    );
    assert_eq!(
        baseline_summary["quality_failure_samples"],
        serde_json::json!([
            {
                "path": "b.pdf",
                "failed_checks": 4,
                "failed_check_types": [
                    "required_text",
                    "text_recall",
                    "reading_order",
                    "table_structure"
                ]
            }
        ])
    );
}

#[test]
fn bench_directory_baseline_summary_separates_successful_and_failed_pages() {
    let dir = temp_dir("bench-dir-baseline-partial");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First baseline")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-partial",
        "case \"$1\" in *b.pdf) printf 'bad baseline' >&2; exit 9;; *) printf 'ok';; esac",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench directory with partially failing baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["document_count"], 2);
    assert_eq!(baseline_summary["successful_documents"], 1);
    assert_eq!(baseline_summary["failed_documents"], 1);
    assert_eq!(baseline_summary["timed_out_documents"], 0);
    assert_eq!(baseline_summary["timed_out_pages"], 0);
    assert_eq!(baseline_summary["successful_pages"], 1);
    assert_eq!(baseline_summary["failed_pages"], 1);
    assert_eq!(baseline_summary["success_rate"], 0.5);
    assert_eq!(baseline_summary["comparison"]["speed_comparable"], false);
    assert_eq!(baseline_summary["comparison"]["glyphrush_speedup"], 0.0);
    assert_eq!(baseline_summary["failure_samples"][0]["path"], "b.pdf");
    assert_eq!(
        baseline_summary["failure_samples"][0]["error_kind"],
        "execution_failed"
    );
    assert_eq!(
        json["documents"][1]["baselines"][0]["error_kind"],
        "execution_failed"
    );
    assert!(
        baseline_summary["failure_samples"][0]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("bad baseline")
    );
    assert!(
        baseline_summary["successful_pages_per_sec"]
            .as_f64()
            .unwrap()
            > 0.0
    );
}

#[test]
fn bench_directory_require_baselines_rejects_partial_failures_after_writing_json() {
    let dir = temp_dir("bench-dir-require-baseline-partial");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First baseline")).unwrap();
    let baseline = write_baseline_script(
        "require-baseline-dir-partial",
        "case \"$1\" in *b.pdf) printf 'strict corpus baseline failed' >&2; exit 9;; *) printf 'ok';; esac",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--require-baselines",
        ])
        .output()
        .expect("run glyphrush bench directory requiring baselines");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["document_count"], 2);
    assert_eq!(baseline_summary["successful_documents"], 1);
    assert_eq!(baseline_summary["failed_documents"], 1);
    assert_eq!(baseline_summary["failure_samples"][0]["path"], "b.pdf");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench baselines required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_directory_require_speedup_rejects_slow_glyphrush_after_writing_json() {
    let dir = temp_dir("bench-dir-require-speedup");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second speedup baseline")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First speedup baseline")).unwrap();
    let baseline = write_baseline_script("baseline-dir-fast", "printf 'fast baseline'");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--require-speedup",
            "mock=1000000.0",
        ])
        .output()
        .expect("run glyphrush directory bench requiring speedup");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(
        json["requirements"]["require_speedups"],
        serde_json::json!([
            {
                "baseline": "mock",
                "min_glyphrush_speedup": 1000000.0
            }
        ])
    );
    assert_eq!(baseline_summary["name"], "mock");
    assert_eq!(baseline_summary["successful_documents"], 2);
    assert_eq!(baseline_summary["failed_documents"], 0);
    assert!(
        baseline_summary["comparison"]["speed_comparable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        baseline_summary["comparison"]["glyphrush_speedup"]
            .as_f64()
            .unwrap()
            < 1000000.0
    );
    let claim = &json["speedup_claims"][0];
    assert_eq!(claim["baseline"], "mock");
    assert_eq!(claim["required_glyphrush_speedup"], 1000000.0);
    assert!(
        claim["actual_glyphrush_speedup"].as_f64().unwrap() < 1000000.0,
        "claim should preserve measured speedup: {claim:?}"
    );
    assert_eq!(claim["speed_comparable"], true);
    assert_eq!(claim["speed_passed"], false);
    assert_eq!(claim["quality_backed"], false);
    assert_eq!(claim["claim_passed"], false);
    assert_eq!(claim["status"], "speedup_failed");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench speedup required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bench_directory_require_speedup_claim_accepts_quality_backed_speedup() {
    let dir = temp_dir("bench-dir-require-speedup-claim");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second Claim Quality")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First Claim Quality")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-claim-quality",
        "case \"$1\" in *a.pdf) printf 'First Claim Quality';; *) printf 'Second Claim Quality';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "a.pdf",
              "expect": {
                "required_text": ["First Claim Quality"]
              }
            },
            {
              "path": "b.pdf",
              "expect": {
                "required_text": ["Second Claim Quality"]
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
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--eval-manifest",
            manifest_path.to_str().unwrap(),
            "--require-speedup-claim",
            "mock=0.000001",
        ])
        .output()
        .expect("run glyphrush directory bench requiring speedup claim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let claim = &json["speedup_claims"][0];

    assert_eq!(
        json["requirements"]["require_speedup_claims"],
        serde_json::json!([
            {
                "baseline": "mock",
                "min_glyphrush_speedup": 0.000001
            }
        ])
    );
    assert_eq!(json["quality_status"], "checked");
    assert_eq!(json["quality"]["quality_passed"], true);
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][0]["quality_failed_documents"], 0);
    assert_eq!(claim["baseline"], "mock");
    assert_eq!(claim["speed_comparable"], true);
    assert_eq!(claim["speed_passed"], true);
    assert_eq!(claim["glyphrush_quality_checked"], true);
    assert_eq!(claim["glyphrush_quality_passed"], true);
    assert_eq!(claim["baseline_quality_checked"], true);
    assert_eq!(claim["baseline_quality_passed"], true);
    assert_eq!(claim["quality_backed"], true);
    assert_eq!(claim["claim_passed"], true);
    assert_eq!(claim["status"], "passed");
}

#[test]
fn bench_directory_baseline_summary_counts_timed_out_documents() {
    let dir = temp_dir("bench-dir-baseline-timeout");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second timeout baseline")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First timeout baseline")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-timeout",
        "case \"$1\" in *b.pdf) exec sleep 30;; *) printf 'ok';; esac",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
            "--baseline-timeout-ms",
            "5000",
        ])
        .output()
        .expect("run glyphrush bench directory with timed-out baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["document_count"], 2);
    assert_eq!(baseline_summary["successful_documents"], 1);
    assert_eq!(baseline_summary["failed_documents"], 1);
    assert_eq!(baseline_summary["timed_out_documents"], 1);
    assert_eq!(baseline_summary["timed_out_pages"], 1);
    assert_eq!(baseline_summary["successful_pages"], 1);
    assert_eq!(baseline_summary["failed_pages"], 1);
    assert_eq!(baseline_summary["comparison"]["speed_comparable"], false);
    assert_eq!(baseline_summary["failure_samples"][0]["path"], "b.pdf");
    assert_eq!(
        baseline_summary["failure_samples"][0]["error_kind"],
        "timeout"
    );
    assert!(
        baseline_summary["failure_samples"][0]["error"]
            .as_str()
            .unwrap()
            .contains("timed out after 5000 ms")
    );
    assert_eq!(json["documents"][0]["baselines"][0]["timed_out"], false);
    assert_eq!(json["documents"][1]["baselines"][0]["timed_out"], true);
    assert_eq!(
        json["documents"][1]["baselines"][0]["error_kind"],
        "timeout"
    );
}

#[test]
fn bench_directory_baseline_summary_reports_successful_empty_outputs() {
    let dir = temp_dir("bench-dir-baseline-empty-output");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First baseline")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-empty",
        "case \"$1\" in *b.pdf) exit 0;; *) printf 'ok';; esac",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "bench",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("mock={}", baseline.display()),
        ])
        .output()
        .expect("run glyphrush bench directory with empty-output baseline");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["successful_documents"], 2);
    assert_eq!(baseline_summary["failed_documents"], 0);
    assert_eq!(baseline_summary["empty_output_documents"], 1);
    assert_eq!(baseline_summary["empty_output_pages"], 1);
    assert_eq!(json["documents"][0]["baselines"][0]["empty_output"], false);
    assert_eq!(json["documents"][1]["baselines"][0]["empty_output"], true);
}

#[test]
fn baseline_wrapper_describe_modes_identify_comparison_targets() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wrappers = [
        (
            "liteparse-text.sh",
            "liteparse",
            "run-llama/liteparse",
            "lit parse",
        ),
        (
            "liteparse-no-ocr-text.sh",
            "liteparse-no-ocr",
            "run-llama/liteparse",
            "--no-ocr",
        ),
        ("pymupdf-text.sh", "pymupdf", "PyMuPDF", "page.get_text"),
        (
            "pdfplumber-text.sh",
            "pdfplumber",
            "pdfplumber",
            "extract_text",
        ),
        ("marker-text.sh", "marker", "Marker", "marker_single"),
        ("docling-text.sh", "docling", "Docling", "docling"),
    ];

    for (script, name, target, command_hint) in wrappers {
        let output = Command::new(workspace_root.join("tools/baselines").join(script))
            .arg("--describe")
            .output()
            .unwrap_or_else(|error| panic!("run {script} --describe: {error}"));

        assert!(
            output.status.success(),
            "{script} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: Value =
            serde_json::from_slice(&output.stdout).expect("baseline describe output is json");
        assert_eq!(json["name"], name);
        assert_eq!(json["target"], target);
        assert!(
            json["command_hint"]
                .as_str()
                .unwrap()
                .contains(command_hint),
            "{script} command_hint: {}",
            json["command_hint"]
        );
    }
}

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

#[test]
fn python_baseline_wrappers_use_project_local_venv_python() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = temp_dir("baseline-local-python");
    let pdf_path = root.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Local Python baseline")).unwrap();
    let python = root
        .join(".glyphrush-baselines")
        .join("venv")
        .join("bin")
        .join("python3");
    write_executable(
        &python,
        "#!/bin/sh\nprintf 'local python %s\\n' \"${2:-missing-pdf}\"\n",
    );

    for script in ["pymupdf-text.sh", "pdfplumber-text.sh"] {
        let output = Command::new(workspace_root.join("tools/baselines").join(script))
            .env("GLYPHRUSH_BASELINE_ROOT", &root)
            .env_remove("GLYPHRUSH_BASELINE_PYTHON")
            .arg(&pdf_path)
            .output()
            .unwrap_or_else(|error| panic!("run {script} with project-local python: {error}"));

        assert!(
            output.status.success(),
            "{script} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("local python {}\n", pdf_path.display())
        );
    }
}

#[test]
fn tesseract_rendered_image_ocr_wrapper_describes_and_invokes_tesseract() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wrapper = workspace_root.join("tools/ocr/tesseract-rendered-image.sh");

    let describe = Command::new(&wrapper)
        .arg("--describe")
        .output()
        .expect("run tesseract OCR wrapper --describe");
    assert!(
        describe.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&describe.stderr)
    );
    let describe_json: Value =
        serde_json::from_slice(&describe.stdout).expect("OCR wrapper describe output is json");
    assert_eq!(describe_json["name"], "tesseract-rendered-image");
    assert_eq!(describe_json["target"], "Tesseract OCR");
    assert_eq!(describe_json["input"], "rendered-image");
    assert_eq!(describe_json["requires"], serde_json::json!(["tesseract"]));

    let dir = temp_dir("tesseract-rendered-image-wrapper");
    let image = dir.join("page.ppm");
    let log_path = dir.join("tesseract.log");
    fs::write(&image, b"P6\n1 1\n255\n\0\0\0").unwrap();
    let fake_tesseract = dir.join("tesseract");
    write_executable(
        &fake_tesseract,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf 'Fake OCR text from %s\\n' \"$1\"\n",
            log_path.display()
        ),
    );

    let output = Command::new(&wrapper)
        .env("TESSERACT_BIN", &fake_tesseract)
        .env("TESSERACT_LANG", "eng")
        .env("TESSERACT_PSM", "6")
        .arg(&image)
        .arg("3")
        .output()
        .expect("run tesseract OCR wrapper");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("Fake OCR text from {}\n", image.display())
    );
    assert_eq!(
        fs::read_to_string(log_path).unwrap(),
        format!("{} stdout -l eng --psm 6\n", image.display())
    );
}

#[test]
fn baseline_check_reports_wrapper_describe_health() {
    let healthy = write_baseline_script(
        "baseline-check-healthy",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"mock","target":"Mock Parser","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'mock baseline output'"#,
    );
    let missing = temp_dir("baseline-check-missing").join("missing-baseline.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline",
            &format!("mock={}", healthy.display()),
            "--baseline",
            &format!("missing={}", missing.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["report_version"], "glyphrush-baseline-check-report-v1");
    assert_eq!(json["baseline_count"], 2);
    assert_eq!(json["describe_success_count"], 1);
    assert_eq!(json["all_described"], false);
    assert_eq!(json["baselines"][0]["name"], "mock");
    assert_eq!(json["baselines"][0]["describe"]["success"], true);
    assert_eq!(json["baselines"][0]["describe"]["valid_json_object"], true);
    assert_eq!(json["baselines"][0]["description"]["target"], "Mock Parser");
    assert_eq!(json["baselines"][1]["name"], "missing");
    assert_eq!(json["baselines"][1]["describe"]["success"], false);
    assert_eq!(
        json["baselines"][1]["describe"]["error_kind"],
        "spawn_failed"
    );
    assert!(
        json["baselines"][1]["describe"]["error"]
            .as_str()
            .unwrap()
            .contains("missing-baseline.sh")
    );
}

#[test]
fn baseline_check_classifies_failed_describe_probe_kinds() {
    let execution_failed = write_baseline_script(
        "baseline-check-describe-execution-failed",
        r#"if [ "${1:-}" = "--describe" ]; then printf 'describe failed' >&2; exit 7; fi
printf 'unused'"#,
    );
    let missing_dependency = write_baseline_script(
        "baseline-check-describe-missing-dependency",
        r#"if [ "${1:-}" = "--describe" ]; then printf 'lit missing' >&2; exit 127; fi
printf 'unused'"#,
    );
    let invalid = write_baseline_script(
        "baseline-check-describe-invalid",
        r#"if [ "${1:-}" = "--describe" ]; then printf 'not json'; exit 0; fi
printf 'unused'"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline",
            &format!("execution={}", execution_failed.display()),
            "--baseline",
            &format!("dependency={}", missing_dependency.display()),
            "--baseline",
            &format!("invalid={}", invalid.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check with failed describe probes");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["describe_success_count"], 0);
    assert_eq!(json["all_described"], false);
    assert_eq!(
        json["baselines"][0]["describe"]["error_kind"],
        "execution_failed"
    );
    assert_eq!(
        json["baselines"][1]["describe"]["error_kind"],
        "missing_dependency"
    );
    assert_eq!(
        json["baselines"][2]["describe"]["error_kind"],
        "invalid_describe_output"
    );
}

#[test]
fn baseline_check_classifies_timed_out_describe_probe_kind() {
    let slow = write_baseline_script(
        "baseline-check-describe-timeout",
        r#"if [ "${1:-}" = "--describe" ]; then sleep 2; printf '{"name":"slow"}'; exit 0; fi
printf 'unused'"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline",
            &format!("slow={}", slow.display()),
            "--baseline-timeout-ms",
            "50",
        ])
        .output()
        .expect("run glyphrush baseline-check with timed out describe probe");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baselines"][0]["describe"]["timed_out"], true);
    assert_eq!(json["baselines"][0]["describe"]["error_kind"], "timeout");
}

#[test]
fn baseline_check_preset_describes_core_glyphrush_v0_baselines() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .current_dir(&workspace_root)
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline-preset",
            "glyphrush-v0",
        ])
        .output()
        .expect("run glyphrush baseline-check with baseline preset");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baseline_count"], 4);
    assert_eq!(
        json["requested_baseline_presets"],
        serde_json::json!(["glyphrush-v0"])
    );
    assert_eq!(json["describe_success_count"], 4);
    assert_eq!(json["all_described"], true);
    assert_eq!(
        json["baselines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|baseline| baseline["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["liteparse", "liteparse-no-ocr", "pymupdf", "pdfplumber"]
    );
    assert_eq!(
        json["baselines"][0]["description"]["target"],
        "run-llama/liteparse"
    );
    assert_eq!(json["baselines"][2]["description"]["target"], "PyMuPDF");
    assert_eq!(json["baselines"][3]["description"]["target"], "pdfplumber");
}

#[test]
fn bench_reports_requested_baseline_preset() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pdf_path = write_test_pdf("bench-baseline-preset", "Benchmark baseline preset");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .current_dir(&workspace_root)
        .args([
            "--backend",
            "lopdf",
            "bench",
            pdf_path.to_str().unwrap(),
            "--baseline-preset",
            "glyphrush-v0",
        ])
        .output()
        .expect("run glyphrush bench with baseline preset");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("bench output is json");

    assert_eq!(
        json["requested_baseline_presets"],
        serde_json::json!(["glyphrush-v0"])
    );
    assert_eq!(json["baselines"].as_array().unwrap().len(), 4);
}

#[test]
fn baseline_check_can_smoke_test_wrappers_against_pdf() {
    let pdf_path = write_test_pdf("baseline-check-smoke-pdf", "Smoke baseline PDF");
    let ok = write_baseline_script(
        "baseline-check-smoke-ok",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"ok","target":"Smoke OK","kind":"text-baseline-wrapper"}'; exit 0; fi
if [ -f "${1:-}" ]; then printf 'smoke output\n'; exit 0; fi
printf 'missing pdf' >&2
exit 66"#,
    );
    let failing = write_baseline_script(
        "baseline-check-smoke-failing",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"failing","target":"Smoke Failing","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'parser dependency missing' >&2
exit 127"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("ok={}", ok.display()),
            "--baseline",
            &format!("failing={}", failing.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check with smoke PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baseline_count"], 2);
    assert_eq!(json["describe_success_count"], 2);
    assert_eq!(json["all_described"], true);
    assert_eq!(
        json["smoke_pdf"].as_str().unwrap(),
        pdf_path.to_string_lossy()
    );
    assert_eq!(json["smoke_success_count"], 1);
    assert_eq!(json["all_smoke_passed"], false);

    assert_eq!(json["baselines"][0]["name"], "ok");
    assert_eq!(json["baselines"][0]["smoke"]["success"], true);
    assert_eq!(json["baselines"][0]["smoke"]["exit_status"], 0);
    assert_eq!(json["baselines"][0]["smoke"]["output_bytes"], 13);
    assert_eq!(
        json["baselines"][0]["smoke"]["stdout_sha256"],
        sha256_hex("smoke output\n")
    );
    assert_eq!(json["baselines"][0]["smoke"]["stdout_line_count"], 1);
    assert_eq!(json["baselines"][0]["smoke"]["stdout_word_count"], 2);
    assert_eq!(json["baselines"][0]["smoke"]["empty_output"], false);
    assert_eq!(json["baselines"][0]["smoke"]["error"], Value::Null);

    assert_eq!(json["baselines"][1]["name"], "failing");
    assert_eq!(json["baselines"][1]["smoke"]["success"], false);
    assert_eq!(json["baselines"][1]["smoke"]["exit_status"], 127);
    assert_eq!(
        json["baselines"][1]["smoke"]["error_kind"],
        "missing_dependency"
    );
    assert!(
        json["baselines"][1]["smoke"]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing")
    );
    assert!(
        json["baselines"][1]["smoke"]["error"]
            .as_str()
            .unwrap()
            .contains("status Some(127)")
    );
}

#[test]
fn baseline_check_can_smoke_test_wrappers_against_directory() {
    let dir = temp_dir("baseline-check-smoke-dir");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline smoke")).unwrap();
    fs::write(dir.join("a.PDF"), minimal_pdf("First baseline smoke")).unwrap();
    fs::write(dir.join("ignore.txt"), "not a pdf").unwrap();
    let ok = write_baseline_script(
        "baseline-check-smoke-dir-ok",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"ok","target":"Directory Smoke OK","kind":"text-baseline-wrapper"}'; exit 0; fi
if [ -f "${1:-}" ]; then printf 'dir smoke %s\n' "$(basename "$1")"; exit 0; fi
printf 'missing pdf' >&2
exit 66"#,
    );
    let failing = write_baseline_script(
        "baseline-check-smoke-dir-failing",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"failing","target":"Directory Smoke Failing","kind":"text-baseline-wrapper"}'; exit 0; fi
if [ "$(basename "${1:-}")" = "b.pdf" ]; then printf 'parser dependency missing for b.pdf' >&2; exit 127; fi
printf 'partial smoke %s\n' "$(basename "$1")""#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--pdf",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("ok={}", ok.display()),
            "--baseline",
            &format!("failing={}", failing.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check with smoke directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baseline_count"], 2);
    assert_eq!(json["smoke_pdf"].as_str().unwrap(), dir.to_string_lossy());
    assert_eq!(json["smoke_document_count"], 2);
    assert_eq!(json["smoke_success_count"], 1);
    assert_eq!(json["all_smoke_passed"], false);

    assert_eq!(json["baselines"][0]["name"], "ok");
    assert_eq!(json["baselines"][0]["smoke"]["success"], true);
    assert_eq!(json["baselines"][0]["smoke"]["document_count"], 2);
    assert_eq!(json["baselines"][0]["smoke"]["successful_documents"], 2);
    assert_eq!(json["baselines"][0]["smoke"]["failed_documents"], 0);
    assert_eq!(
        json["baselines"][0]["smoke"]["documents"][0]["path"],
        "a.PDF"
    );
    assert_eq!(
        json["baselines"][0]["smoke"]["documents"][1]["path"],
        "b.pdf"
    );
    assert_eq!(
        json["baselines"][0]["smoke"]["documents"][0]["stdout_sha256"],
        sha256_hex("dir smoke a.PDF\n")
    );

    assert_eq!(json["baselines"][1]["name"], "failing");
    assert_eq!(json["baselines"][1]["smoke"]["success"], false);
    assert_eq!(json["baselines"][1]["smoke"]["document_count"], 2);
    assert_eq!(json["baselines"][1]["smoke"]["successful_documents"], 1);
    assert_eq!(json["baselines"][1]["smoke"]["failed_documents"], 1);
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][0]["success"],
        true
    );
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][1]["success"],
        false
    );
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][1]["exit_status"],
        127
    );
    assert!(
        json["baselines"][1]["smoke"]["documents"][1]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing for b.pdf")
    );
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][1]["error_kind"],
        "missing_dependency"
    );

    let failure_samples = json["baselines"][1]["smoke"]["failure_samples"]
        .as_array()
        .unwrap();
    assert_eq!(failure_samples.len(), 1);
    assert_eq!(failure_samples[0]["path"], "b.pdf");
    assert_eq!(failure_samples[0]["exit_status"], 127);
    assert_eq!(failure_samples[0]["error_kind"], "missing_dependency");
    assert!(
        failure_samples[0]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing for b.pdf")
    );
    assert!(
        failure_samples[0]["error"]
            .as_str()
            .unwrap()
            .contains("status Some(127)")
    );
}

#[test]
fn baseline_check_strict_passes_when_describe_and_smoke_pass() {
    let pdf_path = write_test_pdf("baseline-check-strict-pass-pdf", "Strict baseline PDF");
    let ok = write_baseline_script(
        "baseline-check-strict-ok",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"ok","target":"Strict OK","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'strict smoke output\n'"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--strict",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("ok={}", ok.display()),
        ])
        .output()
        .expect("run strict glyphrush baseline-check with healthy smoke PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["strict"], true);
    assert_eq!(json["all_described"], true);
    assert_eq!(json["all_smoke_passed"], true);
    assert_eq!(json["smoke_success_count"], 1);
}

#[test]
fn baseline_check_strict_exits_nonzero_when_smoke_fails_after_writing_json() {
    let pdf_path = write_test_pdf("baseline-check-strict-fail-pdf", "Strict failing PDF");
    let failing = write_baseline_script(
        "baseline-check-strict-failing",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"failing","target":"Strict Failing","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'parser dependency missing' >&2
exit 127"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--strict",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("failing={}", failing.display()),
        ])
        .output()
        .expect("run strict glyphrush baseline-check with failing smoke PDF");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["strict"], true);
    assert_eq!(json["all_described"], true);
    assert_eq!(json["all_smoke_passed"], false);
    assert_eq!(json["smoke_success_count"], 0);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("baseline-check strict failed"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn inspect_directory_reports_sorted_pdf_inventory() {
    let dir = temp_dir("inspect-dir");
    fs::write(dir.join("two.PDF"), minimal_pdf("Two")).unwrap();
    fs::write(dir.join("one.pdf"), minimal_pdf("One")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(["--backend", "lopdf", "inspect", dir.to_str().unwrap()])
        .output()
        .expect("run glyphrush inspect on directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("inspect directory output is json");

    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["document_count"], 2);
    assert_eq!(json["page_count"], 2);
    assert_eq!(
        json["corpus_fingerprint"],
        expected_corpus_fingerprint(&json)
    );
    assert_eq!(json["corpus_fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(json["documents"][0]["path"].as_str().unwrap(), "one.pdf");
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
        fs::metadata(dir.join("one.pdf")).unwrap().len()
    );
    assert_eq!(json["documents"][1]["path"].as_str().unwrap(), "two.PDF");
    assert_eq!(json["documents"][1]["metadata"]["parser_name"], "glyphrush");
    assert_eq!(
        json["documents"][1]["metadata"]["source_size_bytes"],
        fs::metadata(dir.join("two.PDF")).unwrap().len()
    );
}

#[test]
fn inspect_directory_pages_reports_sorted_corpus_triage() {
    let dir = temp_dir("inspect-dir-pages");
    fs::write(
        dir.join("b-scan.pdf"),
        minimal_pdf_with_full_page_image_and_text("tiny"),
    )
    .unwrap();
    fs::write(dir.join("a-native.pdf"), minimal_pdf("Native directory")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            dir.to_str().unwrap(),
            "--pages",
        ])
        .output()
        .expect("run glyphrush inspect --pages on directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("inspect directory pages output is json");

    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["document_count"], 2);
    assert_eq!(json["page_count"], 2);
    assert_eq!(json["fallback_pages"], 1);
    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["ocr_applied_pages"], 0);
    assert_eq!(json["warnings_count"], 1);
    assert_eq!(
        json["corpus_fingerprint"],
        expected_corpus_fingerprint(&json)
    );
    assert_eq!(json["documents"][0]["path"], "a-native.pdf");
    assert_eq!(json["documents"][0]["page_count"], 1);
    assert_eq!(json["documents"][0]["fallback_pages"], 0);
    assert_eq!(
        json["documents"][0]["pages"][0]["route"],
        "native_fast_path"
    );
    assert_eq!(
        json["documents"][0]["pages"][0]["quality_flags"],
        serde_json::json!([])
    );

    assert_eq!(json["documents"][1]["path"], "b-scan.pdf");
    assert_eq!(json["documents"][1]["page_count"], 1);
    assert_eq!(json["documents"][1]["fallback_pages"], 1);
    assert_eq!(json["documents"][1]["ocr_required_pages"], 1);
    assert_eq!(json["documents"][1]["warnings_count"], 1);
    assert_eq!(json["documents"][1]["pages"][0]["page_index"], 0);
    assert_eq!(json["documents"][1]["pages"][0]["route"], "ocr_fallback");
    assert_eq!(
        json["documents"][1]["pages"][0]["quality_flags"],
        serde_json::json!(["requires_ocr", "low_confidence_text"])
    );
    assert_eq!(
        json["documents"][1]["pages"][0]["warnings"],
        serde_json::json!(["p000000: requires_ocr_without_ocr_output"])
    );
}

#[test]
fn inspect_directory_pages_jobs_preserve_sorted_corpus_triage() {
    let dir = temp_dir("inspect-dir-pages-jobs");
    fs::write(
        dir.join("c-scan.pdf"),
        minimal_pdf_with_full_page_image_and_text("tiny"),
    )
    .unwrap();
    fs::write(dir.join("a-native.pdf"), minimal_pdf("Native jobs A")).unwrap();
    fs::write(dir.join("b-native.pdf"), minimal_pdf("Native jobs B")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            dir.to_str().unwrap(),
            "--pages",
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush inspect --pages on directory with jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("inspect directory pages output is json");

    assert_eq!(json["worker_count"], 2);
    assert_eq!(json["document_count"], 3);
    assert_eq!(json["page_count"], 3);
    assert_eq!(json["fallback_pages"], 1);
    assert_eq!(json["ocr_required_pages"], 1);
    assert_eq!(json["warnings_count"], 1);
    assert_eq!(
        json["corpus_fingerprint"],
        expected_corpus_fingerprint(&json)
    );
    assert_eq!(json["documents"][0]["path"], "a-native.pdf");
    assert_eq!(json["documents"][1]["path"], "b-native.pdf");
    assert_eq!(json["documents"][2]["path"], "c-scan.pdf");
    assert_eq!(
        json["documents"][2]["pages"][0]["quality_flags"],
        serde_json::json!(["requires_ocr", "low_confidence_text"])
    );
}

#[test]
fn inspect_directory_pages_with_cache_dir_aggregates_hits_and_misses() {
    let dir = temp_dir("inspect-dir-pages-cache");
    fs::write(dir.join("b.pdf"), minimal_pdf("Inspect cache B")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("Inspect cache A")).unwrap();
    let cache_dir = dir.join("cache");

    let first = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            dir.to_str().unwrap(),
            "--pages",
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush inspect directory --pages with cache miss");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_json: Value =
        serde_json::from_slice(&first.stdout).expect("inspect directory output is json");

    let second = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "inspect",
            dir.to_str().unwrap(),
            "--pages",
            "--cache-dir",
            cache_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush inspect directory --pages with cache hit");
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_json: Value =
        serde_json::from_slice(&second.stdout).expect("inspect directory output is json");

    assert_eq!(first_json["cache_hits"], 0);
    assert_eq!(first_json["cache_misses"], 2);
    assert_eq!(second_json["cache_hits"], 2);
    assert_eq!(second_json["cache_misses"], 0);
    assert_eq!(second_json["documents"][0]["path"], "a.pdf");
    assert_eq!(second_json["documents"][1]["path"], "b.pdf");
    assert_eq!(second_json["documents"][0]["cache_status"], "hit");
    assert_eq!(second_json["documents"][1]["cache_status"], "hit");
    assert_eq!(
        second_json["corpus_fingerprint"],
        expected_corpus_fingerprint(&second_json)
    );
}

#[test]
fn debug_page_explains_classifier_decision_for_a_page() {
    let pdf_path = write_test_pdf("debug", "Hello Glyphrush");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on one page of a multi-page PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on inherited rotation PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on inherited media box PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on CropBox PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on hybrid image");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
            "--ocr-sidecar",
            sidecar_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush debug-page with ocr sidecar");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on form-wrapped image");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on substantial hybrid image");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "debug-page",
            pdf_path.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run glyphrush debug-page on ruled table");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("debug output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with layout-reflowed required text");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

    assert_eq!(json["passed"], true);
    assert_eq!(
        json["documents"][0]["checks"]["required_text"]["actual"]["missing"],
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

    let json = run_json(["eval", manifest_path.to_str().unwrap()]);

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

    let json = run_json([
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--category",
            "scanned",
        ])
        .output()
        .expect("run glyphrush eval with empty category selection");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with empty manifest");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with required category coverage");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with minimum category coverage");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let json = run_json([
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--jobs",
            "2",
        ])
        .output()
        .expect("run glyphrush eval with jobs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with backend-specific expectations");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let first = run_json([
        "eval",
        manifest_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let second = run_json([
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

    let json = run_json(["eval", manifest_path.to_str().unwrap()]);

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with stale source provenance");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with warning expectations");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with missing warning expectation");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with route count checks");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing route count check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with quality flag count checks");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing quality flag count check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with image artifact count checks");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing image artifact count check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with page image artifact count checks");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing page image artifact count check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with route reason count checks");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing route reason count check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with text recall");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush eval with span bbox");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing text recall");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with reading-order check");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush eval with two-column reading-order check");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush eval with heading plus two-column reading-order check");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing reading-order check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with OCR-required classification");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing OCR-required classification");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with quality-flag classification");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing quality-flag classification");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with silent-failure check");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing silent-failure check");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with empty text output page");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with expected empty text output page");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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
                    "min_cell_f1": 1.0
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
        .expect("run glyphrush eval with table structure");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with multiple same-page table expectations");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with empty table cells");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with ruled table structure");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush eval with positioned table structure");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");
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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval with failing table structure");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
            "--span-geometry",
        ])
        .output()
        .expect("run glyphrush eval with page layout block counts");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "eval",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("run glyphrush eval");

    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("eval output is json");

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

fn write_test_pdf(label: &str, text: &str) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("sample.pdf");
    fs::write(&path, minimal_pdf(text)).unwrap();
    path
}

fn write_baseline_script(label: &str, body: &str) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("baseline.sh");
    write_executable(&path, &format!("#!/bin/sh\n{body}\n"));
    path
}

fn write_executable(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn write_ocr_command_script(label: &str, log_path: &std::path::Path) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("ocr-command.sh");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s:%s\\n' \"$1\" \"$2\" >> '{}'\nprintf 'Command OCR text page %s' \"$2\"\n",
            log_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn write_rendered_ocr_command_script(label: &str, log_path: &std::path::Path) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("rendered-ocr-command.sh");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ ! -f \"$1\" ]; then echo 'missing rendered image' >&2; exit 2; fi\nheader=$(dd if=\"$1\" bs=2 count=1 2>/dev/null)\nbytes=$(wc -c < \"$1\" | tr -d ' ')\nprintf '%s\\t%s\\t%s\\t%s\\n' \"$1\" \"$2\" \"$header\" \"$bytes\" >> '{}'\nprintf 'Rendered OCR text page %s' \"$2\"\n",
            log_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn start_ocr_http_server(
    response_text: &'static str,
) -> (String, Receiver<String>, JoinHandle<()>) {
    start_ocr_http_server_with_response("text/plain", response_text)
}

fn start_ocr_http_server_with_response(
    content_type: &'static str,
    response_body: &'static str,
) -> (String, Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind OCR HTTP test server");
    let url = format!(
        "http://{}/ocr",
        listener.local_addr().expect("read OCR HTTP server addr")
    );
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept OCR HTTP request");
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 512];
        loop {
            let read = stream.read(&mut chunk).expect("read OCR HTTP request");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if complete_http_request(&buffer) {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buffer).into_owned();
        request_tx.send(request).expect("send OCR HTTP request");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write OCR HTTP response");
    });

    (url, request_rx, server)
}

#[cfg(feature = "pdfium")]
fn start_rendered_ocr_http_server(
    response_text: &'static str,
) -> (String, Receiver<RenderedOcrHttpObservation>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind rendered OCR HTTP test server");
    listener
        .set_nonblocking(true)
        .expect("set rendered OCR HTTP server nonblocking");
    let url = format!(
        "http://{}/ocr",
        listener.local_addr().expect("read OCR HTTP server addr")
    );
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        request_tx
                            .send(RenderedOcrHttpObservation {
                                request: String::new(),
                                rendered_image_path: None,
                                image_existed: false,
                                header: None,
                                bytes: None,
                            })
                            .expect("send missing rendered OCR HTTP request observation");
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept rendered OCR HTTP request: {error}"),
            }
        };

        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 512];
        loop {
            let read = stream
                .read(&mut chunk)
                .expect("read rendered OCR HTTP request");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if complete_http_request(&buffer) {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buffer).into_owned();
        let body = http_request_body(&buffer);
        let rendered_image_path = serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("rendered_image_path")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        let (image_existed, header, bytes) = rendered_image_path
            .as_deref()
            .map(|path| {
                let path = Path::new(path);
                let image_existed = path.exists();
                let header = fs::read(path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.into_iter().take(2).collect()).ok());
                let bytes = fs::metadata(path)
                    .ok()
                    .map(|metadata| metadata.len() as usize);
                (image_existed, header, bytes)
            })
            .unwrap_or((false, None, None));
        request_tx
            .send(RenderedOcrHttpObservation {
                request,
                rendered_image_path,
                image_existed,
                header,
                bytes,
            })
            .expect("send rendered OCR HTTP request observation");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_text.len(),
            response_text
        );
        stream
            .write_all(response.as_bytes())
            .expect("write rendered OCR HTTP response");
    });

    (url, request_rx, server)
}

fn complete_http_request(buffer: &[u8]) -> bool {
    let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_end = header_end + 4;
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    buffer.len() >= header_end + content_length
}

#[cfg(feature = "pdfium")]
fn http_request_body(buffer: &[u8]) -> &[u8] {
    let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
        return &[];
    };
    &buffer[(header_end + 4)..]
}

fn run_json<const N: usize>(args: [&str; N]) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_glyphrush"));
    if !args.iter().copied().any(|arg| arg == "--backend") {
        command.args(["--backend", "lopdf"]);
    }
    let output = command.args(args).output().expect("run glyphrush command");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("command output is json")
}

fn source_modified_unix_ms(path: &std::path::Path) -> u64 {
    fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn rewrite_until_modified_ms_changes(path: &std::path::Path, bytes: &[u8]) -> u64 {
    let initial = source_modified_unix_ms(path);
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(20));
        fs::write(path, bytes).unwrap();
        let modified = source_modified_unix_ms(path);
        if modified != initial {
            return modified;
        }
    }
    panic!("failed to change modified timestamp for {}", path.display());
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "glyphrush-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn minimal_pdf(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    minimal_pdf_with_stream(&stream)
}

fn minimal_pdf_with_stream(stream: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_encrypted_pdf(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Filter /Standard /V 1 /R 2 /O <> /U <> /P -4 >>".to_string(),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(
        &mut pdf,
        "<< /Size {} /Root 1 0 R /Encrypt 6 0 R >>",
        objects.len() + 1
    )
    .unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_inherited_rotation(text: &str, rotation_degrees: i16) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [3 0 R] /Count 1 /Rotate {rotation_degrees} >>"),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_inherited_media_box(text: &str, width: f32, height: f32) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 36 360 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 {width} {height}] >>"),
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_page_crop_box(
    text: &str,
    media_width: f32,
    media_height: f32,
    crop_width: f32,
    crop_height: f32,
) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 36 360 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {media_width} {media_height}] /CropBox [0 0 {crop_width} {crop_height}] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_streams(streams: &[&str]) -> Vec<u8> {
    let page_count = streams.len();
    assert!(page_count > 0);
    let font_object_id = 3 + page_count;
    let kids = (0..page_count)
        .map(|index| format!("{} 0 R", index + 3))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"),
    ];

    for (index, stream) in streams.iter().enumerate() {
        let content_object_id = 4 + page_count + index;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_object_id} 0 R >> >> /Contents {content_object_id} 0 R >>"
        ));
        assert!(!stream.is_empty());
    }

    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    for stream in streams {
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_nonzero_crop_box_stream(stream: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [100 100 406 496] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_full_page_image_and_text(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream =
        format!("q 612 0 0 792 0 0 cm /Im1 Do Q BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let image_data = "0";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_overlapping_half_page_images_and_text(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!(
        "q 306 0 0 792 0 0 cm /Im1 Do Q q 306 0 0 792 0 0 cm /Im1 Do Q BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET"
    );
    let image_data = "0";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_nonzero_crop_box_full_page_image_and_sparse_text(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!(
        "q 306 0 0 396 100 100 cm /Im1 Do Q BT /F1 24 Tf 120 450 Td ({escaped_text}) Tj ET"
    );
    let image_data = "0";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [100 100 406 496] /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_nonzero_crop_box_off_crop_image_and_text(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream =
        format!("q 50 0 0 50 0 0 cm /Im1 Do Q BT /F1 24 Tf 120 450 Td ({escaped_text}) Tj ET");
    let image_data = "0";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [100 100 406 496] /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_form_wrapped_full_page_image_and_text(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let page_stream =
        format!("q 612 0 0 792 0 0 cm /Fm1 Do Q BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let form_stream = "q 1 0 0 1 0 0 cm /Im1 Do Q";
    let image_data = "0";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Fm1 6 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{page_stream}\nendstream",
            page_stream.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Im1 7 0 R >> >> /Length {} >>\nstream\n{form_stream}\nendstream",
            form_stream.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_small_form_wrapped_image_and_text(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let page_stream =
        format!("q 612 0 0 792 0 0 cm /Fm1 Do Q BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let form_stream = "q 0.1 0 0 0.1 0 0 cm /Im1 Do Q";
    let image_data = "0";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Fm1 6 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{page_stream}\nendstream",
            page_stream.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Im1 7 0 R >> >> /Length {} >>\nstream\n{form_stream}\nendstream",
            form_stream.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_widget_annotation(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R] >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R /Annots [6 0 R] >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Annot /Subtype /Widget /Rect [72 690 180 720] /T (Name) >>".to_string(),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_text_annotation(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R /Annots [6 0 R] >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Annot /Subtype /Text /Rect [72 690 180 720] /Contents (Unextracted annotation note) >>".to_string(),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_catalog_acroform(text: &str) -> Vec<u8> {
    let escaped_text = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped_text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R] >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /FT /Tx /T (HiddenName) /V (Unextracted value) >>".to_string(),
    ];

    let mut pdf = Vec::new();
    writeln!(&mut pdf, "%PDF-1.4").unwrap();

    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
        writeln!(&mut pdf, "{object}").unwrap();
        writeln!(&mut pdf, "endobj").unwrap();
    }

    let xref_start = pdf.len();
    writeln!(&mut pdf, "xref").unwrap();
    writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
    for offset in offsets.iter().skip(1) {
        writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut pdf, "trailer").unwrap();
    writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();

    pdf
}

fn minimal_pdf_with_ruled_table() -> Vec<u8> {
    let stream = [
        "72 600 m 360 600 l S",
        "72 560 m 360 560 l S",
        "72 520 m 360 520 l S",
        "72 480 m 360 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "BT /F1 12 Tf 84 574 Td (Part Value) Tj ET",
        "BT /F1 12 Tf 84 534 Td (A 1) Tj ET",
        "BT /F1 12 Tf 84 494 Td (B 2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

fn minimal_pdf_with_aligned_whitespace_ruled_table() -> Vec<u8> {
    let stream = [
        "72 600 m 504 600 l S",
        "72 560 m 504 560 l S",
        "72 520 m 504 520 l S",
        "72 480 m 504 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "504 480 m 504 600 l S",
        "BT /F1 12 Tf 18 TL 84 574 Td (Part          Value        Note) Tj T* (A                          missing value) Tj T* (B             2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

fn minimal_pdf_with_positioned_ruled_table() -> Vec<u8> {
    let stream = [
        "72 600 m 360 600 l S",
        "72 560 m 360 560 l S",
        "72 520 m 360 520 l S",
        "72 480 m 360 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "BT /F1 12 Tf 84 574 Td (Part) Tj ET",
        "BT /F1 12 Tf 228 574 Td (Value) Tj ET",
        "BT /F1 12 Tf 84 534 Td (A) Tj ET",
        "BT /F1 12 Tf 228 534 Td (1) Tj ET",
        "BT /F1 12 Tf 84 494 Td (B) Tj ET",
        "BT /F1 12 Tf 228 494 Td (2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

fn minimal_pdf_with_positioned_ruled_table_empty_cells() -> Vec<u8> {
    let stream = [
        "72 600 m 504 600 l S",
        "72 560 m 504 560 l S",
        "72 520 m 504 520 l S",
        "72 480 m 504 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "504 480 m 504 600 l S",
        "BT /F1 12 Tf 84 574 Td (Part) Tj ET",
        "BT /F1 12 Tf 228 574 Td (Value) Tj ET",
        "BT /F1 12 Tf 372 574 Td (Note) Tj ET",
        "BT /F1 12 Tf 84 534 Td (A) Tj ET",
        "BT /F1 12 Tf 372 534 Td (missing value) Tj ET",
        "BT /F1 12 Tf 84 494 Td (B) Tj ET",
        "BT /F1 12 Tf 228 494 Td (2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes.as_ref());
    format!("{:x}", hasher.finalize())
}

fn expected_corpus_fingerprint(json: &Value) -> Value {
    let mut payload = String::from("glyphrush-corpus-v1\n");
    for document in json["documents"].as_array().unwrap() {
        payload.push_str(document["path"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(document["document_fingerprint"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(&document["page_count"].as_u64().unwrap().to_string());
        payload.push('\n');
    }
    Value::String(sha256_hex(payload))
}

fn expected_generated_manifest_corpus_fingerprint(json: &Value) -> Value {
    let mut payload = String::from("glyphrush-corpus-v1\n");
    for document in json["documents"].as_array().unwrap() {
        payload.push_str(document["path"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(document["document_fingerprint"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(
            &document["expect"]["page_count"]
                .as_u64()
                .unwrap()
                .to_string(),
        );
        payload.push('\n');
    }
    Value::String(sha256_hex(payload))
}

fn capability<'a>(capabilities: &'a [Value], id: &str) -> &'a Value {
    capabilities
        .iter()
        .find(|capability| capability["id"] == id)
        .unwrap_or_else(|| panic!("missing capability {id}"))
}
