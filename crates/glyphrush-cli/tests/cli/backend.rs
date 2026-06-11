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
fn backend_check_reports_lopdf_and_pending_pdfium_mupdf_candidates() {
    let json = glyphrush(&["--backend", "lopdf", "backend-check"]);

    assert_eq!(json["report_version"], "glyphrush-backend-check-report-v1");
    assert_eq!(json["selected_backend"], "lopdf");
    assert_eq!(
        json["enabled_backend_count"],
        if cfg!(feature = "pdfium") { 2 } else { 1 }
    );
    assert_eq!(json["candidate_backend_count"], 3);
    assert_eq!(
        json["decision_gate"],
        "mupdf_rejected_on_agpl_license_pdfium_is_the_fast_path"
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
    let json = glyphrush(&["--backend", "auto", "backend-check"]);

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
    let json = glyphrush_exact(&["backend-check"]);

    assert_eq!(
        json["selected_backend"],
        if cfg!(feature = "pdfium") {
            "pdfium"
        } else {
            "lopdf"
        }
    );
}

#[cfg(feature = "pdfium")]
#[test]
fn backend_check_reports_feature_gated_pdfium_backend() {
    let json = glyphrush(&["--backend", "pdfium", "backend-check"]);

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
fn pdfium_backend_flags_ruled_table_vector_paths() {
    let dir = temp_dir("pdfium-ruled-table");
    let pdf_path = dir.join("table.pdf");
    fs::write(&pdf_path, minimal_pdf_with_ruled_table()).unwrap();

    let json = glyphrush(&[
        "--backend",
        "pdfium",
        "debug-page",
        pdf_path.to_str().unwrap(),
        "0",
    ]);

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
fn backend_check_smoke_pdf_reports_selected_backend_extraction_summary() {
    let pdf_path = write_test_pdf("backend-check-smoke", "Backend smoke text");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "backend-check",
        "--pdf",
        pdf_path.to_str().unwrap(),
    ]);
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "backend-check",
        "--pdf",
        dir.to_str().unwrap(),
    ]);
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

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "backend-check",
        "--pdf",
        dir.to_str().unwrap(),
        "--jobs",
        "2",
    ]);
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
