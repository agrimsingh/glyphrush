use crate::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
};

use anyhow::{Context, Result, anyhow, bail};
use glyphrush_core::{
    CacheStatus, DocumentArtifact, DocumentMetadata, PageQuality, PageRoute, SpanProvenance,
    sha256_hex,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) const EVAL_REPORT_VERSION: &str = "glyphrush-eval-report-v1";

#[derive(Debug, Deserialize)]
pub(crate) struct EvalManifest {
    #[serde(default)]
    pub(crate) required_categories: Vec<String>,
    #[serde(default)]
    pub(crate) min_category_counts: BTreeMap<String, usize>,
    pub(crate) documents: Vec<EvalManifestDocument>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EvalManifestDocument {
    pub(crate) path: String,
    pub(crate) category: Option<String>,
    pub(crate) document_fingerprint: Option<String>,
    pub(crate) source_size_bytes: Option<u64>,
    pub(crate) source_modified_unix_ms: Option<u64>,
    #[serde(default)]
    pub(crate) expect: Value,
    #[serde(default)]
    pub(crate) expect_by_backend: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct EvalExpectations {
    pub(crate) page_count: Option<usize>,
    pub(crate) fallback_pages: Option<u32>,
    pub(crate) ocr_required_pages: Option<u32>,
    pub(crate) ocr_applied_pages: Option<u32>,
    pub(crate) image_artifact_count: Option<u32>,
    pub(crate) warnings_count: Option<usize>,
    pub(crate) route_counts: Option<RouteCounts>,
    pub(crate) quality_flag_counts: Option<QualityFlagCounts>,
    #[serde(default)]
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) text_recall: Option<TextRecallExpectation>,
    pub(crate) reading_order: Option<ReadingOrderExpectation>,
    pub(crate) ocr_required_classification: Option<OcrRequiredClassificationExpectation>,
    pub(crate) silent_failures: Option<SilentFailuresExpectation>,
    #[serde(default)]
    pub(crate) quality_flag_classification: Vec<QualityFlagClassificationExpectation>,
    #[serde(default)]
    pub(crate) table_structure: Vec<TableStructureExpectation>,
    #[serde(default)]
    pub(crate) span_bbox: Vec<SpanBBoxExpectation>,
    #[serde(default)]
    pub(crate) required_text: Vec<String>,
    #[serde(default)]
    pub(crate) baseline_required_text: Vec<String>,
    #[serde(default)]
    pub(crate) required_warnings: Vec<String>,
    #[serde(default)]
    pub(crate) pages: Vec<EvalPageExpectation>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TextRecallExpectation {
    pub(crate) expected: String,
    pub(crate) min_word_recall: Option<f64>,
    pub(crate) min_char_recall: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReadingOrderExpectation {
    #[serde(default)]
    pub(crate) expected_sequence: Vec<String>,
    pub(crate) min_score: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReadingOrderMatch {
    pub(crate) snippet: String,
    pub(crate) position: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReadingOrderInversion {
    pub(crate) before: String,
    pub(crate) after: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OcrRequiredClassificationExpectation {
    #[serde(default)]
    pub(crate) expected_pages: Vec<u32>,
    pub(crate) min_precision: Option<f64>,
    pub(crate) min_recall: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SilentFailuresExpectation {
    pub(crate) max_count: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SilentFailurePage {
    pub(crate) page: u32,
    pub(crate) flags: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) empty_text_output: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct QualityFlagClassificationExpectation {
    pub(crate) flag: PageQuality,
    #[serde(default)]
    pub(crate) expected_pages: Vec<u32>,
    pub(crate) min_precision: Option<f64>,
    pub(crate) min_recall: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TableStructureExpectation {
    pub(crate) page: u32,
    #[serde(default)]
    pub(crate) expected_rows: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_row_precision: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_row_recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_row_f1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_cell_precision: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_cell_recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_cell_f1: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) baseline: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SpanBBoxExpectation {
    pub(crate) page: u32,
    pub(crate) text: String,
    pub(crate) provenance: Option<SpanProvenance>,
    pub(crate) min_x0: Option<f32>,
    pub(crate) max_x0: Option<f32>,
    pub(crate) min_y0: Option<f32>,
    pub(crate) max_y0: Option<f32>,
    pub(crate) min_x1: Option<f32>,
    pub(crate) max_x1: Option<f32>,
    pub(crate) min_y1: Option<f32>,
    pub(crate) max_y1: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct TableCell {
    pub(crate) row: usize,
    pub(crate) column: usize,
    pub(crate) text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EvalPageExpectation {
    pub(crate) index: u32,
    pub(crate) artifact_id: Option<String>,
    pub(crate) page_fingerprint: Option<String>,
    pub(crate) route: Option<PageRoute>,
    pub(crate) empty_text_output: Option<bool>,
    pub(crate) image_artifact_count: Option<u32>,
    pub(crate) layout_block_counts: Option<DebugLayoutSummary>,
    #[serde(default)]
    pub(crate) required_text: Vec<String>,
    #[serde(default)]
    pub(crate) required_flags: Vec<PageQuality>,
    #[serde(default)]
    pub(crate) required_reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvalOutput {
    pub(crate) report_version: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) run_metadata: BenchmarkRunMetadata,
    pub(crate) run_configuration: RunConfiguration,
    pub(crate) manifest_path: String,
    pub(crate) manifest_sha256: String,
    pub(crate) corpus_fingerprint: String,
    pub(crate) document_count: usize,
    pub(crate) category_counts: BTreeMap<String, usize>,
    pub(crate) category_summaries: BTreeMap<String, EvalCategorySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category_coverage: Option<CategoryCoverageOutput>,
    pub(crate) page_count: usize,
    pub(crate) worker_count: usize,
    pub(crate) cache_hits: u32,
    pub(crate) cache_misses: u32,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) image_artifact_pages: u32,
    pub(crate) empty_text_output_pages: usize,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) fallback_action_counts: FallbackActionCounts,
    pub(crate) warnings_count: usize,
    pub(crate) passed: bool,
    pub(crate) quality_passed: bool,
    pub(crate) quality_failed: bool,
    pub(crate) failed_checks: u32,
    pub(crate) failure_samples: Vec<EvalFailureSample>,
    pub(crate) documents: Vec<EvalDocumentOutput>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CategoryCoverageOutput {
    pub(crate) required: Vec<String>,
    pub(crate) present: Vec<String>,
    pub(crate) missing: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) min_category_counts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) under_minimum: BTreeMap<String, CategoryMinimumCoverageOutput>,
    pub(crate) passed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CategoryMinimumCoverageOutput {
    pub(crate) required: usize,
    pub(crate) actual: usize,
}

pub(crate) struct EvalOutputContext<'a> {
    pub(crate) run_metadata: BenchmarkRunMetadata,
    pub(crate) run_configuration: RunConfiguration,
    pub(crate) manifest_path: &'a Path,
    pub(crate) manifest_sha256: String,
    pub(crate) required_categories: Vec<String>,
    pub(crate) min_category_counts: BTreeMap<String, usize>,
    pub(crate) worker_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct EvalCategorySummary {
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) passed_documents: usize,
    pub(crate) failed_documents: usize,
    pub(crate) failed_checks: u32,
    pub(crate) quality_passed: bool,
    pub(crate) quality_failed: bool,
}

impl EvalCategorySummary {
    pub(crate) fn add_document(&mut self, document: &EvalDocumentOutput) {
        let document_failed_checks = document
            .checks
            .values()
            .filter(|check| !check.passed)
            .count() as u32;

        self.document_count += 1;
        self.page_count += document.page_count;
        self.failed_checks += document_failed_checks;
        if document.passed {
            self.passed_documents += 1;
        } else {
            self.failed_documents += 1;
        }
        self.quality_passed = self.failed_checks == 0;
        self.quality_failed = !self.quality_passed;
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct EvalDocumentOutput {
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<String>,
    pub(crate) document_fingerprint: String,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) page_count: usize,
    pub(crate) artifact_cache_status: CacheStatus,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) image_artifact_pages: u32,
    pub(crate) empty_text_output_pages: usize,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) fallback_action_counts: FallbackActionCounts,
    pub(crate) warnings_count: usize,
    pub(crate) passed: bool,
    pub(crate) checks: BTreeMap<String, EvalCheckOutput>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvalFailureSample {
    pub(crate) path: String,
    pub(crate) check: String,
    pub(crate) expected: serde_json::Value,
    pub(crate) actual: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EvalCheckOutput {
    pub(crate) passed: bool,
    pub(crate) expected: serde_json::Value,
    pub(crate) actual: serde_json::Value,
}

pub(crate) fn load_baseline_quality_expectations(
    manifest_path: &Path,
    category: Option<&str>,
) -> Result<BaselineQualityInputs> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read eval manifest {}", manifest_path.display()))?;
    let manifest: EvalManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode eval manifest {}", manifest_path.display()))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let category_filter = normalize_manifest_category_filter(category);
    let mut expectations_by_path = BTreeMap::new();
    let mut categories_by_path = BTreeMap::new();

    for document in manifest.documents {
        if !manifest_category_filter_matches(
            &category_filter,
            eval_manifest_document_category(&document),
        ) {
            continue;
        }
        let path_key = manifest_path_key(&resolve_manifest_path(manifest_dir, &document.path));
        let document_category = normalize_manifest_category(document.category.as_deref())
            .unwrap_or_else(|| "uncategorized".to_string());
        categories_by_path.insert(path_key.clone(), document_category);
        let expectations = base_eval_expectations(&document)?;
        let required_text = baseline_required_text_expectations(&expectations);
        if required_text.is_empty()
            && expectations.text_recall.is_none()
            && expectations.reading_order.is_none()
            && expectations.table_structure.is_empty()
        {
            continue;
        }

        let category = normalize_manifest_category(document.category.as_deref());
        expectations_by_path.insert(
            path_key,
            BaselineQualityExpectations {
                category,
                required_text,
                text_recall: expectations.text_recall,
                reading_order: expectations.reading_order,
                table_structure: expectations.table_structure,
            },
        );
    }

    Ok(BaselineQualityInputs {
        expectations_by_path,
        categories_by_path,
    })
}

pub(crate) fn selected_eval_manifest_path_keys(
    manifest_path: &Path,
    category: Option<&str>,
) -> Result<BTreeSet<PathBuf>> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read eval manifest {}", manifest_path.display()))?;
    let manifest: EvalManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode eval manifest {}", manifest_path.display()))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let category_filter = normalize_manifest_category_filter(category);

    Ok(manifest
        .documents
        .into_iter()
        .filter(|document| {
            manifest_category_filter_matches(
                &category_filter,
                eval_manifest_document_category(document),
            )
        })
        .map(|document| manifest_path_key(&resolve_manifest_path(manifest_dir, &document.path)))
        .collect())
}

pub(crate) fn base_eval_expectations(document: &EvalManifestDocument) -> Result<EvalExpectations> {
    decode_eval_expectations(
        eval_expectation_object(&document.expect, "expect", &document.path)?,
        &document.path,
        "expect",
    )
}

pub(crate) fn eval_expectations_for_backend(
    document: &EvalManifestDocument,
    backend: &str,
) -> Result<EvalExpectations> {
    let mut expectations = eval_expectation_object(&document.expect, "expect", &document.path)?;
    if let Some(override_value) = document.expect_by_backend.get(backend) {
        let override_expectations = eval_expectation_object(
            override_value,
            &format!("expect_by_backend.{backend}"),
            &document.path,
        )?;
        merge_eval_expectations(&mut expectations, override_expectations);
    }

    decode_eval_expectations(
        expectations,
        &document.path,
        &format!("expect plus expect_by_backend.{backend}"),
    )
}

pub(crate) fn eval_expectation_object(value: &Value, label: &str, path: &str) -> Result<Value> {
    match value {
        Value::Null => Ok(json!({})),
        Value::Object(_) => Ok(value.clone()),
        _ => bail!("{label} for manifest document {path} must be a JSON object"),
    }
}

pub(crate) fn merge_eval_expectations(base: &mut Value, overlay: Value) {
    let Some(base_object) = base.as_object_mut() else {
        unreachable!("eval expectations are normalized to an object before merge");
    };
    let Value::Object(overlay_object) = overlay else {
        unreachable!("eval expectation override is normalized to an object before merge");
    };
    for (key, value) in overlay_object {
        base_object.insert(key, value);
    }
}

pub(crate) fn decode_eval_expectations(
    value: Value,
    path: &str,
    label: &str,
) -> Result<EvalExpectations> {
    serde_json::from_value(value)
        .with_context(|| format!("decode {label} for manifest document {path}"))
}

pub(crate) fn expected_pages_for_quality(
    artifact: &DocumentArtifact,
    flag: PageQuality,
) -> Vec<u32> {
    artifact
        .pages
        .iter()
        .filter(|page| page.quality.flags.contains(&flag))
        .map(|page| page.page_index)
        .collect()
}

pub(crate) fn is_backend_neutral_required_text_anchor(line: &str) -> bool {
    !line.chars().any(|ch| ch.is_control() || ch == '|')
}

pub(crate) fn is_substantive_required_text_anchor(line: &str) -> bool {
    let mut has_letter = false;
    let mut alphanumeric_count = 0;
    for ch in line.chars() {
        if ch.is_alphabetic() {
            has_letter = true;
        }
        if ch.is_alphanumeric() {
            alphanumeric_count += 1;
        }
    }

    has_letter && alphanumeric_count >= 4
}

pub(crate) fn manifest_path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

pub(crate) struct TextRecallScore {
    pub(crate) word_recall: f64,
    pub(crate) char_recall: f64,
    pub(crate) missing_words: Vec<String>,
}

impl TextRecallScore {
    pub(crate) fn passed(&self, expectation: &TextRecallExpectation) -> bool {
        self.word_recall >= expectation.min_word_recall.unwrap_or(1.0)
            && self.char_recall >= expectation.min_char_recall.unwrap_or(1.0)
    }
}

pub(crate) struct ReadingOrderOutcome {
    pub(crate) score: f64,
    pub(crate) matched: Vec<ReadingOrderMatch>,
    pub(crate) missing: Vec<String>,
    pub(crate) inversion_count: usize,
    pub(crate) inversions: Vec<ReadingOrderInversion>,
}

pub(crate) fn empty_text_output_page_count_from_artifact(artifact: &DocumentArtifact) -> usize {
    artifact
        .pages
        .iter()
        .filter(|page| plain_text_from_page(page).is_empty())
        .count()
}

pub(crate) fn eval_document_failed_checks(document: &EvalDocumentOutput) -> u32 {
    document
        .checks
        .values()
        .filter(|check| !check.passed)
        .count() as u32
}

pub(crate) fn eval_manifest<B: PdfBackend + Sync>(
    backend: &B,
    manifest_path: &Path,
    category: Option<&str>,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
    jobs: usize,
) -> Result<EvalOutput> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read eval manifest {}", manifest_path.display()))?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);
    let mut manifest: EvalManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode eval manifest {}", manifest_path.display()))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let category_filter = normalize_manifest_category_filter(category);
    let required_categories =
        normalize_required_categories(&manifest.required_categories, category);
    let min_category_counts =
        normalize_min_category_counts(&manifest.min_category_counts, category);
    if !category_filter.is_empty() {
        manifest.documents.retain(|document| {
            manifest_category_filter_matches(
                &category_filter,
                eval_manifest_document_category(document),
            )
        });
    }
    let worker_count = document_worker_count(backend, jobs, manifest.documents.len());

    let documents = if worker_count == 1 {
        manifest
            .documents
            .into_iter()
            .map(|document| eval_document(backend, manifest_dir, document, ocr, cache_dir, options))
            .collect::<Result<Vec<_>>>()?
    } else {
        eval_documents_parallel(
            backend,
            manifest_dir,
            manifest.documents,
            ocr,
            cache_dir,
            options,
            worker_count,
        )?
    };
    Ok(eval_output_from_documents(
        EvalOutputContext {
            run_metadata: benchmark_run_metadata(backend),
            run_configuration: run_configuration(ocr, options),
            manifest_path,
            manifest_sha256,
            required_categories,
            min_category_counts,
            worker_count,
        },
        documents,
    ))
}

pub(crate) fn eval_documents_parallel<B: PdfBackend + Sync>(
    backend: &B,
    manifest_dir: &Path,
    documents: Vec<EvalManifestDocument>,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
    worker_count: usize,
) -> Result<Vec<EvalDocumentOutput>> {
    let mut indexed_documents = documents.into_iter().enumerate().collect::<Vec<_>>();
    let mut evaluated_documents = Vec::with_capacity(indexed_documents.len());

    while !indexed_documents.is_empty() {
        let take_count = worker_count.min(indexed_documents.len());
        let chunk = indexed_documents.drain(..take_count).collect::<Vec<_>>();
        let mut chunk_results = Vec::with_capacity(chunk.len());
        thread::scope(|scope| -> Result<()> {
            let handles = chunk
                .into_iter()
                .map(|(index, document)| {
                    scope.spawn(move || {
                        eval_document(backend, manifest_dir, document, ocr, cache_dir, options)
                            .map(|output| (index, output))
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                chunk_results.push(
                    handle
                        .join()
                        .map_err(|_| anyhow!("eval worker panicked"))??,
                );
            }

            Ok(())
        })?;
        evaluated_documents.extend(chunk_results);
    }

    evaluated_documents.sort_by_key(|(index, _)| *index);
    Ok(evaluated_documents
        .into_iter()
        .map(|(_, document)| document)
        .collect())
}

#[derive(Clone, Copy)]
pub(crate) enum EvalArtifactSelection {
    ExactManifest,
    MatchingArtifacts,
}

pub(crate) fn eval_manifest_from_artifacts(
    run_metadata: BenchmarkRunMetadata,
    run_configuration: RunConfiguration,
    manifest_path: &Path,
    category: Option<&str>,
    coverage_preset: Option<CoveragePreset>,
    artifacts_by_path: &BTreeMap<PathBuf, &DocumentArtifact>,
    selection: EvalArtifactSelection,
) -> Result<EvalOutput> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read eval manifest {}", manifest_path.display()))?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);
    let manifest: EvalManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode eval manifest {}", manifest_path.display()))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let category_filter = normalize_manifest_category_filter(category);
    let mut required_categories = coverage_preset
        .into_iter()
        .flat_map(|preset| preset.categories().iter().copied())
        .map(str::to_string)
        .collect::<Vec<_>>();
    required_categories.extend(normalize_required_categories(
        &manifest.required_categories,
        category,
    ));
    let required_categories = normalize_required_categories(&required_categories, None);
    let min_category_counts =
        normalize_min_category_counts(&manifest.min_category_counts, category);

    let mut selected_document_count = 0usize;
    let mut documents = Vec::new();
    for document in manifest.documents {
        if !manifest_category_filter_matches(
            &category_filter,
            eval_manifest_document_category(&document),
        ) {
            continue;
        }
        selected_document_count += 1;
        let pdf_path = resolve_manifest_path(manifest_dir, &document.path);
        let key = manifest_path_key(&pdf_path);
        let Some(artifact) = artifacts_by_path.get(&key) else {
            match selection {
                EvalArtifactSelection::ExactManifest => {
                    bail!(
                        "eval manifest document {} was not part of this benchmark",
                        pdf_path.display()
                    );
                }
                EvalArtifactSelection::MatchingArtifacts => continue,
            }
        };
        documents.push(eval_document_from_artifact(document, artifact)?);
    }

    if documents.is_empty() {
        if selected_document_count == 0 {
            return Ok(eval_output_from_documents(
                EvalOutputContext {
                    run_metadata,
                    run_configuration,
                    manifest_path,
                    manifest_sha256,
                    required_categories,
                    min_category_counts,
                    worker_count: 1,
                },
                documents,
            ));
        }
        bail!(
            "eval manifest {} did not match any benchmarked PDF",
            manifest_path.display()
        );
    }

    Ok(eval_output_from_documents(
        EvalOutputContext {
            run_metadata,
            run_configuration,
            manifest_path,
            manifest_sha256,
            required_categories,
            min_category_counts,
            worker_count: 1,
        },
        documents,
    ))
}

pub(crate) fn eval_output_from_documents(
    context: EvalOutputContext<'_>,
    documents: Vec<EvalDocumentOutput>,
) -> EvalOutput {
    let document_failed_checks = documents
        .iter()
        .flat_map(|document| document.checks.values())
        .filter(|check| !check.passed)
        .count() as u32;
    let cache_hits = documents
        .iter()
        .filter(|document| document.artifact_cache_status == CacheStatus::Hit)
        .count() as u32;
    let cache_misses = documents
        .iter()
        .filter(|document| document.artifact_cache_status == CacheStatus::Miss)
        .count() as u32;
    let page_count = documents
        .iter()
        .map(|document| document.page_count)
        .sum::<usize>();
    let category_counts = documents
        .iter()
        .fold(BTreeMap::new(), |mut counts, document| {
            let category = eval_document_category(document);
            *counts.entry(category.to_string()).or_default() += 1;
            counts
        });
    let category_coverage = category_coverage(
        context.required_categories,
        context.min_category_counts,
        &category_counts,
    );
    let category_coverage_failed = category_coverage
        .as_ref()
        .is_some_and(|coverage| !coverage.passed);
    let document_count_failed = documents.is_empty();
    let failed_checks = document_failed_checks
        + u32::from(category_coverage_failed)
        + u32::from(document_count_failed);
    let mut failure_samples = Vec::new();
    if document_count_failed {
        failure_samples.push(EvalFailureSample {
            path: context.manifest_path.to_string_lossy().into_owned(),
            check: "document_count".to_string(),
            expected: json!({ "min": 1 }),
            actual: json!(0),
        });
    }
    if let Some(coverage) = category_coverage
        .as_ref()
        .filter(|coverage| !coverage.passed)
    {
        let check = if coverage.missing.is_empty() {
            "min_category_counts"
        } else {
            "required_categories"
        };
        failure_samples.push(EvalFailureSample {
            path: context.manifest_path.to_string_lossy().into_owned(),
            check: check.to_string(),
            expected: json!({
                "required": coverage.required,
                "min_category_counts": coverage.min_category_counts,
            }),
            actual: json!({
                "present": coverage.present,
                "missing": coverage.missing,
                "under_minimum": coverage.under_minimum,
            }),
        });
    }
    failure_samples.extend(
        documents
            .iter()
            .flat_map(|document| {
                document
                    .checks
                    .iter()
                    .filter(|(_, check)| !check.passed)
                    .map(|(name, check)| EvalFailureSample {
                        path: document.path.clone(),
                        check: name.clone(),
                        expected: check.expected.clone(),
                        actual: check.actual.clone(),
                    })
            })
            .take(10usize.saturating_sub(failure_samples.len())),
    );
    let passed = failed_checks == 0;
    let category_summaries = documents
        .iter()
        .fold(BTreeMap::new(), |mut summaries, document| {
            summaries
                .entry(eval_document_category(document).to_string())
                .or_insert_with(EvalCategorySummary::default)
                .add_document(document);
            summaries
        });
    let corpus_fingerprint = corpus_fingerprint(documents.iter().map(|document| {
        (
            document.path.as_str(),
            document.document_fingerprint.as_str(),
            document.page_count,
        )
    }));
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
    let image_artifact_count = documents
        .iter()
        .map(|document| document.image_artifact_count)
        .sum();
    let image_artifact_pages = documents
        .iter()
        .map(|document| document.image_artifact_pages)
        .sum();
    let empty_text_output_pages = documents
        .iter()
        .map(|document| document.empty_text_output_pages)
        .sum();
    let warnings_count = documents
        .iter()
        .map(|document| document.warnings_count)
        .sum();
    let route_counts = documents
        .iter()
        .fold(RouteCounts::default(), |mut counts, document| {
            counts.add(document.route_counts);
            counts
        });
    let route_reason_counts = documents
        .iter()
        .fold(BTreeMap::new(), |mut counts, document| {
            for (reason, count) in &document.route_reason_counts {
                *counts.entry(reason.clone()).or_default() += count;
            }
            counts
        });
    let quality_flag_counts =
        documents
            .iter()
            .fold(QualityFlagCounts::default(), |mut counts, document| {
                counts.add(document.quality_flag_counts);
                counts
            });
    let fallback_action_counts =
        documents
            .iter()
            .fold(FallbackActionCounts::default(), |mut counts, document| {
                counts.add(document.fallback_action_counts);
                counts
            });

    EvalOutput {
        report_version: EVAL_REPORT_VERSION,
        backend: context.run_metadata.backend,
        run_metadata: context.run_metadata,
        run_configuration: context.run_configuration,
        manifest_path: context.manifest_path.to_string_lossy().into_owned(),
        manifest_sha256: context.manifest_sha256,
        corpus_fingerprint,
        document_count: documents.len(),
        category_counts,
        category_summaries,
        category_coverage,
        page_count,
        worker_count: context.worker_count,
        cache_hits,
        cache_misses,
        fallback_pages,
        ocr_pages: ocr_required_pages,
        ocr_required_pages,
        ocr_applied_pages,
        image_artifact_count,
        image_artifact_pages,
        empty_text_output_pages,
        route_counts,
        route_reason_counts,
        quality_flag_counts,
        fallback_action_counts,
        warnings_count,
        passed,
        quality_passed: passed,
        quality_failed: !passed,
        failed_checks,
        failure_samples,
        documents,
    }
}

pub(crate) fn eval_document<B: PdfBackend>(
    backend: &B,
    manifest_dir: &Path,
    document: EvalManifestDocument,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
) -> Result<EvalDocumentOutput> {
    let pdf_path = resolve_manifest_path(manifest_dir, &document.path);
    let artifact = parse_pdf(backend, &pdf_path, ocr, cache_dir, options)?;
    eval_document_from_artifact(document, &artifact)
}

pub(crate) fn eval_document_from_artifact(
    document: EvalManifestDocument,
    artifact: &DocumentArtifact,
) -> Result<EvalDocumentOutput> {
    let mut checks = BTreeMap::new();
    let expect = eval_expectations_for_backend(&document, &artifact.metadata.backend)?;

    if let Some(expected) = &document.document_fingerprint {
        insert_check(
            &mut checks,
            "document_fingerprint",
            expected.clone(),
            artifact.document_fingerprint.clone(),
        );
    }
    if let Some(expected) = document.source_size_bytes {
        insert_check(
            &mut checks,
            "source_size_bytes",
            expected,
            artifact.metadata.source_size_bytes,
        );
    }
    if let Some(expected) = document.source_modified_unix_ms {
        insert_check(
            &mut checks,
            "source_modified_unix_ms",
            expected,
            artifact.metadata.source_modified_unix_ms,
        );
    }
    if let Some(expected) = expect.page_count {
        insert_check(&mut checks, "page_count", expected, artifact.pages.len());
    }
    if let Some(expected) = expect.fallback_pages {
        insert_check(
            &mut checks,
            "fallback_pages",
            expected,
            artifact.global_diagnostics.fallback_pages,
        );
    }
    if let Some(expected) = expect.ocr_required_pages {
        insert_check(
            &mut checks,
            "ocr_required_pages",
            expected,
            artifact.global_diagnostics.ocr_required_pages,
        );
    }
    if let Some(expected) = expect.ocr_applied_pages {
        insert_check(
            &mut checks,
            "ocr_applied_pages",
            expected,
            artifact.global_diagnostics.ocr_applied_pages,
        );
    }
    if let Some(expected) = expect.image_artifact_count {
        insert_check(
            &mut checks,
            "image_artifact_count",
            expected,
            image_artifact_count_from_artifact(artifact),
        );
    }
    if let Some(expected) = expect.warnings_count {
        insert_check(
            &mut checks,
            "warnings_count",
            expected,
            artifact.global_diagnostics.warnings.len(),
        );
    }
    if let Some(expected) = expect.route_counts {
        insert_check(
            &mut checks,
            "route_counts",
            expected,
            route_counts_from_artifact(artifact),
        );
    }
    if let Some(expected) = expect.quality_flag_counts {
        insert_check(
            &mut checks,
            "quality_flag_counts",
            expected,
            quality_flag_counts_from_artifact(artifact),
        );
    }
    if !expect.route_reason_counts.is_empty() {
        insert_check(
            &mut checks,
            "route_reason_counts",
            expect.route_reason_counts.clone(),
            route_reason_counts_from_artifact(artifact),
        );
    }
    if !expect.required_text.is_empty() {
        insert_required_text_check(&mut checks, &expect.required_text, artifact);
    }
    if !expect.required_warnings.is_empty() {
        insert_required_warnings_check(&mut checks, &expect.required_warnings, artifact);
    }
    if let Some(expectation) = &expect.text_recall {
        insert_text_recall_check(&mut checks, expectation, artifact);
    }
    if let Some(expectation) = &expect.reading_order {
        insert_reading_order_check(&mut checks, expectation, artifact);
    }
    if let Some(expectation) = &expect.ocr_required_classification {
        insert_ocr_required_classification_check(&mut checks, expectation, artifact);
    }
    if let Some(expectation) = &expect.silent_failures {
        insert_silent_failures_check(&mut checks, expectation, &expect, artifact);
    }
    for expectation in &expect.quality_flag_classification {
        insert_quality_flag_classification_check(&mut checks, expectation, artifact);
    }
    let mut table_expectation_counts = BTreeMap::new();
    for expectation in &expect.table_structure {
        *table_expectation_counts
            .entry(expectation.page)
            .or_insert(0usize) += 1;
    }
    let mut table_expectation_seen = BTreeMap::new();
    for expectation in &expect.table_structure {
        let seen = table_expectation_seen
            .entry(expectation.page)
            .or_insert(0usize);
        let expectation_index = (table_expectation_counts[&expectation.page] > 1).then_some(*seen);
        *seen += 1;
        insert_table_structure_check(&mut checks, expectation, expectation_index, artifact);
    }
    for (index, expectation) in expect.span_bbox.iter().enumerate() {
        insert_span_bbox_check(&mut checks, index, expectation, artifact);
    }
    for page_expectation in &expect.pages {
        insert_page_expectation_checks(&mut checks, page_expectation, artifact);
    }

    let passed = checks.values().all(|check| check.passed);
    let category = normalize_manifest_category(document.category.as_deref());

    Ok(EvalDocumentOutput {
        path: document.path,
        category,
        document_fingerprint: artifact.document_fingerprint.clone(),
        metadata: artifact.metadata.clone(),
        page_count: artifact.pages.len(),
        artifact_cache_status: artifact.global_diagnostics.cache_status.clone(),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        image_artifact_count: image_artifact_count_from_artifact(artifact),
        image_artifact_pages: image_artifact_pages_from_artifact(artifact),
        empty_text_output_pages: empty_text_output_page_count_from_artifact(artifact),
        route_counts: route_counts_from_artifact(artifact),
        route_reason_counts: route_reason_counts_from_artifact(artifact),
        quality_flag_counts: quality_flag_counts_from_artifact(artifact),
        fallback_action_counts: fallback_action_counts_from_artifact(artifact),
        warnings_count: artifact.global_diagnostics.warnings.len(),
        passed,
        checks,
    })
}

pub(crate) fn eval_document_category(document: &EvalDocumentOutput) -> &str {
    document.category.as_deref().unwrap_or("uncategorized")
}

pub(crate) fn eval_manifest_document_category(document: &EvalManifestDocument) -> &str {
    document
        .category
        .as_deref()
        .map(str::trim)
        .filter(|category| !category.is_empty())
        .unwrap_or("uncategorized")
}

pub(crate) fn manifest_category_filter_argument(
    category: Option<&str>,
    preset: Option<CoveragePreset>,
) -> Option<String> {
    preset
        .map(|preset| preset.categories().join(","))
        .or_else(|| category.map(str::to_string))
}

pub(crate) fn manifest_category_filter_matches(filter: &BTreeSet<String>, category: &str) -> bool {
    filter.is_empty() || filter.contains(category)
}

pub(crate) struct TableStructureScore {
    pub(crate) actual_rows: Vec<Vec<String>>,
    pub(crate) missing_rows: Vec<Vec<String>>,
    pub(crate) extra_rows: Vec<Vec<String>>,
    pub(crate) missing_cells: Vec<TableCell>,
    pub(crate) extra_cells: Vec<TableCell>,
    pub(crate) row_precision: f64,
    pub(crate) row_recall: f64,
    pub(crate) row_f1: f64,
    pub(crate) cell_precision: f64,
    pub(crate) cell_recall: f64,
    pub(crate) cell_f1: f64,
}

impl TableStructureScore {
    pub(crate) fn matched_row_count(&self, expected_rows: &[Vec<String>]) -> usize {
        expected_rows.len().saturating_sub(self.missing_rows.len())
    }

    pub(crate) fn matched_cell_count(&self, expected_rows: &[Vec<String>]) -> usize {
        table_cells(expected_rows)
            .len()
            .saturating_sub(self.missing_cells.len())
    }
}
