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
fn bench_reports_timing_and_fallback_counts() {
    let pdf_path = write_test_pdf("bench", "Hello Glyphrush");

    let json = glyphrush(&["--backend", "lopdf", "bench", pdf_path.to_str().unwrap()]);

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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--require-quality",
    ]);
    assert!(!output.status.success());
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--jobs",
        "2",
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--require-speedup",
        "mock=1000000.0",
    ]);
    assert!(!output.status.success());
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--require-speedup-claim",
        "mock=0.000001",
    ]);
    assert!(!output.status.success());
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
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
fn bench_reports_external_baseline_output_digest_and_text_stats() {
    let pdf_path = write_test_pdf("bench-baseline-stats", "Hello Baseline Stats");
    let baseline = write_baseline_script("baseline-stats", "printf 'alpha beta\\ncharlie'");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
    let baseline = &json["baselines"][0];

    assert_eq!(baseline["success"], true);
    assert_eq!(baseline["empty_output"], false);
    assert_eq!(baseline["stdout_line_count"], 2);
    assert_eq!(baseline["stdout_word_count"], 3);
    assert_eq!(baseline["stdout_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn bench_reports_failed_external_baseline_without_hiding_glyphrush_metrics() {
    let pdf_path = write_test_pdf("bench-baseline-fail", "Hello Failed Baseline");
    let baseline = write_baseline_script("baseline-fail", "printf 'bad baseline' >&2\nexit 7");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("broken={}", baseline.display()),
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("missing={}", baseline.display()),
    ]);
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("broken={}", baseline.display()),
        "--require-baselines",
    ]);
    assert!(!output.status.success());

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--baseline",
        &format!("broken={}", baseline.display()),
    ]);
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

    let json = glyphrush(&[
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
    ]);
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

    let json = glyphrush(&[
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
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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

    let json = glyphrush(&[
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
    ]);
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("bad={}", bad_baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--require-baseline-quality",
    ]);
    assert!(!output.status.success());
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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
fn bench_with_eval_manifest_scores_baseline_only_required_text() {
    let dir = temp_dir("bench-eval-baseline-only-required-text");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let baseline = write_baseline_script(
        "baseline-only-required-text",
        "printf 'OCR recovered serial number ABC123'",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "ocr_required_pages": 1,
                "quality_flag_counts": {
                  "requires_ocr": 1,
                  "low_confidence_text": 1,
                  "broken_encoding": 0,
                  "layout_uncertain": 0,
                  "table_uncertain": 0,
                  "unsupported_feature": 0
                },
                "pages": [
                  {
                    "index": 0,
                    "empty_text_output": true,
                    "required_flags": ["requires_ocr", "low_confidence_text"]
                  }
                ],
                "baseline_required_text": ["OCR recovered serial number ABC123"],
                "silent_failures": { "max_count": 0 }
              }
            }
          ]
        }"#,
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["quality"]["documents"][0]["passed"], true);
    assert!(json["quality"]["documents"][0]["checks"]["required_text"].is_null());
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][0]["quality"]["passed"], true);
    assert_eq!(
        json["baselines"][0]["quality"]["required_text"]["expected"],
        serde_json::json!(["OCR recovered serial number ABC123"])
    );
    assert_eq!(
        json["baselines"][0]["quality"]["required_text"]["missing"],
        serde_json::json!([])
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

    let json = glyphrush(&[
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
    ]);
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
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("table={}", table_baseline.display()),
        "--baseline",
        &format!("wrong={}", wrong_table_baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
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
fn bench_with_eval_manifest_skips_non_baseline_table_structure() {
    let dir = temp_dir("bench-eval-baseline-table-structure-skipped");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value |) Tj T* (| A | 1 |) Tj ET",
        ),
    )
    .unwrap();
    let table_baseline = write_baseline_script(
        "baseline-table-non-baseline-expectation",
        "printf 'Part\\tValue\\nB\\t2'",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "table.pdf",
              "expect": {
                "baseline_required_text": ["Part"],
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("table={}", table_baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
    let baseline_quality = &json["baselines"][0]["quality"];

    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(baseline_quality["passed"], true);
    assert_eq!(baseline_quality["failed_checks"], 0);
    assert!(baseline_quality.get("table_structure").is_none());
}

#[test]
fn bench_with_eval_manifest_scores_opted_in_baseline_table_structure() {
    let dir = temp_dir("bench-eval-baseline-table-structure-opted-in");
    let pdf_path = dir.join("table.pdf");
    fs::write(
        &pdf_path,
        minimal_pdf_with_stream(
            "BT /F1 12 Tf 72 720 Td 24 TL (| Part | Value |) Tj T* (| A | 1 |) Tj ET",
        ),
    )
    .unwrap();
    let table_baseline = write_baseline_script(
        "baseline-table-opted-in-good",
        "printf '| Part | Value |\\n| --- | --- |\\n| A | 1 |'",
    );
    let wrong_table_baseline = write_baseline_script(
        "baseline-table-opted-in-bad",
        "printf 'Part\\tValue\\nB\\t2'",
    );
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
        "bench",
        pdf_path.to_str().unwrap(),
        "--baseline",
        &format!("table={}", table_baseline.display()),
        "--baseline",
        &format!("wrong={}", wrong_table_baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
    let table = &json["baselines"][0]["quality"];
    let wrong = &json["baselines"][1]["quality"];

    assert_eq!(table["passed"], true);
    assert_eq!(table["failed_checks"], 0);
    assert_eq!(table["table_structure"][0]["passed"], true);
    assert_eq!(table["table_structure"][0]["cell_recall"], 1.0);
    assert_eq!(wrong["passed"], false);
    assert_eq!(wrong["failed_checks"], 1);
    assert_eq!(wrong["table_structure"][0]["passed"], false);
}

#[test]
fn bench_with_ocr_sidecar_reports_applied_ocr_pages() {
    let dir = temp_dir("bench-ocr-sidecar");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_stream("0 0 m 10 10 l S")).unwrap();
    let sidecar_dir = dir.join("ocr");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(sidecar_dir.join("scan.p000000.txt"), "Sidecar OCR text").unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
    ]);

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

    let json = glyphrush(&["--backend", "lopdf", "bench", pdf_path.to_str().unwrap()]);

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

    let json = glyphrush(&["--backend", "lopdf", "bench", pdf_path.to_str().unwrap()]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--ocr-sidecar",
        sidecar_dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());

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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--require-coverage-preset",
        "glyphrush-v0",
    ]);
    assert!(!output.status.success());

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
fn bench_with_cache_dir_reports_miss_then_hit() {
    let dir = temp_dir("bench-cache");
    let pdf_path = dir.join("bench-cache.pdf");
    fs::write(&pdf_path, minimal_pdf("Bench cache")).unwrap();
    let cache_dir = dir.join("cache");

    let first = glyphrush(&[
        "bench",
        pdf_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let second = glyphrush(&[
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        pdf_path.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
        "--cache-probe",
    ]);

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

    let json = glyphrush(&[
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

    assert_eq!(json["document_count"], 1);
    assert_eq!(json["documents"][0]["path"], "a.pdf");
    assert_eq!(baseline_summary["document_count"], 1);
    assert_eq!(baseline_summary["quality_status"], "checked");
    assert_eq!(baseline_summary["quality_documents"], 1);
    assert_eq!(baseline_summary["quality_unchecked_documents"], 0);
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
