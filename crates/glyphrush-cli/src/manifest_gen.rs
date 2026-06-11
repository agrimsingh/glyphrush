use crate::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
};

use anyhow::{Context, Result, anyhow, bail};
use glyphrush_core::{BBox, DocumentArtifact, PageArtifact, PageQuality, PageRoute};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedEvalManifest {
    pub(crate) manifest_version: &'static str,
    pub(crate) generator: GeneratedManifestGenerator,
    pub(crate) corpus_fingerprint: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) required_categories: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) min_category_counts: BTreeMap<String, usize>,
    pub(crate) documents: Vec<GeneratedManifestDocument>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedManifestGenerator {
    pub(crate) parser_name: &'static str,
    pub(crate) parser_version: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) backend_version: &'static str,
    pub(crate) span_geometry: bool,
    pub(crate) ocr_sidecar: bool,
    pub(crate) ocr_command: bool,
    pub(crate) ocr_http_url: bool,
    pub(crate) ocr_timeout_ms: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedManifestDocument {
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<String>,
    pub(crate) document_fingerprint: String,
    pub(crate) source_size_bytes: u64,
    pub(crate) source_modified_unix_ms: u64,
    pub(crate) expect: GeneratedManifestExpectations,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedManifestExpectations {
    pub(crate) page_count: usize,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) warnings_count: usize,
    pub(crate) required_warnings: Vec<String>,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) ocr_required_classification: OcrRequiredClassificationExpectation,
    pub(crate) quality_flag_classification: Vec<QualityFlagClassificationExpectation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) table_structure: Vec<TableStructureExpectation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) span_bbox: Vec<SpanBBoxExpectation>,
    pub(crate) silent_failures: GeneratedSilentFailuresExpectation,
    pub(crate) pages: Vec<GeneratedPageExpectation>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedSilentFailuresExpectation {
    pub(crate) max_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedPageExpectation {
    pub(crate) index: u32,
    pub(crate) artifact_id: String,
    pub(crate) page_fingerprint: String,
    pub(crate) route: PageRoute,
    pub(crate) empty_text_output: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) required_text: Vec<String>,
    pub(crate) image_artifact_count: u32,
    pub(crate) layout_block_counts: DebugLayoutSummary,
    pub(crate) required_flags: Vec<PageQuality>,
    pub(crate) required_reasons: Vec<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct ManifestRunConfig<'a> {
    pub(crate) category: Option<&'a str>,
    pub(crate) category_from_path: bool,
    pub(crate) required_categories: &'a [String],
    pub(crate) min_category_counts: &'a [CategoryCountSpec],
    pub(crate) ocr: OcrOptions<'a>,
    pub(crate) cache_dir: Option<&'a Path>,
    pub(crate) extraction: ExtractionOptions,
    pub(crate) jobs: usize,
}

pub(crate) fn manifest_required_categories_with_preset(
    required_categories: &[String],
    preset: Option<CoveragePreset>,
) -> Vec<String> {
    let mut categories = preset
        .into_iter()
        .flat_map(|preset| preset.categories().iter().copied())
        .map(str::to_string)
        .collect::<Vec<_>>();
    categories.extend(required_categories.iter().cloned());
    categories
}

pub(crate) fn manifest_min_category_counts_with_preset(
    min_category_counts: &[CategoryCountSpec],
    preset: Option<CoveragePreset>,
) -> Vec<CategoryCountSpec> {
    let mut counts = preset
        .into_iter()
        .flat_map(|preset| preset.categories().iter().copied())
        .map(|category| CategoryCountSpec {
            category: category.to_string(),
            count: 1,
        })
        .collect::<Vec<_>>();
    counts.extend(min_category_counts.iter().cloned());
    counts
}

pub(crate) fn generate_eval_manifest<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    config: ManifestRunConfig<'_>,
) -> Result<GeneratedEvalManifest> {
    let category = normalize_manifest_category(config.category);
    let required_categories = normalize_required_categories(config.required_categories, None);
    let min_category_counts = min_category_counts_from_specs(config.min_category_counts);
    let documents = if path.is_dir() {
        let pdfs = if config.category_from_path {
            discover_manifest_pdfs_from_category_paths(path)?
        } else {
            discover_pdfs(path)?
        };
        let worker_count = document_worker_count(backend, config.jobs, pdfs.len());
        if worker_count == 1 {
            pdfs.into_iter()
                .map(|pdf| {
                    let document_category = pdf.category.as_deref().or(category.as_deref());
                    generated_manifest_document(
                        backend,
                        &pdf.path,
                        pdf.label,
                        config.ocr,
                        config.cache_dir,
                        config.extraction,
                        document_category,
                    )
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            generate_manifest_documents_parallel(
                backend,
                pdfs,
                config.ocr,
                config.cache_dir,
                config.extraction,
                worker_count,
                category.as_deref(),
            )?
        }
    } else {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToString::to_string)
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        vec![generated_manifest_document(
            backend,
            path,
            label,
            config.ocr,
            config.cache_dir,
            config.extraction,
            category.as_deref(),
        )?]
    };
    let corpus_fingerprint = corpus_fingerprint(documents.iter().map(|document| {
        (
            document.path.as_str(),
            document.document_fingerprint.as_str(),
            document.expect.page_count,
        )
    }));

    Ok(GeneratedEvalManifest {
        manifest_version: "glyphrush-eval-manifest-v1",
        generator: GeneratedManifestGenerator {
            parser_name: PARSER_NAME,
            parser_version: PARSER_VERSION,
            backend: backend.name(),
            backend_version: backend.version(),
            span_geometry: config.extraction.span_geometry,
            ocr_sidecar: config.ocr.sidecar.is_some(),
            ocr_command: config.ocr.command.is_some(),
            ocr_http_url: config.ocr.http_url.is_some(),
            ocr_timeout_ms: duration_millis(config.ocr.timeout),
        },
        corpus_fingerprint,
        required_categories,
        min_category_counts,
        documents,
    })
}

pub(crate) fn generate_manifest_documents_parallel<B: PdfBackend + Sync>(
    backend: &B,
    pdfs: Vec<DiscoveredPdf>,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
    worker_count: usize,
    category: Option<&str>,
) -> Result<Vec<GeneratedManifestDocument>> {
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
                        let document_category = pdf.category.as_deref().or(category);
                        generated_manifest_document(
                            backend,
                            &pdf.path,
                            pdf.label,
                            ocr,
                            cache_dir,
                            options,
                            document_category,
                        )
                        .map(|document| (*index, document))
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                chunk_results.push(
                    handle
                        .join()
                        .map_err(|_| anyhow!("manifest worker panicked"))??,
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

pub(crate) fn generated_manifest_document<B: PdfBackend>(
    backend: &B,
    path: &Path,
    label: String,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
    category: Option<&str>,
) -> Result<GeneratedManifestDocument> {
    let artifact = parse_pdf(backend, path, ocr, cache_dir, options)?;

    Ok(GeneratedManifestDocument {
        path: label,
        category: category.map(str::to_string),
        document_fingerprint: artifact.document_fingerprint.clone(),
        source_size_bytes: artifact.metadata.source_size_bytes,
        source_modified_unix_ms: artifact.metadata.source_modified_unix_ms,
        expect: generated_manifest_expectations(&artifact),
    })
}

pub(crate) fn generated_manifest_expectations(
    artifact: &DocumentArtifact,
) -> GeneratedManifestExpectations {
    GeneratedManifestExpectations {
        page_count: artifact.pages.len(),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        image_artifact_count: image_artifact_count_from_artifact(artifact),
        warnings_count: artifact.global_diagnostics.warnings.len(),
        required_warnings: artifact.global_diagnostics.warnings.clone(),
        route_counts: route_counts_from_artifact(artifact),
        route_reason_counts: route_reason_counts_from_artifact(artifact),
        quality_flag_counts: quality_flag_counts_from_artifact(artifact),
        ocr_required_classification: OcrRequiredClassificationExpectation {
            expected_pages: expected_pages_for_quality(artifact, PageQuality::RequiresOcr),
            min_precision: Some(1.0),
            min_recall: Some(1.0),
        },
        quality_flag_classification: generated_quality_flag_classification(artifact),
        table_structure: generated_table_structure_expectations(artifact),
        span_bbox: generated_span_bbox_expectations(artifact),
        silent_failures: GeneratedSilentFailuresExpectation { max_count: 0 },
        pages: artifact
            .pages
            .iter()
            .map(generated_page_expectation)
            .collect(),
    }
}

pub(crate) fn generated_quality_flag_classification(
    artifact: &DocumentArtifact,
) -> Vec<QualityFlagClassificationExpectation> {
    [
        PageQuality::LowConfidenceText,
        PageQuality::BrokenEncoding,
        PageQuality::LayoutUncertain,
        PageQuality::TableUncertain,
        PageQuality::UnsupportedFeature,
    ]
    .into_iter()
    .filter_map(|flag| {
        let expected_pages = expected_pages_for_quality(artifact, flag.clone());
        (!expected_pages.is_empty()).then_some(QualityFlagClassificationExpectation {
            flag,
            expected_pages,
            min_precision: Some(1.0),
            min_recall: Some(1.0),
        })
    })
    .collect()
}

pub(crate) fn generated_table_structure_expectations(
    artifact: &DocumentArtifact,
) -> Vec<TableStructureExpectation> {
    artifact
        .pages
        .iter()
        .filter_map(|page| {
            let expected_rows = table_rows_for_page(artifact, page.page_index);
            (expected_rows.len() >= 2).then_some(TableStructureExpectation {
                page: page.page_index,
                expected_rows,
                min_row_precision: None,
                min_row_recall: Some(1.0),
                min_row_f1: None,
                min_cell_precision: None,
                min_cell_recall: Some(1.0),
                min_cell_f1: Some(1.0),
                baseline: false,
            })
        })
        .collect()
}

pub(crate) fn generated_span_bbox_expectations(
    artifact: &DocumentArtifact,
) -> Vec<SpanBBoxExpectation> {
    const MAX_SPAN_BBOX_EXPECTATIONS: usize = 10;

    artifact
        .pages
        .iter()
        .filter_map(generated_span_bbox_expectation_for_page)
        .take(MAX_SPAN_BBOX_EXPECTATIONS)
        .collect()
}

pub(crate) fn generated_span_bbox_expectation_for_page(
    page: &PageArtifact,
) -> Option<SpanBBoxExpectation> {
    const BBOX_TOLERANCE: f32 = 0.5;
    const MAX_SAMPLE_TEXT_CHARS: usize = 80;

    page.native_spans
        .iter()
        .chain(page.ocr_spans.iter())
        .filter(|span| !is_page_wide_bbox(&span.bbox, page))
        .find(|span| is_substantive_required_text_anchor(span.text.trim()))
        .map(|span| {
            let text = span
                .text
                .trim()
                .chars()
                .take(MAX_SAMPLE_TEXT_CHARS)
                .collect::<String>();
            SpanBBoxExpectation {
                page: page.page_index,
                text,
                provenance: Some(span.provenance.clone()),
                min_x0: Some(span.bbox.x0 - BBOX_TOLERANCE),
                max_x0: Some(span.bbox.x0 + BBOX_TOLERANCE),
                min_y0: Some(span.bbox.y0 - BBOX_TOLERANCE),
                max_y0: Some(span.bbox.y0 + BBOX_TOLERANCE),
                min_x1: Some(span.bbox.x1 - BBOX_TOLERANCE),
                max_x1: Some(span.bbox.x1 + BBOX_TOLERANCE),
                min_y1: Some(span.bbox.y1 - BBOX_TOLERANCE),
                max_y1: Some(span.bbox.y1 + BBOX_TOLERANCE),
            }
        })
}

pub(crate) fn is_page_wide_bbox(bbox: &BBox, page: &PageArtifact) -> bool {
    nearly_equal_f32(bbox.x0, 0.0)
        && nearly_equal_f32(bbox.y0, 0.0)
        && nearly_equal_f32(bbox.x1, page.dimensions.width)
        && nearly_equal_f32(bbox.y1, page.dimensions.height)
}

pub(crate) fn nearly_equal_f32(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.001
}

pub(crate) fn generated_page_expectation(page: &PageArtifact) -> GeneratedPageExpectation {
    GeneratedPageExpectation {
        index: page.page_index,
        artifact_id: page.artifact_id.clone(),
        page_fingerprint: page.fingerprint.as_hex().to_string(),
        route: page.route.route,
        empty_text_output: plain_text_from_page(page).is_empty(),
        required_text: generated_page_required_text(page),
        image_artifact_count: page.image_artifacts.len() as u32,
        layout_block_counts: layout_summary_from_page(page),
        required_flags: page.quality.flags.clone(),
        required_reasons: page.route.reasons.clone(),
    }
}

pub(crate) fn generated_page_required_text(page: &PageArtifact) -> Vec<String> {
    const MAX_ANCHOR_CHARS: usize = 160;

    let page_text = quality_text_from_page(page);
    let fallback_line = page_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty());

    const MIN_NEUTRAL_ANCHOR_CHARS: usize = 12;
    const MAX_NEUTRAL_ANCHOR_CHARS: usize = 60;

    let neutral_anchor_candidate = |line: &&str| {
        is_substantive_required_text_anchor(line)
            && is_backend_neutral_required_text_anchor(line)
            && line_is_contained_in_a_single_span(line, page)
            && (MIN_NEUTRAL_ANCHOR_CHARS..=MAX_NEUTRAL_ANCHOR_CHARS).contains(&line.chars().count())
    };

    page_text
        .lines()
        .map(str::trim)
        .find(|line| neutral_anchor_candidate(line) && line_looks_like_prose_anchor(line))
        .or_else(|| {
            page_text
                .lines()
                .map(str::trim)
                .find(neutral_anchor_candidate)
        })
        .or_else(|| {
            page_text.lines().map(str::trim).find(|line| {
                is_substantive_required_text_anchor(line)
                    && is_backend_neutral_required_text_anchor(line)
                    && line_is_contained_in_a_single_span(line, page)
            })
        })
        .or_else(|| {
            page_text.lines().map(str::trim).find(|line| {
                is_substantive_required_text_anchor(line)
                    && is_backend_neutral_required_text_anchor(line)
            })
        })
        .or_else(|| {
            page_text
                .lines()
                .map(str::trim)
                .find(|line| is_substantive_required_text_anchor(line))
        })
        .or(fallback_line)
        .map(|line| {
            if line.chars().count() <= MAX_ANCHOR_CHARS {
                line.to_string()
            } else {
                line.chars().take(MAX_ANCHOR_CHARS).collect()
            }
        })
        .into_iter()
        .collect()
}

pub(crate) fn normalize_manifest_category(category: Option<&str>) -> Option<String> {
    category
        .map(str::trim)
        .filter(|category| !category.is_empty())
        .map(str::to_string)
}

pub(crate) fn normalize_manifest_category_filter(category: Option<&str>) -> BTreeSet<String> {
    category
        .into_iter()
        .flat_map(|category| category.split(','))
        .filter_map(|category| normalize_manifest_category(Some(category)))
        .collect()
}

pub(crate) fn normalize_required_categories(
    categories: &[String],
    filter: Option<&str>,
) -> Vec<String> {
    let filter = normalize_manifest_category_filter(filter);
    let mut categories = categories
        .iter()
        .filter_map(|category| normalize_manifest_category(Some(category)))
        .filter(|category| manifest_category_filter_matches(&filter, category))
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();
    categories
}

pub(crate) fn min_category_counts_from_specs(
    specs: &[CategoryCountSpec],
) -> BTreeMap<String, usize> {
    specs
        .iter()
        .filter_map(|spec| {
            normalize_manifest_category(Some(&spec.category)).map(|category| (category, spec.count))
        })
        .fold(BTreeMap::new(), |mut counts, (category, count)| {
            counts
                .entry(category)
                .and_modify(|existing| *existing = (*existing).max(count))
                .or_insert(count);
            counts
        })
}

#[derive(Clone)]
pub(crate) struct DiscoveredPdf {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
    pub(crate) category: Option<String>,
}

pub(crate) fn discover_pdfs(path: &Path) -> Result<Vec<DiscoveredPdf>> {
    let mut pdfs = Vec::new();
    collect_discovered_pdfs(path, path, false, &mut pdfs)?;
    pdfs.sort_by(|left, right| left.label.cmp(&right.label));

    if pdfs.is_empty() {
        bail!("no PDF files found in {}", path.display());
    }

    Ok(pdfs)
}

pub(crate) fn discover_manifest_pdfs_from_category_paths(
    path: &Path,
) -> Result<Vec<DiscoveredPdf>> {
    let mut pdfs = Vec::new();
    collect_discovered_pdfs(path, path, true, &mut pdfs)?;
    pdfs.sort_by(|left, right| left.label.cmp(&right.label));

    if pdfs.is_empty() {
        bail!("no PDF files found in {}", path.display());
    }

    Ok(pdfs)
}

pub(crate) fn collect_discovered_pdfs(
    root: &Path,
    directory: &Path,
    infer_category_from_path: bool,
    pdfs: &mut Vec<DiscoveredPdf>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("read directory {}", directory.display()))?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry_path.display()))?;
        if file_type.is_dir() {
            collect_discovered_pdfs(root, &entry_path, infer_category_from_path, pdfs)?;
        } else if file_type.is_file() && path_has_pdf_extension(&entry_path) {
            let relative_path = entry_path
                .strip_prefix(root)
                .unwrap_or(entry_path.as_path());
            let label = path_label(relative_path);
            let category = infer_category_from_path
                .then(|| category_from_relative_pdf_path(relative_path))
                .flatten();
            pdfs.push(DiscoveredPdf {
                path: entry_path,
                label,
                category,
            });
        }
    }

    Ok(())
}

pub(crate) fn path_has_pdf_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}
