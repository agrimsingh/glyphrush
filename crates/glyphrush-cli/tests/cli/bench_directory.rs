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
fn bench_directory_summary_reports_external_baseline_description_probe_status() {
    let dir = temp_dir("bench-dir-baseline-describe-failure");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second describe failure")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First describe failure")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-describe-failure",
        "if [ \"${1:-}\" = \"--describe\" ]; then printf 'not json'; exit 0; fi\nprintf 'baseline output'",
    );

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["target"], "MockParse");
    assert_eq!(json["documents"][0]["baselines"][0]["target"], "MockParse");
    assert_eq!(json["documents"][1]["baselines"][0]["target"], "MockParse");
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
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

    let json = glyphrush(&[
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
fn bench_directory_with_eval_category_filters_multiple_quality_categories() {
    let dir = temp_dir("bench-dir-eval-category-set-filter");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean Bench Set")).unwrap();
    fs::write(dir.join("table.pdf"), minimal_pdf("Table Bench Set")).unwrap();
    fs::write(dir.join("scan.pdf"), minimal_pdf("Scan Bench Set")).unwrap();
    let baseline = write_baseline_script(
        "bench-dir-category-set-baseline",
        "case \"$1\" in *clean.pdf) printf 'Clean Bench Set';; *table.pdf) printf 'Table Bench Set';; *) printf 'Scan Bench Set';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean Bench Set"]
              }
            },
            {
              "path": "table.pdf",
              "category": "tables",
              "expect": {
                "required_text": ["Table Bench Set"]
              }
            },
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["missing scanned bench set text"]
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
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--eval-category",
        "clean_digital,tables",
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--require-baseline-quality",
    ]);

    assert_eq!(json["quality"]["document_count"], 2);
    assert_eq!(json["quality"]["documents"][0]["path"], "clean.pdf");
    assert_eq!(json["quality"]["documents"][1]["path"], "table.pdf");
    assert_eq!(
        json["quality"]["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "tables": 1
        })
    );
    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][0]["quality_documents"], 2);
    assert_eq!(json["baselines"][0]["quality_failed_documents"], 0);
    assert_eq!(
        json["baselines"][0]["quality_category_summaries"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["clean_digital".to_string(), "tables".to_string()]
    );
    assert!(json["category_summaries"]["scanned"].is_null());
}

#[test]
fn bench_directory_with_eval_category_preset_filters_native_text_surface() {
    let dir = temp_dir("bench-dir-eval-category-preset-native-text");
    fs::write(dir.join("clean.pdf"), minimal_pdf("Clean Preset Bench")).unwrap();
    fs::write(dir.join("table.pdf"), minimal_pdf("Table Preset Bench")).unwrap();
    fs::write(dir.join("scan.pdf"), minimal_pdf("Scan Preset Bench")).unwrap();
    let baseline = write_baseline_script(
        "bench-dir-category-preset-baseline",
        "case \"$1\" in *clean.pdf) printf 'Clean Preset Bench';; *table.pdf) printf 'Table Preset Bench';; *) printf 'Wrong Scanned Preset Bench';; esac",
    );
    let manifest_path = dir.join("corpus.json");
    fs::write(
        &manifest_path,
        r#"{
          "documents": [
            {
              "path": "clean.pdf",
              "category": "clean_digital",
              "expect": {
                "required_text": ["Clean Preset Bench"]
              }
            },
            {
              "path": "table.pdf",
              "category": "tables",
              "expect": {
                "required_text": ["Table Preset Bench"]
              }
            },
            {
              "path": "scan.pdf",
              "category": "scanned",
              "expect": {
                "required_text": ["missing scanned preset bench text"]
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
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--eval-category-preset",
        "glyphrush-v0-native-text",
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--require-baseline-quality",
    ]);

    assert_eq!(json["document_count"], 2);
    assert_eq!(json["quality"]["document_count"], 2);
    assert_eq!(json["quality"]["documents"][0]["path"], "clean.pdf");
    assert_eq!(json["quality"]["documents"][1]["path"], "table.pdf");
    assert_eq!(
        json["quality"]["category_counts"],
        serde_json::json!({
            "clean_digital": 1,
            "tables": 1
        })
    );
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][0]["quality_documents"], 2);
    assert_eq!(json["baselines"][0]["quality_failed_documents"], 0);
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--eval-category",
        "scanned",
    ]);
    assert!(!output.status.success());

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
fn bench_directory_reports_sorted_documents_and_aggregate_counts() {
    let dir = temp_dir("bench-dir");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second")).unwrap();
    fs::write(dir.join("ignore.txt"), "not a pdf").unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First")).unwrap();
    let ocr_dir = dir.join("ocr");
    fs::create_dir(&ocr_dir).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--span-geometry",
        "--ocr-sidecar",
        ocr_dir.to_str().unwrap(),
        "--ocr-timeout-ms",
        "5678",
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--jobs",
        "2",
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--jobs",
        "2",
        "--ocr-command",
        command.to_str().unwrap(),
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--jobs",
        "2",
        "--baseline",
        &format!("slow={}", baseline.display()),
    ]);

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

    let json = glyphrush(&["--backend", "lopdf", "bench", dir.to_str().unwrap()]);

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

    let json = glyphrush(&["--backend", "lopdf", "bench", dir.to_str().unwrap()]);

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

    let json = glyphrush(&["--backend", "lopdf", "bench", dir.to_str().unwrap()]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--cache-dir",
        cache_dir.to_str().unwrap(),
        "--cache-probe",
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
        "--require-baseline-quality",
    ]);
    assert!(!output.status.success());
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
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
fn bench_directory_reports_unchecked_baseline_quality_by_category() {
    let dir = temp_dir("bench-dir-baseline-quality-unchecked-category");
    fs::write(dir.join("a.pdf"), minimal_pdf("Clean Baseline Checked")).unwrap();
    fs::write(dir.join("b.pdf"), minimal_pdf("Scanned Baseline Unchecked")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-quality-unchecked-category",
        "case \"$1\" in *a.pdf) printf 'Clean Baseline Checked';; *) printf 'Scanned Baseline Unchecked';; esac",
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
                "required_text": ["Clean Baseline Checked"]
              }
            },
            {
              "path": "b.pdf",
              "category": "scanned",
              "expect": {
                "page_count": 1
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
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
    let baseline_summary = &json["baselines"][0];

    assert_eq!(json["quality"]["passed"], true);
    assert_eq!(baseline_summary["quality_status"], "partially_checked");
    assert_eq!(baseline_summary["quality_documents"], 1);
    assert_eq!(baseline_summary["quality_unchecked_documents"], 1);
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
        baseline_summary["quality_unchecked_category_summaries"]["scanned"],
        serde_json::json!({
            "document_count": 1,
            "page_count": 1,
            "not_checked_no_expectations_documents": 1,
            "not_checked_timed_out_documents": 0,
            "not_checked_execution_failed_documents": 0
        })
    );
    assert_eq!(
        baseline_summary["quality_unchecked_samples"],
        serde_json::json!([
            {
                "path": "b.pdf",
                "category": "scanned",
                "quality_status": "not_checked_no_expectations"
            }
        ])
    );
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
                    "min_cell_recall": 1.0,
                    "baseline": true
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
                    "min_cell_recall": 1.0,
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
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--eval-manifest",
        manifest_path.to_str().unwrap(),
    ]);
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--require-baselines",
    ]);
    assert!(!output.status.success());
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

    let (json, output) = glyphrush_expect_failure(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--require-speedup",
        "mock=1000000.0",
    ]);
    assert!(!output.status.success());
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

    let json = glyphrush(&[
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
    ]);
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
fn bench_directory_speedup_claim_identifies_baseline_quality_failure_blocker() {
    let dir = temp_dir("bench-dir-speedup-claim-baseline-quality-blocker");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second Claim Quality")).unwrap();
    fs::write(dir.join("a.pdf"), minimal_pdf("First Claim Quality")).unwrap();
    let baseline = write_baseline_script(
        "baseline-dir-claim-quality-failed",
        "case \"$1\" in *a.pdf) printf 'First Claim Quality';; *) printf 'wrong baseline output';; esac",
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
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("bench directory output is json");
    let claim = &json["speedup_claims"][0];

    assert_eq!(json["quality_status"], "checked");
    assert_eq!(json["quality"]["quality_passed"], true);
    assert_eq!(json["baselines"][0]["quality_status"], "checked");
    assert_eq!(json["baselines"][0]["quality_failed_documents"], 1);
    assert_eq!(claim["baseline"], "mock");
    assert_eq!(claim["speed_comparable"], true);
    assert_eq!(claim["speed_passed"], true);
    assert_eq!(claim["glyphrush_quality_checked"], true);
    assert_eq!(claim["glyphrush_quality_passed"], true);
    assert_eq!(claim["baseline_quality_checked"], true);
    assert_eq!(claim["baseline_quality_passed"], false);
    assert_eq!(claim["glyphrush_quality_backed"], true);
    assert_eq!(claim["quality_backed"], false);
    assert_eq!(claim["quality_blocker"], "baseline_quality_failed");
    assert_eq!(claim["claim_passed"], false);
    assert_eq!(claim["status"], "quality_failed");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bench speedup claim required"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
        "--baseline-timeout-ms",
        "5000",
    ]);
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "bench",
        dir.to_str().unwrap(),
        "--baseline",
        &format!("mock={}", baseline.display()),
    ]);
    let baseline_summary = &json["baselines"][0];

    assert_eq!(baseline_summary["successful_documents"], 2);
    assert_eq!(baseline_summary["failed_documents"], 0);
    assert_eq!(baseline_summary["empty_output_documents"], 1);
    assert_eq!(baseline_summary["empty_output_pages"], 1);
    assert_eq!(json["documents"][0]["baselines"][0]["empty_output"], false);
    assert_eq!(json["documents"][1]["baselines"][0]["empty_output"], true);
}
