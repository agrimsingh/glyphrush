use anyhow::{Context, Result};
use glyphrush_core::{DocumentMetadata, parse_extracted_pages};
use glyphrush_lopdf::{LopdfExtractionOptions, extract_pages};
use lopdf::Document;
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

const PARSER_NAME: &str = "glyphrush";
const LOPDF_BACKEND_NAME: &str = "lopdf";
const LOPDF_BACKEND_VERSION: &str = "lopdf-adapter-v0";

fn noop_ocr(_signals: &glyphrush_core::PageSignals) -> Result<(Option<String>, u64)> {
    Ok((None, 0))
}

fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn parse_pdf_bytes_internal(bytes: &[u8], span_geometry: bool) -> Result<String> {
    let document = Document::load_mem(bytes).context("load PDF from bytes")?;

    if document.is_encrypted() {
        anyhow::bail!("encrypted PDFs are not supported by the v0 CLI without a password");
    }

    let pages = extract_pages(
        &document,
        LopdfExtractionOptions {
            span_geometry,
            page_jobs: 1,
        },
        &noop_ocr,
    )?;

    let fingerprint = sha256_hex(bytes);
    let metadata = DocumentMetadata {
        parser_name: PARSER_NAME.to_string(),
        parser_version: env!("CARGO_PKG_VERSION").to_string(),
        backend: LOPDF_BACKEND_NAME.to_string(),
        backend_version: LOPDF_BACKEND_VERSION.to_string(),
        source_size_bytes: bytes.len() as u64,
        source_modified_unix_ms: 0,
    };

    let artifact = parse_extracted_pages(fingerprint, pages).with_metadata(metadata);
    serde_json::to_string(&artifact).context("serialize document artifact")
}

/// Parse PDF bytes through the shared lopdf extraction path and core artifact model.
///
/// OCR adapters (sidecar, command, HTTP) are process and network seams that do not
/// apply to the wasm surface. Pages that need OCR keep their `requires_ocr` flags
/// and warnings exactly like a no-OCR CLI run.
#[wasm_bindgen]
pub fn parse_pdf_bytes(bytes: &[u8], span_geometry: bool) -> Result<String, JsError> {
    parse_pdf_bytes_internal(bytes, span_geometry).map_err(|error| JsError::new(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_bytes() {
        let error = parse_pdf_bytes_internal(&[], false).unwrap_err();
        assert!(error.to_string().contains("load PDF"));
    }
}
