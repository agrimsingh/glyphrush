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
fn manifest_generates_eval_manifest_for_single_pdf() {
    let dir = temp_dir("manifest-single");
    let pdf_path = dir.join("single.pdf");
    fs::write(&pdf_path, minimal_pdf("Manifest single")).unwrap();

    let (json, output) =
        glyphrush_with_output(&["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()]);
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

    let json = glyphrush(&["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()]);

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

    let json = glyphrush(&["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()]);

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

    let (json, output) = glyphrush_with_output(&[
        "--backend",
        "lopdf",
        "manifest",
        pdf_path.to_str().unwrap(),
        "--span-geometry",
    ]);
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

    let (json, output) =
        glyphrush_with_output(&["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()]);
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

    let (json, output) =
        glyphrush_with_output(&["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()]);
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

    let json = glyphrush(&["--backend", "lopdf", "manifest", pdf_path.to_str().unwrap()]);

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

    let (json, output) =
        glyphrush_with_output(&["--backend", "lopdf", "manifest", dir.to_str().unwrap()]);
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

    let (json, output) = glyphrush_with_output(&[
        "--backend",
        "lopdf",
        "manifest",
        dir.to_str().unwrap(),
        "--category",
        "datasheet",
    ]);
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

    let eval_json = glyphrush(&["eval", manifest_path.to_str().unwrap()]);
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

    let (json, output) = glyphrush_with_output(&[
        "--backend",
        "lopdf",
        "manifest",
        dir.to_str().unwrap(),
        "--category-from-path",
    ]);
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

    let eval_json = glyphrush(&["eval", manifest_path.to_str().unwrap()]);
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

    let (json, output) = glyphrush_with_output(&[
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
    ]);
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

    let (json, output) = glyphrush_with_output(&[
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
    ]);
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

    let (json, output) = glyphrush_with_output(&[
        "--backend",
        "lopdf",
        "manifest",
        dir.to_str().unwrap(),
        "--category",
        "clean_digital",
        "--coverage-preset",
        "glyphrush-v0",
    ]);
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
fn manifest_coverage_preset_generates_glyphrush_v0_native_text_category_gate() {
    let dir = temp_dir("manifest-native-text-coverage-preset");
    fs::write(
        dir.join("clean.pdf"),
        minimal_pdf("Native text coverage preset manifest"),
    )
    .unwrap();

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "manifest",
        dir.to_str().unwrap(),
        "--category",
        "clean_digital",
        "--coverage-preset",
        "glyphrush-v0-native-text",
    ]);

    assert_eq!(
        json["required_categories"],
        serde_json::json!([
            "academic_columns",
            "clean_digital",
            "forms",
            "hybrid",
            "large",
            "rotated",
            "tables",
            "weird_encoding"
        ])
    );
    assert!(
        json["required_categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|category| category != "scanned")
    );
    assert_eq!(
        json["min_category_counts"],
        serde_json::json!({
            "academic_columns": 1,
            "clean_digital": 1,
            "forms": 1,
            "hybrid": 1,
            "large": 1,
            "rotated": 1,
            "tables": 1,
            "weird_encoding": 1
        })
    );
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
