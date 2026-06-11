use crate::*;

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Instant, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use glyphrush_core::{
    CacheStatus, DocumentArtifact, DocumentMetadata, ExtractedPage, PageArtifact, PageDimensions,
    PageQuality, PageRoute, PageTimings, parse_extracted_pages, sha256_hex,
};
use glyphrush_lopdf::{LopdfExtractionOptions, extract_page_by_index, extract_pages};
use lopdf::Document;
use serde::Serialize;

pub(crate) const LOPDF_BACKEND_NAME: &str = "lopdf";

pub(crate) const LOPDF_BACKEND_VERSION: &str = "lopdf-adapter-v0";

pub(crate) const PARSER_NAME: &str = "glyphrush";

pub(crate) const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const BASELINE_CHECK_REPORT_VERSION: &str = "glyphrush-baseline-check-report-v1";

pub(crate) const BACKEND_CHECK_REPORT_VERSION: &str = "glyphrush-backend-check-report-v1";

#[derive(Debug, Serialize)]
pub(crate) struct InspectOutput {
    pub(crate) backend: &'static str,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) page_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackendCheckOutput {
    pub(crate) report_version: &'static str,
    pub(crate) parser_name: &'static str,
    pub(crate) parser_version: &'static str,
    pub(crate) selected_backend: &'static str,
    pub(crate) enabled_backend_count: usize,
    pub(crate) candidate_backend_count: usize,
    pub(crate) decision_gate: &'static str,
    pub(crate) backends: Vec<BackendCapabilityOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) smoke: Option<BackendSmokeOutput>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackendSmokeOutput {
    pub(crate) mode: &'static str,
    pub(crate) path: String,
    pub(crate) backend: &'static str,
    pub(crate) success: bool,
    pub(crate) wall_us: u128,
    pub(crate) source_size_bytes: Option<u64>,
    pub(crate) document_fingerprint: Option<String>,
    pub(crate) page_count: Option<usize>,
    pub(crate) extracted_page_count: Option<usize>,
    pub(crate) native_text_bytes: Option<usize>,
    pub(crate) image_artifact_count: Option<usize>,
    pub(crate) fallback_pages: Option<u32>,
    pub(crate) ocr_required_pages: Option<u32>,
    pub(crate) worker_count: Option<usize>,
    pub(crate) document_count: Option<usize>,
    pub(crate) successful_documents: Option<usize>,
    pub(crate) failed_documents: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) failure_samples: Vec<BackendSmokeFailureSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) documents: Vec<BackendSmokeOutput>,
    pub(crate) error_kind: Option<&'static str>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackendSmokeFailureSample {
    pub(crate) path: String,
    pub(crate) error_kind: Option<&'static str>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackendCapabilityOutput {
    pub(crate) name: &'static str,
    pub(crate) status: BackendStatus,
    pub(crate) selected: bool,
    pub(crate) version: Option<&'static str>,
    pub(crate) capabilities: BackendCapabilityMatrix,
    pub(crate) limitations: Vec<&'static str>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackendStatus {
    Enabled,
    NotWired,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackendCapabilityMatrix {
    pub(crate) open_pdf: bool,
    pub(crate) page_count: bool,
    pub(crate) native_text: bool,
    pub(crate) span_geometry: &'static str,
    pub(crate) image_metadata: bool,
    pub(crate) render_pages: bool,
    pub(crate) builtin_ocr: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct InspectPagesOutput {
    pub(crate) backend: &'static str,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) page_count: usize,
    pub(crate) worker_count: usize,
    pub(crate) cache_status: CacheStatus,
    pub(crate) cache_key: Option<String>,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) warnings_count: usize,
    pub(crate) pages: Vec<InspectPageSummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InspectPageSummary {
    pub(crate) page_index: u32,
    pub(crate) artifact_id: String,
    pub(crate) page_fingerprint: String,
    pub(crate) dimensions: PageDimensions,
    pub(crate) route: PageRoute,
    pub(crate) quality_flags: Vec<PageQuality>,
    pub(crate) reasons: Vec<String>,
    pub(crate) native_span_count: usize,
    pub(crate) native_text_bytes: usize,
    pub(crate) ocr_span_count: usize,
    pub(crate) image_artifact_count: usize,
    pub(crate) layout_block_count: usize,
    pub(crate) layout: DebugLayoutSummary,
    pub(crate) timings: PageTimings,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusInspectOutput {
    pub(crate) backend: &'static str,
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) corpus_fingerprint: String,
    pub(crate) documents: Vec<CorpusInspectDocument>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusInspectDocument {
    pub(crate) path: String,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) page_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusInspectPagesOutput {
    pub(crate) backend: &'static str,
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) worker_count: usize,
    pub(crate) cache_hits: u32,
    pub(crate) cache_misses: u32,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) warnings_count: usize,
    pub(crate) corpus_fingerprint: String,
    pub(crate) documents: Vec<CorpusInspectPagesDocument>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusInspectPagesDocument {
    pub(crate) path: String,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) page_count: usize,
    pub(crate) cache_status: CacheStatus,
    pub(crate) cache_key: Option<String>,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) warnings_count: usize,
    pub(crate) pages: Vec<InspectPageSummary>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExtractionOptions {
    pub(crate) span_geometry: bool,
    pub(crate) page_jobs: usize,
}

pub(crate) fn baseline_specs_with_preset(
    baselines: &[BaselineSpec],
    preset: Option<BaselinePreset>,
) -> Vec<BaselineSpec> {
    let mut specs = preset
        .into_iter()
        .flat_map(|preset| preset.specs().iter().copied())
        .map(|(name, command)| BaselineSpec {
            name: name.to_string(),
            command: PathBuf::from(command),
        })
        .collect::<Vec<_>>();
    specs.extend(baselines.iter().cloned());
    specs
}

pub(crate) fn baseline_preset_names(preset: Option<BaselinePreset>) -> Vec<&'static str> {
    preset
        .into_iter()
        .map(BaselinePreset::name)
        .collect::<Vec<_>>()
}

pub(crate) fn backend_check_output<B: PdfBackend + Sync>(
    backend: &B,
    smoke_pdf: Option<&Path>,
    jobs: usize,
) -> BackendCheckOutput {
    let selected_backend = backend.name();
    let backends = vec![
        BackendCapabilityOutput {
            name: "lopdf",
            status: BackendStatus::Enabled,
            selected: selected_backend == "lopdf",
            version: Some(LOPDF_BACKEND_VERSION),
            capabilities: BackendCapabilityMatrix {
                open_pdf: true,
                page_count: true,
                native_text: true,
                span_geometry: "bounded_simple_text",
                image_metadata: true,
                render_pages: false,
                builtin_ocr: false,
            },
            limitations: vec![
                "no_page_rendering",
                "no_builtin_ocr",
                "bounded_simple_span_geometry",
            ],
        },
        BackendCapabilityOutput {
            name: "pdfium",
            #[cfg(feature = "pdfium")]
            status: BackendStatus::Enabled,
            #[cfg(not(feature = "pdfium"))]
            status: BackendStatus::NotWired,
            selected: selected_backend == "pdfium",
            #[cfg(feature = "pdfium")]
            version: Some(PDFIUM_BACKEND_VERSION),
            #[cfg(not(feature = "pdfium"))]
            version: None,
            #[cfg(feature = "pdfium")]
            capabilities: BackendCapabilityMatrix {
                open_pdf: true,
                page_count: true,
                native_text: true,
                span_geometry: "pdfium_text_segments",
                image_metadata: true,
                render_pages: true,
                builtin_ocr: false,
            },
            #[cfg(not(feature = "pdfium"))]
            capabilities: BackendCapabilityMatrix {
                open_pdf: false,
                page_count: false,
                native_text: false,
                span_geometry: "not_available",
                image_metadata: false,
                render_pages: false,
                builtin_ocr: false,
            },
            limitations: vec![
                #[cfg(not(feature = "pdfium"))]
                "adapter_not_implemented",
                #[cfg(feature = "pdfium")]
                "no_builtin_ocr",
                "license_packaging_spike_required",
            ],
        },
        BackendCapabilityOutput {
            name: "mupdf",
            status: BackendStatus::NotWired,
            selected: selected_backend == "mupdf",
            version: None,
            capabilities: BackendCapabilityMatrix {
                open_pdf: false,
                page_count: false,
                native_text: false,
                span_geometry: "not_available",
                image_metadata: false,
                render_pages: false,
                builtin_ocr: false,
            },
            limitations: vec![
                "adapter_not_implemented",
                "rejected_agpl_license_incompatible_with_mit_distribution",
            ],
        },
    ];
    let enabled_backend_count = backends
        .iter()
        .filter(|backend| backend.status == BackendStatus::Enabled)
        .count();

    BackendCheckOutput {
        report_version: BACKEND_CHECK_REPORT_VERSION,
        parser_name: PARSER_NAME,
        parser_version: PARSER_VERSION,
        selected_backend,
        enabled_backend_count,
        candidate_backend_count: backends.len(),
        decision_gate: "mupdf_rejected_on_agpl_license_pdfium_is_the_fast_path",
        backends,
        smoke: smoke_pdf.map(|path| backend_smoke_output(backend, path, jobs)),
    }
}

pub(crate) fn backend_smoke_output<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    jobs: usize,
) -> BackendSmokeOutput {
    if path.is_dir() {
        return backend_smoke_directory_output(backend, path, jobs);
    }

    backend_smoke_pdf_output(backend, path, path.display().to_string())
}

pub(crate) fn backend_smoke_directory_output<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    jobs: usize,
) -> BackendSmokeOutput {
    let started = Instant::now();
    let result = discover_pdfs(path).map(|pdfs| {
        let worker_count = document_worker_count(backend, jobs, pdfs.len());
        let documents = if worker_count == 1 {
            pdfs.into_iter()
                .map(|pdf| backend_smoke_pdf_output(backend, &pdf.path, pdf.label))
                .collect::<Vec<_>>()
        } else {
            backend_smoke_directory_parallel(backend, pdfs, worker_count)
        };
        (worker_count, documents)
    });
    let wall_us = started.elapsed().as_micros().max(1);

    match result {
        Ok((worker_count, documents)) => {
            let successful_documents = documents.iter().filter(|document| document.success).count();
            let failed_documents = documents.len().saturating_sub(successful_documents);
            let page_count = documents
                .iter()
                .map(|document| document.page_count.unwrap_or_default())
                .sum::<usize>();
            let extracted_page_count = documents
                .iter()
                .map(|document| document.extracted_page_count.unwrap_or_default())
                .sum::<usize>();
            let native_text_bytes = documents
                .iter()
                .map(|document| document.native_text_bytes.unwrap_or_default())
                .sum::<usize>();
            let image_artifact_count = documents
                .iter()
                .map(|document| document.image_artifact_count.unwrap_or_default())
                .sum::<usize>();
            let fallback_pages = documents
                .iter()
                .map(|document| document.fallback_pages.unwrap_or_default())
                .sum::<u32>();
            let ocr_required_pages = documents
                .iter()
                .map(|document| document.ocr_required_pages.unwrap_or_default())
                .sum::<u32>();
            let failure_samples = backend_smoke_failure_samples(&documents);

            BackendSmokeOutput {
                mode: "directory",
                path: path.display().to_string(),
                backend: backend.name(),
                success: failed_documents == 0,
                wall_us,
                source_size_bytes: None,
                document_fingerprint: None,
                page_count: Some(page_count),
                extracted_page_count: Some(extracted_page_count),
                native_text_bytes: Some(native_text_bytes),
                image_artifact_count: Some(image_artifact_count),
                fallback_pages: Some(fallback_pages),
                ocr_required_pages: Some(ocr_required_pages),
                worker_count: Some(worker_count),
                document_count: Some(documents.len()),
                successful_documents: Some(successful_documents),
                failed_documents: Some(failed_documents),
                failure_samples,
                error: (failed_documents > 0)
                    .then(|| format!("{failed_documents} backend smoke document(s) failed")),
                error_kind: None,
                documents,
            }
        }
        Err(error) => BackendSmokeOutput {
            mode: "directory",
            path: path.display().to_string(),
            backend: backend.name(),
            success: false,
            wall_us,
            source_size_bytes: None,
            document_fingerprint: None,
            page_count: Some(0),
            extracted_page_count: Some(0),
            native_text_bytes: Some(0),
            image_artifact_count: Some(0),
            fallback_pages: Some(0),
            ocr_required_pages: Some(0),
            worker_count: Some(1),
            document_count: Some(0),
            successful_documents: Some(0),
            failed_documents: Some(0),
            failure_samples: Vec::new(),
            documents: Vec::new(),
            error_kind: Some("pdf_discovery_failed"),
            error: Some(format!("{error:#}")),
        },
    }
}

pub(crate) fn backend_smoke_failure_samples(
    documents: &[BackendSmokeOutput],
) -> Vec<BackendSmokeFailureSample> {
    documents
        .iter()
        .filter(|document| !document.success)
        .take(3)
        .map(|document| BackendSmokeFailureSample {
            path: document.path.clone(),
            error_kind: document.error_kind,
            error: document.error.clone(),
        })
        .collect()
}

pub(crate) fn backend_smoke_directory_parallel<B: PdfBackend + Sync>(
    backend: &B,
    pdfs: Vec<DiscoveredPdf>,
    worker_count: usize,
) -> Vec<BackendSmokeOutput> {
    let mut outputs = thread::scope(|scope| {
        let handles = pdfs
            .chunks(worker_count)
            .enumerate()
            .flat_map(|(offset, chunk)| {
                chunk.iter().enumerate().map(move |(chunk_index, pdf)| {
                    let index = offset * worker_count + chunk_index;
                    scope.spawn(move || {
                        (
                            index,
                            backend_smoke_pdf_output(backend, &pdf.path, pdf.label.clone()),
                        )
                    })
                })
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| panic!("backend smoke worker panicked"))
            })
            .collect::<Vec<_>>()
    });

    outputs.sort_by_key(|(index, _)| *index);
    outputs
        .into_iter()
        .map(|(_, output)| output)
        .collect::<Vec<_>>()
}

pub(crate) fn backend_smoke_pdf_output<B: PdfBackend>(
    backend: &B,
    path: &Path,
    output_path: String,
) -> BackendSmokeOutput {
    let started = Instant::now();
    let source_size_bytes = source_size_bytes(path);
    let source_size_for_failure = source_size_bytes.as_ref().ok().copied();
    let fingerprint = document_fingerprint(path);
    let fingerprint_for_failure = fingerprint.as_ref().ok().cloned();
    let result = (|| -> Result<_> {
        let source_size_bytes = source_size_bytes?;
        let fingerprint = fingerprint?;
        let document = backend.load_document(path)?;
        let page_count = backend.page_count(&document);
        let pages = backend.extract_pages(
            &document,
            path,
            OcrOptions::default(),
            ExtractionOptions {
                span_geometry: false,
                page_jobs: 1,
            },
        )?;
        let extracted_page_count = pages.len();
        let artifact = parse_extracted_pages(fingerprint.clone(), pages);
        let native_text_bytes = artifact
            .pages
            .iter()
            .flat_map(|page| page.native_spans.iter())
            .map(|span| span.text.len())
            .sum::<usize>();
        let image_artifact_count = artifact
            .pages
            .iter()
            .map(|page| page.image_artifacts.len())
            .sum::<usize>();

        Ok((
            source_size_bytes,
            fingerprint,
            page_count,
            extracted_page_count,
            native_text_bytes,
            image_artifact_count,
            artifact.global_diagnostics.fallback_pages,
            artifact.global_diagnostics.ocr_required_pages,
        ))
    })();
    let wall_us = started.elapsed().as_micros().max(1);

    match result {
        Ok((
            source_size_bytes,
            document_fingerprint,
            page_count,
            extracted_page_count,
            native_text_bytes,
            image_artifact_count,
            fallback_pages,
            ocr_required_pages,
        )) => BackendSmokeOutput {
            mode: "single_pdf",
            path: output_path,
            backend: backend.name(),
            success: true,
            wall_us,
            source_size_bytes: Some(source_size_bytes),
            document_fingerprint: Some(document_fingerprint),
            page_count: Some(page_count),
            extracted_page_count: Some(extracted_page_count),
            native_text_bytes: Some(native_text_bytes),
            image_artifact_count: Some(image_artifact_count),
            fallback_pages: Some(fallback_pages),
            ocr_required_pages: Some(ocr_required_pages),
            worker_count: None,
            document_count: None,
            successful_documents: None,
            failed_documents: None,
            failure_samples: Vec::new(),
            documents: Vec::new(),
            error_kind: None,
            error: None,
        },
        Err(error) => BackendSmokeOutput {
            mode: "single_pdf",
            path: output_path,
            backend: backend.name(),
            success: false,
            wall_us,
            source_size_bytes: source_size_for_failure,
            document_fingerprint: fingerprint_for_failure,
            page_count: None,
            extracted_page_count: None,
            native_text_bytes: None,
            image_artifact_count: None,
            fallback_pages: None,
            ocr_required_pages: None,
            worker_count: None,
            document_count: None,
            successful_documents: None,
            failed_documents: None,
            failure_samples: Vec::new(),
            documents: Vec::new(),
            error_kind: backend_smoke_error_kind(&error),
            error: Some(format!("{error:#}")),
        },
    }
}

pub(crate) fn backend_smoke_error_kind(error: &anyhow::Error) -> Option<&'static str> {
    let error = format!("{error:#}");
    if error.contains("encrypted PDFs are not supported") {
        Some("encrypted_pdf_requires_password")
    } else {
        None
    }
}

pub(crate) trait PdfBackend {
    type Document;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn supports_parallel_documents(&self) -> bool {
        true
    }
    fn supports_page_render_for_ocr(&self) -> bool {
        false
    }
    fn load_document(&self, path: &Path) -> Result<Self::Document>;
    fn page_count(&self, document: &Self::Document) -> usize;
    fn extract_pages(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
    ) -> Result<Vec<ExtractedPage>>;
    fn extract_page(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
        page_index: u32,
    ) -> Result<ExtractedPage>;
    fn ocr_check_rendered_image(
        &self,
        pdf: &Path,
        page_index: u32,
        ocr: OcrOptions<'_>,
        strict: bool,
    ) -> OcrCheckOutput {
        ocr_render_backend_required_check_output(pdf, page_index, ocr, strict)
    }
}

pub(crate) struct LopdfBackend;

impl PdfBackend for LopdfBackend {
    type Document = Document;

    fn name(&self) -> &'static str {
        LOPDF_BACKEND_NAME
    }

    fn version(&self) -> &'static str {
        LOPDF_BACKEND_VERSION
    }

    fn load_document(&self, path: &Path) -> Result<Self::Document> {
        let document =
            Document::load(path).with_context(|| format!("load PDF {}", path.display()))?;

        if document.is_encrypted() {
            bail!("encrypted PDFs are not supported by the v0 CLI without a password");
        }

        Ok(document)
    }

    fn page_count(&self, document: &Self::Document) -> usize {
        document.get_pages().len()
    }

    fn extract_pages(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
    ) -> Result<Vec<ExtractedPage>> {
        let lopdf_options = LopdfExtractionOptions {
            span_geometry: options.span_geometry,
            page_jobs: options.page_jobs,
        };
        extract_pages(document, lopdf_options, &|signals| {
            load_ocr_if_needed(source_path, ocr, signals)
        })
    }

    fn extract_page(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
        page_index: u32,
    ) -> Result<ExtractedPage> {
        let lopdf_options = LopdfExtractionOptions {
            span_geometry: options.span_geometry,
            page_jobs: options.page_jobs,
        };
        extract_page_by_index(
            document,
            lopdf_options,
            &|signals| load_ocr_if_needed(source_path, ocr, signals),
            page_index,
        )
    }
}

#[cfg(feature = "pdfium")]
impl PdfBackend for PdfiumBackend {
    type Document = PdfiumDocument;

    fn name(&self) -> &'static str {
        PDFIUM_BACKEND_NAME
    }

    fn version(&self) -> &'static str {
        PDFIUM_BACKEND_VERSION
    }

    fn supports_parallel_documents(&self) -> bool {
        false
    }

    fn supports_page_render_for_ocr(&self) -> bool {
        true
    }

    fn load_document(&self, path: &Path) -> Result<Self::Document> {
        let pdf_document = load_pdfium_document_from_file(path)?;
        let page_count = pdfium_page_count(&pdf_document)?;

        Ok(PdfiumDocument {
            pdf_document,
            page_count,
        })
    }

    fn page_count(&self, document: &Self::Document) -> usize {
        document.page_count
    }

    fn extract_pages(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
    ) -> Result<Vec<ExtractedPage>> {
        extract_pdfium_pages(document, source_path, ocr, options)
    }

    fn extract_page(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
        page_index: u32,
    ) -> Result<ExtractedPage> {
        extract_pdfium_page_by_index(document, source_path, ocr, options, page_index)
    }

    fn ocr_check_rendered_image(
        &self,
        pdf: &Path,
        page_index: u32,
        ocr: OcrOptions<'_>,
        strict: bool,
    ) -> OcrCheckOutput {
        pdfium_ocr_check_rendered_image_output(pdf, page_index, ocr, strict)
    }
}

pub(crate) fn inspect_corpus<B: PdfBackend>(
    backend: &B,
    path: &Path,
) -> Result<CorpusInspectOutput> {
    let documents = discover_pdfs(path)?
        .into_iter()
        .map(|pdf| -> Result<CorpusInspectDocument> {
            let (document, fingerprint) = load_document(backend, &pdf.path)?;
            Ok(CorpusInspectDocument {
                path: pdf.label,
                metadata: document_metadata(backend, &pdf.path)?,
                document_fingerprint: fingerprint,
                page_count: backend.page_count(&document),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let page_count = documents.iter().map(|document| document.page_count).sum();
    let corpus_fingerprint = corpus_fingerprint(documents.iter().map(|document| {
        (
            document.path.as_str(),
            document.document_fingerprint.as_str(),
            document.page_count,
        )
    }));

    Ok(CorpusInspectOutput {
        backend: backend.name(),
        document_count: documents.len(),
        page_count,
        corpus_fingerprint,
        documents,
    })
}

pub(crate) fn inspect_corpus_pages<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    cache_dir: Option<&Path>,
    jobs: usize,
) -> Result<CorpusInspectPagesOutput> {
    let pdfs = discover_pdfs(path)?;
    let worker_count = document_worker_count(backend, jobs, pdfs.len());
    let documents = if worker_count == 1 {
        pdfs.into_iter()
            .map(|pdf| inspect_corpus_pages_document(backend, pdf, cache_dir))
            .collect::<Result<Vec<_>>>()?
    } else {
        inspect_corpus_pages_parallel(backend, pdfs, cache_dir, worker_count)?
    };
    let page_count = documents.iter().map(|document| document.page_count).sum();
    let cache_hits = documents
        .iter()
        .filter(|document| document.cache_status == CacheStatus::Hit)
        .count() as u32;
    let cache_misses = documents
        .iter()
        .filter(|document| document.cache_status == CacheStatus::Miss)
        .count() as u32;
    let fallback_pages = documents
        .iter()
        .map(|document| document.fallback_pages)
        .sum();
    let ocr_required_pages = documents
        .iter()
        .map(|document| document.ocr_required_pages)
        .sum();
    let ocr_applied_pages = documents
        .iter()
        .map(|document| document.ocr_applied_pages)
        .sum();
    let warnings_count = documents
        .iter()
        .map(|document| document.warnings_count)
        .sum();
    let corpus_fingerprint = corpus_fingerprint(documents.iter().map(|document| {
        (
            document.path.as_str(),
            document.document_fingerprint.as_str(),
            document.page_count,
        )
    }));

    Ok(CorpusInspectPagesOutput {
        backend: backend.name(),
        document_count: documents.len(),
        page_count,
        worker_count,
        cache_hits,
        cache_misses,
        fallback_pages,
        ocr_required_pages,
        ocr_applied_pages,
        warnings_count,
        corpus_fingerprint,
        documents,
    })
}

pub(crate) fn inspect_corpus_pages_parallel<B: PdfBackend + Sync>(
    backend: &B,
    pdfs: Vec<DiscoveredPdf>,
    cache_dir: Option<&Path>,
    worker_count: usize,
) -> Result<Vec<CorpusInspectPagesDocument>> {
    let indexed_pdfs = pdfs.into_iter().enumerate().collect::<Vec<_>>();
    let mut indexed_documents = Vec::with_capacity(indexed_pdfs.len());

    for chunk in indexed_pdfs.chunks(worker_count) {
        let mut chunk_results = Vec::with_capacity(chunk.len());
        thread::scope(|scope| -> Result<()> {
            let handles = chunk
                .iter()
                .map(|(index, pdf)| {
                    let pdf = pdf.clone();
                    scope.spawn(move || {
                        inspect_corpus_pages_document(backend, pdf, cache_dir)
                            .map(|document| (*index, document))
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                chunk_results.push(
                    handle
                        .join()
                        .map_err(|_| anyhow!("inspect pages worker panicked"))??,
                );
            }

            Ok(())
        })?;
        indexed_documents.extend(chunk_results);
    }

    indexed_documents.sort_by_key(|(index, _)| *index);
    Ok(indexed_documents
        .into_iter()
        .map(|(_, document)| document)
        .collect())
}

pub(crate) fn inspect_corpus_pages_document<B: PdfBackend>(
    backend: &B,
    pdf: DiscoveredPdf,
    cache_dir: Option<&Path>,
) -> Result<CorpusInspectPagesDocument> {
    let artifact = parse_pdf(
        backend,
        &pdf.path,
        OcrOptions::default(),
        cache_dir,
        ExtractionOptions {
            span_geometry: false,
            page_jobs: 1,
        },
    )?;
    let warnings = artifact.global_diagnostics.warnings.clone();
    let pages = inspect_page_summaries(&artifact, &warnings);
    Ok(CorpusInspectPagesDocument {
        path: pdf.label,
        metadata: artifact.metadata,
        document_fingerprint: artifact.document_fingerprint,
        page_count: artifact.pages.len(),
        cache_status: artifact.global_diagnostics.cache_status.clone(),
        cache_key: artifact.global_diagnostics.cache_key.clone(),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        warnings_count: warnings.len(),
        pages,
    })
}

pub(crate) fn inspect_pages<B: PdfBackend>(
    backend: &B,
    path: &Path,
    cache_dir: Option<&Path>,
    jobs: usize,
) -> Result<InspectPagesOutput> {
    let artifact = parse_pdf(
        backend,
        path,
        OcrOptions::default(),
        cache_dir,
        ExtractionOptions {
            span_geometry: false,
            page_jobs: jobs.max(1),
        },
    )?;
    let warnings = artifact.global_diagnostics.warnings.clone();
    let pages = inspect_page_summaries(&artifact, &warnings);

    Ok(InspectPagesOutput {
        backend: backend.name(),
        metadata: artifact.metadata,
        document_fingerprint: artifact.document_fingerprint,
        page_count: pages.len(),
        worker_count: artifact.global_diagnostics.worker_count,
        cache_status: artifact.global_diagnostics.cache_status.clone(),
        cache_key: artifact.global_diagnostics.cache_key.clone(),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        warnings_count: warnings.len(),
        pages,
    })
}

pub(crate) fn inspect_page_summaries(
    artifact: &DocumentArtifact,
    warnings: &[String],
) -> Vec<InspectPageSummary> {
    artifact
        .pages
        .iter()
        .map(|page| InspectPageSummary {
            page_index: page.page_index,
            artifact_id: page.artifact_id.clone(),
            page_fingerprint: page.fingerprint.as_hex().to_string(),
            dimensions: page.dimensions.clone(),
            route: page.route.route,
            quality_flags: page.quality.flags.clone(),
            reasons: page.route.reasons.clone(),
            native_span_count: page.native_spans.len(),
            native_text_bytes: page
                .native_spans
                .iter()
                .map(|span| span.text.len())
                .sum::<usize>(),
            ocr_span_count: page.ocr_spans.len(),
            image_artifact_count: page.image_artifacts.len(),
            layout_block_count: page.layout_blocks.len(),
            layout: layout_summary_from_page(page),
            timings: page.timings.clone(),
            warnings: warnings_for_page(warnings, page.page_index),
        })
        .collect()
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))
))]
pub(crate) fn peak_rss_bytes() -> u64 {
    getrusage_maxrss()
        .map(|maxrss_kb| maxrss_kb.saturating_mul(1024))
        .unwrap_or_default()
}

/// Prose lines are the anchors most likely to be reproduced by any correct
/// text extractor; figure-diagram fragments, dotted TOC rows, and token
/// sequences diverge across extractors even when content is intact.
pub(crate) fn line_looks_like_prose_anchor(line: &str) -> bool {
    let word_count = line.split_whitespace().count();
    let alphanumeric_count = line.chars().filter(|ch| ch.is_alphanumeric()).count();
    let alphabetic_count = line.chars().filter(|ch| ch.is_alphabetic()).count();

    word_count >= 4 && alphanumeric_count > 0 && alphabetic_count * 10 >= alphanumeric_count * 7
}

/// True when the candidate anchor line exists inside one extracted span, so
/// it does not depend on Glyphrush's row-joining decisions. A line stitched
/// together from multiple positioned fragments (form headers, footers with
/// page numbers) is layout-flavored and unfair to external text baselines.
pub(crate) fn line_is_contained_in_a_single_span(line: &str, page: &PageArtifact) -> bool {
    let squashed_line = squashed_required_text_anchor(line);
    if squashed_line.is_empty() {
        return false;
    }
    page.native_spans
        .iter()
        .chain(page.ocr_spans.iter())
        .any(|span| squashed_required_text_anchor(&span.text).contains(&squashed_line))
}

pub(crate) fn parse_pdf<B: PdfBackend>(
    backend: &B,
    path: &Path,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
) -> Result<DocumentArtifact> {
    let fingerprint = document_fingerprint(path)?;
    let source_size_bytes = source_size_bytes(path)?;
    let source_modified_unix_ms = source_modified_unix_ms(path)?;
    let cache_key = cache_key(
        backend.name(),
        backend.version(),
        &fingerprint,
        path,
        ocr,
        options,
    )?;
    let mut cache_ignored_warning = None;
    if let Some(cache_dir) = cache_dir {
        let cached = load_cached_artifact(cache_dir, &cache_key)?;
        cache_ignored_warning = cached.ignored_warning;
        if let Some(mut artifact) = cached.artifact {
            clear_page_stage_timings(&mut artifact);
            artifact.metadata =
                document_metadata_with_source(backend, source_size_bytes, source_modified_unix_ms);
            artifact.global_diagnostics.cache_status = CacheStatus::Hit;
            artifact.global_diagnostics.cache_key = Some(cache_key);
            set_artifact_worker_count(&mut artifact, options);
            return Ok(artifact);
        }
    }

    let open_start = Instant::now();
    let document = backend.load_document(path)?;
    let open_us = open_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;
    let mut pages = backend.extract_pages(&document, path, ocr, options)?;
    if let Some(first_page) = pages.first_mut() {
        first_page.timings.open_us = open_us;
    }
    let mut artifact = parse_extracted_pages(fingerprint, pages).with_metadata(
        document_metadata_with_source(backend, source_size_bytes, source_modified_unix_ms),
    );
    set_artifact_worker_count(&mut artifact, options);
    if let Some(cache_dir) = cache_dir {
        artifact.global_diagnostics.cache_status = CacheStatus::Miss;
        artifact.global_diagnostics.cache_key = Some(cache_key.clone());
        if let Some(warning) = cache_ignored_warning {
            artifact.global_diagnostics.warnings.push(warning);
        }
        store_cached_artifact(cache_dir, &cache_key, &artifact)?;
    }
    Ok(artifact)
}

pub(crate) fn set_artifact_worker_count(
    artifact: &mut DocumentArtifact,
    options: ExtractionOptions,
) {
    artifact.global_diagnostics.worker_count =
        effective_page_worker_count(options, artifact.pages.len());
}

pub(crate) fn effective_page_worker_count(options: ExtractionOptions, page_count: usize) -> usize {
    options.page_jobs.max(1).min(page_count.max(1))
}

pub(crate) fn load_document<B: PdfBackend>(
    backend: &B,
    path: &Path,
) -> Result<(B::Document, String)> {
    let fingerprint = document_fingerprint(path)?;
    let document = backend.load_document(path)?;

    Ok((document, fingerprint))
}

pub(crate) fn document_metadata<B: PdfBackend>(
    backend: &B,
    path: &Path,
) -> Result<DocumentMetadata> {
    Ok(document_metadata_with_source(
        backend,
        source_size_bytes(path)?,
        source_modified_unix_ms(path)?,
    ))
}

pub(crate) fn document_metadata_with_source<B: PdfBackend>(
    backend: &B,
    source_size_bytes: u64,
    source_modified_unix_ms: u64,
) -> DocumentMetadata {
    DocumentMetadata {
        parser_name: PARSER_NAME.to_string(),
        parser_version: PARSER_VERSION.to_string(),
        backend: backend.name().to_string(),
        backend_version: backend.version().to_string(),
        source_size_bytes,
        source_modified_unix_ms,
    }
}

pub(crate) fn document_fingerprint(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

pub(crate) fn source_size_bytes(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("read metadata {}", path.display()))?
        .len())
}

pub(crate) fn source_modified_unix_ms(path: &Path) -> Result<u64> {
    let modified = fs::metadata(path)
        .with_context(|| format!("read metadata {}", path.display()))?
        .modified()
        .with_context(|| format!("read modified time {}", path.display()))?;
    Ok(modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64)
}
