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
fn inspect_reports_pdf_page_count_and_fingerprint() {
    let pdf_path = write_test_pdf("inspect", "Hello Glyphrush");

    let json = glyphrush(&["--backend", "lopdf", "inspect", pdf_path.to_str().unwrap()]);
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

    let json = glyphrush(&["--backend", "lopdf", "inspect", pdf_path.to_str().unwrap()]);

    assert_eq!(json["backend"], "lopdf");
    assert_eq!(json["page_count"], 1);
}

#[test]
fn inspect_pages_reports_page_level_quality_triage() {
    let dir = temp_dir("inspect-pages");
    let pdf_path = dir.join("scan.pdf");
    fs::write(&pdf_path, minimal_pdf_with_full_page_image_and_text("tiny")).unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "inspect",
        pdf_path.to_str().unwrap(),
        "--pages",
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "inspect",
        pdf_path.to_str().unwrap(),
        "--pages",
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "inspect",
        pdf_path.to_str().unwrap(),
        "--pages",
        "--jobs",
        "2",
    ]);

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
fn inspect_directory_reports_sorted_pdf_inventory() {
    let dir = temp_dir("inspect-dir");
    fs::write(dir.join("two.PDF"), minimal_pdf("Two")).unwrap();
    fs::write(dir.join("one.pdf"), minimal_pdf("One")).unwrap();

    let json = glyphrush(&["--backend", "lopdf", "inspect", dir.to_str().unwrap()]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "inspect",
        dir.to_str().unwrap(),
        "--pages",
    ]);

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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "inspect",
        dir.to_str().unwrap(),
        "--pages",
        "--jobs",
        "2",
    ]);

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
