use crate::*;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow, bail};
use glyphrush_core::{
    CacheStatus, DocumentArtifact, DocumentMetadata, ImageArtifact, LayoutBlockKind, PageArtifact,
    PageDimensions, PageQuality, PageQualityReport, PageRoute, PageSignals, PageTimings,
    sha256_hex,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const BENCH_REPORT_VERSION: &str = "glyphrush-bench-report-v1";

pub(crate) const DEFAULT_BASELINE_TIMEOUT_MS: u64 = 120_000;

#[global_allocator]
pub(crate) static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

pub(crate) static ALLOCATED_BYTES_TOTAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    pub(crate) static ALLOCATED_BYTES_THREAD: Cell<u64> = const { Cell::new(0) };
}

pub(crate) struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATED_BYTES_TOTAL.fetch_add(layout.size() as u64, Ordering::Relaxed);
            ALLOCATED_BYTES_THREAD.with(|bytes| {
                bytes.set(bytes.get().saturating_add(layout.size() as u64));
            });
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            ALLOCATED_BYTES_TOTAL.fetch_add(new_size as u64, Ordering::Relaxed);
            ALLOCATED_BYTES_THREAD.with(|bytes| {
                bytes.set(bytes.get().saturating_add(new_size as u64));
            });
        }
        new_ptr
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BenchQualityStatus {
    Checked,
    NotCheckedNoEvalManifest,
}

#[derive(Debug, Serialize)]
pub(crate) struct BenchOutput {
    pub(crate) report_version: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) run_metadata: BenchmarkRunMetadata,
    pub(crate) run_configuration: RunConfiguration,
    pub(crate) requirements: BenchmarkRequirements,
    pub(crate) speedup_claims: Vec<BenchmarkSpeedupClaim>,
    pub(crate) requested_baseline_presets: Vec<&'static str>,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) page_count: usize,
    pub(crate) worker_count: usize,
    pub(crate) wall_us: u128,
    pub(crate) pages_per_sec: f64,
    pub(crate) artifact_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) allocated_bytes_per_page: f64,
    pub(crate) text_output_bytes: u64,
    pub(crate) text_output_line_count: usize,
    pub(crate) text_output_word_count: usize,
    pub(crate) empty_text_output: bool,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) stage_timings_us: BenchStageTimings,
    pub(crate) page_latency_us: PageLatencySummary,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_latency_us: RouteLatencySummary,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) image_artifact_pages: u32,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) fallback_action_counts: FallbackActionCounts,
    pub(crate) warnings_count: usize,
    pub(crate) warnings: Vec<String>,
    pub(crate) cache_status: CacheStatus,
    pub(crate) cache_key: Option<String>,
    pub(crate) baselines: Vec<BaselineBenchOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) silent_failure_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) silent_failure_pages: Option<Vec<BenchmarkSilentFailurePage>>,
    pub(crate) quality_status: BenchQualityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality: Option<EvalOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cache_probe: Option<CacheProbeOutput>,
    #[serde(skip)]
    pub(crate) page_latencies_us: Vec<u64>,
    #[serde(skip)]
    pub(crate) artifact: DocumentArtifact,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusBenchOutput {
    pub(crate) report_version: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) run_metadata: BenchmarkRunMetadata,
    pub(crate) run_configuration: RunConfiguration,
    pub(crate) requirements: BenchmarkRequirements,
    pub(crate) speedup_claims: Vec<BenchmarkSpeedupClaim>,
    pub(crate) requested_baseline_presets: Vec<&'static str>,
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) worker_count: usize,
    pub(crate) corpus_fingerprint: String,
    pub(crate) wall_us: u128,
    pub(crate) pages_per_sec: f64,
    pub(crate) artifact_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) allocated_bytes_per_page: f64,
    pub(crate) text_output_bytes: u64,
    pub(crate) text_output_line_count: usize,
    pub(crate) text_output_word_count: usize,
    pub(crate) empty_text_output_documents: usize,
    pub(crate) empty_text_output_pages: usize,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) stage_timings_us: BenchStageTimings,
    pub(crate) page_latency_us: PageLatencySummary,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_latency_us: RouteLatencySummary,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) image_artifact_pages: u32,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) fallback_action_counts: FallbackActionCounts,
    pub(crate) warnings_count: usize,
    pub(crate) warning_samples: Vec<CorpusWarningSample>,
    pub(crate) cache_hits: u32,
    pub(crate) cache_misses: u32,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) category_summaries: BTreeMap<String, CorpusBenchmarkCategorySummary>,
    pub(crate) baselines: Vec<CorpusBaselineBenchOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) silent_failure_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) silent_failure_pages: Option<Vec<BenchmarkSilentFailurePage>>,
    pub(crate) quality_status: BenchQualityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality: Option<EvalOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cache_probe: Option<CorpusCacheProbeOutput>,
    pub(crate) documents: Vec<CorpusBenchDocument>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusBenchDocument {
    #[serde(skip)]
    pub(crate) source_path: PathBuf,
    pub(crate) path: String,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) page_count: usize,
    pub(crate) wall_us: u128,
    pub(crate) pages_per_sec: f64,
    pub(crate) artifact_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) allocated_bytes_per_page: f64,
    pub(crate) text_output_bytes: u64,
    pub(crate) text_output_line_count: usize,
    pub(crate) text_output_word_count: usize,
    pub(crate) empty_text_output: bool,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) stage_timings_us: BenchStageTimings,
    pub(crate) page_latency_us: PageLatencySummary,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_latency_us: RouteLatencySummary,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) image_artifact_pages: u32,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) fallback_action_counts: FallbackActionCounts,
    pub(crate) warnings_count: usize,
    pub(crate) warnings: Vec<String>,
    pub(crate) cache_status: CacheStatus,
    pub(crate) cache_key: Option<String>,
    pub(crate) baselines: Vec<BaselineBenchOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cache_probe: Option<CacheProbeOutput>,
    #[serde(skip)]
    pub(crate) page_latencies_us: Vec<u64>,
    #[serde(skip)]
    pub(crate) artifact: DocumentArtifact,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct CorpusBenchmarkCategorySummary {
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) wall_us: u128,
    pub(crate) pages_per_sec: f64,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) route_counts: RouteCounts,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) warnings_count: usize,
    pub(crate) failed_checks: u32,
    pub(crate) quality_passed: bool,
    pub(crate) quality_failed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BenchmarkSilentFailurePage {
    pub(crate) path: String,
    pub(crate) page: u64,
    pub(crate) flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) empty_text_output: Option<bool>,
}

pub(crate) struct BenchmarkSilentFailureSummary {
    pub(crate) count: usize,
    pub(crate) pages: Vec<BenchmarkSilentFailurePage>,
}

impl CorpusBenchmarkCategorySummary {
    pub(crate) fn add_document(&mut self, document: &CorpusBenchDocument, failed_checks: u32) {
        self.document_count += 1;
        self.page_count += document.page_count;
        self.wall_us += document.wall_us;
        self.fallback_pages += document.fallback_pages;
        self.ocr_required_pages += document.ocr_required_pages;
        self.ocr_applied_pages += document.ocr_applied_pages;
        self.route_counts.add(document.route_counts);
        self.quality_flag_counts.add(document.quality_flag_counts);
        self.warnings_count += document.warnings_count;
        self.failed_checks += failed_checks;
        self.pages_per_sec = pages_per_sec(self.page_count, self.wall_us);
        self.quality_passed = self.failed_checks == 0;
        self.quality_failed = !self.quality_passed;
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct BenchmarkRunMetadata {
    pub(crate) parser_name: &'static str,
    pub(crate) parser_version: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) backend_version: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct RunConfiguration {
    pub(crate) span_geometry: bool,
    pub(crate) ocr_sidecar: bool,
    pub(crate) ocr_command: bool,
    pub(crate) ocr_http_url: bool,
    pub(crate) ocr_command_input: OcrCommandInput,
    pub(crate) ocr_timeout_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct BenchmarkRequirements {
    pub(crate) require_quality: bool,
    pub(crate) require_baselines: bool,
    pub(crate) require_baseline_quality: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) require_coverage_preset: Option<&'static str>,
    pub(crate) require_speedups: Vec<BenchmarkSpeedupRequirement>,
    pub(crate) require_speedup_claims: Vec<BenchmarkSpeedupRequirement>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BenchmarkSpeedupClaim {
    pub(crate) baseline: String,
    pub(crate) required_glyphrush_speedup: f64,
    pub(crate) actual_glyphrush_speedup: f64,
    pub(crate) speed_comparable: bool,
    pub(crate) speed_passed: bool,
    pub(crate) glyphrush_quality_checked: bool,
    pub(crate) glyphrush_quality_passed: bool,
    pub(crate) baseline_quality_checked: bool,
    pub(crate) baseline_quality_passed: bool,
    pub(crate) glyphrush_quality_backed: bool,
    pub(crate) quality_backed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality_blocker: Option<BenchmarkSpeedupClaimQualityBlocker>,
    pub(crate) claim_passed: bool,
    pub(crate) status: BenchmarkSpeedupClaimStatus,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BenchmarkSpeedupClaimStatus {
    Passed,
    BaselineNotRun,
    NotSpeedComparable,
    SpeedupFailed,
    QualityNotChecked,
    QualityFailed,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BenchmarkSpeedupClaimQualityBlocker {
    GlyphrushQualityNotChecked,
    GlyphrushQualityFailed,
    BaselineQualityNotChecked,
    BaselineQualityFailed,
}

pub(crate) struct BenchmarkSpeedupClaimInput<'a> {
    pub(crate) requirement: &'a BenchmarkSpeedupRequirement,
    pub(crate) baseline_was_run: bool,
    pub(crate) actual_glyphrush_speedup: f64,
    pub(crate) speed_comparable: bool,
    pub(crate) speed_passed: bool,
    pub(crate) glyphrush_quality_checked: bool,
    pub(crate) glyphrush_quality_passed: bool,
    pub(crate) baseline_quality_checked: bool,
    pub(crate) baseline_quality_passed: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusWarningSample {
    pub(crate) path: String,
    pub(crate) warning: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct QualityFlagCounts {
    pub(crate) requires_ocr: u32,
    pub(crate) low_confidence_text: u32,
    pub(crate) broken_encoding: u32,
    pub(crate) layout_uncertain: u32,
    pub(crate) table_uncertain: u32,
    pub(crate) unsupported_feature: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct FallbackActionCounts {
    pub(crate) ocr_requested_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) heavy_layout_pages: u32,
    pub(crate) table_recovery_pages: u32,
    pub(crate) render_pages: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RouteCounts {
    pub(crate) native_fast_path: u32,
    pub(crate) needs_fallback: u32,
    pub(crate) ocr_fallback: u32,
    pub(crate) unsupported: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct RouteLatencySummary {
    pub(crate) native_fast_path: PageLatencySummary,
    pub(crate) needs_fallback: PageLatencySummary,
    pub(crate) ocr_fallback: PageLatencySummary,
    pub(crate) unsupported: PageLatencySummary,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct TextOutputMetrics {
    pub(crate) bytes: u64,
    pub(crate) line_count: usize,
    pub(crate) word_count: usize,
    pub(crate) empty: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct BenchStageTimings {
    pub(crate) open_us: u64,
    pub(crate) classify_us: u64,
    pub(crate) native_extract_us: u64,
    pub(crate) layout_us: u64,
    pub(crate) table_us: u64,
    pub(crate) render_us: u64,
    pub(crate) ocr_us: u64,
    pub(crate) merge_us: u64,
    pub(crate) total_us: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct PageLatencySummary {
    pub(crate) p50_us: u64,
    pub(crate) p95_us: u64,
    pub(crate) max_us: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct DebugPageOutput {
    pub(crate) backend: &'static str,
    pub(crate) metadata: DocumentMetadata,
    pub(crate) document_fingerprint: String,
    pub(crate) artifact_id: String,
    pub(crate) page_fingerprint: String,
    pub(crate) document_page_count: usize,
    pub(crate) extracted_page_count: usize,
    pub(crate) page_index: u32,
    pub(crate) dimensions: PageDimensions,
    pub(crate) signals: PageSignals,
    pub(crate) quality: PageQualityReport,
    pub(crate) text_output: TextOutputMetrics,
    pub(crate) layout: DebugLayoutSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) layout_strategy: Option<String>,
    pub(crate) timings: PageTimings,
    pub(crate) image_artifacts: Vec<ImageArtifact>,
    pub(crate) warnings: Vec<String>,
    pub(crate) decision: glyphrush_core::RouteDecision,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DebugLayoutSummary {
    pub(crate) block_count: usize,
    pub(crate) paragraph_blocks: usize,
    pub(crate) heading_blocks: usize,
    pub(crate) list_blocks: usize,
    pub(crate) table_blocks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) table_rows: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) table_cells: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) table_cells_with_bbox: Option<usize>,
    pub(crate) figure_blocks: usize,
    pub(crate) header_blocks: usize,
    pub(crate) footer_blocks: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct BenchRunConfig<'a> {
    pub(crate) ocr: OcrOptions<'a>,
    pub(crate) cache_dir: Option<&'a Path>,
    pub(crate) cache_probe: bool,
    pub(crate) jobs: usize,
    pub(crate) extraction: ExtractionOptions,
    pub(crate) baselines: &'a [BaselineSpec],
    pub(crate) requested_baseline_presets: &'a [&'static str],
    pub(crate) baseline_timeout: Duration,
    pub(crate) require_quality: bool,
    pub(crate) require_baselines: bool,
    pub(crate) require_baseline_quality: bool,
    pub(crate) require_coverage_preset: Option<CoveragePreset>,
    pub(crate) required_speedups: &'a [BenchmarkSpeedupRequirement],
    pub(crate) required_speedup_claims: &'a [BenchmarkSpeedupRequirement],
}

pub(crate) fn warnings_for_page(warnings: &[String], page_index: u32) -> Vec<String> {
    let prefix = format!("p{page_index:06}:");
    warnings
        .iter()
        .filter(|warning| warning.starts_with(&prefix))
        .cloned()
        .collect()
}

pub(crate) fn bench_corpus<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BaselineQualityInputs>,
    selected_path_keys: Option<&BTreeSet<PathBuf>>,
) -> Result<CorpusBenchOutput> {
    if config.cache_probe && config.cache_dir.is_none() {
        bail!("--cache-probe requires --cache-dir");
    }

    let mut pdfs = discover_pdfs(path)?;
    if let Some(selected_path_keys) = selected_path_keys
        && !selected_path_keys.is_empty()
    {
        pdfs.retain(|pdf| selected_path_keys.contains(&manifest_path_key(&pdf.path)));
    }
    let worker_count = document_worker_count(backend, config.jobs, pdfs.len());
    let documents = if worker_count == 1 {
        pdfs.into_iter()
            .map(|pdf| bench_corpus_document(backend, pdf, config, baseline_quality))
            .collect::<Result<Vec<_>>>()?
    } else {
        bench_corpus_parallel(backend, pdfs, config, baseline_quality, worker_count)?
    };
    let wall_us = corpus_parser_wall_us_from_documents(&documents, worker_count);
    let page_count = documents.iter().map(|document| document.page_count).sum();
    let artifact_bytes = documents
        .iter()
        .map(|document| document.artifact_bytes)
        .sum();
    let allocated_bytes = documents
        .iter()
        .map(|document| document.allocated_bytes)
        .sum();
    let text_output_bytes = documents
        .iter()
        .map(|document| document.text_output_bytes)
        .sum();
    let text_output_line_count = documents
        .iter()
        .map(|document| document.text_output_line_count)
        .sum();
    let text_output_word_count = documents
        .iter()
        .map(|document| document.text_output_word_count)
        .sum();
    let empty_text_output_documents = documents
        .iter()
        .filter(|document| document.empty_text_output)
        .count();
    let empty_text_output_pages = documents
        .iter()
        .map(|document| empty_text_output_page_count_from_artifact(&document.artifact))
        .sum();
    let peak_rss_bytes = documents
        .iter()
        .map(|document| document.peak_rss_bytes)
        .max()
        .unwrap_or_default();
    let fallback_pages = documents
        .iter()
        .map(|document| document.fallback_pages)
        .sum();
    let ocr_pages = documents.iter().map(|document| document.ocr_pages).sum();
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
    let route_counts = documents
        .iter()
        .fold(RouteCounts::default(), |mut counts, document| {
            counts.add(document.route_counts);
            counts
        });
    let route_reason_counts = documents
        .iter()
        .fold(BTreeMap::new(), |mut counts, document| {
            add_reason_counts(&mut counts, &document.route_reason_counts);
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
    let warnings_count = documents
        .iter()
        .map(|document| document.warnings_count)
        .sum();
    let warning_samples = documents
        .iter()
        .flat_map(|document| {
            document.warnings.iter().map(|warning| CorpusWarningSample {
                path: document.path.clone(),
                warning: warning.clone(),
            })
        })
        .take(10)
        .collect();
    let stage_timings_us =
        documents
            .iter()
            .fold(BenchStageTimings::default(), |mut timings, document| {
                timings.add(document.stage_timings_us);
                timings
            });
    let page_latency_us = page_latency_from_documents(&documents);
    let route_latency_us = route_latency_from_documents(&documents);
    let cache_hits = documents
        .iter()
        .filter(|document| document.cache_status == CacheStatus::Hit)
        .count() as u32;
    let cache_misses = documents
        .iter()
        .filter(|document| document.cache_status == CacheStatus::Miss)
        .count() as u32;
    let baseline_outputs =
        aggregate_corpus_baselines(&documents, config.baselines, page_count, baseline_quality);
    let cache_probe_output = aggregate_corpus_cache_probe(&documents, page_count);
    let corpus_fingerprint = corpus_fingerprint(documents.iter().map(|document| {
        (
            document.path.as_str(),
            document.document_fingerprint.as_str(),
            document.page_count,
        )
    }));

    Ok(CorpusBenchOutput {
        report_version: BENCH_REPORT_VERSION,
        backend: backend.name(),
        run_metadata: benchmark_run_metadata(backend),
        run_configuration: run_configuration(config.ocr, config.extraction),
        requirements: benchmark_requirements(config),
        speedup_claims: Vec::new(),
        requested_baseline_presets: config.requested_baseline_presets.to_vec(),
        document_count: documents.len(),
        page_count,
        worker_count,
        corpus_fingerprint,
        wall_us,
        pages_per_sec: pages_per_sec(page_count, wall_us),
        artifact_bytes,
        allocated_bytes,
        allocated_bytes_per_page: bytes_per_page(allocated_bytes, page_count),
        text_output_bytes,
        text_output_line_count,
        text_output_word_count,
        empty_text_output_documents,
        empty_text_output_pages,
        peak_rss_bytes,
        stage_timings_us,
        page_latency_us,
        route_counts,
        route_latency_us,
        route_reason_counts,
        fallback_pages,
        ocr_pages,
        ocr_required_pages,
        ocr_applied_pages,
        image_artifact_count,
        image_artifact_pages,
        quality_flag_counts,
        fallback_action_counts,
        warnings_count,
        warning_samples,
        cache_hits,
        cache_misses,
        category_summaries: BTreeMap::new(),
        baselines: baseline_outputs,
        silent_failure_count: None,
        silent_failure_pages: None,
        quality_status: BenchQualityStatus::NotCheckedNoEvalManifest,
        quality: None,
        cache_probe: cache_probe_output,
        documents,
    })
}

pub(crate) fn corpus_parser_wall_us_from_documents(
    documents: &[CorpusBenchDocument],
    worker_count: usize,
) -> u128 {
    let worker_count = worker_count.max(1);
    documents
        .chunks(worker_count)
        .map(|chunk| {
            chunk
                .iter()
                .map(|document| document.wall_us)
                .max()
                .unwrap_or_default()
        })
        .sum()
}

pub(crate) fn bench_corpus_parallel<B: PdfBackend + Sync>(
    backend: &B,
    pdfs: Vec<DiscoveredPdf>,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BaselineQualityInputs>,
    worker_count: usize,
) -> Result<Vec<CorpusBenchDocument>> {
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
                        bench_corpus_document(backend, pdf, config, baseline_quality)
                            .map(|document| (*index, document))
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                chunk_results.push(
                    handle
                        .join()
                        .map_err(|_| anyhow!("corpus benchmark worker panicked"))??,
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

pub(crate) fn bench_corpus_document<B: PdfBackend>(
    backend: &B,
    pdf: DiscoveredPdf,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BaselineQualityInputs>,
) -> Result<CorpusBenchDocument> {
    let bench = bench_pdf(
        backend,
        &pdf.path,
        config,
        baseline_quality.and_then(|quality| quality.expectation_for_path(&pdf.path)),
    )?;
    Ok(CorpusBenchDocument {
        source_path: pdf.path,
        path: pdf.label,
        metadata: bench.metadata,
        document_fingerprint: bench.document_fingerprint,
        page_count: bench.page_count,
        wall_us: bench.wall_us,
        pages_per_sec: bench.pages_per_sec,
        artifact_bytes: bench.artifact_bytes,
        allocated_bytes: bench.allocated_bytes,
        allocated_bytes_per_page: bench.allocated_bytes_per_page,
        text_output_bytes: bench.text_output_bytes,
        text_output_line_count: bench.text_output_line_count,
        text_output_word_count: bench.text_output_word_count,
        empty_text_output: bench.empty_text_output,
        peak_rss_bytes: bench.peak_rss_bytes,
        stage_timings_us: bench.stage_timings_us,
        page_latency_us: bench.page_latency_us,
        route_counts: bench.route_counts,
        route_latency_us: bench.route_latency_us,
        route_reason_counts: bench.route_reason_counts,
        fallback_pages: bench.fallback_pages,
        ocr_pages: bench.ocr_pages,
        ocr_required_pages: bench.ocr_required_pages,
        ocr_applied_pages: bench.ocr_applied_pages,
        image_artifact_count: bench.image_artifact_count,
        image_artifact_pages: bench.image_artifact_pages,
        quality_flag_counts: bench.quality_flag_counts,
        fallback_action_counts: bench.fallback_action_counts,
        warnings_count: bench.warnings_count,
        warnings: bench.warnings,
        cache_status: bench.cache_status,
        cache_key: bench.cache_key,
        baselines: bench.baselines,
        cache_probe: bench.cache_probe,
        page_latencies_us: bench.page_latencies_us,
        artifact: bench.artifact,
    })
}

pub(crate) fn bench_pdf<B: PdfBackend>(
    backend: &B,
    path: &Path,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BaselineQualityExpectations>,
) -> Result<BenchOutput> {
    if config.cache_probe && config.cache_dir.is_none() {
        bail!("--cache-probe requires --cache-dir");
    }

    if config.cache_probe
        && let Some(cache_dir) = config.cache_dir
    {
        remove_cached_artifact_for_document(
            backend.name(),
            backend.version(),
            path,
            config.ocr,
            cache_dir,
            config.extraction,
        )?;
    }

    let uses_page_workers = config.extraction.page_jobs > 1;
    let allocated_start = allocated_bytes_total(uses_page_workers);
    let start = Instant::now();
    let artifact = parse_pdf(
        backend,
        path,
        config.ocr,
        config.cache_dir,
        config.extraction,
    )?;
    let wall_us = start.elapsed().as_micros();
    let allocated_bytes = allocated_bytes_total(uses_page_workers).saturating_sub(allocated_start);
    let page_count = artifact.pages.len();
    let stage_timings_us = stage_timings_from_artifact(&artifact);
    let page_latency_us = page_latency_from_artifact(&artifact);
    let page_latencies_us = page_latencies_from_artifact(&artifact);
    let artifact_bytes = serde_json::to_vec(&artifact)?.len() as u64;
    let text_output_metrics = text_output_metrics_from_artifact(&artifact);
    let peak_rss_bytes = peak_rss_bytes();
    let baseline_outputs = run_external_baselines(
        path,
        config.baselines,
        baseline_quality,
        wall_us,
        text_output_metrics.bytes,
        config.baseline_timeout,
    );
    let cache_probe_output = if config.cache_probe {
        let cold = cache_probe_run_from_artifact(
            &artifact,
            wall_us,
            artifact_bytes,
            allocated_bytes,
            peak_rss_bytes,
        );
        Some(run_cache_probe(
            backend,
            path,
            config.ocr,
            config.cache_dir.expect("validated cache probe cache dir"),
            config.extraction,
            cold,
        )?)
    } else {
        None
    };

    Ok(BenchOutput {
        report_version: BENCH_REPORT_VERSION,
        backend: backend.name(),
        run_metadata: benchmark_run_metadata(backend),
        run_configuration: run_configuration(config.ocr, config.extraction),
        requirements: benchmark_requirements(config),
        speedup_claims: Vec::new(),
        requested_baseline_presets: config.requested_baseline_presets.to_vec(),
        metadata: artifact.metadata.clone(),
        document_fingerprint: artifact.document_fingerprint.clone(),
        page_count,
        worker_count: artifact.global_diagnostics.worker_count,
        wall_us,
        pages_per_sec: pages_per_sec(page_count, wall_us),
        artifact_bytes,
        allocated_bytes,
        allocated_bytes_per_page: bytes_per_page(allocated_bytes, page_count),
        text_output_bytes: text_output_metrics.bytes,
        text_output_line_count: text_output_metrics.line_count,
        text_output_word_count: text_output_metrics.word_count,
        empty_text_output: text_output_metrics.empty,
        peak_rss_bytes,
        stage_timings_us,
        page_latency_us,
        route_counts: route_counts_from_artifact(&artifact),
        route_latency_us: route_latency_from_artifact(&artifact),
        route_reason_counts: route_reason_counts_from_artifact(&artifact),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_pages: artifact.global_diagnostics.ocr_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        image_artifact_count: image_artifact_count_from_artifact(&artifact),
        image_artifact_pages: image_artifact_pages_from_artifact(&artifact),
        quality_flag_counts: quality_flag_counts_from_artifact(&artifact),
        fallback_action_counts: fallback_action_counts_from_artifact(&artifact),
        warnings_count: artifact.global_diagnostics.warnings.len(),
        warnings: artifact.global_diagnostics.warnings.clone(),
        cache_status: artifact.global_diagnostics.cache_status.clone(),
        cache_key: artifact.global_diagnostics.cache_key.clone(),
        baselines: baseline_outputs,
        silent_failure_count: None,
        silent_failure_pages: None,
        quality_status: BenchQualityStatus::NotCheckedNoEvalManifest,
        quality: None,
        cache_probe: cache_probe_output,
        page_latencies_us,
        artifact,
    })
}

pub(crate) fn benchmark_run_metadata<B: PdfBackend>(backend: &B) -> BenchmarkRunMetadata {
    BenchmarkRunMetadata {
        parser_name: PARSER_NAME,
        parser_version: PARSER_VERSION,
        backend: backend.name(),
        backend_version: backend.version(),
    }
}

pub(crate) fn run_configuration(
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
) -> RunConfiguration {
    RunConfiguration {
        span_geometry: options.span_geometry,
        ocr_sidecar: ocr.sidecar.is_some(),
        ocr_command: ocr.command.is_some(),
        ocr_http_url: ocr.http_url.is_some(),
        ocr_command_input: ocr.command_input,
        ocr_timeout_ms: duration_millis(ocr.timeout),
    }
}

pub(crate) fn benchmark_requirements(config: BenchRunConfig<'_>) -> BenchmarkRequirements {
    BenchmarkRequirements {
        require_quality: config.require_quality,
        require_baselines: config.require_baselines,
        require_baseline_quality: config.require_baseline_quality,
        require_coverage_preset: config.require_coverage_preset.map(CoveragePreset::name),
        require_speedups: config.required_speedups.to_vec(),
        require_speedup_claims: config.required_speedup_claims.to_vec(),
    }
}

pub(crate) fn benchmark_coverage_requirement_error(
    quality: Option<&EvalOutput>,
    coverage_preset: Option<CoveragePreset>,
) -> Option<String> {
    let preset = coverage_preset?;
    let Some(quality) = quality else {
        return Some(format!(
            "bench coverage preset {} required: no eval manifest quality report was checked",
            preset.name()
        ));
    };
    let Some(coverage) = quality.category_coverage.as_ref() else {
        return Some(format!(
            "bench coverage preset {} required: no category coverage was checked",
            preset.name()
        ));
    };

    (!coverage.passed).then(|| {
        let missing = if coverage.missing.is_empty() {
            "none".to_string()
        } else {
            coverage.missing.join(",")
        };
        format!(
            "bench coverage preset {} failed: missing categories {missing}",
            preset.name()
        )
    })
}

pub(crate) fn cache_probe_run_from_bench(bench: &BenchOutput) -> CacheProbeRunOutput {
    CacheProbeRunOutput {
        cache_status: bench.cache_status.clone(),
        wall_us: bench.wall_us,
        pages_per_sec: bench.pages_per_sec,
        artifact_bytes: bench.artifact_bytes,
        allocated_bytes: bench.allocated_bytes,
        allocated_bytes_per_page: bench.allocated_bytes_per_page,
        text_output_bytes: bench.text_output_bytes,
        text_output_line_count: bench.text_output_line_count,
        text_output_word_count: bench.text_output_word_count,
        empty_text_output: bench.empty_text_output,
        peak_rss_bytes: bench.peak_rss_bytes,
        stage_timings_us: bench.stage_timings_us,
        page_latency_us: bench.page_latency_us,
        route_counts: bench.route_counts,
        route_latency_us: bench.route_latency_us,
        route_reason_counts: bench.route_reason_counts.clone(),
        fallback_pages: bench.fallback_pages,
        ocr_required_pages: bench.ocr_required_pages,
        ocr_applied_pages: bench.ocr_applied_pages,
        image_artifact_count: bench.image_artifact_count,
        image_artifact_pages: bench.image_artifact_pages,
        quality_flag_counts: bench.quality_flag_counts,
        fallback_action_counts: bench.fallback_action_counts,
        warnings_count: bench.warnings_count,
        warnings: bench.warnings.clone(),
        cache_key: bench.cache_key.clone(),
    }
}

pub(crate) fn speedup(cold_wall_us: u128, warm_wall_us: u128) -> f64 {
    if warm_wall_us == 0 {
        return 0.0;
    }

    cold_wall_us as f64 / warm_wall_us as f64
}

pub(crate) fn allocated_bytes_total(include_worker_threads: bool) -> u64 {
    if include_worker_threads {
        ALLOCATED_BYTES_TOTAL.load(Ordering::Relaxed)
    } else {
        ALLOCATED_BYTES_THREAD.with(Cell::get)
    }
}

pub(crate) fn bytes_per_page(bytes: u64, page_count: usize) -> f64 {
    if page_count == 0 {
        return 0.0;
    }

    bytes as f64 / page_count as f64
}

pub(crate) fn byte_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }

    numerator as f64 / denominator as f64
}

impl QualityFlagCounts {
    pub(crate) fn add(&mut self, other: QualityFlagCounts) {
        self.requires_ocr += other.requires_ocr;
        self.low_confidence_text += other.low_confidence_text;
        self.broken_encoding += other.broken_encoding;
        self.layout_uncertain += other.layout_uncertain;
        self.table_uncertain += other.table_uncertain;
        self.unsupported_feature += other.unsupported_feature;
    }
}

impl FallbackActionCounts {
    pub(crate) fn add(&mut self, other: FallbackActionCounts) {
        self.ocr_requested_pages += other.ocr_requested_pages;
        self.ocr_applied_pages += other.ocr_applied_pages;
        self.heavy_layout_pages += other.heavy_layout_pages;
        self.table_recovery_pages += other.table_recovery_pages;
        self.render_pages += other.render_pages;
    }
}

pub(crate) fn quality_flag_counts_from_artifact(artifact: &DocumentArtifact) -> QualityFlagCounts {
    let mut counts = QualityFlagCounts::default();

    for flag in artifact
        .pages
        .iter()
        .flat_map(|page| page.quality.flags.iter())
    {
        match flag {
            PageQuality::RequiresOcr => counts.requires_ocr += 1,
            PageQuality::LowConfidenceText => counts.low_confidence_text += 1,
            PageQuality::BrokenEncoding => counts.broken_encoding += 1,
            PageQuality::LayoutUncertain => counts.layout_uncertain += 1,
            PageQuality::TableUncertain => counts.table_uncertain += 1,
            PageQuality::UnsupportedFeature => counts.unsupported_feature += 1,
        }
    }

    counts
}

pub(crate) fn image_artifact_count_from_artifact(artifact: &DocumentArtifact) -> u32 {
    artifact
        .pages
        .iter()
        .map(|page| page.image_artifacts.len() as u32)
        .sum()
}

pub(crate) fn image_artifact_pages_from_artifact(artifact: &DocumentArtifact) -> u32 {
    artifact
        .pages
        .iter()
        .filter(|page| !page.image_artifacts.is_empty())
        .count() as u32
}

pub(crate) fn fallback_action_counts_from_artifact(
    artifact: &DocumentArtifact,
) -> FallbackActionCounts {
    let mut counts = FallbackActionCounts::default();

    for page in &artifact.pages {
        counts.ocr_requested_pages += u32::from(page.route.run_ocr);
        counts.ocr_applied_pages += u32::from(!page.ocr_spans.is_empty());
        counts.heavy_layout_pages += u32::from(page.route.run_heavy_layout);
        counts.table_recovery_pages += u32::from(page.route.run_table_recovery);
        counts.render_pages += u32::from(page.timings.render_us > 0);
    }

    counts
}

impl RouteCounts {
    pub(crate) fn add(&mut self, other: RouteCounts) {
        self.native_fast_path += other.native_fast_path;
        self.needs_fallback += other.needs_fallback;
        self.ocr_fallback += other.ocr_fallback;
        self.unsupported += other.unsupported;
    }
}

pub(crate) fn route_counts_from_artifact(artifact: &DocumentArtifact) -> RouteCounts {
    let mut counts = RouteCounts::default();

    for route in artifact.pages.iter().map(|page| &page.route.route) {
        match route {
            PageRoute::NativeFastPath => counts.native_fast_path += 1,
            PageRoute::NeedsFallback => counts.needs_fallback += 1,
            PageRoute::OcrFallback => counts.ocr_fallback += 1,
            PageRoute::Unsupported => counts.unsupported += 1,
        }
    }

    counts
}

pub(crate) fn route_reason_counts_from_artifact(
    artifact: &DocumentArtifact,
) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();

    for reason in artifact
        .pages
        .iter()
        .flat_map(|page| page.route.reasons.iter())
    {
        *counts.entry(reason.clone()).or_default() += 1;
    }

    counts
}

pub(crate) fn add_reason_counts(
    target: &mut BTreeMap<String, u32>,
    source: &BTreeMap<String, u32>,
) {
    for (reason, count) in source {
        *target.entry(reason.clone()).or_default() += count;
    }
}

impl BenchStageTimings {
    pub(crate) fn add(&mut self, other: BenchStageTimings) {
        self.open_us += other.open_us;
        self.classify_us += other.classify_us;
        self.native_extract_us += other.native_extract_us;
        self.layout_us += other.layout_us;
        self.table_us += other.table_us;
        self.render_us += other.render_us;
        self.ocr_us += other.ocr_us;
        self.merge_us += other.merge_us;
        self.total_us += other.total_us;
    }
}

pub(crate) fn stage_timings_from_artifact(artifact: &DocumentArtifact) -> BenchStageTimings {
    let mut timings = BenchStageTimings::default();

    for page in &artifact.pages {
        timings.open_us += page.timings.open_us;
        timings.classify_us += page.timings.classify_us;
        timings.native_extract_us += page.timings.native_extract_us;
        timings.layout_us += page.timings.layout_us;
        timings.table_us += page.timings.table_us;
        timings.render_us += page.timings.render_us;
        timings.ocr_us += page.timings.ocr_us;
        timings.merge_us += page.timings.merge_us;
        timings.total_us += page.timings.total_us();
    }

    timings
}

pub(crate) fn page_latency_from_artifact(artifact: &DocumentArtifact) -> PageLatencySummary {
    page_latency_from_values(page_latencies_from_artifact(artifact))
}

pub(crate) fn page_latency_from_documents(documents: &[CorpusBenchDocument]) -> PageLatencySummary {
    let values = documents
        .iter()
        .flat_map(|document| document.page_latencies_us.iter().copied())
        .collect::<Vec<_>>();

    page_latency_from_values(values)
}

pub(crate) fn route_latency_from_artifact(artifact: &DocumentArtifact) -> RouteLatencySummary {
    route_latency_from_pages(artifact.pages.iter())
}

pub(crate) fn route_latency_from_documents(
    documents: &[CorpusBenchDocument],
) -> RouteLatencySummary {
    route_latency_from_pages(
        documents
            .iter()
            .flat_map(|document| document.artifact.pages.iter()),
    )
}

pub(crate) fn route_latency_from_pages<'a>(
    pages: impl Iterator<Item = &'a PageArtifact>,
) -> RouteLatencySummary {
    let mut native_fast_path = Vec::new();
    let mut needs_fallback = Vec::new();
    let mut ocr_fallback = Vec::new();
    let mut unsupported = Vec::new();

    for page in pages {
        let latency = page.timings.total_us();
        match page.route.route {
            PageRoute::NativeFastPath => native_fast_path.push(latency),
            PageRoute::NeedsFallback => needs_fallback.push(latency),
            PageRoute::OcrFallback => ocr_fallback.push(latency),
            PageRoute::Unsupported => unsupported.push(latency),
        }
    }

    RouteLatencySummary {
        native_fast_path: page_latency_from_values(native_fast_path),
        needs_fallback: page_latency_from_values(needs_fallback),
        ocr_fallback: page_latency_from_values(ocr_fallback),
        unsupported: page_latency_from_values(unsupported),
    }
}

pub(crate) fn page_latencies_from_artifact(artifact: &DocumentArtifact) -> Vec<u64> {
    artifact
        .pages
        .iter()
        .map(|page| page.timings.total_us())
        .collect()
}

pub(crate) fn page_latency_from_values(mut values: Vec<u64>) -> PageLatencySummary {
    if values.is_empty() {
        return PageLatencySummary::default();
    }

    values.sort_unstable();

    PageLatencySummary {
        p50_us: percentile_us(&values, 0.50),
        p95_us: percentile_us(&values, 0.95),
        max_us: values.last().copied().unwrap_or_default(),
    }
}

pub(crate) fn combined_speedup_claim_requirements(
    speedups: &[BenchmarkSpeedupRequirement],
    speedup_claims: &[BenchmarkSpeedupRequirement],
) -> Vec<BenchmarkSpeedupRequirement> {
    let mut requirements = speedups.to_vec();
    requirements.extend_from_slice(speedup_claims);
    requirements
}

pub(crate) fn speedup_claims(
    baselines: &[BaselineBenchOutput],
    requirements: &[BenchmarkSpeedupRequirement],
    glyphrush_quality_status: &BenchQualityStatus,
    glyphrush_quality: Option<&EvalOutput>,
) -> Vec<BenchmarkSpeedupClaim> {
    let glyphrush_quality_checked = matches!(glyphrush_quality_status, BenchQualityStatus::Checked);
    let glyphrush_quality_passed = glyphrush_quality
        .is_some_and(|quality| quality.quality_passed && quality.failed_checks == 0);

    requirements
        .iter()
        .map(|requirement| {
            let baseline = baselines
                .iter()
                .find(|baseline| baseline.name == requirement.baseline);
            let comparison = baseline.map(|baseline| baseline.comparison);
            let actual_glyphrush_speedup =
                comparison.map_or(0.0, |comparison| comparison.glyphrush_speedup);
            let speed_comparable = comparison.is_some_and(|comparison| comparison.speed_comparable);
            let speed_passed =
                speed_comparable && actual_glyphrush_speedup >= requirement.min_glyphrush_speedup;
            let baseline_quality_checked = baseline.is_some_and(|baseline| {
                matches!(baseline.quality_status, BaselineQualityStatus::Checked)
            });
            let baseline_quality_passed = baseline
                .and_then(|baseline| baseline.quality.as_ref())
                .is_some_and(|quality| quality.passed);
            speedup_claim(BenchmarkSpeedupClaimInput {
                requirement,
                baseline_was_run: baseline.is_some(),
                actual_glyphrush_speedup,
                speed_comparable,
                speed_passed,
                glyphrush_quality_checked,
                glyphrush_quality_passed,
                baseline_quality_checked,
                baseline_quality_passed,
            })
        })
        .collect()
}

pub(crate) fn corpus_speedup_claims(
    baselines: &[CorpusBaselineBenchOutput],
    requirements: &[BenchmarkSpeedupRequirement],
    glyphrush_quality_status: &BenchQualityStatus,
    glyphrush_quality: Option<&EvalOutput>,
) -> Vec<BenchmarkSpeedupClaim> {
    let glyphrush_quality_checked = matches!(glyphrush_quality_status, BenchQualityStatus::Checked);
    let glyphrush_quality_passed = glyphrush_quality
        .is_some_and(|quality| quality.quality_passed && quality.failed_checks == 0);

    requirements
        .iter()
        .map(|requirement| {
            let baseline = baselines
                .iter()
                .find(|baseline| baseline.name == requirement.baseline);
            let comparison = baseline.map(|baseline| baseline.comparison);
            let actual_glyphrush_speedup =
                comparison.map_or(0.0, |comparison| comparison.glyphrush_speedup);
            let speed_comparable = comparison.is_some_and(|comparison| comparison.speed_comparable);
            let speed_passed =
                speed_comparable && actual_glyphrush_speedup >= requirement.min_glyphrush_speedup;
            let baseline_quality_checked = baseline.is_some_and(|baseline| {
                matches!(
                    baseline.quality_status,
                    CorpusBaselineQualityStatus::Checked
                )
            });
            let baseline_quality_passed = baseline_quality_checked
                && baseline.is_some_and(|baseline| {
                    baseline.quality_documents > 0 && baseline.quality_failed_documents == 0
                });
            speedup_claim(BenchmarkSpeedupClaimInput {
                requirement,
                baseline_was_run: baseline.is_some(),
                actual_glyphrush_speedup,
                speed_comparable,
                speed_passed,
                glyphrush_quality_checked,
                glyphrush_quality_passed,
                baseline_quality_checked,
                baseline_quality_passed,
            })
        })
        .collect()
}

pub(crate) fn speedup_claim(input: BenchmarkSpeedupClaimInput<'_>) -> BenchmarkSpeedupClaim {
    let glyphrush_quality_backed =
        input.glyphrush_quality_checked && input.glyphrush_quality_passed;
    let quality_checked = input.glyphrush_quality_checked && input.baseline_quality_checked;
    let quality_backed =
        glyphrush_quality_backed && input.baseline_quality_checked && input.baseline_quality_passed;
    let quality_blocker = if !input.glyphrush_quality_checked {
        Some(BenchmarkSpeedupClaimQualityBlocker::GlyphrushQualityNotChecked)
    } else if !input.glyphrush_quality_passed {
        Some(BenchmarkSpeedupClaimQualityBlocker::GlyphrushQualityFailed)
    } else if !input.baseline_quality_checked {
        Some(BenchmarkSpeedupClaimQualityBlocker::BaselineQualityNotChecked)
    } else if !input.baseline_quality_passed {
        Some(BenchmarkSpeedupClaimQualityBlocker::BaselineQualityFailed)
    } else {
        None
    };
    let status = if !input.baseline_was_run {
        BenchmarkSpeedupClaimStatus::BaselineNotRun
    } else if !input.speed_comparable {
        BenchmarkSpeedupClaimStatus::NotSpeedComparable
    } else if !input.speed_passed {
        BenchmarkSpeedupClaimStatus::SpeedupFailed
    } else if !quality_checked {
        BenchmarkSpeedupClaimStatus::QualityNotChecked
    } else if !quality_backed {
        BenchmarkSpeedupClaimStatus::QualityFailed
    } else {
        BenchmarkSpeedupClaimStatus::Passed
    };

    BenchmarkSpeedupClaim {
        baseline: input.requirement.baseline.clone(),
        required_glyphrush_speedup: input.requirement.min_glyphrush_speedup,
        actual_glyphrush_speedup: input.actual_glyphrush_speedup,
        speed_comparable: input.speed_comparable,
        speed_passed: input.speed_passed,
        glyphrush_quality_checked: input.glyphrush_quality_checked,
        glyphrush_quality_passed: input.glyphrush_quality_passed,
        baseline_quality_checked: input.baseline_quality_checked,
        baseline_quality_passed: input.baseline_quality_passed,
        glyphrush_quality_backed,
        quality_backed,
        quality_blocker,
        claim_passed: matches!(status, BenchmarkSpeedupClaimStatus::Passed),
        status,
    }
}

pub(crate) fn speedup_claim_requirement_error(
    claims: &[BenchmarkSpeedupClaim],
    requirements: &[BenchmarkSpeedupRequirement],
) -> Option<String> {
    for requirement in requirements {
        let Some(claim) = claims.iter().find(|claim| {
            claim.baseline == requirement.baseline
                && claim.required_glyphrush_speedup == requirement.min_glyphrush_speedup
        }) else {
            return Some(format!(
                "bench speedup claim required: baseline {} claim was not evaluated",
                requirement.baseline
            ));
        };
        if !claim.claim_passed {
            return Some(format!(
                "bench speedup claim required: baseline {} status {}",
                requirement.baseline,
                speedup_claim_status_label(claim.status)
            ));
        }
    }

    None
}

pub(crate) fn speedup_claim_status_label(status: BenchmarkSpeedupClaimStatus) -> &'static str {
    match status {
        BenchmarkSpeedupClaimStatus::Passed => "passed",
        BenchmarkSpeedupClaimStatus::BaselineNotRun => "baseline_not_run",
        BenchmarkSpeedupClaimStatus::NotSpeedComparable => "not_speed_comparable",
        BenchmarkSpeedupClaimStatus::SpeedupFailed => "speedup_failed",
        BenchmarkSpeedupClaimStatus::QualityNotChecked => "quality_not_checked",
        BenchmarkSpeedupClaimStatus::QualityFailed => "quality_failed",
    }
}

pub(crate) fn text_output_metrics_from_artifact(artifact: &DocumentArtifact) -> TextOutputMetrics {
    let text = plain_text_from_artifact(artifact);
    text_output_metrics_from_text(&text)
}

pub(crate) fn text_output_metrics_from_page(page: &PageArtifact) -> TextOutputMetrics {
    let text = plain_text_from_page(page);
    text_output_metrics_from_text(&text)
}

pub(crate) fn text_output_metrics_from_text(text: &str) -> TextOutputMetrics {
    TextOutputMetrics {
        bytes: text.len() as u64,
        line_count: text.lines().count(),
        word_count: text.split_whitespace().count(),
        empty: text.is_empty(),
    }
}

pub(crate) fn layout_summary_from_page(page: &PageArtifact) -> DebugLayoutSummary {
    let mut summary = DebugLayoutSummary {
        block_count: page.layout_blocks.len(),
        ..DebugLayoutSummary::default()
    };

    for block in &page.layout_blocks {
        match block.kind {
            LayoutBlockKind::Paragraph => summary.paragraph_blocks += 1,
            LayoutBlockKind::Heading => summary.heading_blocks += 1,
            LayoutBlockKind::List => summary.list_blocks += 1,
            LayoutBlockKind::Table => {
                summary.table_blocks += 1;
                if let Some(table) = &block.table {
                    add_table_summary_counts(
                        &mut summary,
                        table_rows_from_grid(table).len(),
                        table.rows.iter().map(|row| row.cells.len()).sum(),
                        table
                            .rows
                            .iter()
                            .flat_map(|row| &row.cells)
                            .filter(|cell| cell.bbox.is_some())
                            .count(),
                    );
                } else {
                    let rows = parse_table_rows(&block.text);
                    add_table_summary_counts(
                        &mut summary,
                        rows.len(),
                        rows.iter().map(Vec::len).sum(),
                        0,
                    );
                }
            }
            LayoutBlockKind::Figure => summary.figure_blocks += 1,
            LayoutBlockKind::Header => summary.header_blocks += 1,
            LayoutBlockKind::Footer => summary.footer_blocks += 1,
        }
    }

    summary
}

pub(crate) fn corpus_baseline_quality_status(
    runs: &[(&CorpusBenchDocument, &BaselineBenchOutput)],
    quality_documents: usize,
) -> CorpusBaselineQualityStatus {
    if !runs.is_empty() && quality_documents == runs.len() {
        return CorpusBaselineQualityStatus::Checked;
    }
    if quality_documents > 0 {
        return CorpusBaselineQualityStatus::PartiallyChecked;
    }
    if runs.iter().all(|(_, run)| {
        matches!(
            run.quality_status,
            BaselineQualityStatus::NotCheckedNoExpectations
        )
    }) {
        CorpusBaselineQualityStatus::NotCheckedNoExpectations
    } else {
        CorpusBaselineQualityStatus::NotCheckedBaselineFailures
    }
}

pub(crate) fn corpus_fingerprint<'a>(
    documents: impl IntoIterator<Item = (&'a str, &'a str, usize)>,
) -> String {
    let mut payload = String::from("glyphrush-corpus-v1\n");
    for (path, document_fingerprint, page_count) in documents {
        payload.push_str(path);
        payload.push('\t');
        payload.push_str(document_fingerprint);
        payload.push('\t');
        payload.push_str(&page_count.to_string());
        payload.push('\n');
    }
    sha256_hex(payload)
}

pub(crate) fn benchmark_category_summaries(
    documents: &[CorpusBenchDocument],
    quality: &EvalOutput,
) -> BTreeMap<String, CorpusBenchmarkCategorySummary> {
    let quality_by_fingerprint = quality
        .documents
        .iter()
        .map(|document| (document.document_fingerprint.as_str(), document))
        .collect::<BTreeMap<_, _>>();

    documents
        .iter()
        .fold(BTreeMap::new(), |mut summaries, document| {
            let Some(quality_document) =
                quality_by_fingerprint.get(document.document_fingerprint.as_str())
            else {
                return summaries;
            };
            let category = eval_document_category(quality_document);
            let failed_checks = eval_document_failed_checks(quality_document);
            summaries
                .entry(category.to_string())
                .or_insert_with(CorpusBenchmarkCategorySummary::default)
                .add_document(document, failed_checks);
            summaries
        })
}

pub(crate) fn benchmark_silent_failure_summary(
    quality: &EvalOutput,
) -> Option<BenchmarkSilentFailureSummary> {
    let mut saw_silent_failure_check = false;
    let mut count = 0usize;
    let mut pages = Vec::new();

    for document in &quality.documents {
        let Some(check) = document.checks.get("silent_failures") else {
            continue;
        };
        saw_silent_failure_check = true;
        let document_pages = check
            .actual
            .get("pages")
            .and_then(Value::as_array)
            .map(|pages| {
                pages
                    .iter()
                    .filter_map(|page| benchmark_silent_failure_page(&document.path, page))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        count += check
            .actual
            .get("count")
            .and_then(Value::as_u64)
            .map(|count| count as usize)
            .unwrap_or(document_pages.len());
        pages.extend(document_pages);
    }

    saw_silent_failure_check.then_some(BenchmarkSilentFailureSummary { count, pages })
}

pub(crate) fn benchmark_silent_failure_page(
    path: &str,
    page: &Value,
) -> Option<BenchmarkSilentFailurePage> {
    Some(BenchmarkSilentFailurePage {
        path: path.to_string(),
        page: page.get("page")?.as_u64()?,
        flags: page
            .get("flags")
            .and_then(Value::as_array)
            .map(|flags| {
                flags
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        empty_text_output: page.get("empty_text_output").and_then(Value::as_bool),
    })
}

pub(crate) fn document_text(artifact: &DocumentArtifact) -> String {
    artifact
        .pages
        .iter()
        .map(quality_text_from_page)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn document_worker_count<B: PdfBackend>(
    backend: &B,
    requested_jobs: usize,
    document_count: usize,
) -> usize {
    let worker_count = requested_jobs.max(1).min(document_count.max(1));
    if backend.supports_parallel_documents() {
        worker_count
    } else {
        1
    }
}

pub(crate) fn path_label(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn category_from_relative_pdf_path(path: &Path) -> Option<String> {
    let mut components = path.components();
    let Some(Component::Normal(category)) = components.next() else {
        return None;
    };
    components.next()?;
    normalize_manifest_category(category.to_str())
}

pub(crate) fn pages_per_sec(page_count: usize, wall_us: u128) -> f64 {
    if wall_us == 0 {
        page_count as f64
    } else {
        page_count as f64 / (wall_us as f64 / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lopdf_document_worker_count_respects_requested_jobs() {
        assert_eq!(document_worker_count(&LopdfBackend, 4, 3), 3);
    }

    #[cfg(feature = "pdfium")]
    #[test]
    fn pdfium_document_worker_count_serializes_corpus_jobs() {
        assert_eq!(document_worker_count(&PdfiumBackend, 4, 3), 1);
    }
}
