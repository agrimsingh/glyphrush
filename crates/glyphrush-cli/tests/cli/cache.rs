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
fn cache_key_does_not_reuse_prior_schema_artifacts() {
    let dir = temp_dir("parse-cache-schema-version");
    let pdf_path = dir.join("cache-schema.pdf");
    fs::write(&pdf_path, minimal_pdf("Cache schema")).unwrap();
    let cache_dir = dir.join("cache");

    let json = glyphrush(&[
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
    let old_v40_key = sha256_hex(format!(
        "glyphrush-cache-v40:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v41_key = sha256_hex(format!(
        "glyphrush-cache-v41:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let old_v42_key = sha256_hex(format!(
        "glyphrush-cache-v42:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
        env!("CARGO_PKG_VERSION")
    ));
    let expected_current_key = sha256_hex(format!(
        "glyphrush-cache-v43:glyphrush:{}:lopdf:lopdf-adapter-v0:{fingerprint}:no-sidecar:span-geometry=false",
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
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v40_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v41_key
    );
    assert_ne!(
        json["global_diagnostics"]["cache_key"]
            .as_str()
            .expect("cache key is present"),
        old_v42_key
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

    let first = glyphrush(&[
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
    let second = glyphrush(&[
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

    let first = glyphrush(&[
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
    let second = glyphrush(&[
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

    let default = glyphrush(&[
        "parse",
        pdf_path.to_str().unwrap(),
        "--format",
        "json",
        "--cache-dir",
        cache_dir.to_str().unwrap(),
    ]);
    let geometry = glyphrush(&[
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
