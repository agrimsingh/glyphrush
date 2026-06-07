use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output as ProcessOutput, Stdio},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glyphrush_core::{
    BBox, CacheStatus, DocumentArtifact, DocumentMetadata, ExtractedImage, ExtractedPage,
    ExtractedTextSpan, ImageArtifact, LayoutBlockKind, PageArtifact, PageDimensions, PageQuality,
    PageQualityReport, PageRoute, PageSignals, PageTimings, SpanProvenance, TextSpan,
    classify_page, parse_extracted_pages,
};
use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const LOPDF_BACKEND_NAME: &str = "lopdf";
const LOPDF_BACKEND_VERSION: &str = "lopdf-adapter-v0";
const PARSER_NAME: &str = "glyphrush";
const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");
const BENCH_REPORT_VERSION: &str = "glyphrush-bench-report-v1";
const EVAL_REPORT_VERSION: &str = "glyphrush-eval-report-v1";
const BASELINE_CHECK_REPORT_VERSION: &str = "glyphrush-baseline-check-report-v1";
const BACKEND_CHECK_REPORT_VERSION: &str = "glyphrush-backend-check-report-v1";
const MAX_POSITIONED_SPAN_CONTENT_BYTES: usize = 64 * 1024;
const MAX_POSITIONED_SPAN_NATIVE_TEXT_BYTES: u32 = 4 * 1024;
const MAX_BBOX_OVERLAP_COMPARISONS: usize = 16_384;
const CACHE_SCHEMA_VERSION: &str = "glyphrush-cache-v36";
const DEFAULT_BASELINE_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_OCR_TIMEOUT_MS: u64 = 120_000;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

static ALLOCATED_BYTES_TOTAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static ALLOCATED_BYTES_THREAD: Cell<u64> = const { Cell::new(0) };
}

struct CountingAllocator;

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

#[derive(Debug, Parser)]
#[command(name = "glyphrush")]
#[command(about = "Adaptive fast PDF parser with explicit quality flags")]
struct Cli {
    #[arg(long, value_enum, default_value_t = BackendChoice::Lopdf, global = true)]
    backend: BackendChoice,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackendChoice {
    Lopdf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CoveragePreset {
    #[value(name = "glyphrush-v0")]
    GlyphrushV0,
}

const GLYPHRUSH_V0_COVERAGE_CATEGORIES: &[&str] = &[
    "clean_digital",
    "scanned",
    "hybrid",
    "academic_columns",
    "tables",
    "forms",
    "rotated",
    "weird_encoding",
    "large",
];

impl CoveragePreset {
    fn categories(self) -> &'static [&'static str] {
        match self {
            Self::GlyphrushV0 => GLYPHRUSH_V0_COVERAGE_CATEGORIES,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BaselinePreset {
    #[value(name = "glyphrush-v0")]
    GlyphrushV0,
}

const GLYPHRUSH_V0_BASELINES: &[(&str, &str)] = &[
    ("liteparse", "tools/baselines/liteparse-text.sh"),
    (
        "liteparse-no-ocr",
        "tools/baselines/liteparse-no-ocr-text.sh",
    ),
    ("pymupdf", "tools/baselines/pymupdf-text.sh"),
    ("pdfplumber", "tools/baselines/pdfplumber-text.sh"),
];

impl BaselinePreset {
    fn name(self) -> &'static str {
        match self {
            Self::GlyphrushV0 => "glyphrush-v0",
        }
    }

    fn specs(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::GlyphrushV0 => GLYPHRUSH_V0_BASELINES,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    Inspect {
        pdf: PathBuf,
        #[arg(long)]
        pages: bool,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
    Parse {
        pdf: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        #[arg(long)]
        span_geometry: bool,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
    Bench {
        pdf: PathBuf,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        #[arg(long)]
        eval_manifest: Option<PathBuf>,
        #[arg(long)]
        eval_category: Option<String>,
        #[arg(long)]
        require_quality: bool,
        #[arg(long)]
        require_baselines: bool,
        #[arg(long)]
        require_baseline_quality: bool,
        #[arg(long)]
        cache_probe: bool,
        #[arg(long)]
        span_geometry: bool,
        #[arg(long, value_name = "NAME=EXECUTABLE")]
        baseline: Vec<BaselineSpec>,
        #[arg(long, value_enum)]
        baseline_preset: Option<BaselinePreset>,
        #[arg(long, default_value_t = DEFAULT_BASELINE_TIMEOUT_MS)]
        baseline_timeout_ms: u64,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
    BaselineCheck {
        #[arg(long, value_name = "NAME=EXECUTABLE")]
        baseline: Vec<BaselineSpec>,
        #[arg(long, value_enum)]
        baseline_preset: Option<BaselinePreset>,
        #[arg(long)]
        pdf: Option<PathBuf>,
        #[arg(long, default_value_t = DEFAULT_BASELINE_TIMEOUT_MS)]
        baseline_timeout_ms: u64,
        #[arg(long)]
        strict: bool,
    },
    BackendCheck {
        #[arg(long)]
        pdf: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
    Manifest {
        pdf: PathBuf,
        #[arg(long)]
        category: Option<String>,
        #[arg(long, value_enum)]
        coverage_preset: Option<CoveragePreset>,
        #[arg(long)]
        required_category: Vec<String>,
        #[arg(long)]
        min_category_count: Vec<CategoryCountSpec>,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        #[arg(long)]
        span_geometry: bool,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
    DebugPage {
        pdf: PathBuf,
        page_index: u32,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        span_geometry: bool,
    },
    Eval {
        manifest: PathBuf,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        #[arg(long)]
        span_geometry: bool,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
    Markdown,
}

#[derive(Debug, Serialize)]
struct InspectOutput {
    backend: &'static str,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    page_count: usize,
}

#[derive(Debug, Serialize)]
struct BackendCheckOutput {
    report_version: &'static str,
    parser_name: &'static str,
    parser_version: &'static str,
    selected_backend: &'static str,
    enabled_backend_count: usize,
    candidate_backend_count: usize,
    decision_gate: &'static str,
    backends: Vec<BackendCapabilityOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    smoke: Option<BackendSmokeOutput>,
}

#[derive(Debug, Serialize)]
struct BackendSmokeOutput {
    mode: &'static str,
    path: String,
    backend: &'static str,
    success: bool,
    wall_us: u128,
    source_size_bytes: Option<u64>,
    document_fingerprint: Option<String>,
    page_count: Option<usize>,
    extracted_page_count: Option<usize>,
    native_text_bytes: Option<usize>,
    image_artifact_count: Option<usize>,
    fallback_pages: Option<u32>,
    ocr_required_pages: Option<u32>,
    worker_count: Option<usize>,
    document_count: Option<usize>,
    successful_documents: Option<usize>,
    failed_documents: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    documents: Vec<BackendSmokeOutput>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BackendCapabilityOutput {
    name: &'static str,
    status: BackendStatus,
    selected: bool,
    version: Option<&'static str>,
    capabilities: BackendCapabilityMatrix,
    limitations: Vec<&'static str>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum BackendStatus {
    Enabled,
    NotWired,
}

#[derive(Debug, Serialize)]
struct BackendCapabilityMatrix {
    open_pdf: bool,
    page_count: bool,
    native_text: bool,
    span_geometry: &'static str,
    image_metadata: bool,
    render_pages: bool,
    builtin_ocr: bool,
}

#[derive(Debug, Serialize)]
struct InspectPagesOutput {
    backend: &'static str,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    page_count: usize,
    worker_count: usize,
    cache_status: CacheStatus,
    cache_key: Option<String>,
    fallback_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    warnings_count: usize,
    pages: Vec<InspectPageSummary>,
}

#[derive(Debug, Serialize)]
struct InspectPageSummary {
    page_index: u32,
    artifact_id: String,
    page_fingerprint: String,
    dimensions: PageDimensions,
    route: PageRoute,
    quality_flags: Vec<PageQuality>,
    reasons: Vec<String>,
    native_span_count: usize,
    native_text_bytes: usize,
    ocr_span_count: usize,
    image_artifact_count: usize,
    layout_block_count: usize,
    timings: PageTimings,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CorpusInspectOutput {
    backend: &'static str,
    document_count: usize,
    page_count: usize,
    corpus_fingerprint: String,
    documents: Vec<CorpusInspectDocument>,
}

#[derive(Debug, Serialize)]
struct CorpusInspectDocument {
    path: String,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    page_count: usize,
}

#[derive(Debug, Serialize)]
struct CorpusInspectPagesOutput {
    backend: &'static str,
    document_count: usize,
    page_count: usize,
    worker_count: usize,
    cache_hits: u32,
    cache_misses: u32,
    fallback_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    warnings_count: usize,
    corpus_fingerprint: String,
    documents: Vec<CorpusInspectPagesDocument>,
}

#[derive(Debug, Serialize)]
struct CorpusInspectPagesDocument {
    path: String,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    page_count: usize,
    cache_status: CacheStatus,
    cache_key: Option<String>,
    fallback_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    warnings_count: usize,
    pages: Vec<InspectPageSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum BenchQualityStatus {
    Checked,
    NotCheckedNoEvalManifest,
}

#[derive(Debug, Serialize)]
struct BenchOutput {
    report_version: &'static str,
    backend: &'static str,
    run_metadata: BenchmarkRunMetadata,
    run_configuration: RunConfiguration,
    requirements: BenchmarkRequirements,
    requested_baseline_presets: Vec<&'static str>,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    page_count: usize,
    worker_count: usize,
    wall_us: u128,
    pages_per_sec: f64,
    artifact_bytes: u64,
    allocated_bytes: u64,
    allocated_bytes_per_page: f64,
    text_output_bytes: u64,
    text_output_line_count: usize,
    text_output_word_count: usize,
    empty_text_output: bool,
    peak_rss_bytes: u64,
    stage_timings_us: BenchStageTimings,
    page_latency_us: PageLatencySummary,
    route_counts: RouteCounts,
    route_latency_us: RouteLatencySummary,
    route_reason_counts: BTreeMap<String, u32>,
    fallback_pages: u32,
    ocr_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    image_artifact_pages: u32,
    quality_flag_counts: QualityFlagCounts,
    fallback_action_counts: FallbackActionCounts,
    warnings_count: usize,
    warnings: Vec<String>,
    cache_status: CacheStatus,
    cache_key: Option<String>,
    baselines: Vec<BaselineBenchOutput>,
    quality_status: BenchQualityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<EvalOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_probe: Option<CacheProbeOutput>,
    #[serde(skip)]
    page_latencies_us: Vec<u64>,
    #[serde(skip)]
    artifact: DocumentArtifact,
}

#[derive(Debug, Serialize)]
struct CorpusBenchOutput {
    report_version: &'static str,
    backend: &'static str,
    run_metadata: BenchmarkRunMetadata,
    run_configuration: RunConfiguration,
    requirements: BenchmarkRequirements,
    requested_baseline_presets: Vec<&'static str>,
    document_count: usize,
    page_count: usize,
    worker_count: usize,
    corpus_fingerprint: String,
    wall_us: u128,
    pages_per_sec: f64,
    artifact_bytes: u64,
    allocated_bytes: u64,
    allocated_bytes_per_page: f64,
    text_output_bytes: u64,
    text_output_line_count: usize,
    text_output_word_count: usize,
    empty_text_output_documents: usize,
    empty_text_output_pages: usize,
    peak_rss_bytes: u64,
    stage_timings_us: BenchStageTimings,
    page_latency_us: PageLatencySummary,
    route_counts: RouteCounts,
    route_latency_us: RouteLatencySummary,
    route_reason_counts: BTreeMap<String, u32>,
    fallback_pages: u32,
    ocr_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    image_artifact_pages: u32,
    quality_flag_counts: QualityFlagCounts,
    fallback_action_counts: FallbackActionCounts,
    warnings_count: usize,
    warning_samples: Vec<CorpusWarningSample>,
    cache_hits: u32,
    cache_misses: u32,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    category_summaries: BTreeMap<String, CorpusBenchmarkCategorySummary>,
    baselines: Vec<CorpusBaselineBenchOutput>,
    quality_status: BenchQualityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<EvalOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_probe: Option<CorpusCacheProbeOutput>,
    documents: Vec<CorpusBenchDocument>,
}

#[derive(Debug, Serialize)]
struct CorpusBenchDocument {
    #[serde(skip)]
    source_path: PathBuf,
    path: String,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    page_count: usize,
    wall_us: u128,
    pages_per_sec: f64,
    artifact_bytes: u64,
    allocated_bytes: u64,
    allocated_bytes_per_page: f64,
    text_output_bytes: u64,
    text_output_line_count: usize,
    text_output_word_count: usize,
    empty_text_output: bool,
    peak_rss_bytes: u64,
    stage_timings_us: BenchStageTimings,
    page_latency_us: PageLatencySummary,
    route_counts: RouteCounts,
    route_latency_us: RouteLatencySummary,
    route_reason_counts: BTreeMap<String, u32>,
    fallback_pages: u32,
    ocr_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    image_artifact_pages: u32,
    quality_flag_counts: QualityFlagCounts,
    fallback_action_counts: FallbackActionCounts,
    warnings_count: usize,
    warnings: Vec<String>,
    cache_status: CacheStatus,
    cache_key: Option<String>,
    baselines: Vec<BaselineBenchOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_probe: Option<CacheProbeOutput>,
    #[serde(skip)]
    page_latencies_us: Vec<u64>,
    #[serde(skip)]
    artifact: DocumentArtifact,
}

#[derive(Clone, Debug, Default, Serialize)]
struct CorpusBenchmarkCategorySummary {
    document_count: usize,
    page_count: usize,
    wall_us: u128,
    pages_per_sec: f64,
    fallback_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    route_counts: RouteCounts,
    quality_flag_counts: QualityFlagCounts,
    warnings_count: usize,
    failed_checks: u32,
    quality_passed: bool,
    quality_failed: bool,
}

impl CorpusBenchmarkCategorySummary {
    fn add_document(&mut self, document: &CorpusBenchDocument, failed_checks: u32) {
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
struct BenchmarkRunMetadata {
    parser_name: &'static str,
    parser_version: &'static str,
    backend: &'static str,
    backend_version: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct RunConfiguration {
    span_geometry: bool,
    ocr_sidecar: bool,
    ocr_command: bool,
    ocr_timeout_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct BenchmarkRequirements {
    require_quality: bool,
    require_baselines: bool,
    require_baseline_quality: bool,
}

#[derive(Debug, Serialize)]
struct CacheProbeOutput {
    cold: CacheProbeRunOutput,
    warm: CacheProbeRunOutput,
    cache_key_match: bool,
    warm_speedup: f64,
}

#[derive(Debug, Serialize)]
struct CacheProbeRunOutput {
    cache_status: CacheStatus,
    wall_us: u128,
    pages_per_sec: f64,
    artifact_bytes: u64,
    allocated_bytes: u64,
    allocated_bytes_per_page: f64,
    text_output_bytes: u64,
    text_output_line_count: usize,
    text_output_word_count: usize,
    empty_text_output: bool,
    peak_rss_bytes: u64,
    stage_timings_us: BenchStageTimings,
    page_latency_us: PageLatencySummary,
    route_counts: RouteCounts,
    route_latency_us: RouteLatencySummary,
    route_reason_counts: BTreeMap<String, u32>,
    fallback_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    image_artifact_pages: u32,
    quality_flag_counts: QualityFlagCounts,
    fallback_action_counts: FallbackActionCounts,
    warnings_count: usize,
    warnings: Vec<String>,
    cache_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct CorpusCacheProbeOutput {
    document_count: usize,
    cold_wall_us: u128,
    warm_wall_us: u128,
    cold_pages_per_sec: f64,
    warm_pages_per_sec: f64,
    cold_allocated_bytes: u64,
    warm_allocated_bytes: u64,
    cold_allocated_bytes_per_page: f64,
    warm_allocated_bytes_per_page: f64,
    cold_fallback_action_counts: FallbackActionCounts,
    warm_fallback_action_counts: FallbackActionCounts,
    cold_stage_timings_us: BenchStageTimings,
    warm_stage_timings_us: BenchStageTimings,
    warm_speedup: f64,
    cold_cache_misses: u32,
    warm_cache_hits: u32,
}

#[derive(Debug, Serialize)]
struct CorpusWarningSample {
    path: String,
    warning: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
struct QualityFlagCounts {
    requires_ocr: u32,
    low_confidence_text: u32,
    broken_encoding: u32,
    layout_uncertain: u32,
    table_uncertain: u32,
    unsupported_feature: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct FallbackActionCounts {
    ocr_requested_pages: u32,
    ocr_applied_pages: u32,
    heavy_layout_pages: u32,
    table_recovery_pages: u32,
    render_pages: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
struct RouteCounts {
    native_fast_path: u32,
    needs_fallback: u32,
    ocr_fallback: u32,
    unsupported: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct RouteLatencySummary {
    native_fast_path: PageLatencySummary,
    needs_fallback: PageLatencySummary,
    ocr_fallback: PageLatencySummary,
    unsupported: PageLatencySummary,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct TextOutputMetrics {
    bytes: u64,
    line_count: usize,
    word_count: usize,
    empty: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct BenchStageTimings {
    open_us: u64,
    classify_us: u64,
    native_extract_us: u64,
    layout_us: u64,
    table_us: u64,
    render_us: u64,
    ocr_us: u64,
    merge_us: u64,
    total_us: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct PageLatencySummary {
    p50_us: u64,
    p95_us: u64,
    max_us: u64,
}

#[derive(Clone, Debug)]
struct BaselineSpec {
    name: String,
    command: PathBuf,
}

#[derive(Clone, Debug)]
struct CategoryCountSpec {
    category: String,
    count: usize,
}

impl FromStr for BaselineSpec {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (name, command) = value
            .split_once('=')
            .ok_or_else(|| "baseline must use NAME=EXECUTABLE".to_string())?;
        let name = name.trim();
        let command = command.trim();

        if name.is_empty() {
            return Err("baseline name cannot be empty".to_string());
        }
        if command.is_empty() {
            return Err("baseline executable cannot be empty".to_string());
        }

        Ok(Self {
            name: name.to_string(),
            command: PathBuf::from(command),
        })
    }
}

impl FromStr for CategoryCountSpec {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (category, count) = value
            .split_once('=')
            .ok_or_else(|| "category count must use NAME=COUNT".to_string())?;
        let category = category.trim();
        let count = count.trim();

        if category.is_empty() {
            return Err("category name cannot be empty".to_string());
        }
        if count.is_empty() {
            return Err("category count cannot be empty".to_string());
        }
        let count = count
            .parse::<usize>()
            .map_err(|_| "category count must be a positive integer".to_string())?;
        if count == 0 {
            return Err("category count must be greater than zero".to_string());
        }

        Ok(Self {
            category: category.to_string(),
            count,
        })
    }
}

#[derive(Debug, Serialize)]
struct BaselineCheckOutput {
    report_version: &'static str,
    run_metadata: BenchmarkRunMetadata,
    strict: bool,
    requested_baseline_presets: Vec<&'static str>,
    baseline_count: usize,
    describe_success_count: usize,
    all_described: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    smoke_pdf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    smoke_document_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    smoke_success_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    all_smoke_passed: Option<bool>,
    baselines: Vec<BaselineCheckResult>,
}

#[derive(Debug, Serialize)]
struct BaselineCheckResult {
    name: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Value>,
    describe: BaselineDescribeCheck,
    #[serde(skip_serializing_if = "Option::is_none")]
    smoke: Option<BaselineSmokeCheck>,
}

#[derive(Debug, Serialize)]
struct BaselineDescribeCheck {
    success: bool,
    exit_status: Option<i32>,
    timed_out: bool,
    timeout_ms: u64,
    wall_us: u128,
    stdout_bytes: u64,
    stderr_bytes: u64,
    stderr_preview: Option<String>,
    valid_json_object: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BaselineSmokeCheck {
    success: bool,
    exit_status: Option<i32>,
    timed_out: bool,
    timeout_ms: u64,
    wall_us: u128,
    output_bytes: u64,
    stdout_sha256: Option<String>,
    stdout_line_count: usize,
    stdout_word_count: usize,
    stderr_bytes: u64,
    empty_output: bool,
    stderr_preview: Option<String>,
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    successful_documents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_documents: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    documents: Vec<BaselineSmokeDocument>,
}

#[derive(Debug, Serialize)]
struct BaselineSmokeDocument {
    path: String,
    success: bool,
    exit_status: Option<i32>,
    timed_out: bool,
    timeout_ms: u64,
    wall_us: u128,
    output_bytes: u64,
    stdout_sha256: Option<String>,
    stdout_line_count: usize,
    stdout_word_count: usize,
    stderr_bytes: u64,
    empty_output: bool,
    stderr_preview: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineBenchOutput {
    name: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Value>,
    comparison: BaselineComparisonOutput,
    success: bool,
    exit_status: Option<i32>,
    timed_out: bool,
    timeout_ms: u64,
    wall_us: u128,
    output_bytes: u64,
    stdout_sha256: Option<String>,
    stdout_line_count: usize,
    stdout_word_count: usize,
    stderr_bytes: u64,
    empty_output: bool,
    stderr_preview: Option<String>,
    error: Option<String>,
    quality_status: BaselineQualityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<BaselineQualityOutput>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum BaselineQualityStatus {
    Checked,
    NotCheckedNoExpectations,
    NotCheckedTimedOut,
    NotCheckedExecutionFailed,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct BaselineComparisonOutput {
    speed_comparable: bool,
    glyphrush_wall_us: u128,
    baseline_wall_us: u128,
    glyphrush_speedup: f64,
    baseline_speedup: f64,
    glyphrush_text_output_bytes: u64,
    baseline_output_bytes: u64,
    baseline_to_glyphrush_output_bytes: f64,
}

#[derive(Clone, Debug)]
struct BaselineQualityExpectations {
    category: Option<String>,
    required_text: Vec<String>,
    text_recall: Option<TextRecallExpectation>,
    reading_order: Option<ReadingOrderExpectation>,
    table_structure: Vec<TableStructureExpectation>,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineQualityOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    passed: bool,
    failed_checks: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_text: Option<BaselineRequiredTextOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text_recall: Option<BaselineTextRecallOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reading_order: Option<BaselineReadingOrderOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    table_structure: Option<Vec<BaselineTableStructureOutput>>,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineRequiredTextOutput {
    passed: bool,
    expected: Vec<String>,
    missing: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineTextRecallOutput {
    passed: bool,
    word_recall: f64,
    char_recall: f64,
    missing_words: Vec<String>,
    min_word_recall: f64,
    min_char_recall: f64,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineReadingOrderOutput {
    passed: bool,
    score: f64,
    matched: Vec<ReadingOrderMatch>,
    missing: Vec<String>,
    inversion_count: usize,
    inversions: Vec<ReadingOrderInversion>,
    min_score: f64,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineTableStructureOutput {
    page: u32,
    passed: bool,
    extracted_rows: Vec<Vec<String>>,
    row_precision: f64,
    row_recall: f64,
    row_f1: f64,
    missing_rows: Vec<Vec<String>>,
    extra_rows: Vec<Vec<String>>,
    cell_precision: f64,
    cell_recall: f64,
    cell_f1: f64,
    missing_cells: Vec<TableCell>,
    extra_cells: Vec<TableCell>,
    min_row_precision: f64,
    min_row_recall: f64,
    min_row_f1: f64,
    min_cell_precision: f64,
    min_cell_recall: f64,
    min_cell_f1: f64,
}

#[derive(Debug, Serialize)]
struct CorpusBaselineBenchOutput {
    name: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Value>,
    comparison: BaselineComparisonOutput,
    document_count: usize,
    successful_documents: usize,
    failed_documents: usize,
    timed_out_documents: usize,
    successful_pages: usize,
    failed_pages: usize,
    timed_out_pages: usize,
    empty_output_documents: usize,
    empty_output_pages: usize,
    success_rate: f64,
    quality_status: CorpusBaselineQualityStatus,
    quality_documents: usize,
    quality_unchecked_documents: usize,
    quality_passed_documents: usize,
    quality_failed_documents: usize,
    quality_failed_checks: u32,
    quality_required_text_failed_documents: usize,
    quality_text_recall_failed_documents: usize,
    quality_reading_order_failed_documents: usize,
    quality_table_structure_failed_documents: usize,
    quality_category_summaries: BTreeMap<String, CorpusBaselineQualityCategorySummary>,
    quality_pass_rate: f64,
    failure_samples: Vec<CorpusBaselineFailureSample>,
    quality_failure_samples: Vec<CorpusBaselineQualityFailureSample>,
    wall_us: u128,
    pages_per_sec: f64,
    successful_pages_per_sec: f64,
    output_bytes: u64,
    stderr_bytes: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CorpusBaselineQualityStatus {
    Checked,
    PartiallyChecked,
    NotCheckedNoExpectations,
    NotCheckedBaselineFailures,
}

#[derive(Debug, Serialize)]
struct CorpusBaselineFailureSample {
    path: String,
    exit_status: Option<i32>,
    error: Option<String>,
    stderr_preview: Option<String>,
}

#[derive(Debug, Serialize)]
struct CorpusBaselineQualityFailureSample {
    path: String,
    failed_checks: u32,
    failed_check_types: Vec<&'static str>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct CorpusBaselineQualityCategorySummary {
    document_count: usize,
    page_count: usize,
    passed_documents: usize,
    failed_documents: usize,
    failed_checks: u32,
    quality_pass_rate: f64,
    quality_passed: bool,
    quality_failed: bool,
}

impl CorpusBaselineQualityCategorySummary {
    fn add_document(&mut self, page_count: usize, quality: &BaselineQualityOutput) {
        self.document_count += 1;
        self.page_count += page_count;
        self.failed_checks += quality.failed_checks;
        if quality.passed {
            self.passed_documents += 1;
        } else {
            self.failed_documents += 1;
        }
        self.quality_pass_rate = self.passed_documents as f64 / self.document_count as f64;
        self.quality_passed = self.failed_checks == 0;
        self.quality_failed = !self.quality_passed;
    }
}

#[derive(Debug, Serialize)]
struct DebugPageOutput {
    backend: &'static str,
    metadata: DocumentMetadata,
    document_fingerprint: String,
    artifact_id: String,
    page_fingerprint: String,
    document_page_count: usize,
    extracted_page_count: usize,
    page_index: u32,
    dimensions: PageDimensions,
    signals: PageSignals,
    quality: PageQualityReport,
    text_output: TextOutputMetrics,
    layout: DebugLayoutSummary,
    timings: PageTimings,
    image_artifacts: Vec<ImageArtifact>,
    warnings: Vec<String>,
    decision: glyphrush_core::RouteDecision,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct DebugLayoutSummary {
    block_count: usize,
    paragraph_blocks: usize,
    heading_blocks: usize,
    list_blocks: usize,
    table_blocks: usize,
    figure_blocks: usize,
    header_blocks: usize,
    footer_blocks: usize,
}

#[derive(Debug, Serialize)]
struct GeneratedEvalManifest {
    manifest_version: &'static str,
    generator: GeneratedManifestGenerator,
    corpus_fingerprint: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required_categories: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    min_category_counts: BTreeMap<String, usize>,
    documents: Vec<GeneratedManifestDocument>,
}

#[derive(Debug, Serialize)]
struct GeneratedManifestGenerator {
    parser_name: &'static str,
    parser_version: &'static str,
    backend: &'static str,
    backend_version: &'static str,
    span_geometry: bool,
    ocr_sidecar: bool,
    ocr_command: bool,
    ocr_timeout_ms: u64,
}

#[derive(Debug, Serialize)]
struct GeneratedManifestDocument {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    document_fingerprint: String,
    source_size_bytes: u64,
    source_modified_unix_ms: u64,
    expect: GeneratedManifestExpectations,
}

#[derive(Debug, Serialize)]
struct GeneratedManifestExpectations {
    page_count: usize,
    fallback_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    warnings_count: usize,
    required_warnings: Vec<String>,
    route_counts: RouteCounts,
    route_reason_counts: BTreeMap<String, u32>,
    quality_flag_counts: QualityFlagCounts,
    ocr_required_classification: OcrRequiredClassificationExpectation,
    quality_flag_classification: Vec<QualityFlagClassificationExpectation>,
    silent_failures: GeneratedSilentFailuresExpectation,
    pages: Vec<GeneratedPageExpectation>,
}

#[derive(Debug, Serialize)]
struct GeneratedSilentFailuresExpectation {
    max_count: usize,
}

#[derive(Debug, Serialize)]
struct GeneratedPageExpectation {
    index: u32,
    artifact_id: String,
    page_fingerprint: String,
    route: PageRoute,
    empty_text_output: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required_text: Vec<String>,
    image_artifact_count: u32,
    layout_block_counts: DebugLayoutSummary,
    required_flags: Vec<PageQuality>,
    required_reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EvalManifest {
    #[serde(default)]
    required_categories: Vec<String>,
    #[serde(default)]
    min_category_counts: BTreeMap<String, usize>,
    documents: Vec<EvalManifestDocument>,
}

#[derive(Debug, Deserialize)]
struct EvalManifestDocument {
    path: String,
    category: Option<String>,
    document_fingerprint: Option<String>,
    source_size_bytes: Option<u64>,
    source_modified_unix_ms: Option<u64>,
    #[serde(default)]
    expect: EvalExpectations,
}

#[derive(Debug, Default, Deserialize)]
struct EvalExpectations {
    page_count: Option<usize>,
    fallback_pages: Option<u32>,
    ocr_required_pages: Option<u32>,
    ocr_applied_pages: Option<u32>,
    image_artifact_count: Option<u32>,
    warnings_count: Option<usize>,
    route_counts: Option<RouteCounts>,
    quality_flag_counts: Option<QualityFlagCounts>,
    #[serde(default)]
    route_reason_counts: BTreeMap<String, u32>,
    text_recall: Option<TextRecallExpectation>,
    reading_order: Option<ReadingOrderExpectation>,
    ocr_required_classification: Option<OcrRequiredClassificationExpectation>,
    silent_failures: Option<SilentFailuresExpectation>,
    #[serde(default)]
    quality_flag_classification: Vec<QualityFlagClassificationExpectation>,
    #[serde(default)]
    table_structure: Vec<TableStructureExpectation>,
    #[serde(default)]
    span_bbox: Vec<SpanBBoxExpectation>,
    #[serde(default)]
    required_text: Vec<String>,
    #[serde(default)]
    required_warnings: Vec<String>,
    #[serde(default)]
    pages: Vec<EvalPageExpectation>,
}

#[derive(Clone, Debug, Deserialize)]
struct TextRecallExpectation {
    expected: String,
    min_word_recall: Option<f64>,
    min_char_recall: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReadingOrderExpectation {
    #[serde(default)]
    expected_sequence: Vec<String>,
    min_score: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
struct ReadingOrderMatch {
    snippet: String,
    position: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ReadingOrderInversion {
    before: String,
    after: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OcrRequiredClassificationExpectation {
    #[serde(default)]
    expected_pages: Vec<u32>,
    min_precision: Option<f64>,
    min_recall: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SilentFailuresExpectation {
    max_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SilentFailurePage {
    page: u32,
    flags: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    empty_text_output: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QualityFlagClassificationExpectation {
    flag: PageQuality,
    #[serde(default)]
    expected_pages: Vec<u32>,
    min_precision: Option<f64>,
    min_recall: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
struct TableStructureExpectation {
    page: u32,
    #[serde(default)]
    expected_rows: Vec<Vec<String>>,
    min_row_precision: Option<f64>,
    min_row_recall: Option<f64>,
    min_row_f1: Option<f64>,
    min_cell_precision: Option<f64>,
    min_cell_recall: Option<f64>,
    min_cell_f1: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SpanBBoxExpectation {
    page: u32,
    text: String,
    provenance: Option<SpanProvenance>,
    min_x0: Option<f32>,
    max_x0: Option<f32>,
    min_y0: Option<f32>,
    max_y0: Option<f32>,
    min_x1: Option<f32>,
    max_x1: Option<f32>,
    min_y1: Option<f32>,
    max_y1: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct TableCell {
    row: usize,
    column: usize,
    text: String,
}

#[derive(Debug, Deserialize)]
struct EvalPageExpectation {
    index: u32,
    artifact_id: Option<String>,
    page_fingerprint: Option<String>,
    route: Option<PageRoute>,
    empty_text_output: Option<bool>,
    image_artifact_count: Option<u32>,
    layout_block_counts: Option<DebugLayoutSummary>,
    #[serde(default)]
    required_text: Vec<String>,
    #[serde(default)]
    required_flags: Vec<PageQuality>,
    #[serde(default)]
    required_reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EvalOutput {
    report_version: &'static str,
    backend: &'static str,
    run_metadata: BenchmarkRunMetadata,
    run_configuration: RunConfiguration,
    manifest_path: String,
    manifest_sha256: String,
    corpus_fingerprint: String,
    document_count: usize,
    category_counts: BTreeMap<String, usize>,
    category_summaries: BTreeMap<String, EvalCategorySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category_coverage: Option<CategoryCoverageOutput>,
    page_count: usize,
    worker_count: usize,
    cache_hits: u32,
    cache_misses: u32,
    fallback_pages: u32,
    ocr_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    image_artifact_pages: u32,
    empty_text_output_pages: usize,
    route_counts: RouteCounts,
    route_reason_counts: BTreeMap<String, u32>,
    quality_flag_counts: QualityFlagCounts,
    fallback_action_counts: FallbackActionCounts,
    warnings_count: usize,
    passed: bool,
    quality_passed: bool,
    quality_failed: bool,
    failed_checks: u32,
    failure_samples: Vec<EvalFailureSample>,
    documents: Vec<EvalDocumentOutput>,
}

#[derive(Clone, Debug, Serialize)]
struct CategoryCoverageOutput {
    required: Vec<String>,
    present: Vec<String>,
    missing: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    min_category_counts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    under_minimum: BTreeMap<String, CategoryMinimumCoverageOutput>,
    passed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CategoryMinimumCoverageOutput {
    required: usize,
    actual: usize,
}

struct EvalOutputContext<'a> {
    run_metadata: BenchmarkRunMetadata,
    run_configuration: RunConfiguration,
    manifest_path: &'a Path,
    manifest_sha256: String,
    required_categories: Vec<String>,
    min_category_counts: BTreeMap<String, usize>,
    worker_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
struct EvalCategorySummary {
    document_count: usize,
    page_count: usize,
    passed_documents: usize,
    failed_documents: usize,
    failed_checks: u32,
    quality_passed: bool,
    quality_failed: bool,
}

impl EvalCategorySummary {
    fn add_document(&mut self, document: &EvalDocumentOutput) {
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
struct EvalDocumentOutput {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    document_fingerprint: String,
    metadata: DocumentMetadata,
    page_count: usize,
    artifact_cache_status: CacheStatus,
    fallback_pages: u32,
    ocr_pages: u32,
    ocr_required_pages: u32,
    ocr_applied_pages: u32,
    image_artifact_count: u32,
    image_artifact_pages: u32,
    empty_text_output_pages: usize,
    route_counts: RouteCounts,
    route_reason_counts: BTreeMap<String, u32>,
    quality_flag_counts: QualityFlagCounts,
    fallback_action_counts: FallbackActionCounts,
    warnings_count: usize,
    passed: bool,
    checks: BTreeMap<String, EvalCheckOutput>,
}

#[derive(Debug, Serialize)]
struct EvalFailureSample {
    path: String,
    check: String,
    expected: serde_json::Value,
    actual: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
struct EvalCheckOutput {
    passed: bool,
    expected: serde_json::Value,
    actual: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Default)]
struct ExtractionOptions {
    span_geometry: bool,
    page_jobs: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct OcrOptions<'a> {
    sidecar: Option<&'a Path>,
    command: Option<&'a Path>,
    timeout: Duration,
}

impl<'a> OcrOptions<'a> {
    fn new(sidecar: Option<&'a Path>, command: Option<&'a Path>, timeout_ms: u64) -> Result<Self> {
        if sidecar.is_some() && command.is_some() {
            bail!("choose either --ocr-sidecar or --ocr-command, not both");
        }

        Ok(Self {
            sidecar,
            command,
            timeout: Duration::from_millis(timeout_ms),
        })
    }
}

#[derive(Clone, Copy)]
struct BenchRunConfig<'a> {
    ocr: OcrOptions<'a>,
    cache_dir: Option<&'a Path>,
    cache_probe: bool,
    jobs: usize,
    extraction: ExtractionOptions,
    baselines: &'a [BaselineSpec],
    requested_baseline_presets: &'a [&'static str],
    baseline_timeout: Duration,
    requirements: BenchmarkRequirements,
}

#[derive(Clone, Copy)]
struct ManifestRunConfig<'a> {
    category: Option<&'a str>,
    required_categories: &'a [String],
    min_category_counts: &'a [CategoryCountSpec],
    ocr: OcrOptions<'a>,
    cache_dir: Option<&'a Path>,
    extraction: ExtractionOptions,
    jobs: usize,
}

fn manifest_required_categories_with_preset(
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

fn manifest_min_category_counts_with_preset(
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

fn baseline_specs_with_preset(
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

fn baseline_preset_names(preset: Option<BaselinePreset>) -> Vec<&'static str> {
    preset
        .into_iter()
        .map(BaselinePreset::name)
        .collect::<Vec<_>>()
}

fn backend_check_output<B: PdfBackend + Sync>(
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
            version: Some(backend.version()),
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
            status: BackendStatus::NotWired,
            selected: selected_backend == "pdfium",
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
                "license_packaging_spike_required",
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
        decision_gate: "pdfium_mupdf_spike_required_before_backend_lock_in",
        backends,
        smoke: smoke_pdf.map(|path| backend_smoke_output(backend, path, jobs)),
    }
}

fn backend_smoke_output<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    jobs: usize,
) -> BackendSmokeOutput {
    if path.is_dir() {
        return backend_smoke_directory_output(backend, path, jobs);
    }

    backend_smoke_pdf_output(backend, path, path.display().to_string())
}

fn backend_smoke_directory_output<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    jobs: usize,
) -> BackendSmokeOutput {
    let started = Instant::now();
    let result = discover_pdfs(path).map(|pdfs| {
        let worker_count = jobs.max(1).min(pdfs.len().max(1));
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
                error: (failed_documents > 0)
                    .then(|| format!("{failed_documents} backend smoke document(s) failed")),
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
            documents: Vec::new(),
            error: Some(format!("{error:#}")),
        },
    }
}

fn backend_smoke_directory_parallel<B: PdfBackend + Sync>(
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

fn backend_smoke_pdf_output<B: PdfBackend>(
    backend: &B,
    path: &Path,
    output_path: String,
) -> BackendSmokeOutput {
    let started = Instant::now();
    let result = (|| -> Result<_> {
        let source_size_bytes = source_size_bytes(path)?;
        let fingerprint = document_fingerprint(path)?;
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
            documents: Vec::new(),
            error: None,
        },
        Err(error) => BackendSmokeOutput {
            mode: "single_pdf",
            path: output_path,
            backend: backend.name(),
            success: false,
            wall_us,
            source_size_bytes: None,
            document_fingerprint: None,
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
            documents: Vec::new(),
            error: Some(format!("{error:#}")),
        },
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.backend {
        BackendChoice::Lopdf => run_command(&LopdfBackend, cli.command),
    }
}

fn run_command<B: PdfBackend + Sync>(backend: &B, command: Commands) -> Result<()> {
    match command {
        Commands::Inspect {
            pdf,
            pages,
            cache_dir,
            jobs,
        } => {
            if !pages && cache_dir.is_some() {
                bail!("inspect --cache-dir requires --pages");
            }
            if pdf.is_dir() {
                if pages {
                    write_json(&inspect_corpus_pages(
                        backend,
                        &pdf,
                        cache_dir.as_deref(),
                        jobs,
                    )?)?;
                } else {
                    write_json(&inspect_corpus(backend, &pdf)?)?;
                }
            } else if pages {
                write_json(&inspect_pages(backend, &pdf, cache_dir.as_deref(), jobs)?)?;
            } else {
                let (document, fingerprint) = load_document(backend, &pdf)?;
                write_json(&InspectOutput {
                    backend: backend.name(),
                    metadata: document_metadata(backend, &pdf)?,
                    document_fingerprint: fingerprint,
                    page_count: backend.page_count(&document),
                })?;
            }
        }
        Commands::Parse {
            pdf,
            format,
            ocr_sidecar,
            ocr_command,
            ocr_timeout_ms,
            cache_dir,
            span_geometry,
            jobs,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_timeout_ms,
            )?;
            let artifact = parse_pdf(
                backend,
                &pdf,
                ocr,
                cache_dir.as_deref(),
                ExtractionOptions {
                    span_geometry,
                    page_jobs: jobs.max(1),
                },
            )?;
            match format {
                OutputFormat::Json => write_json(&artifact)?,
                OutputFormat::Text => {
                    write_plain_text(&artifact)?;
                    write_warnings(&artifact)?;
                }
                OutputFormat::Markdown => {
                    write_markdown(&artifact)?;
                    write_warnings(&artifact)?;
                }
            }
        }
        Commands::Bench {
            pdf,
            ocr_sidecar,
            ocr_command,
            ocr_timeout_ms,
            cache_dir,
            eval_manifest: eval_manifest_path,
            eval_category,
            require_quality,
            require_baselines,
            require_baseline_quality,
            cache_probe,
            span_geometry,
            baseline,
            baseline_preset,
            baseline_timeout_ms,
            jobs,
        } => {
            let page_jobs = if pdf.is_dir() { 1 } else { jobs.max(1) };
            let options = ExtractionOptions {
                span_geometry,
                page_jobs,
            };
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_timeout_ms,
            )?;
            let requested_baseline_presets = baseline_preset_names(baseline_preset);
            let baseline_specs = baseline_specs_with_preset(&baseline, baseline_preset);
            let requirements = BenchmarkRequirements {
                require_quality,
                require_baselines,
                require_baseline_quality,
            };
            let bench_config = BenchRunConfig {
                ocr,
                cache_dir: cache_dir.as_deref(),
                cache_probe,
                jobs,
                extraction: options,
                baselines: &baseline_specs,
                requested_baseline_presets: &requested_baseline_presets,
                baseline_timeout: Duration::from_millis(baseline_timeout_ms),
                requirements,
            };
            let run_configuration = run_configuration(ocr, options);
            let baseline_quality = eval_manifest_path
                .as_deref()
                .map(|manifest| {
                    load_baseline_quality_expectations(manifest, eval_category.as_deref())
                })
                .transpose()?;
            if pdf.is_dir() {
                let mut output =
                    bench_corpus(backend, &pdf, bench_config, baseline_quality.as_ref())?;
                if let Some(manifest) = eval_manifest_path.as_deref() {
                    let quality = {
                        let artifacts_by_path = output
                            .documents
                            .iter()
                            .map(|document| {
                                (manifest_path_key(&document.source_path), &document.artifact)
                            })
                            .collect::<BTreeMap<_, _>>();
                        eval_manifest_from_artifacts(
                            benchmark_run_metadata(backend),
                            run_configuration,
                            manifest,
                            eval_category.as_deref(),
                            &artifacts_by_path,
                            EvalArtifactSelection::ExactManifest,
                        )?
                    };
                    output.category_summaries =
                        benchmark_category_summaries(&output.documents, &quality);
                    output.quality_status = BenchQualityStatus::Checked;
                    output.quality = Some(quality);
                }
                let failed_checks = output
                    .quality
                    .as_ref()
                    .map(|quality| quality.failed_checks)
                    .unwrap_or_default();
                write_json(&output)?;
                if failed_checks > 0 {
                    bail!("bench quality failed: {failed_checks} check(s) failed");
                }
                if require_quality && !matches!(output.quality_status, BenchQualityStatus::Checked)
                {
                    bail!("bench quality required: no eval manifest quality report was checked");
                }
                if require_baselines
                    && let Some(error) = corpus_baseline_requirement_error(&output.baselines)
                {
                    bail!("{error}");
                }
                if require_baseline_quality
                    && let Some(error) =
                        corpus_baseline_quality_requirement_error(&output.baselines)
                {
                    bail!("{error}");
                }
            } else {
                let mut output = bench_pdf(
                    backend,
                    &pdf,
                    bench_config,
                    baseline_quality
                        .as_ref()
                        .and_then(|quality| quality.get(&manifest_path_key(&pdf))),
                )?;
                if let Some(manifest) = eval_manifest_path.as_deref() {
                    let quality = {
                        let mut artifacts_by_path = BTreeMap::new();
                        artifacts_by_path.insert(manifest_path_key(&pdf), &output.artifact);
                        eval_manifest_from_artifacts(
                            benchmark_run_metadata(backend),
                            run_configuration,
                            manifest,
                            eval_category.as_deref(),
                            &artifacts_by_path,
                            EvalArtifactSelection::MatchingArtifacts,
                        )?
                    };
                    output.quality_status = BenchQualityStatus::Checked;
                    output.quality = Some(quality);
                }
                let failed_checks = output
                    .quality
                    .as_ref()
                    .map(|quality| quality.failed_checks)
                    .unwrap_or_default();
                write_json(&output)?;
                if failed_checks > 0 {
                    bail!("bench quality failed: {failed_checks} check(s) failed");
                }
                if require_quality && !matches!(output.quality_status, BenchQualityStatus::Checked)
                {
                    bail!("bench quality required: no eval manifest quality report was checked");
                }
                if require_baselines
                    && let Some(error) = baseline_requirement_error(&output.baselines)
                {
                    bail!("{error}");
                }
                if require_baseline_quality
                    && let Some(error) = baseline_quality_requirement_error(&output.baselines)
                {
                    bail!("{error}");
                }
            }
        }
        Commands::BaselineCheck {
            baseline,
            baseline_preset,
            pdf,
            baseline_timeout_ms,
            strict,
        } => {
            let requested_baseline_presets = baseline_preset_names(baseline_preset);
            let baseline_specs = baseline_specs_with_preset(&baseline, baseline_preset);
            let output = baseline_check(
                backend,
                &baseline_specs,
                &requested_baseline_presets,
                pdf.as_deref(),
                Duration::from_millis(baseline_timeout_ms),
                strict,
            );
            let error = baseline_check_error(&output);
            write_json(&output)?;
            if let Some(error) = error {
                bail!("{error}");
            }
        }
        Commands::BackendCheck { pdf, jobs } => {
            let output = backend_check_output(backend, pdf.as_deref(), jobs);
            let smoke_failed = output
                .smoke
                .as_ref()
                .map(|smoke| !smoke.success)
                .unwrap_or(false);
            write_json(&output)?;
            if smoke_failed {
                bail!("backend smoke failed");
            }
        }
        Commands::Manifest {
            pdf,
            category,
            coverage_preset,
            required_category,
            min_category_count,
            ocr_sidecar,
            ocr_command,
            ocr_timeout_ms,
            cache_dir,
            span_geometry,
            jobs,
        } => {
            let page_jobs = if pdf.is_dir() { 1 } else { jobs.max(1) };
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_timeout_ms,
            )?;
            let required_categories =
                manifest_required_categories_with_preset(&required_category, coverage_preset);
            let min_category_counts =
                manifest_min_category_counts_with_preset(&min_category_count, coverage_preset);
            let manifest = generate_eval_manifest(
                backend,
                &pdf,
                ManifestRunConfig {
                    category: category.as_deref(),
                    required_categories: &required_categories,
                    min_category_counts: &min_category_counts,
                    ocr,
                    cache_dir: cache_dir.as_deref(),
                    extraction: ExtractionOptions {
                        span_geometry,
                        page_jobs,
                    },
                    jobs,
                },
            )?;
            write_json(&manifest)?;
        }
        Commands::DebugPage {
            pdf,
            page_index,
            ocr_sidecar,
            ocr_command,
            ocr_timeout_ms,
            span_geometry,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_timeout_ms,
            )?;
            let fingerprint = document_fingerprint(&pdf)?;
            let open_start = Instant::now();
            let document = backend.load_document(&pdf)?;
            let open_us = open_start
                .elapsed()
                .as_micros()
                .max(1)
                .min(u64::MAX as u128) as u64;
            let document_page_count = backend.page_count(&document);
            let page = backend.extract_page(
                &document,
                &pdf,
                ocr,
                ExtractionOptions {
                    span_geometry,
                    page_jobs: 1,
                },
                page_index,
            )?;
            let artifact = parse_extracted_pages(fingerprint.clone(), vec![page]);
            let warnings = artifact.global_diagnostics.warnings.clone();
            let page = artifact
                .pages
                .into_iter()
                .next()
                .context("debug-page extraction returned no page artifact")?;
            let mut page = page;
            page.timings.open_us = open_us;
            let text_output = text_output_metrics_from_page(&page);
            let layout = layout_summary_from_page(&page);
            write_json(&DebugPageOutput {
                backend: backend.name(),
                metadata: document_metadata(backend, &pdf)?,
                document_fingerprint: fingerprint,
                artifact_id: page.artifact_id.clone(),
                page_fingerprint: page.fingerprint.as_hex().to_string(),
                document_page_count,
                extracted_page_count: 1,
                page_index,
                dimensions: page.dimensions.clone(),
                signals: page.signals.clone(),
                quality: page.quality.clone(),
                text_output,
                layout,
                timings: page.timings.clone(),
                image_artifacts: page.image_artifacts.clone(),
                warnings,
                decision: page.route.clone(),
            })?;
        }
        Commands::Eval {
            manifest,
            category,
            ocr_sidecar,
            ocr_command,
            ocr_timeout_ms,
            cache_dir,
            span_geometry,
            jobs,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_timeout_ms,
            )?;
            let output = eval_manifest(
                backend,
                &manifest,
                category.as_deref(),
                ocr,
                cache_dir.as_deref(),
                ExtractionOptions {
                    span_geometry,
                    page_jobs: 1,
                },
                jobs,
            )?;
            let passed = output.passed;
            let failed_checks = output.failed_checks;
            write_json(&output)?;
            if !passed {
                bail!("eval failed: {failed_checks} check(s) failed");
            }
        }
    }

    Ok(())
}

trait PdfBackend {
    type Document;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
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
}

struct LopdfBackend;

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
        extract_lopdf_pages(document, source_path, ocr, options)
    }

    fn extract_page(
        &self,
        document: &Self::Document,
        source_path: &Path,
        ocr: OcrOptions<'_>,
        options: ExtractionOptions,
        page_index: u32,
    ) -> Result<ExtractedPage> {
        extract_lopdf_page_by_index(document, source_path, ocr, options, page_index)
    }
}

fn inspect_corpus<B: PdfBackend>(backend: &B, path: &Path) -> Result<CorpusInspectOutput> {
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

fn inspect_corpus_pages<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    cache_dir: Option<&Path>,
    jobs: usize,
) -> Result<CorpusInspectPagesOutput> {
    let pdfs = discover_pdfs(path)?;
    let worker_count = jobs.max(1).min(pdfs.len().max(1));
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

fn inspect_corpus_pages_parallel<B: PdfBackend + Sync>(
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

fn inspect_corpus_pages_document<B: PdfBackend>(
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

fn inspect_pages<B: PdfBackend>(
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

fn inspect_page_summaries(
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
            timings: page.timings.clone(),
            warnings: warnings_for_page(warnings, page.page_index),
        })
        .collect()
}

fn warnings_for_page(warnings: &[String], page_index: u32) -> Vec<String> {
    let prefix = format!("p{page_index:06}:");
    warnings
        .iter()
        .filter(|warning| warning.starts_with(&prefix))
        .cloned()
        .collect()
}

fn bench_corpus<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BTreeMap<PathBuf, BaselineQualityExpectations>>,
) -> Result<CorpusBenchOutput> {
    if config.cache_probe && config.cache_dir.is_none() {
        bail!("--cache-probe requires --cache-dir");
    }

    let pdfs = discover_pdfs(path)?;
    let worker_count = config.jobs.max(1).min(pdfs.len().max(1));
    let documents = if worker_count == 1 {
        pdfs.into_iter()
            .map(|pdf| bench_corpus_document(backend, pdf, config, baseline_quality))
            .collect::<Result<Vec<_>>>()?
    } else {
        bench_corpus_parallel(backend, pdfs, config, baseline_quality, worker_count)?
    };
    let wall_us = documents.iter().map(|document| document.wall_us).sum();
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
    let baseline_outputs = aggregate_corpus_baselines(&documents, config.baselines, page_count);
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
        requirements: config.requirements,
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
        quality_status: BenchQualityStatus::NotCheckedNoEvalManifest,
        quality: None,
        cache_probe: cache_probe_output,
        documents,
    })
}

fn bench_corpus_parallel<B: PdfBackend + Sync>(
    backend: &B,
    pdfs: Vec<DiscoveredPdf>,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BTreeMap<PathBuf, BaselineQualityExpectations>>,
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

fn bench_corpus_document<B: PdfBackend>(
    backend: &B,
    pdf: DiscoveredPdf,
    config: BenchRunConfig<'_>,
    baseline_quality: Option<&BTreeMap<PathBuf, BaselineQualityExpectations>>,
) -> Result<CorpusBenchDocument> {
    let bench = bench_pdf(
        backend,
        &pdf.path,
        config,
        baseline_quality.and_then(|quality| quality.get(&manifest_path_key(&pdf.path))),
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

fn bench_pdf<B: PdfBackend>(
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
        requirements: config.requirements,
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
        quality_status: BenchQualityStatus::NotCheckedNoEvalManifest,
        quality: None,
        cache_probe: cache_probe_output,
        page_latencies_us,
        artifact,
    })
}

fn benchmark_run_metadata<B: PdfBackend>(backend: &B) -> BenchmarkRunMetadata {
    BenchmarkRunMetadata {
        parser_name: PARSER_NAME,
        parser_version: PARSER_VERSION,
        backend: backend.name(),
        backend_version: backend.version(),
    }
}

fn run_configuration(ocr: OcrOptions<'_>, options: ExtractionOptions) -> RunConfiguration {
    RunConfiguration {
        span_geometry: options.span_geometry,
        ocr_sidecar: ocr.sidecar.is_some(),
        ocr_command: ocr.command.is_some(),
        ocr_timeout_ms: duration_millis(ocr.timeout),
    }
}

fn run_cache_probe<B: PdfBackend>(
    backend: &B,
    path: &Path,
    ocr: OcrOptions<'_>,
    cache_dir: &Path,
    options: ExtractionOptions,
    cold: CacheProbeRunOutput,
) -> Result<CacheProbeOutput> {
    let warm_config = BenchRunConfig {
        ocr,
        cache_dir: Some(cache_dir),
        cache_probe: false,
        jobs: 1,
        extraction: options,
        baselines: &[],
        requested_baseline_presets: &[],
        baseline_timeout: Duration::from_millis(DEFAULT_BASELINE_TIMEOUT_MS),
        requirements: BenchmarkRequirements::default(),
    };
    let warm_bench = bench_pdf(backend, path, warm_config, None)?;
    let warm = cache_probe_run_from_bench(&warm_bench);

    Ok(CacheProbeOutput {
        cache_key_match: cold.cache_key == warm.cache_key,
        warm_speedup: speedup(cold.wall_us, warm.wall_us),
        cold,
        warm,
    })
}

fn cache_probe_run_from_artifact(
    artifact: &DocumentArtifact,
    wall_us: u128,
    artifact_bytes: u64,
    allocated_bytes: u64,
    peak_rss_bytes: u64,
) -> CacheProbeRunOutput {
    let page_count = artifact.pages.len();
    let text_output_metrics = text_output_metrics_from_artifact(artifact);

    CacheProbeRunOutput {
        cache_status: artifact.global_diagnostics.cache_status.clone(),
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
        stage_timings_us: stage_timings_from_artifact(artifact),
        page_latency_us: page_latency_from_artifact(artifact),
        route_counts: route_counts_from_artifact(artifact),
        route_latency_us: route_latency_from_artifact(artifact),
        route_reason_counts: route_reason_counts_from_artifact(artifact),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        image_artifact_count: image_artifact_count_from_artifact(artifact),
        image_artifact_pages: image_artifact_pages_from_artifact(artifact),
        quality_flag_counts: quality_flag_counts_from_artifact(artifact),
        fallback_action_counts: fallback_action_counts_from_artifact(artifact),
        warnings_count: artifact.global_diagnostics.warnings.len(),
        warnings: artifact.global_diagnostics.warnings.clone(),
        cache_key: artifact.global_diagnostics.cache_key.clone(),
    }
}

fn cache_probe_run_from_bench(bench: &BenchOutput) -> CacheProbeRunOutput {
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

fn aggregate_corpus_cache_probe(
    documents: &[CorpusBenchDocument],
    page_count: usize,
) -> Option<CorpusCacheProbeOutput> {
    let probes = documents
        .iter()
        .filter_map(|document| document.cache_probe.as_ref())
        .collect::<Vec<_>>();

    if probes.is_empty() {
        return None;
    }

    let cold_wall_us = probes.iter().map(|probe| probe.cold.wall_us).sum();
    let warm_wall_us = probes.iter().map(|probe| probe.warm.wall_us).sum();
    let cold_allocated_bytes = probes.iter().map(|probe| probe.cold.allocated_bytes).sum();
    let warm_allocated_bytes = probes.iter().map(|probe| probe.warm.allocated_bytes).sum();
    let cold_fallback_action_counts =
        probes
            .iter()
            .fold(FallbackActionCounts::default(), |mut counts, probe| {
                counts.add(probe.cold.fallback_action_counts);
                counts
            });
    let warm_fallback_action_counts =
        probes
            .iter()
            .fold(FallbackActionCounts::default(), |mut counts, probe| {
                counts.add(probe.warm.fallback_action_counts);
                counts
            });
    let cold_stage_timings_us =
        probes
            .iter()
            .fold(BenchStageTimings::default(), |mut timings, probe| {
                timings.add(probe.cold.stage_timings_us);
                timings
            });
    let warm_stage_timings_us =
        probes
            .iter()
            .fold(BenchStageTimings::default(), |mut timings, probe| {
                timings.add(probe.warm.stage_timings_us);
                timings
            });
    let cold_cache_misses = probes
        .iter()
        .filter(|probe| probe.cold.cache_status == CacheStatus::Miss)
        .count() as u32;
    let warm_cache_hits = probes
        .iter()
        .filter(|probe| probe.warm.cache_status == CacheStatus::Hit)
        .count() as u32;

    Some(CorpusCacheProbeOutput {
        document_count: probes.len(),
        cold_wall_us,
        warm_wall_us,
        cold_pages_per_sec: pages_per_sec(page_count, cold_wall_us),
        warm_pages_per_sec: pages_per_sec(page_count, warm_wall_us),
        cold_allocated_bytes,
        warm_allocated_bytes,
        cold_allocated_bytes_per_page: bytes_per_page(cold_allocated_bytes, page_count),
        warm_allocated_bytes_per_page: bytes_per_page(warm_allocated_bytes, page_count),
        cold_fallback_action_counts,
        warm_fallback_action_counts,
        cold_stage_timings_us,
        warm_stage_timings_us,
        warm_speedup: speedup(cold_wall_us, warm_wall_us),
        cold_cache_misses,
        warm_cache_hits,
    })
}

fn speedup(cold_wall_us: u128, warm_wall_us: u128) -> f64 {
    if warm_wall_us == 0 {
        return 0.0;
    }

    cold_wall_us as f64 / warm_wall_us as f64
}

fn allocated_bytes_total(include_worker_threads: bool) -> u64 {
    if include_worker_threads {
        ALLOCATED_BYTES_TOTAL.load(Ordering::Relaxed)
    } else {
        ALLOCATED_BYTES_THREAD.with(Cell::get)
    }
}

fn bytes_per_page(bytes: u64, page_count: usize) -> f64 {
    if page_count == 0 {
        return 0.0;
    }

    bytes as f64 / page_count as f64
}

fn byte_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }

    numerator as f64 / denominator as f64
}

fn baseline_comparison(
    glyphrush_wall_us: u128,
    baseline_wall_us: u128,
    glyphrush_text_output_bytes: u64,
    baseline_output_bytes: u64,
    speed_comparable: bool,
) -> BaselineComparisonOutput {
    BaselineComparisonOutput {
        speed_comparable,
        glyphrush_wall_us,
        baseline_wall_us,
        glyphrush_speedup: if speed_comparable {
            speedup(baseline_wall_us, glyphrush_wall_us)
        } else {
            0.0
        },
        baseline_speedup: if speed_comparable {
            speedup(glyphrush_wall_us, baseline_wall_us)
        } else {
            0.0
        },
        glyphrush_text_output_bytes,
        baseline_output_bytes,
        baseline_to_glyphrush_output_bytes: byte_ratio(
            baseline_output_bytes,
            glyphrush_text_output_bytes,
        ),
    }
}

impl QualityFlagCounts {
    fn add(&mut self, other: QualityFlagCounts) {
        self.requires_ocr += other.requires_ocr;
        self.low_confidence_text += other.low_confidence_text;
        self.broken_encoding += other.broken_encoding;
        self.layout_uncertain += other.layout_uncertain;
        self.table_uncertain += other.table_uncertain;
        self.unsupported_feature += other.unsupported_feature;
    }
}

impl FallbackActionCounts {
    fn add(&mut self, other: FallbackActionCounts) {
        self.ocr_requested_pages += other.ocr_requested_pages;
        self.ocr_applied_pages += other.ocr_applied_pages;
        self.heavy_layout_pages += other.heavy_layout_pages;
        self.table_recovery_pages += other.table_recovery_pages;
        self.render_pages += other.render_pages;
    }
}

fn quality_flag_counts_from_artifact(artifact: &DocumentArtifact) -> QualityFlagCounts {
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

fn image_artifact_count_from_artifact(artifact: &DocumentArtifact) -> u32 {
    artifact
        .pages
        .iter()
        .map(|page| page.image_artifacts.len() as u32)
        .sum()
}

fn image_artifact_pages_from_artifact(artifact: &DocumentArtifact) -> u32 {
    artifact
        .pages
        .iter()
        .filter(|page| !page.image_artifacts.is_empty())
        .count() as u32
}

fn fallback_action_counts_from_artifact(artifact: &DocumentArtifact) -> FallbackActionCounts {
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
    fn add(&mut self, other: RouteCounts) {
        self.native_fast_path += other.native_fast_path;
        self.needs_fallback += other.needs_fallback;
        self.ocr_fallback += other.ocr_fallback;
        self.unsupported += other.unsupported;
    }
}

fn route_counts_from_artifact(artifact: &DocumentArtifact) -> RouteCounts {
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

fn route_reason_counts_from_artifact(artifact: &DocumentArtifact) -> BTreeMap<String, u32> {
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

fn add_reason_counts(target: &mut BTreeMap<String, u32>, source: &BTreeMap<String, u32>) {
    for (reason, count) in source {
        *target.entry(reason.clone()).or_default() += count;
    }
}

impl BenchStageTimings {
    fn add(&mut self, other: BenchStageTimings) {
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

fn stage_timings_from_artifact(artifact: &DocumentArtifact) -> BenchStageTimings {
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

fn page_latency_from_artifact(artifact: &DocumentArtifact) -> PageLatencySummary {
    page_latency_from_values(page_latencies_from_artifact(artifact))
}

fn page_latency_from_documents(documents: &[CorpusBenchDocument]) -> PageLatencySummary {
    let values = documents
        .iter()
        .flat_map(|document| document.page_latencies_us.iter().copied())
        .collect::<Vec<_>>();

    page_latency_from_values(values)
}

fn route_latency_from_artifact(artifact: &DocumentArtifact) -> RouteLatencySummary {
    route_latency_from_pages(artifact.pages.iter())
}

fn route_latency_from_documents(documents: &[CorpusBenchDocument]) -> RouteLatencySummary {
    route_latency_from_pages(
        documents
            .iter()
            .flat_map(|document| document.artifact.pages.iter()),
    )
}

fn route_latency_from_pages<'a>(
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

fn page_latencies_from_artifact(artifact: &DocumentArtifact) -> Vec<u64> {
    artifact
        .pages
        .iter()
        .map(|page| page.timings.total_us())
        .collect()
}

fn page_latency_from_values(mut values: Vec<u64>) -> PageLatencySummary {
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

fn percentile_us(sorted_values: &[u64], percentile: f64) -> u64 {
    let last_index = sorted_values.len().saturating_sub(1);
    let index = (last_index as f64 * percentile).ceil() as usize;

    sorted_values[index.min(last_index)]
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn peak_rss_bytes() -> u64 {
    getrusage_maxrss().unwrap_or_default()
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))
))]
fn peak_rss_bytes() -> u64 {
    getrusage_maxrss()
        .map(|maxrss_kb| maxrss_kb.saturating_mul(1024))
        .unwrap_or_default()
}

#[cfg(target_os = "freebsd")]
fn peak_rss_bytes() -> u64 {
    getrusage_maxrss().unwrap_or_default()
}

#[cfg(unix)]
fn getrusage_maxrss() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    let usage = unsafe { usage.assume_init() };
    (usage.ru_maxrss >= 0).then_some(usage.ru_maxrss as u64)
}

#[cfg(not(unix))]
fn peak_rss_bytes() -> u64 {
    0
}

fn run_external_baselines(
    path: &Path,
    baselines: &[BaselineSpec],
    baseline_quality: Option<&BaselineQualityExpectations>,
    glyphrush_wall_us: u128,
    glyphrush_text_output_bytes: u64,
    timeout: Duration,
) -> Vec<BaselineBenchOutput> {
    baselines
        .iter()
        .map(|baseline| {
            run_external_baseline(
                path,
                baseline,
                baseline_quality,
                glyphrush_wall_us,
                glyphrush_text_output_bytes,
                timeout,
            )
        })
        .collect()
}

fn baseline_requirement_error(baselines: &[BaselineBenchOutput]) -> Option<String> {
    if baselines.is_empty() {
        return Some("bench baselines required: no baselines were requested".to_string());
    }

    let failed = baselines
        .iter()
        .filter(|baseline| !baseline.success)
        .count();
    (failed > 0).then(|| format!("bench baselines required: {failed} baseline run(s) failed"))
}

fn corpus_baseline_requirement_error(baselines: &[CorpusBaselineBenchOutput]) -> Option<String> {
    if baselines.is_empty() {
        return Some("bench baselines required: no baselines were requested".to_string());
    }

    let failed = baselines
        .iter()
        .map(|baseline| baseline.failed_documents)
        .sum::<usize>();
    (failed > 0)
        .then(|| format!("bench baselines required: {failed} baseline document run(s) failed"))
}

fn baseline_quality_requirement_error(baselines: &[BaselineBenchOutput]) -> Option<String> {
    if baselines.is_empty() {
        return Some("bench baseline quality required: no baselines were requested".to_string());
    }

    let unchecked = baselines
        .iter()
        .filter(|baseline| !matches!(baseline.quality_status, BaselineQualityStatus::Checked))
        .count();
    if unchecked > 0 {
        return Some(format!(
            "bench baseline quality required: {unchecked} baseline run(s) were not quality-checked"
        ));
    }

    let failed = baselines
        .iter()
        .filter(|baseline| {
            baseline
                .quality
                .as_ref()
                .is_some_and(|quality| !quality.passed)
        })
        .count();
    (failed > 0).then(|| {
        format!("bench baseline quality required: {failed} baseline quality run(s) failed")
    })
}

fn corpus_baseline_quality_requirement_error(
    baselines: &[CorpusBaselineBenchOutput],
) -> Option<String> {
    if baselines.is_empty() {
        return Some("bench baseline quality required: no baselines were requested".to_string());
    }

    let unchecked = baselines
        .iter()
        .map(|baseline| baseline.quality_unchecked_documents)
        .sum::<usize>();
    if unchecked > 0 {
        return Some(format!(
            "bench baseline quality required: {unchecked} baseline document run(s) were not quality-checked"
        ));
    }

    let failed = baselines
        .iter()
        .map(|baseline| baseline.quality_failed_documents)
        .sum::<usize>();
    (failed > 0).then(|| {
        format!("bench baseline quality required: {failed} baseline quality document run(s) failed")
    })
}

fn baseline_check<B: PdfBackend>(
    backend: &B,
    baselines: &[BaselineSpec],
    requested_baseline_presets: &[&'static str],
    smoke_pdf: Option<&Path>,
    timeout: Duration,
    strict: bool,
) -> BaselineCheckOutput {
    let smoke_targets = smoke_pdf.map(baseline_smoke_targets);
    let baselines = baselines
        .iter()
        .map(|baseline| check_external_baseline(baseline, smoke_targets.as_ref(), timeout))
        .collect::<Vec<_>>();
    let describe_success_count = baselines
        .iter()
        .filter(|baseline| baseline.describe.success)
        .count();
    let smoke_success_count = smoke_pdf.map(|_| {
        baselines
            .iter()
            .filter(|baseline| baseline.smoke.as_ref().is_some_and(|smoke| smoke.success))
            .count()
    });

    BaselineCheckOutput {
        report_version: BASELINE_CHECK_REPORT_VERSION,
        run_metadata: benchmark_run_metadata(backend),
        strict,
        requested_baseline_presets: requested_baseline_presets.to_vec(),
        baseline_count: baselines.len(),
        describe_success_count,
        all_described: !baselines.is_empty() && describe_success_count == baselines.len(),
        smoke_pdf: smoke_pdf.map(|path| path.to_string_lossy().into_owned()),
        smoke_document_count: smoke_targets
            .as_ref()
            .and_then(|targets| targets.as_ref().ok())
            .map(Vec::len),
        smoke_success_count,
        all_smoke_passed: smoke_success_count
            .map(|count| !baselines.is_empty() && count == baselines.len()),
        baselines,
    }
}

fn baseline_check_error(output: &BaselineCheckOutput) -> Option<String> {
    if output.baseline_count == 0 {
        return Some("baseline-check requires at least one --baseline".to_string());
    }

    baseline_check_strict_error(output)
}

fn baseline_check_strict_error(output: &BaselineCheckOutput) -> Option<String> {
    if !output.strict {
        return None;
    }

    if !output.all_described {
        return Some(format!(
            "baseline-check strict failed: {}/{} baseline describe probe(s) passed",
            output.describe_success_count, output.baseline_count
        ));
    }

    if output.all_smoke_passed == Some(false) {
        return Some(format!(
            "baseline-check strict failed: {}/{} baseline smoke probe(s) passed",
            output.smoke_success_count.unwrap_or_default(),
            output.baseline_count
        ));
    }

    None
}

fn check_external_baseline(
    baseline: &BaselineSpec,
    smoke_targets: Option<&Result<Vec<DiscoveredPdf>>>,
    timeout: Duration,
) -> BaselineCheckResult {
    let (description, describe) = describe_external_baseline_probe(baseline, timeout);
    let smoke =
        smoke_targets.map(|targets| smoke_external_baseline_probe(baseline, targets, timeout));

    BaselineCheckResult {
        name: baseline.name.clone(),
        command: baseline.command.to_string_lossy().into_owned(),
        description,
        describe,
        smoke,
    }
}

fn baseline_smoke_targets(path: &Path) -> Result<Vec<DiscoveredPdf>> {
    if path.is_dir() {
        discover_pdfs(path)
    } else {
        Ok(vec![DiscoveredPdf {
            path: path.to_path_buf(),
            label: path.to_string_lossy().into_owned(),
        }])
    }
}

fn describe_external_baseline_probe(
    baseline: &BaselineSpec,
    timeout: Duration,
) -> (Option<Value>, BaselineDescribeCheck) {
    let mut command = ProcessCommand::new(&baseline.command);
    command.arg("--describe");
    let timeout_ms = duration_millis(timeout);
    let result = command_output_with_timeout(command, timeout);

    match result {
        Ok(timed_output) => {
            let output = timed_output.output;
            let description = if !timed_output.timed_out && output.status.success() {
                serde_json::from_slice::<Value>(&output.stdout)
                    .ok()
                    .filter(Value::is_object)
            } else {
                None
            };
            let valid_json_object = description.is_some();
            let success = !timed_output.timed_out && output.status.success() && valid_json_object;
            let error = baseline_describe_error(&output, timed_output.timed_out, valid_json_object);

            (
                description,
                BaselineDescribeCheck {
                    success,
                    exit_status: output.status.code(),
                    timed_out: timed_output.timed_out,
                    timeout_ms,
                    wall_us: timed_output.wall_us,
                    stdout_bytes: output.stdout.len() as u64,
                    stderr_bytes: output.stderr.len() as u64,
                    stderr_preview: stderr_preview(&output.stderr),
                    valid_json_object,
                    error,
                },
            )
        }
        Err(error) => (
            None,
            BaselineDescribeCheck {
                success: false,
                exit_status: None,
                timed_out: false,
                timeout_ms,
                wall_us: 0,
                stdout_bytes: 0,
                stderr_bytes: 0,
                stderr_preview: None,
                valid_json_object: false,
                error: Some(format!("{}: {error}", baseline.command.display())),
            },
        ),
    }
}

fn baseline_describe_error(
    output: &ProcessOutput,
    timed_out: bool,
    valid_json_object: bool,
) -> Option<String> {
    if timed_out {
        Some("baseline describe timed out".to_string())
    } else if !output.status.success() {
        Some(format!(
            "baseline describe exited with status {:?}",
            output.status.code()
        ))
    } else if output.stdout.is_empty() {
        Some("baseline describe produced no stdout".to_string())
    } else if !valid_json_object {
        Some("baseline describe stdout was not a JSON object".to_string())
    } else {
        None
    }
}

fn smoke_external_baseline_probe(
    baseline: &BaselineSpec,
    targets: &Result<Vec<DiscoveredPdf>>,
    timeout: Duration,
) -> BaselineSmokeCheck {
    let targets = match targets {
        Ok(targets) => targets,
        Err(error) => {
            return BaselineSmokeCheck {
                success: false,
                exit_status: None,
                timed_out: false,
                timeout_ms: duration_millis(timeout),
                wall_us: 0,
                output_bytes: 0,
                stdout_sha256: None,
                stdout_line_count: 0,
                stdout_word_count: 0,
                stderr_bytes: 0,
                empty_output: false,
                stderr_preview: None,
                error: Some(error.to_string()),
                document_count: Some(0),
                successful_documents: Some(0),
                failed_documents: Some(0),
                documents: Vec::new(),
            };
        }
    };

    let documents = targets
        .iter()
        .map(|target| smoke_external_baseline_document_probe(baseline, target, timeout))
        .collect::<Vec<_>>();

    if documents.len() == 1 {
        return baseline_smoke_check_from_document(&documents[0]);
    }

    let successful_documents = documents.iter().filter(|document| document.success).count();
    let failed_documents = documents.len().saturating_sub(successful_documents);
    let output_bytes = documents.iter().map(|document| document.output_bytes).sum();
    let stdout_line_count = documents
        .iter()
        .map(|document| document.stdout_line_count)
        .sum();
    let stdout_word_count = documents
        .iter()
        .map(|document| document.stdout_word_count)
        .sum();
    let stderr_bytes = documents.iter().map(|document| document.stderr_bytes).sum();
    let wall_us = documents.iter().map(|document| document.wall_us).sum();
    let timed_out = documents.iter().any(|document| document.timed_out);
    let empty_output = successful_documents > 0
        && documents
            .iter()
            .filter(|document| document.success)
            .all(|document| document.empty_output);
    let stderr_preview = documents
        .iter()
        .find_map(|document| document.stderr_preview.clone());
    let error = documents.iter().find_map(|document| document.error.clone());

    BaselineSmokeCheck {
        success: failed_documents == 0,
        exit_status: None,
        timed_out,
        timeout_ms: duration_millis(timeout),
        wall_us,
        output_bytes,
        stdout_sha256: None,
        stdout_line_count,
        stdout_word_count,
        stderr_bytes,
        empty_output,
        stderr_preview,
        error,
        document_count: Some(documents.len()),
        successful_documents: Some(successful_documents),
        failed_documents: Some(failed_documents),
        documents,
    }
}

fn smoke_external_baseline_document_probe(
    baseline: &BaselineSpec,
    target: &DiscoveredPdf,
    timeout: Duration,
) -> BaselineSmokeDocument {
    let mut command = ProcessCommand::new(&baseline.command);
    command.arg(&target.path);
    let timeout_ms = duration_millis(timeout);
    let result = command_output_with_timeout(command, timeout);

    match result {
        Ok(timed_output) => {
            let output = timed_output.output;
            let success = output.status.success() && !timed_output.timed_out;
            BaselineSmokeDocument {
                path: target.label.clone(),
                success,
                exit_status: output.status.code(),
                timed_out: timed_output.timed_out,
                timeout_ms,
                wall_us: timed_output.wall_us,
                output_bytes: output.stdout.len() as u64,
                stdout_sha256: Some(stdout_sha256(&output.stdout)),
                stdout_line_count: stdout_line_count(&output.stdout),
                stdout_word_count: stdout_word_count(&output.stdout),
                stderr_bytes: output.stderr.len() as u64,
                empty_output: output.status.success() && output.stdout.is_empty(),
                stderr_preview: stderr_preview(&output.stderr),
                error: baseline_smoke_error(&output, timed_output.timed_out),
            }
        }
        Err(error) => BaselineSmokeDocument {
            path: target.label.clone(),
            success: false,
            exit_status: None,
            timed_out: false,
            timeout_ms,
            wall_us: 0,
            output_bytes: 0,
            stdout_sha256: None,
            stdout_line_count: 0,
            stdout_word_count: 0,
            stderr_bytes: 0,
            empty_output: false,
            stderr_preview: None,
            error: Some(format!("{}: {error}", baseline.command.display())),
        },
    }
}

fn baseline_smoke_check_from_document(document: &BaselineSmokeDocument) -> BaselineSmokeCheck {
    BaselineSmokeCheck {
        success: document.success,
        exit_status: document.exit_status,
        timed_out: document.timed_out,
        timeout_ms: document.timeout_ms,
        wall_us: document.wall_us,
        output_bytes: document.output_bytes,
        stdout_sha256: document.stdout_sha256.clone(),
        stdout_line_count: document.stdout_line_count,
        stdout_word_count: document.stdout_word_count,
        stderr_bytes: document.stderr_bytes,
        empty_output: document.empty_output,
        stderr_preview: document.stderr_preview.clone(),
        error: document.error.clone(),
        document_count: None,
        successful_documents: None,
        failed_documents: None,
        documents: Vec::new(),
    }
}

fn baseline_smoke_error(output: &ProcessOutput, timed_out: bool) -> Option<String> {
    if timed_out {
        Some("baseline smoke timed out".to_string())
    } else if !output.status.success() {
        Some(format!(
            "baseline smoke exited with status {:?}",
            output.status.code()
        ))
    } else {
        None
    }
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn run_external_baseline(
    path: &Path,
    baseline: &BaselineSpec,
    baseline_quality: Option<&BaselineQualityExpectations>,
    glyphrush_wall_us: u128,
    glyphrush_text_output_bytes: u64,
    timeout: Duration,
) -> BaselineBenchOutput {
    let description = describe_external_baseline(baseline, timeout);
    let mut command_process = ProcessCommand::new(&baseline.command);
    command_process.arg(path);
    let result = command_output_with_timeout(command_process, timeout);
    let command = baseline.command.to_string_lossy().into_owned();

    match result {
        Ok(timed_output) => {
            let output = timed_output.output;
            let wall_us = timed_output.wall_us;
            let success = output.status.success() && !timed_output.timed_out;
            let quality_status =
                baseline_quality_status(baseline_quality, timed_output.timed_out, success);
            let quality = baseline_quality
                .filter(|_| success)
                .map(|expectations| baseline_quality_from_stdout(&output.stdout, expectations));
            let output_bytes = output.stdout.len() as u64;
            BaselineBenchOutput {
                name: baseline.name.clone(),
                command,
                description,
                comparison: baseline_comparison(
                    glyphrush_wall_us,
                    wall_us,
                    glyphrush_text_output_bytes,
                    output_bytes,
                    success,
                ),
                success,
                exit_status: output.status.code(),
                timed_out: timed_output.timed_out,
                timeout_ms: timeout.as_millis().min(u64::MAX as u128) as u64,
                wall_us,
                output_bytes,
                stdout_sha256: Some(stdout_sha256(&output.stdout)),
                stdout_line_count: stdout_line_count(&output.stdout),
                stdout_word_count: stdout_word_count(&output.stdout),
                stderr_bytes: output.stderr.len() as u64,
                empty_output: output.status.success() && output.stdout.is_empty(),
                stderr_preview: stderr_preview(&output.stderr),
                error: timed_output
                    .timed_out
                    .then(|| format!("baseline timed out after {} ms", timeout.as_millis())),
                quality_status,
                quality,
            }
        }
        Err(error) => BaselineBenchOutput {
            name: baseline.name.clone(),
            command,
            description,
            comparison: baseline_comparison(
                glyphrush_wall_us,
                0,
                glyphrush_text_output_bytes,
                0,
                false,
            ),
            success: false,
            exit_status: None,
            timed_out: false,
            timeout_ms: timeout.as_millis().min(u64::MAX as u128) as u64,
            wall_us: 0,
            output_bytes: 0,
            stdout_sha256: None,
            stdout_line_count: 0,
            stdout_word_count: 0,
            stderr_bytes: 0,
            empty_output: false,
            stderr_preview: None,
            error: Some(error.to_string()),
            quality_status: baseline_quality
                .map(|_| BaselineQualityStatus::NotCheckedExecutionFailed)
                .unwrap_or(BaselineQualityStatus::NotCheckedNoExpectations),
            quality: None,
        },
    }
}

fn baseline_quality_status(
    baseline_quality: Option<&BaselineQualityExpectations>,
    timed_out: bool,
    success: bool,
) -> BaselineQualityStatus {
    match (baseline_quality.is_some(), timed_out, success) {
        (true, false, true) => BaselineQualityStatus::Checked,
        (true, true, _) => BaselineQualityStatus::NotCheckedTimedOut,
        (true, false, false) => BaselineQualityStatus::NotCheckedExecutionFailed,
        (false, _, _) => BaselineQualityStatus::NotCheckedNoExpectations,
    }
}

struct TimedProcessOutput {
    output: ProcessOutput,
    timed_out: bool,
    wall_us: u128,
}

fn command_output_with_timeout(
    mut command: ProcessCommand,
    timeout: Duration,
) -> io::Result<TimedProcessOutput> {
    let start = Instant::now();
    configure_timeout_command(&mut command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    loop {
        if start.elapsed() >= timeout {
            kill_timed_out_child(&mut child);
            let output = child.wait_with_output()?;
            return Ok(TimedProcessOutput {
                output,
                timed_out: true,
                wall_us: start.elapsed().as_micros(),
            });
        }

        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            return Ok(TimedProcessOutput {
                output,
                timed_out: false,
                wall_us: start.elapsed().as_micros(),
            });
        }

        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn configure_timeout_command(command: &mut ProcessCommand) {
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
}

#[cfg(not(unix))]
fn configure_timeout_command(_command: &mut ProcessCommand) {}

fn kill_timed_out_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pgid = child.id() as libc::pid_t;
        if pgid > 0 {
            let killed_group = unsafe { libc::kill(-pgid, libc::SIGKILL) } == 0;
            if killed_group {
                return;
            }
        }
    }

    let _ = child.kill();
}

fn describe_external_baseline(baseline: &BaselineSpec, timeout: Duration) -> Option<Value> {
    describe_external_baseline_probe(baseline, timeout).0
}

fn load_baseline_quality_expectations(
    manifest_path: &Path,
    category: Option<&str>,
) -> Result<BTreeMap<PathBuf, BaselineQualityExpectations>> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read eval manifest {}", manifest_path.display()))?;
    let manifest: EvalManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode eval manifest {}", manifest_path.display()))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let category = normalize_manifest_category(category);
    let mut expectations_by_path = BTreeMap::new();

    for document in manifest.documents {
        if let Some(category) = category.as_deref()
            && eval_manifest_document_category(&document) != category
        {
            continue;
        }
        let required_text = baseline_required_text_expectations(&document.expect);
        if required_text.is_empty()
            && document.expect.text_recall.is_none()
            && document.expect.reading_order.is_none()
            && document.expect.table_structure.is_empty()
        {
            continue;
        }

        let category = normalize_manifest_category(document.category.as_deref());
        expectations_by_path.insert(
            manifest_path_key(&resolve_manifest_path(manifest_dir, &document.path)),
            BaselineQualityExpectations {
                category,
                required_text,
                text_recall: document.expect.text_recall,
                reading_order: document.expect.reading_order,
                table_structure: document.expect.table_structure,
            },
        );
    }

    Ok(expectations_by_path)
}

fn baseline_required_text_expectations(expectations: &EvalExpectations) -> Vec<String> {
    let mut required_text = Vec::new();
    for text in &expectations.required_text {
        if !required_text.contains(text) {
            required_text.push(text.clone());
        }
    }
    for page in &expectations.pages {
        for text in &page.required_text {
            if !required_text.contains(text) {
                required_text.push(text.clone());
            }
        }
    }

    required_text
}

fn generate_eval_manifest<B: PdfBackend + Sync>(
    backend: &B,
    path: &Path,
    config: ManifestRunConfig<'_>,
) -> Result<GeneratedEvalManifest> {
    let category = normalize_manifest_category(config.category);
    let required_categories = normalize_required_categories(config.required_categories, None);
    let min_category_counts = min_category_counts_from_specs(config.min_category_counts);
    let documents = if path.is_dir() {
        let pdfs = discover_pdfs(path)?;
        let worker_count = config.jobs.max(1).min(pdfs.len().max(1));
        if worker_count == 1 {
            pdfs.into_iter()
                .map(|pdf| {
                    generated_manifest_document(
                        backend,
                        &pdf.path,
                        pdf.label,
                        config.ocr,
                        config.cache_dir,
                        config.extraction,
                        category.as_deref(),
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
            ocr_timeout_ms: duration_millis(config.ocr.timeout),
        },
        corpus_fingerprint,
        required_categories,
        min_category_counts,
        documents,
    })
}

fn generate_manifest_documents_parallel<B: PdfBackend + Sync>(
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
                        generated_manifest_document(
                            backend, &pdf.path, pdf.label, ocr, cache_dir, options, category,
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

fn generated_manifest_document<B: PdfBackend>(
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

fn generated_manifest_expectations(artifact: &DocumentArtifact) -> GeneratedManifestExpectations {
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
        silent_failures: GeneratedSilentFailuresExpectation { max_count: 0 },
        pages: artifact
            .pages
            .iter()
            .map(generated_page_expectation)
            .collect(),
    }
}

fn generated_quality_flag_classification(
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

fn expected_pages_for_quality(artifact: &DocumentArtifact, flag: PageQuality) -> Vec<u32> {
    artifact
        .pages
        .iter()
        .filter(|page| page.quality.flags.contains(&flag))
        .map(|page| page.page_index)
        .collect()
}

fn generated_page_expectation(page: &PageArtifact) -> GeneratedPageExpectation {
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

fn generated_page_required_text(page: &PageArtifact) -> Vec<String> {
    const MAX_ANCHOR_CHARS: usize = 160;

    let page_text = quality_text_from_page(page);
    let fallback_line = page_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty());

    page_text
        .lines()
        .map(str::trim)
        .find(|line| is_substantive_required_text_anchor(line))
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

fn is_substantive_required_text_anchor(line: &str) -> bool {
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

fn manifest_path_key(path: &Path) -> PathBuf {
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

fn baseline_quality_from_stdout(
    stdout: &[u8],
    expectations: &BaselineQualityExpectations,
) -> BaselineQualityOutput {
    let actual_text = String::from_utf8_lossy(stdout);
    let required_text = (!expectations.required_text.is_empty()).then(|| {
        let missing = expectations
            .required_text
            .iter()
            .filter(|text| !actual_text.contains(text.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        BaselineRequiredTextOutput {
            passed: missing.is_empty(),
            expected: expectations.required_text.clone(),
            missing,
        }
    });
    let text_recall = expectations.text_recall.as_ref().map(|expectation| {
        let word_recall = multiset_recall(
            normalize_words(&expectation.expected),
            normalize_words(&actual_text),
        );
        let char_recall = multiset_recall(
            normalize_chars(&expectation.expected),
            normalize_chars(&actual_text),
        );
        let min_word_recall = expectation.min_word_recall.unwrap_or(1.0);
        let min_char_recall = expectation.min_char_recall.unwrap_or(1.0);
        let missing_words = missing_multiset_items(
            normalize_words(&expectation.expected),
            normalize_words(&actual_text),
        );
        BaselineTextRecallOutput {
            passed: word_recall >= min_word_recall && char_recall >= min_char_recall,
            word_recall,
            char_recall,
            missing_words,
            min_word_recall,
            min_char_recall,
        }
    });
    let reading_order = expectations
        .reading_order
        .as_ref()
        .map(|expectation| baseline_reading_order_from_text(&actual_text, expectation));
    let table_structure = (!expectations.table_structure.is_empty()).then(|| {
        expectations
            .table_structure
            .iter()
            .map(|expectation| baseline_table_structure_from_text(&actual_text, expectation))
            .collect::<Vec<_>>()
    });
    let failed_checks = u32::from(required_text.as_ref().is_some_and(|check| !check.passed))
        + u32::from(text_recall.as_ref().is_some_and(|check| !check.passed))
        + u32::from(reading_order.as_ref().is_some_and(|check| !check.passed))
        + table_structure
            .as_ref()
            .map(|checks| checks.iter().filter(|check| !check.passed).count() as u32)
            .unwrap_or_default();

    BaselineQualityOutput {
        category: expectations.category.clone(),
        passed: failed_checks == 0,
        failed_checks,
        required_text,
        text_recall,
        reading_order,
        table_structure,
    }
}

fn baseline_reading_order_from_text(
    actual_text: &str,
    expectation: &ReadingOrderExpectation,
) -> BaselineReadingOrderOutput {
    let positions = expectation
        .expected_sequence
        .iter()
        .map(|snippet| actual_text.find(snippet))
        .collect::<Vec<_>>();
    let matched = expectation
        .expected_sequence
        .iter()
        .zip(positions.iter())
        .filter_map(|(snippet, position)| {
            position.map(|position| ReadingOrderMatch {
                snippet: snippet.clone(),
                position,
            })
        })
        .collect::<Vec<_>>();
    let missing = expectation
        .expected_sequence
        .iter()
        .zip(positions.iter())
        .filter(|(_, position)| position.is_none())
        .map(|(snippet, _)| snippet.clone())
        .collect::<Vec<_>>();
    let (score, inversion_count, inversions) =
        reading_order_score(&expectation.expected_sequence, &positions);
    let min_score = expectation.min_score.unwrap_or(1.0);

    BaselineReadingOrderOutput {
        passed: score >= min_score,
        score,
        matched,
        missing,
        inversion_count,
        inversions,
        min_score,
    }
}

fn baseline_table_structure_from_text(
    actual_text: &str,
    expectation: &TableStructureExpectation,
) -> BaselineTableStructureOutput {
    let expected_rows = normalize_table_rows(&expectation.expected_rows);
    let actual_rows = parse_table_rows(actual_text);
    let missing_rows = missing_multiset_items(expected_rows.clone(), actual_rows.clone());
    let extra_rows = missing_multiset_items(actual_rows.clone(), expected_rows.clone());
    let expected_cells = table_cells(&expected_rows);
    let actual_cells = table_cells(&actual_rows);
    let missing_cells = missing_multiset_items(expected_cells.clone(), actual_cells.clone());
    let extra_cells = missing_multiset_items(actual_cells.clone(), expected_cells.clone());
    let row_recall = ratio(
        expected_rows.len().saturating_sub(missing_rows.len()),
        expected_rows.len(),
    );
    let row_precision = ratio(
        actual_rows.len().saturating_sub(extra_rows.len()),
        actual_rows.len(),
    );
    let row_f1 = f1(row_precision, row_recall);
    let cell_recall = ratio(
        expected_cells.len().saturating_sub(missing_cells.len()),
        expected_cells.len(),
    );
    let cell_precision = ratio(
        actual_cells.len().saturating_sub(extra_cells.len()),
        actual_cells.len(),
    );
    let cell_f1 = f1(cell_precision, cell_recall);
    let min_row_precision = expectation.min_row_precision.unwrap_or(0.0);
    let min_row_recall = expectation.min_row_recall.unwrap_or(1.0);
    let min_row_f1 = expectation.min_row_f1.unwrap_or(0.0);
    let min_cell_precision = expectation.min_cell_precision.unwrap_or(0.0);
    let min_cell_recall = expectation.min_cell_recall.unwrap_or(1.0);
    let min_cell_f1 = expectation.min_cell_f1.unwrap_or(0.0);
    let passed = row_precision >= min_row_precision
        && row_recall >= min_row_recall
        && row_f1 >= min_row_f1
        && cell_precision >= min_cell_precision
        && cell_recall >= min_cell_recall
        && cell_f1 >= min_cell_f1;

    BaselineTableStructureOutput {
        page: expectation.page,
        passed,
        extracted_rows: actual_rows,
        row_precision,
        row_recall,
        row_f1,
        missing_rows,
        extra_rows,
        cell_precision,
        cell_recall,
        cell_f1,
        missing_cells,
        extra_cells,
        min_row_precision,
        min_row_recall,
        min_row_f1,
        min_cell_precision,
        min_cell_recall,
        min_cell_f1,
    }
}

fn stdout_sha256(stdout: &[u8]) -> String {
    sha256_hex(stdout)
}

fn stdout_line_count(stdout: &[u8]) -> usize {
    String::from_utf8_lossy(stdout).lines().count()
}

fn stdout_word_count(stdout: &[u8]) -> usize {
    String::from_utf8_lossy(stdout).split_whitespace().count()
}

fn text_output_metrics_from_artifact(artifact: &DocumentArtifact) -> TextOutputMetrics {
    let text = plain_text_from_artifact(artifact);
    text_output_metrics_from_text(&text)
}

fn text_output_metrics_from_page(page: &PageArtifact) -> TextOutputMetrics {
    let text = plain_text_from_page(page);
    text_output_metrics_from_text(&text)
}

fn text_output_metrics_from_text(text: &str) -> TextOutputMetrics {
    TextOutputMetrics {
        bytes: text.len() as u64,
        line_count: text.lines().count(),
        word_count: text.split_whitespace().count(),
        empty: text.is_empty(),
    }
}

fn layout_summary_from_page(page: &PageArtifact) -> DebugLayoutSummary {
    let mut summary = DebugLayoutSummary {
        block_count: page.layout_blocks.len(),
        ..DebugLayoutSummary::default()
    };

    for block in &page.layout_blocks {
        match block.kind {
            LayoutBlockKind::Paragraph => summary.paragraph_blocks += 1,
            LayoutBlockKind::Heading => summary.heading_blocks += 1,
            LayoutBlockKind::List => summary.list_blocks += 1,
            LayoutBlockKind::Table => summary.table_blocks += 1,
            LayoutBlockKind::Figure => summary.figure_blocks += 1,
            LayoutBlockKind::Header => summary.header_blocks += 1,
            LayoutBlockKind::Footer => summary.footer_blocks += 1,
        }
    }

    summary
}

fn empty_text_output_page_count_from_artifact(artifact: &DocumentArtifact) -> usize {
    artifact
        .pages
        .iter()
        .filter(|page| plain_text_from_page(page).is_empty())
        .count()
}

fn stderr_preview(stderr: &[u8]) -> Option<String> {
    const MAX_CHARS: usize = 500;

    (!stderr.is_empty()).then(|| {
        String::from_utf8_lossy(stderr)
            .chars()
            .take(MAX_CHARS)
            .collect()
    })
}

fn aggregate_corpus_baselines(
    documents: &[CorpusBenchDocument],
    baselines: &[BaselineSpec],
    page_count: usize,
) -> Vec<CorpusBaselineBenchOutput> {
    baselines
        .iter()
        .map(|baseline| {
            let baseline_command = baseline.command_label();
            let runs = documents
                .iter()
                .filter_map(|document| {
                    document
                        .baselines
                        .iter()
                        .find(|run| run.name == baseline.name && run.command == baseline_command)
                        .map(|run| (document, run))
                })
                .collect::<Vec<_>>();
            let wall_us = runs.iter().map(|(_, run)| run.wall_us).sum();
            let glyphrush_wall_us = runs
                .iter()
                .map(|(document, _)| document.wall_us)
                .sum::<u128>();
            let output_bytes = runs.iter().map(|(_, run)| run.output_bytes).sum();
            let glyphrush_text_output_bytes = runs
                .iter()
                .map(|(document, _)| document.text_output_bytes)
                .sum::<u64>();
            let stderr_bytes = runs.iter().map(|(_, run)| run.stderr_bytes).sum();
            let successful_documents = runs.iter().filter(|(_, run)| run.success).count();
            let successful_pages = runs
                .iter()
                .filter(|(_, run)| run.success)
                .map(|(document, _)| document.page_count)
                .sum();
            let failed_pages = runs
                .iter()
                .filter(|(_, run)| !run.success)
                .map(|(document, _)| document.page_count)
                .sum();
            let timed_out_documents = runs.iter().filter(|(_, run)| run.timed_out).count();
            let timed_out_pages = runs
                .iter()
                .filter(|(_, run)| run.timed_out)
                .map(|(document, _)| document.page_count)
                .sum();
            let empty_output_documents = runs.iter().filter(|(_, run)| run.empty_output).count();
            let empty_output_pages = runs
                .iter()
                .filter(|(_, run)| run.empty_output)
                .map(|(document, _)| document.page_count)
                .sum();
            let success_rate = if runs.is_empty() {
                0.0
            } else {
                successful_documents as f64 / runs.len() as f64
            };
            let failed_documents = runs.len().saturating_sub(successful_documents);
            let quality_documents = runs.iter().filter(|(_, run)| run.quality.is_some()).count();
            let quality_unchecked_documents = runs.len().saturating_sub(quality_documents);
            let quality_status = corpus_baseline_quality_status(&runs, quality_documents);
            let quality_passed_documents = runs
                .iter()
                .filter(|(_, run)| run.quality.as_ref().is_some_and(|quality| quality.passed))
                .count();
            let quality_failed_documents =
                quality_documents.saturating_sub(quality_passed_documents);
            let quality_failed_checks = runs
                .iter()
                .filter_map(|(_, run)| run.quality.as_ref())
                .map(|quality| quality.failed_checks)
                .sum();
            let quality_required_text_failed_documents = runs
                .iter()
                .filter(|(_, run)| {
                    run.quality
                        .as_ref()
                        .and_then(|quality| quality.required_text.as_ref())
                        .is_some_and(|check| !check.passed)
                })
                .count();
            let quality_text_recall_failed_documents = runs
                .iter()
                .filter(|(_, run)| {
                    run.quality
                        .as_ref()
                        .and_then(|quality| quality.text_recall.as_ref())
                        .is_some_and(|check| !check.passed)
                })
                .count();
            let quality_reading_order_failed_documents = runs
                .iter()
                .filter(|(_, run)| {
                    run.quality
                        .as_ref()
                        .and_then(|quality| quality.reading_order.as_ref())
                        .is_some_and(|check| !check.passed)
                })
                .count();
            let quality_table_structure_failed_documents = runs
                .iter()
                .filter(|(_, run)| {
                    run.quality
                        .as_ref()
                        .and_then(|quality| quality.table_structure.as_ref())
                        .is_some_and(|checks| checks.iter().any(|check| !check.passed))
                })
                .count();
            let quality_pass_rate = if quality_documents == 0 {
                0.0
            } else {
                quality_passed_documents as f64 / quality_documents as f64
            };
            let quality_category_summaries = runs
                .iter()
                .filter_map(|(document, run)| {
                    let quality = run.quality.as_ref()?;
                    Some((document, quality))
                })
                .fold(BTreeMap::new(), |mut summaries, (document, quality)| {
                    summaries
                        .entry(baseline_quality_category(quality).to_string())
                        .or_insert_with(CorpusBaselineQualityCategorySummary::default)
                        .add_document(document.page_count, quality);
                    summaries
                });
            let failure_samples = runs
                .iter()
                .filter(|(_, run)| !run.success)
                .take(3)
                .map(|(document, run)| CorpusBaselineFailureSample {
                    path: document.path.clone(),
                    exit_status: run.exit_status,
                    error: run.error.clone(),
                    stderr_preview: run.stderr_preview.clone(),
                })
                .collect();
            let quality_failure_samples = runs
                .iter()
                .filter_map(|(document, run)| {
                    let quality = run.quality.as_ref()?;
                    (!quality.passed).then(|| CorpusBaselineQualityFailureSample {
                        path: document.path.clone(),
                        failed_checks: quality.failed_checks,
                        failed_check_types: baseline_quality_failed_check_types(quality),
                    })
                })
                .take(3)
                .collect();
            let description = runs.iter().find_map(|(_, run)| run.description.clone());

            CorpusBaselineBenchOutput {
                name: baseline.name.clone(),
                command: baseline_command,
                description,
                comparison: baseline_comparison(
                    glyphrush_wall_us,
                    wall_us,
                    glyphrush_text_output_bytes,
                    output_bytes,
                    !runs.is_empty() && failed_documents == 0,
                ),
                document_count: runs.len(),
                successful_documents,
                failed_documents,
                timed_out_documents,
                successful_pages,
                failed_pages,
                timed_out_pages,
                empty_output_documents,
                empty_output_pages,
                success_rate,
                quality_status,
                quality_documents,
                quality_unchecked_documents,
                quality_passed_documents,
                quality_failed_documents,
                quality_failed_checks,
                quality_required_text_failed_documents,
                quality_text_recall_failed_documents,
                quality_reading_order_failed_documents,
                quality_table_structure_failed_documents,
                quality_category_summaries,
                quality_pass_rate,
                failure_samples,
                quality_failure_samples,
                wall_us,
                pages_per_sec: pages_per_sec(page_count, wall_us),
                successful_pages_per_sec: pages_per_sec(successful_pages, wall_us),
                output_bytes,
                stderr_bytes,
            }
        })
        .collect()
}

fn corpus_baseline_quality_status(
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

fn corpus_fingerprint<'a>(
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

fn baseline_quality_failed_check_types(quality: &BaselineQualityOutput) -> Vec<&'static str> {
    let mut failed_check_types = Vec::new();

    if quality
        .required_text
        .as_ref()
        .is_some_and(|check| !check.passed)
    {
        failed_check_types.push("required_text");
    }
    if quality
        .text_recall
        .as_ref()
        .is_some_and(|check| !check.passed)
    {
        failed_check_types.push("text_recall");
    }
    if quality
        .reading_order
        .as_ref()
        .is_some_and(|check| !check.passed)
    {
        failed_check_types.push("reading_order");
    }
    if quality
        .table_structure
        .as_ref()
        .is_some_and(|checks| checks.iter().any(|check| !check.passed))
    {
        failed_check_types.push("table_structure");
    }

    failed_check_types
}

fn baseline_quality_category(quality: &BaselineQualityOutput) -> &str {
    quality.category.as_deref().unwrap_or("uncategorized")
}

fn benchmark_category_summaries(
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

fn eval_document_failed_checks(document: &EvalDocumentOutput) -> u32 {
    document
        .checks
        .values()
        .filter(|check| !check.passed)
        .count() as u32
}

impl BaselineSpec {
    fn command_label(&self) -> String {
        self.command.to_string_lossy().into_owned()
    }
}

fn eval_manifest<B: PdfBackend + Sync>(
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
    let category = normalize_manifest_category(category);
    let required_categories =
        normalize_required_categories(&manifest.required_categories, category.as_deref());
    let min_category_counts =
        normalize_min_category_counts(&manifest.min_category_counts, category.as_deref());
    if let Some(category) = category.as_deref() {
        manifest
            .documents
            .retain(|document| eval_manifest_document_category(document) == category);
    }
    let worker_count = jobs.max(1).min(manifest.documents.len().max(1));

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

fn eval_documents_parallel<B: PdfBackend + Sync>(
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
enum EvalArtifactSelection {
    ExactManifest,
    MatchingArtifacts,
}

fn eval_manifest_from_artifacts(
    run_metadata: BenchmarkRunMetadata,
    run_configuration: RunConfiguration,
    manifest_path: &Path,
    category: Option<&str>,
    artifacts_by_path: &BTreeMap<PathBuf, &DocumentArtifact>,
    selection: EvalArtifactSelection,
) -> Result<EvalOutput> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read eval manifest {}", manifest_path.display()))?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);
    let manifest: EvalManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode eval manifest {}", manifest_path.display()))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let category = normalize_manifest_category(category);
    let required_categories =
        normalize_required_categories(&manifest.required_categories, category.as_deref());
    let min_category_counts =
        normalize_min_category_counts(&manifest.min_category_counts, category.as_deref());

    let mut selected_document_count = 0usize;
    let mut documents = Vec::new();
    for document in manifest.documents {
        if let Some(category) = category.as_deref()
            && eval_manifest_document_category(&document) != category
        {
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
        documents.push(eval_document_from_artifact(document, artifact));
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

fn eval_output_from_documents(
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

fn eval_document<B: PdfBackend>(
    backend: &B,
    manifest_dir: &Path,
    document: EvalManifestDocument,
    ocr: OcrOptions<'_>,
    cache_dir: Option<&Path>,
    options: ExtractionOptions,
) -> Result<EvalDocumentOutput> {
    let pdf_path = resolve_manifest_path(manifest_dir, &document.path);
    let artifact = parse_pdf(backend, &pdf_path, ocr, cache_dir, options)?;
    Ok(eval_document_from_artifact(document, &artifact))
}

fn eval_document_from_artifact(
    document: EvalManifestDocument,
    artifact: &DocumentArtifact,
) -> EvalDocumentOutput {
    let mut checks = BTreeMap::new();

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
    if let Some(expected) = document.expect.page_count {
        insert_check(&mut checks, "page_count", expected, artifact.pages.len());
    }
    if let Some(expected) = document.expect.fallback_pages {
        insert_check(
            &mut checks,
            "fallback_pages",
            expected,
            artifact.global_diagnostics.fallback_pages,
        );
    }
    if let Some(expected) = document.expect.ocr_required_pages {
        insert_check(
            &mut checks,
            "ocr_required_pages",
            expected,
            artifact.global_diagnostics.ocr_required_pages,
        );
    }
    if let Some(expected) = document.expect.ocr_applied_pages {
        insert_check(
            &mut checks,
            "ocr_applied_pages",
            expected,
            artifact.global_diagnostics.ocr_applied_pages,
        );
    }
    if let Some(expected) = document.expect.image_artifact_count {
        insert_check(
            &mut checks,
            "image_artifact_count",
            expected,
            image_artifact_count_from_artifact(artifact),
        );
    }
    if let Some(expected) = document.expect.warnings_count {
        insert_check(
            &mut checks,
            "warnings_count",
            expected,
            artifact.global_diagnostics.warnings.len(),
        );
    }
    if let Some(expected) = document.expect.route_counts {
        insert_check(
            &mut checks,
            "route_counts",
            expected,
            route_counts_from_artifact(artifact),
        );
    }
    if let Some(expected) = document.expect.quality_flag_counts {
        insert_check(
            &mut checks,
            "quality_flag_counts",
            expected,
            quality_flag_counts_from_artifact(artifact),
        );
    }
    if !document.expect.route_reason_counts.is_empty() {
        insert_check(
            &mut checks,
            "route_reason_counts",
            document.expect.route_reason_counts.clone(),
            route_reason_counts_from_artifact(artifact),
        );
    }
    if !document.expect.required_text.is_empty() {
        insert_required_text_check(&mut checks, &document.expect.required_text, artifact);
    }
    if !document.expect.required_warnings.is_empty() {
        insert_required_warnings_check(&mut checks, &document.expect.required_warnings, artifact);
    }
    if let Some(expectation) = &document.expect.text_recall {
        insert_text_recall_check(&mut checks, expectation, artifact);
    }
    if let Some(expectation) = &document.expect.reading_order {
        insert_reading_order_check(&mut checks, expectation, artifact);
    }
    if let Some(expectation) = &document.expect.ocr_required_classification {
        insert_ocr_required_classification_check(&mut checks, expectation, artifact);
    }
    if let Some(expectation) = &document.expect.silent_failures {
        insert_silent_failures_check(&mut checks, expectation, &document.expect, artifact);
    }
    for expectation in &document.expect.quality_flag_classification {
        insert_quality_flag_classification_check(&mut checks, expectation, artifact);
    }
    for expectation in &document.expect.table_structure {
        insert_table_structure_check(&mut checks, expectation, artifact);
    }
    for (index, expectation) in document.expect.span_bbox.iter().enumerate() {
        insert_span_bbox_check(&mut checks, index, expectation, artifact);
    }
    for page_expectation in &document.expect.pages {
        insert_page_expectation_checks(&mut checks, page_expectation, artifact);
    }

    let passed = checks.values().all(|check| check.passed);
    let category = normalize_manifest_category(document.category.as_deref());

    EvalDocumentOutput {
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
    }
}

fn eval_document_category(document: &EvalDocumentOutput) -> &str {
    document.category.as_deref().unwrap_or("uncategorized")
}

fn eval_manifest_document_category(document: &EvalManifestDocument) -> &str {
    document
        .category
        .as_deref()
        .map(str::trim)
        .filter(|category| !category.is_empty())
        .unwrap_or("uncategorized")
}

fn normalize_manifest_category(category: Option<&str>) -> Option<String> {
    category
        .map(str::trim)
        .filter(|category| !category.is_empty())
        .map(str::to_string)
}

fn normalize_required_categories(categories: &[String], filter: Option<&str>) -> Vec<String> {
    let filter = normalize_manifest_category(filter);
    let mut categories = categories
        .iter()
        .filter_map(|category| normalize_manifest_category(Some(category)))
        .filter(|category| {
            filter
                .as_deref()
                .is_none_or(|filter| category.as_str() == filter)
        })
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();
    categories
}

fn min_category_counts_from_specs(specs: &[CategoryCountSpec]) -> BTreeMap<String, usize> {
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

fn normalize_min_category_counts(
    categories: &BTreeMap<String, usize>,
    filter: Option<&str>,
) -> BTreeMap<String, usize> {
    let filter = normalize_manifest_category(filter);
    categories
        .iter()
        .filter_map(|(category, count)| {
            let category = normalize_manifest_category(Some(category))?;
            if *count == 0 {
                return None;
            }
            if filter
                .as_deref()
                .is_some_and(|filter| category.as_str() != filter)
            {
                return None;
            }
            Some((category, *count))
        })
        .fold(BTreeMap::new(), |mut counts, (category, count)| {
            counts
                .entry(category)
                .and_modify(|existing| *existing = (*existing).max(count))
                .or_insert(count);
            counts
        })
}

fn category_coverage(
    required: Vec<String>,
    min_category_counts: BTreeMap<String, usize>,
    category_counts: &BTreeMap<String, usize>,
) -> Option<CategoryCoverageOutput> {
    if required.is_empty() && min_category_counts.is_empty() {
        return None;
    }

    let present = category_counts.keys().cloned().collect::<Vec<_>>();
    let missing = required
        .iter()
        .filter(|category| !category_counts.contains_key(category.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let under_minimum = min_category_counts
        .iter()
        .filter_map(|(category, required)| {
            let actual = category_counts.get(category).copied().unwrap_or_default();
            (actual < *required).then(|| {
                (
                    category.clone(),
                    CategoryMinimumCoverageOutput {
                        required: *required,
                        actual,
                    },
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let passed = missing.is_empty() && under_minimum.is_empty();

    Some(CategoryCoverageOutput {
        required,
        present,
        missing,
        min_category_counts,
        under_minimum,
        passed,
    })
}

fn resolve_manifest_path(manifest_dir: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        manifest_dir.join(path)
    }
}

fn insert_check<T>(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    name: &str,
    expected: T,
    actual: T,
) where
    T: PartialEq + Serialize,
{
    let passed = expected == actual;
    checks.insert(
        name.to_string(),
        EvalCheckOutput {
            passed,
            expected: json!(expected),
            actual: json!(actual),
        },
    );
}

fn insert_json_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    name: String,
    expected: serde_json::Value,
    actual: serde_json::Value,
) {
    checks.insert(
        name,
        EvalCheckOutput {
            passed: expected == actual,
            expected,
            actual,
        },
    );
}

fn insert_required_text_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    required_text: &[String],
    artifact: &DocumentArtifact,
) {
    let document_text = document_text(artifact);
    let missing = required_text
        .iter()
        .filter(|text| !document_text.contains(text.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    checks.insert(
        "required_text".to_string(),
        EvalCheckOutput {
            passed: missing.is_empty(),
            expected: json!(required_text),
            actual: json!({ "missing": missing }),
        },
    );
}

fn insert_required_warnings_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    required_warnings: &[String],
    artifact: &DocumentArtifact,
) {
    let warnings = &artifact.global_diagnostics.warnings;
    let missing = required_warnings
        .iter()
        .filter(|warning| !warnings.contains(warning))
        .cloned()
        .collect::<Vec<_>>();
    checks.insert(
        "required_warnings".to_string(),
        EvalCheckOutput {
            passed: missing.is_empty(),
            expected: json!(required_warnings),
            actual: json!({
                "warnings": warnings,
                "missing": missing,
            }),
        },
    );
}

fn insert_text_recall_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &TextRecallExpectation,
    artifact: &DocumentArtifact,
) {
    let actual_text = document_text(artifact);
    let word_recall = multiset_recall(
        normalize_words(&expectation.expected),
        normalize_words(&actual_text),
    );
    let char_recall = multiset_recall(
        normalize_chars(&expectation.expected),
        normalize_chars(&actual_text),
    );
    let min_word_recall = expectation.min_word_recall.unwrap_or(1.0);
    let min_char_recall = expectation.min_char_recall.unwrap_or(1.0);
    let expected_words = normalize_words(&expectation.expected);
    let actual_words = normalize_words(&actual_text);
    let missing_words = missing_multiset_items(expected_words, actual_words);
    let passed = word_recall >= min_word_recall && char_recall >= min_char_recall;

    checks.insert(
        "text_recall".to_string(),
        EvalCheckOutput {
            passed,
            expected: json!({
                "min_word_recall": min_word_recall,
                "min_char_recall": min_char_recall,
            }),
            actual: json!({
                "word_recall": word_recall,
                "char_recall": char_recall,
                "missing_words": missing_words,
            }),
        },
    );
}

fn insert_reading_order_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &ReadingOrderExpectation,
    artifact: &DocumentArtifact,
) {
    let actual_text = document_text(artifact);
    let positions = expectation
        .expected_sequence
        .iter()
        .map(|snippet| actual_text.find(snippet))
        .collect::<Vec<_>>();
    let matched = expectation
        .expected_sequence
        .iter()
        .zip(positions.iter())
        .filter_map(|(snippet, position)| {
            position.map(|position| ReadingOrderMatch {
                snippet: snippet.clone(),
                position,
            })
        })
        .collect::<Vec<_>>();
    let missing = expectation
        .expected_sequence
        .iter()
        .zip(positions.iter())
        .filter(|(_, position)| position.is_none())
        .map(|(snippet, _)| snippet.clone())
        .collect::<Vec<_>>();
    let (score, inversion_count, inversions) =
        reading_order_score(&expectation.expected_sequence, &positions);
    let min_score = expectation.min_score.unwrap_or(1.0);
    let passed = score >= min_score;

    checks.insert(
        "reading_order".to_string(),
        EvalCheckOutput {
            passed,
            expected: json!({
                "expected_sequence": expectation.expected_sequence,
                "min_score": min_score,
            }),
            actual: json!({
                "score": score,
                "matched": matched,
                "missing": missing,
                "inversion_count": inversion_count,
                "inversions": inversions,
            }),
        },
    );
}

fn reading_order_score(
    expected_sequence: &[String],
    positions: &[Option<usize>],
) -> (f64, usize, Vec<ReadingOrderInversion>) {
    if expected_sequence.len() < 2 {
        let score = if positions.iter().all(Option::is_some) {
            1.0
        } else {
            0.0
        };
        return (score, 0, Vec::new());
    }

    let mut ordered_pairs = 0usize;
    let mut inversion_count = 0usize;
    let mut inversions = Vec::new();
    let mut total_pairs = 0usize;

    for left_index in 0..expected_sequence.len() {
        for right_index in (left_index + 1)..expected_sequence.len() {
            total_pairs += 1;
            match (positions[left_index], positions[right_index]) {
                (Some(left_position), Some(right_position)) if left_position <= right_position => {
                    ordered_pairs += 1;
                }
                (Some(_), Some(_)) => {
                    inversion_count += 1;
                    inversions.push(ReadingOrderInversion {
                        before: expected_sequence[left_index].clone(),
                        after: expected_sequence[right_index].clone(),
                    });
                }
                _ => {}
            }
        }
    }

    (
        ordered_pairs as f64 / total_pairs as f64,
        inversion_count,
        inversions,
    )
}

fn insert_ocr_required_classification_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &OcrRequiredClassificationExpectation,
    artifact: &DocumentArtifact,
) {
    let expected_pages = expectation
        .expected_pages
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_pages = artifact
        .pages
        .iter()
        .filter(|page| page.quality.flags.contains(&PageQuality::RequiresOcr))
        .map(|page| page.page_index)
        .collect::<BTreeSet<_>>();

    let true_positive_pages = expected_pages
        .intersection(&actual_pages)
        .copied()
        .collect::<Vec<_>>();
    let false_positive_pages = actual_pages
        .difference(&expected_pages)
        .copied()
        .collect::<Vec<_>>();
    let false_negative_pages = expected_pages
        .difference(&actual_pages)
        .copied()
        .collect::<Vec<_>>();
    let precision = classification_precision(true_positive_pages.len(), actual_pages.len());
    let recall = classification_recall(true_positive_pages.len(), expected_pages.len());
    let min_precision = expectation.min_precision.unwrap_or(1.0);
    let min_recall = expectation.min_recall.unwrap_or(1.0);
    let passed = precision >= min_precision && recall >= min_recall;

    checks.insert(
        "ocr_required_classification".to_string(),
        EvalCheckOutput {
            passed,
            expected: json!({
                "expected_pages": expected_pages.into_iter().collect::<Vec<_>>(),
                "min_precision": min_precision,
                "min_recall": min_recall,
            }),
            actual: json!({
                "actual_pages": actual_pages.into_iter().collect::<Vec<_>>(),
                "precision": precision,
                "recall": recall,
                "true_positive_pages": true_positive_pages,
                "false_positive_pages": false_positive_pages,
                "false_negative_pages": false_negative_pages,
            }),
        },
    );
}

fn insert_quality_flag_classification_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &QualityFlagClassificationExpectation,
    artifact: &DocumentArtifact,
) {
    let expected_pages = expectation
        .expected_pages
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_pages = artifact
        .pages
        .iter()
        .filter(|page| page.quality.flags.contains(&expectation.flag))
        .map(|page| page.page_index)
        .collect::<BTreeSet<_>>();

    let true_positive_pages = expected_pages
        .intersection(&actual_pages)
        .copied()
        .collect::<Vec<_>>();
    let false_positive_pages = actual_pages
        .difference(&expected_pages)
        .copied()
        .collect::<Vec<_>>();
    let false_negative_pages = expected_pages
        .difference(&actual_pages)
        .copied()
        .collect::<Vec<_>>();
    let precision = classification_precision(true_positive_pages.len(), actual_pages.len());
    let recall = classification_recall(true_positive_pages.len(), expected_pages.len());
    let min_precision = expectation.min_precision.unwrap_or(1.0);
    let min_recall = expectation.min_recall.unwrap_or(1.0);
    let passed = precision >= min_precision && recall >= min_recall;
    let flag = page_quality_name(&expectation.flag);

    checks.insert(
        format!("quality_flag_classification.{flag}"),
        EvalCheckOutput {
            passed,
            expected: json!({
                "flag": flag,
                "expected_pages": expected_pages.into_iter().collect::<Vec<_>>(),
                "min_precision": min_precision,
                "min_recall": min_recall,
            }),
            actual: json!({
                "actual_pages": actual_pages.into_iter().collect::<Vec<_>>(),
                "precision": precision,
                "recall": recall,
                "true_positive_pages": true_positive_pages,
                "false_positive_pages": false_positive_pages,
                "false_negative_pages": false_negative_pages,
            }),
        },
    );
}

fn insert_silent_failures_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &SilentFailuresExpectation,
    eval_expectations: &EvalExpectations,
    artifact: &DocumentArtifact,
) {
    let expected_flags = expected_quality_flags_by_page(eval_expectations);
    let expected_empty_text_pages = expected_empty_text_output_pages(eval_expectations);
    let pages = artifact
        .pages
        .iter()
        .filter_map(|page| {
            let expected = expected_flags
                .get(&page.page_index)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let flags = page
                .quality
                .flags
                .iter()
                .filter(|flag| !expected.contains(flag))
                .map(page_quality_name)
                .collect::<Vec<_>>();
            let unexpected_empty_text_output = plain_text_from_page(page).is_empty()
                && !expected_empty_text_pages.contains(&page.page_index)
                && !expected.contains(&PageQuality::RequiresOcr);

            (!flags.is_empty() || unexpected_empty_text_output).then_some(SilentFailurePage {
                page: page.page_index,
                flags,
                empty_text_output: unexpected_empty_text_output.then_some(true),
            })
        })
        .collect::<Vec<_>>();
    let max_count = expectation.max_count.unwrap_or(0);
    let count = pages.len();

    checks.insert(
        "silent_failures".to_string(),
        EvalCheckOutput {
            passed: count <= max_count,
            expected: json!({ "max_count": max_count }),
            actual: json!({
                "count": count,
                "pages": pages,
            }),
        },
    );
}

fn expected_empty_text_output_pages(expectations: &EvalExpectations) -> BTreeSet<u32> {
    expectations
        .pages
        .iter()
        .filter(|page| page.empty_text_output == Some(true))
        .map(|page| page.index)
        .collect()
}

fn expected_quality_flags_by_page(
    expectations: &EvalExpectations,
) -> BTreeMap<u32, Vec<PageQuality>> {
    let mut flags_by_page: BTreeMap<u32, Vec<PageQuality>> = BTreeMap::new();

    for page in &expectations.pages {
        for flag in &page.required_flags {
            insert_expected_quality_flag(&mut flags_by_page, page.index, flag.clone());
        }
    }

    if let Some(expectation) = &expectations.ocr_required_classification {
        for page_index in &expectation.expected_pages {
            insert_expected_quality_flag(&mut flags_by_page, *page_index, PageQuality::RequiresOcr);
        }
    }

    for expectation in &expectations.quality_flag_classification {
        for page_index in &expectation.expected_pages {
            insert_expected_quality_flag(&mut flags_by_page, *page_index, expectation.flag.clone());
        }
    }

    for expectation in &expectations.table_structure {
        insert_expected_quality_flag(
            &mut flags_by_page,
            expectation.page,
            PageQuality::TableUncertain,
        );
    }

    flags_by_page
}

fn insert_expected_quality_flag(
    flags_by_page: &mut BTreeMap<u32, Vec<PageQuality>>,
    page_index: u32,
    flag: PageQuality,
) {
    let flags = flags_by_page.entry(page_index).or_default();
    if !flags.contains(&flag) {
        flags.push(flag);
    }
}

fn page_quality_name(flag: &PageQuality) -> &'static str {
    match flag {
        PageQuality::RequiresOcr => "requires_ocr",
        PageQuality::LowConfidenceText => "low_confidence_text",
        PageQuality::BrokenEncoding => "broken_encoding",
        PageQuality::LayoutUncertain => "layout_uncertain",
        PageQuality::TableUncertain => "table_uncertain",
        PageQuality::UnsupportedFeature => "unsupported_feature",
    }
}

fn insert_table_structure_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &TableStructureExpectation,
    artifact: &DocumentArtifact,
) {
    let expected_rows = normalize_table_rows(&expectation.expected_rows);
    let actual_rows = table_rows_for_page(artifact, expectation.page);
    let missing_rows = missing_multiset_items(expected_rows.clone(), actual_rows.clone());
    let extra_rows = missing_multiset_items(actual_rows.clone(), expected_rows.clone());
    let expected_cells = table_cells(&expected_rows);
    let actual_cells = table_cells(&actual_rows);
    let missing_cells = missing_multiset_items(expected_cells.clone(), actual_cells.clone());
    let extra_cells = missing_multiset_items(actual_cells.clone(), expected_cells.clone());
    let row_recall = ratio(
        expected_rows.len().saturating_sub(missing_rows.len()),
        expected_rows.len(),
    );
    let row_precision = ratio(
        actual_rows.len().saturating_sub(extra_rows.len()),
        actual_rows.len(),
    );
    let row_f1 = f1(row_precision, row_recall);
    let cell_recall = ratio(
        expected_cells.len().saturating_sub(missing_cells.len()),
        expected_cells.len(),
    );
    let cell_precision = ratio(
        actual_cells.len().saturating_sub(extra_cells.len()),
        actual_cells.len(),
    );
    let cell_f1 = f1(cell_precision, cell_recall);
    let min_row_precision = expectation.min_row_precision.unwrap_or(0.0);
    let min_row_recall = expectation.min_row_recall.unwrap_or(1.0);
    let min_row_f1 = expectation.min_row_f1.unwrap_or(0.0);
    let min_cell_precision = expectation.min_cell_precision.unwrap_or(0.0);
    let min_cell_recall = expectation.min_cell_recall.unwrap_or(1.0);
    let min_cell_f1 = expectation.min_cell_f1.unwrap_or(0.0);
    let passed = row_precision >= min_row_precision
        && row_recall >= min_row_recall
        && row_f1 >= min_row_f1
        && cell_precision >= min_cell_precision
        && cell_recall >= min_cell_recall
        && cell_f1 >= min_cell_f1;

    checks.insert(
        format!("table_structure.page_{:06}", expectation.page),
        EvalCheckOutput {
            passed,
            expected: json!({
                "page": expectation.page,
                "expected_rows": expected_rows,
                "min_row_precision": min_row_precision,
                "min_row_recall": min_row_recall,
                "min_row_f1": min_row_f1,
                "min_cell_precision": min_cell_precision,
                "min_cell_recall": min_cell_recall,
                "min_cell_f1": min_cell_f1,
            }),
            actual: json!({
                "extracted_rows": actual_rows,
                "row_precision": row_precision,
                "row_recall": row_recall,
                "row_f1": row_f1,
                "missing_rows": missing_rows,
                "extra_rows": extra_rows,
                "cell_precision": cell_precision,
                "cell_recall": cell_recall,
                "cell_f1": cell_f1,
                "missing_cells": missing_cells,
                "extra_cells": extra_cells,
            }),
        },
    );
}

fn insert_span_bbox_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    index: usize,
    expectation: &SpanBBoxExpectation,
    artifact: &DocumentArtifact,
) {
    let candidates = artifact
        .pages
        .iter()
        .find(|page| page.page_index == expectation.page)
        .map(|page| {
            page.native_spans
                .iter()
                .chain(page.ocr_spans.iter())
                .filter(|span| span.text.contains(&expectation.text))
                .filter(|span| {
                    expectation
                        .provenance
                        .as_ref()
                        .is_none_or(|provenance| &span.provenance == provenance)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let matched_span = candidates
        .iter()
        .copied()
        .find(|span| bbox_bound_failures(&span.bbox, expectation).is_empty());
    let reported_span = matched_span.or_else(|| candidates.first().copied());
    let bound_failures = reported_span
        .map(|span| bbox_bound_failures(&span.bbox, expectation))
        .unwrap_or_else(|| vec!["missing_span".to_string()]);
    let passed = matched_span.is_some();

    checks.insert(
        format!("span_bbox.{index:06}"),
        EvalCheckOutput {
            passed,
            expected: json!({
                "page": expectation.page,
                "text": &expectation.text,
                "provenance": &expectation.provenance,
                "bounds": span_bbox_bounds(expectation),
            }),
            actual: json!({
                "matched": passed,
                "candidate_count": candidates.len(),
                "span": reported_span.map(span_bbox_sample),
                "bound_failures": bound_failures,
            }),
        },
    );
}

fn span_bbox_sample(span: &TextSpan) -> serde_json::Value {
    json!({
        "text": &span.text,
        "provenance": &span.provenance,
        "bbox": &span.bbox,
    })
}

fn span_bbox_bounds(expectation: &SpanBBoxExpectation) -> serde_json::Value {
    json!({
        "min_x0": expectation.min_x0,
        "max_x0": expectation.max_x0,
        "min_y0": expectation.min_y0,
        "max_y0": expectation.max_y0,
        "min_x1": expectation.min_x1,
        "max_x1": expectation.max_x1,
        "min_y1": expectation.min_y1,
        "max_y1": expectation.max_y1,
    })
}

fn bbox_bound_failures(bbox: &BBox, expectation: &SpanBBoxExpectation) -> Vec<String> {
    let mut failures = Vec::new();

    push_min_bound_failure(&mut failures, "x0", bbox.x0, expectation.min_x0);
    push_max_bound_failure(&mut failures, "x0", bbox.x0, expectation.max_x0);
    push_min_bound_failure(&mut failures, "y0", bbox.y0, expectation.min_y0);
    push_max_bound_failure(&mut failures, "y0", bbox.y0, expectation.max_y0);
    push_min_bound_failure(&mut failures, "x1", bbox.x1, expectation.min_x1);
    push_max_bound_failure(&mut failures, "x1", bbox.x1, expectation.max_x1);
    push_min_bound_failure(&mut failures, "y1", bbox.y1, expectation.min_y1);
    push_max_bound_failure(&mut failures, "y1", bbox.y1, expectation.max_y1);

    failures
}

fn push_min_bound_failure(failures: &mut Vec<String>, field: &str, actual: f32, min: Option<f32>) {
    if let Some(min) = min
        && actual < min
    {
        failures.push(format!("{field}_below_min"));
    }
}

fn push_max_bound_failure(failures: &mut Vec<String>, field: &str, actual: f32, max: Option<f32>) {
    if let Some(max) = max
        && actual > max
    {
        failures.push(format!("{field}_above_max"));
    }
}

fn table_rows_for_page(artifact: &DocumentArtifact, page_index: u32) -> Vec<Vec<String>> {
    artifact
        .pages
        .iter()
        .find(|page| page.page_index == page_index)
        .map(|page| {
            page.layout_blocks
                .iter()
                .filter(|block| block.kind == LayoutBlockKind::Table)
                .flat_map(|block| parse_table_rows(&block.text))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_table_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter_map(|line| {
            let row = if line.contains('|') {
                line.split('|')
                    .map(str::trim)
                    .filter(|cell| !cell.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            } else if line.contains('\t') {
                line.split('\t')
                    .map(str::trim)
                    .filter(|cell| !cell.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            } else {
                line.split_whitespace()
                    .map(str::trim)
                    .filter(|cell| !cell.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            };
            (row.len() >= 2).then_some(row)
        })
        .collect()
}

fn normalize_table_rows(rows: &[Vec<String>]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.trim().to_string())
                .filter(|cell| !cell.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|row| !row.is_empty())
        .collect()
}

fn table_cells(rows: &[Vec<String>]) -> Vec<TableCell> {
    rows.iter()
        .enumerate()
        .flat_map(|(row_index, row)| {
            row.iter()
                .enumerate()
                .map(move |(column_index, text)| TableCell {
                    row: row_index,
                    column: column_index,
                    text: text.clone(),
                })
        })
        .collect()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 1.0;
    }

    numerator as f64 / denominator as f64
}

fn f1(precision: f64, recall: f64) -> f64 {
    if precision == 0.0 && recall == 0.0 {
        return 0.0;
    }

    2.0 * precision * recall / (precision + recall)
}

fn classification_precision(true_positive_count: usize, predicted_positive_count: usize) -> f64 {
    if predicted_positive_count == 0 {
        return 1.0;
    }

    true_positive_count as f64 / predicted_positive_count as f64
}

fn classification_recall(true_positive_count: usize, expected_positive_count: usize) -> f64 {
    if expected_positive_count == 0 {
        return 1.0;
    }

    true_positive_count as f64 / expected_positive_count as f64
}

fn document_text(artifact: &DocumentArtifact) -> String {
    artifact
        .pages
        .iter()
        .map(quality_text_from_page)
        .collect::<Vec<_>>()
        .join("\n")
}

fn quality_text_from_page(page: &PageArtifact) -> String {
    let mut parts = if page.layout_blocks.is_empty() {
        page.native_spans
            .iter()
            .chain(page.ocr_spans.iter())
            .map(|span| span.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
    } else {
        page.layout_blocks
            .iter()
            .map(|block| block.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
    };

    let existing_text = parts.join("\n");
    for span in &page.ocr_spans {
        let text = span.text.trim();
        if !text.is_empty() && !existing_text.contains(text) {
            parts.push(text.to_string());
        }
    }

    parts.join("\n")
}

fn normalize_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

fn normalize_chars(text: &str) -> Vec<char> {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn multiset_recall<T>(expected: Vec<T>, actual: Vec<T>) -> f64
where
    T: Ord,
{
    if expected.is_empty() {
        return 1.0;
    }

    let expected_count = expected.len();
    let missing_count = missing_multiset_items(expected, actual).len();
    (expected_count - missing_count) as f64 / expected_count as f64
}

fn missing_multiset_items<T>(mut expected: Vec<T>, mut actual: Vec<T>) -> Vec<T>
where
    T: Ord,
{
    expected.sort();
    actual.sort();

    let mut missing = Vec::new();
    let mut actual = actual.into_iter().peekable();

    for expected_item in expected {
        while actual
            .peek()
            .map(|actual_item| actual_item < &expected_item)
            .unwrap_or(false)
        {
            actual.next();
        }

        if actual
            .peek()
            .map(|actual_item| actual_item == &expected_item)
            .unwrap_or(false)
        {
            actual.next();
        } else {
            missing.push(expected_item);
        }
    }

    missing
}

fn insert_page_expectation_checks(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &EvalPageExpectation,
    artifact: &DocumentArtifact,
) {
    let page = artifact
        .pages
        .iter()
        .find(|page| page.page_index == expectation.index);
    let prefix = format!("page_{:06}", expectation.index);

    if let Some(expected_artifact_id) = &expectation.artifact_id {
        insert_json_check(
            checks,
            format!("{prefix}.artifact_id"),
            json!(expected_artifact_id),
            page.map(|page| json!(page.artifact_id))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    if let Some(expected_page_fingerprint) = &expectation.page_fingerprint {
        insert_json_check(
            checks,
            format!("{prefix}.page_fingerprint"),
            json!(expected_page_fingerprint),
            page.map(|page| json!(page.fingerprint.as_hex()))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    if let Some(expected_route) = expectation.route {
        insert_json_check(
            checks,
            format!("{prefix}.route"),
            json!(expected_route),
            page.map(|page| json!(page.route.route))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    if let Some(expected_empty_text_output) = expectation.empty_text_output {
        insert_json_check(
            checks,
            format!("{prefix}.empty_text_output"),
            json!(expected_empty_text_output),
            page.map(|page| json!(plain_text_from_page(page).is_empty()))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    if let Some(expected_image_artifact_count) = expectation.image_artifact_count {
        insert_json_check(
            checks,
            format!("{prefix}.image_artifact_count"),
            json!(expected_image_artifact_count),
            page.map(|page| json!(page.image_artifacts.len() as u32))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    if let Some(expected_layout_block_counts) = expectation.layout_block_counts {
        insert_json_check(
            checks,
            format!("{prefix}.layout_block_counts"),
            json!(expected_layout_block_counts),
            page.map(|page| json!(layout_summary_from_page(page)))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    if !expectation.required_text.is_empty() {
        let page_text = page.map(quality_text_from_page).unwrap_or_default();
        let missing = expectation
            .required_text
            .iter()
            .filter(|text| !page_text.contains(text.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        checks.insert(
            format!("{prefix}.required_text"),
            EvalCheckOutput {
                passed: missing.is_empty(),
                expected: json!(expectation.required_text),
                actual: json!({ "missing": missing }),
            },
        );
    }

    if !expectation.required_flags.is_empty() {
        let missing = page
            .map(|page| {
                expectation
                    .required_flags
                    .iter()
                    .filter(|flag| !page.quality.flags.contains(flag))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| expectation.required_flags.clone());

        checks.insert(
            format!("{prefix}.required_flags"),
            EvalCheckOutput {
                passed: missing.is_empty(),
                expected: json!(expectation.required_flags),
                actual: json!({ "missing": missing }),
            },
        );
    }

    if !expectation.required_reasons.is_empty() {
        let missing = page
            .map(|page| {
                expectation
                    .required_reasons
                    .iter()
                    .filter(|reason| !page.route.reasons.contains(reason))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| expectation.required_reasons.clone());

        checks.insert(
            format!("{prefix}.required_reasons"),
            EvalCheckOutput {
                passed: missing.is_empty(),
                expected: json!(expectation.required_reasons),
                actual: json!({
                    "reasons": page
                        .map(|page| page.route.reasons.clone())
                        .unwrap_or_default(),
                    "missing": missing,
                }),
            },
        );
    }
}

fn parse_pdf<B: PdfBackend>(
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
    if let Some(cache_dir) = cache_dir
        && let Some(mut artifact) = load_cached_artifact(cache_dir, &cache_key)?
    {
        clear_page_stage_timings(&mut artifact);
        artifact.metadata =
            document_metadata_with_source(backend, source_size_bytes, source_modified_unix_ms);
        artifact.global_diagnostics.cache_status = CacheStatus::Hit;
        artifact.global_diagnostics.cache_key = Some(cache_key);
        set_artifact_worker_count(&mut artifact, options);
        return Ok(artifact);
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
        store_cached_artifact(cache_dir, &cache_key, &artifact)?;
    }
    Ok(artifact)
}

fn set_artifact_worker_count(artifact: &mut DocumentArtifact, options: ExtractionOptions) {
    artifact.global_diagnostics.worker_count =
        effective_page_worker_count(options, artifact.pages.len());
}

fn effective_page_worker_count(options: ExtractionOptions, page_count: usize) -> usize {
    options.page_jobs.max(1).min(page_count.max(1))
}

fn load_document<B: PdfBackend>(backend: &B, path: &Path) -> Result<(B::Document, String)> {
    let fingerprint = document_fingerprint(path)?;
    let document = backend.load_document(path)?;

    Ok((document, fingerprint))
}

fn document_metadata<B: PdfBackend>(backend: &B, path: &Path) -> Result<DocumentMetadata> {
    Ok(document_metadata_with_source(
        backend,
        source_size_bytes(path)?,
        source_modified_unix_ms(path)?,
    ))
}

fn document_metadata_with_source<B: PdfBackend>(
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

fn document_fingerprint(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

fn source_size_bytes(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("read metadata {}", path.display()))?
        .len())
}

fn source_modified_unix_ms(path: &Path) -> Result<u64> {
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

fn cache_key(
    backend_name: &str,
    backend_version: &str,
    document_fingerprint: &str,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
) -> Result<String> {
    let ocr_fingerprint = ocr_fingerprint(ocr, source_path)?;
    Ok(sha256_hex(format!(
        "{CACHE_SCHEMA_VERSION}:{PARSER_NAME}:{PARSER_VERSION}:{backend_name}:{backend_version}:{document_fingerprint}:{ocr_fingerprint}:span-geometry={}",
        options.span_geometry
    )))
}

fn remove_cached_artifact_for_document(
    backend_name: &str,
    backend_version: &str,
    path: &Path,
    ocr: OcrOptions<'_>,
    cache_dir: &Path,
    options: ExtractionOptions,
) -> Result<()> {
    let fingerprint = document_fingerprint(path)?;
    let cache_key = cache_key(
        backend_name,
        backend_version,
        &fingerprint,
        path,
        ocr,
        options,
    )?;
    let path = cache_path(cache_dir, &cache_key);

    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("remove cache artifact {}", path.display()))?;
    }

    Ok(())
}

fn ocr_fingerprint(ocr: OcrOptions<'_>, source_path: &Path) -> Result<String> {
    if let Some(path) = ocr.sidecar {
        return sidecar_fingerprint(path, source_path);
    }

    if let Some(command) = ocr.command {
        return ocr_command_fingerprint(command, ocr.timeout);
    }

    Ok("no-sidecar".to_string())
}

fn sidecar_fingerprint(path: &Path, source_path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok("missing-sidecar".to_string());
    }
    let mut files = fs::read_dir(path)
        .with_context(|| format!("read OCR sidecar directory {}", path.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ty| ty.is_file()).unwrap_or(false))
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|file_name| is_document_sidecar_file(source_path, file_name))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    files.sort();

    let mut payload = Vec::new();
    for file in files {
        payload.extend_from_slice(
            file.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .as_bytes(),
        );
        payload.push(0);
        payload.extend_from_slice(
            &fs::read(&file).with_context(|| format!("read OCR sidecar {}", file.display()))?,
        );
        payload.push(0);
    }

    Ok(sha256_hex(payload))
}

fn ocr_command_fingerprint(command: &Path, timeout: Duration) -> Result<String> {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"ocr-command");
    payload.push(0);
    payload.extend_from_slice(command.to_string_lossy().as_bytes());
    payload.push(0);
    payload.extend_from_slice(duration_millis(timeout).to_string().as_bytes());
    payload.push(0);
    if command.is_file() {
        payload.extend_from_slice(
            &fs::read(command)
                .with_context(|| format!("read OCR command {}", command.display()))?,
        );
    } else {
        payload.extend_from_slice(b"unresolved-command");
    }

    Ok(sha256_hex(payload))
}

fn is_document_sidecar_file(source_path: &Path, file_name: &str) -> bool {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document");
    let prefix = format!("{stem}.p");
    let Some(page_suffix) = file_name
        .strip_prefix(&prefix)
        .and_then(|suffix| suffix.strip_suffix(".txt"))
    else {
        return false;
    };

    page_suffix.len() == 6 && page_suffix.chars().all(|ch| ch.is_ascii_digit())
}

fn load_cached_artifact(cache_dir: &Path, cache_key: &str) -> Result<Option<DocumentArtifact>> {
    let path = cache_path(cache_dir, cache_key);
    if !path.exists() {
        return Ok(None);
    }

    let artifact = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("read cache artifact {}", path.display()))?,
    )
    .with_context(|| format!("decode cache artifact {}", path.display()))?;
    Ok(Some(artifact))
}

fn clear_page_stage_timings(artifact: &mut DocumentArtifact) {
    for page in &mut artifact.pages {
        page.timings = PageTimings::default();
    }
    artifact.global_diagnostics.total_stage_time_us = 0;
}

fn store_cached_artifact(
    cache_dir: &Path,
    cache_key: &str,
    artifact: &DocumentArtifact,
) -> Result<()> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("create cache directory {}", cache_dir.display()))?;
    let path = cache_path(cache_dir, cache_key);
    let bytes = serde_json::to_vec_pretty(artifact)?;
    fs::write(&path, bytes).with_context(|| format!("write cache artifact {}", path.display()))?;
    Ok(())
}

fn cache_path(cache_dir: &Path, cache_key: &str) -> PathBuf {
    cache_dir.join(format!("{cache_key}.json"))
}

#[derive(Clone)]
struct DiscoveredPdf {
    path: PathBuf,
    label: String,
}

fn discover_pdfs(path: &Path) -> Result<Vec<DiscoveredPdf>> {
    let mut pdfs = fs::read_dir(path)
        .with_context(|| format!("read directory {}", path.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ty| ty.is_file()).unwrap_or(false))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false)
        })
        .map(|entry| DiscoveredPdf {
            label: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path(),
        })
        .collect::<Vec<_>>();

    pdfs.sort_by(|left, right| left.label.cmp(&right.label));

    if pdfs.is_empty() {
        bail!("no PDF files found in {}", path.display());
    }

    Ok(pdfs)
}

fn extract_lopdf_pages(
    document: &Document,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
) -> Result<Vec<ExtractedPage>> {
    let pages = document.get_pages().into_iter().collect::<Vec<_>>();
    let worker_count = options.page_jobs.max(1).min(pages.len().max(1));

    if worker_count == 1 {
        return pages
            .into_iter()
            .map(|(page_number, page_id)| {
                extract_lopdf_page(document, source_path, ocr, options, page_number, page_id)
            })
            .collect();
    }

    let mut extracted_pages = Vec::with_capacity(pages.len());
    for chunk in pages.chunks(worker_count) {
        let mut chunk_results = Vec::with_capacity(chunk.len());
        thread::scope(|scope| -> Result<()> {
            let handles = chunk
                .iter()
                .map(|(page_number, page_id)| {
                    scope.spawn(move || {
                        extract_lopdf_page(
                            document,
                            source_path,
                            ocr,
                            options,
                            *page_number,
                            *page_id,
                        )
                        .map(|page| (*page_number, page))
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                chunk_results.push(
                    handle
                        .join()
                        .map_err(|_| anyhow!("page extraction worker panicked"))??,
                );
            }

            Ok(())
        })?;
        extracted_pages.extend(chunk_results);
    }

    extracted_pages.sort_by_key(|(page_number, _)| *page_number);
    Ok(extracted_pages.into_iter().map(|(_, page)| page).collect())
}

fn extract_lopdf_page_by_index(
    document: &Document,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
    page_index: u32,
) -> Result<ExtractedPage> {
    let page_number = page_index
        .checked_add(1)
        .with_context(|| format!("page index {page_index} is too large"))?;
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .with_context(|| format!("page index {page_index} not found"))?;

    extract_lopdf_page(document, source_path, ocr, options, page_number, page_id)
}

fn pages_per_sec(page_count: usize, wall_us: u128) -> f64 {
    if wall_us == 0 {
        page_count as f64
    } else {
        page_count as f64 / (wall_us as f64 / 1_000_000.0)
    }
}

fn extract_lopdf_page(
    document: &Document,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
    page_number: u32,
    page_id: ObjectId,
) -> Result<ExtractedPage> {
    let page_index = page_number.saturating_sub(1);
    let page_box = effective_page_box(document, page_id);
    let dimensions = page_box.dimensions();
    let native_extract_start = Instant::now();
    let native_text = document.extract_text(&[page_number]).unwrap_or_default();
    let native_extract_us = native_extract_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;
    let content = document.get_page_content(page_id).unwrap_or_default();
    let content_len = content.len();
    let native_text_bytes = native_text.trim().len() as u32;
    let rotation_degrees = page_rotation(document, page_id);
    let can_extract_positioned_spans =
        should_extract_positioned_spans(content_len, native_text_bytes, rotation_degrees);
    let span_geometry_capped = options.span_geometry && !can_extract_positioned_spans;
    let native_spans = if options.span_geometry && can_extract_positioned_spans {
        compatible_positioned_text_spans(
            &native_text,
            extract_positioned_text_spans(&content, &page_box),
        )
    } else {
        Vec::new()
    };
    let bbox_overlap_ratio = positioned_bbox_overlap_ratio(&native_spans);
    let glyph_count = native_text.chars().filter(|ch| !ch.is_whitespace()).count() as u32;
    let native_span_count = native_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        .max(native_spans.len())
        .max((native_text_bytes > 0) as usize) as u32;

    let image_artifacts = image_xobject_artifacts(document, page_id, &content, &page_box);
    let image_area_ratio =
        image_area_ratio_hint(&image_artifacts, &content, native_text_bytes, &dimensions);
    let table_start = Instant::now();
    let table_line_density =
        table_line_density(&native_text).max(ruled_table_line_density(&content));
    let table_us = table_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;
    let signals = PageSignals {
        page_index,
        dimensions: dimensions.clone(),
        native_span_count,
        native_text_bytes,
        glyph_count,
        image_area_ratio,
        duplicate_char_ratio: duplicate_char_ratio(&native_text),
        bbox_overlap_ratio,
        broken_encoding_ratio: broken_encoding_ratio(&native_text),
        rotation_degrees,
        table_line_density,
        annotation_count: page_annotation_count(document, page_id),
        form_field_count: page_form_field_count(document, page_id),
        huge_object_count: if content_len > 16 * 1024 * 1024 {
            65
        } else {
            0
        },
        span_geometry_capped,
    };
    let (ocr_text, ocr_us) = load_ocr_if_needed(source_path, ocr, &signals)?;

    Ok(ExtractedPage {
        page_index,
        dimensions,
        native_text,
        native_spans,
        image_artifacts,
        signals,
        ocr_text,
        timings: PageTimings {
            native_extract_us,
            table_us,
            ocr_us,
            ..PageTimings::default()
        },
    })
}

#[derive(Clone, Copy, Debug)]
struct TextGeometryState {
    line_x: f32,
    line_y: f32,
    x: f32,
    y: f32,
    axis_a: f32,
    axis_b: f32,
    axis_c: f32,
    axis_d: f32,
    font_size: f32,
    leading: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scaling: f32,
    text_rise: f32,
}

impl Default for TextGeometryState {
    fn default() -> Self {
        Self {
            line_x: 0.0,
            line_y: 0.0,
            x: 0.0,
            y: 0.0,
            axis_a: 1.0,
            axis_b: 0.0,
            axis_c: 0.0,
            axis_d: 1.0,
            font_size: 12.0,
            leading: 12.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            text_rise: 0.0,
        }
    }
}

impl TextGeometryState {
    fn begin_text_object(&mut self) {
        self.line_x = 0.0;
        self.line_y = 0.0;
        self.x = 0.0;
        self.y = 0.0;
        self.axis_a = 1.0;
        self.axis_b = 0.0;
        self.axis_c = 0.0;
        self.axis_d = 1.0;
    }

    fn move_text_position(&mut self, tx: f32, ty: f32) {
        self.line_x += tx * self.axis_a + ty * self.axis_c;
        self.line_y += tx * self.axis_b + ty * self.axis_d;
        self.x = self.line_x;
        self.y = self.line_y;
    }

    fn set_text_matrix(&mut self, matrix: PdfMatrix) {
        self.axis_a = matrix.a;
        self.axis_b = matrix.b;
        self.axis_c = matrix.c;
        self.axis_d = matrix.d;
        self.line_x = matrix.e;
        self.line_y = matrix.f;
        self.x = matrix.e;
        self.y = matrix.f;
    }

    fn move_to_next_line(&mut self) {
        let leading = if self.leading == 0.0 {
            self.font_size
        } else {
            self.leading
        };
        self.move_text_position(0.0, -leading);
    }

    fn text_matrix(&self) -> PdfMatrix {
        PdfMatrix {
            a: self.axis_a,
            b: self.axis_b,
            c: self.axis_c,
            d: self.axis_d,
            e: self.x + self.axis_c * self.text_rise,
            f: self.y + self.axis_d * self.text_rise,
        }
    }

    fn advance_text(&mut self, width: f32) {
        self.x += width * self.axis_a;
        self.y += width * self.axis_b;
    }

    fn apply_text_array_adjustment(&mut self, adjustment: f32) {
        self.advance_text((-adjustment / 1000.0) * self.font_size * self.horizontal_scaling);
    }

    fn text_width(&self, text: &str) -> f32 {
        estimate_text_width(
            text,
            self.font_size,
            self.char_spacing,
            self.word_spacing,
            self.horizontal_scaling,
        )
    }
}

fn should_extract_positioned_spans(
    content_len: usize,
    native_text_bytes: u32,
    rotation_degrees: i16,
) -> bool {
    content_len <= MAX_POSITIONED_SPAN_CONTENT_BYTES
        && native_text_bytes <= MAX_POSITIONED_SPAN_NATIVE_TEXT_BYTES
        && rotation_degrees.rem_euclid(360) == 0
}

fn extract_positioned_text_spans(
    content_data: &[u8],
    page_box: &PageBox,
) -> Vec<ExtractedTextSpan> {
    let Ok(content) = Content::decode(content_data) else {
        return Vec::new();
    };

    let mut state = TextGeometryState::default();
    let mut matrix = PdfMatrix::identity();
    let mut matrix_stack = Vec::new();
    let mut spans = Vec::new();

    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => matrix_stack.push(matrix),
            "Q" => matrix = matrix_stack.pop().unwrap_or_else(PdfMatrix::identity),
            "cm" => {
                if let Some(next) = PdfMatrix::from_operands(&operation.operands) {
                    matrix = matrix.multiply(next);
                }
            }
            "BT" => state.begin_text_object(),
            "Tf" => {
                if let Some(size) = operation.operands.get(1).and_then(float_operand) {
                    state.font_size = size.abs().max(1.0);
                    if state.leading == 0.0 {
                        state.leading = state.font_size;
                    }
                }
            }
            "Tc" => {
                if let Some(spacing) = operation.operands.first().and_then(float_operand) {
                    state.char_spacing = spacing;
                }
            }
            "Tw" => {
                if let Some(spacing) = operation.operands.first().and_then(float_operand) {
                    state.word_spacing = spacing;
                }
            }
            "Tz" => {
                if let Some(scaling) = operation.operands.first().and_then(float_operand) {
                    state.horizontal_scaling = (scaling / 100.0).max(0.01);
                }
            }
            "Ts" => {
                if let Some(rise) = operation.operands.first().and_then(float_operand) {
                    state.text_rise = rise;
                }
            }
            "TL" => {
                if let Some(leading) = operation.operands.first().and_then(float_operand) {
                    state.leading = leading;
                }
            }
            "Td" => {
                if let Some((tx, ty)) = two_float_operands(&operation.operands) {
                    state.move_text_position(tx, ty);
                }
            }
            "TD" => {
                if let Some((tx, ty)) = two_float_operands(&operation.operands) {
                    state.leading = -ty;
                    state.move_text_position(tx, ty);
                }
            }
            "Tm" => {
                if let Some(text_matrix) = PdfMatrix::from_operands(&operation.operands) {
                    state.set_text_matrix(text_matrix);
                }
            }
            "T*" => state.move_to_next_line(),
            "Tj" => {
                if let Some(text) = operation.operands.first().and_then(text_operand) {
                    push_positioned_span(&mut spans, &mut state, page_box, matrix, text);
                }
            }
            "TJ" => {
                if let Some(text_array) = operation.operands.first() {
                    push_positioned_text_array_spans(
                        &mut spans, &mut state, page_box, matrix, text_array,
                    );
                }
            }
            "'" => {
                state.move_to_next_line();
                if let Some(text) = operation.operands.first().and_then(text_operand) {
                    push_positioned_span(&mut spans, &mut state, page_box, matrix, text);
                }
            }
            "\"" => {
                if let Some(word_spacing) = operation.operands.first().and_then(float_operand) {
                    state.word_spacing = word_spacing;
                }
                if let Some(char_spacing) = operation.operands.get(1).and_then(float_operand) {
                    state.char_spacing = char_spacing;
                }
                state.move_to_next_line();
                if let Some(text) = operation.operands.get(2).and_then(text_operand) {
                    push_positioned_span(&mut spans, &mut state, page_box, matrix, text);
                }
            }
            _ => {}
        }
    }

    spans
}

fn compatible_positioned_text_spans(
    native_text: &str,
    spans: Vec<ExtractedTextSpan>,
) -> Vec<ExtractedTextSpan> {
    let spans = spans
        .into_iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();
    if spans.is_empty() {
        return Vec::new();
    }

    let native = normalize_text_for_span_check(native_text);
    let compatible = spans.iter().all(|span| {
        let text = normalize_text_for_span_check(&span.text);
        !text.is_empty() && native.contains(&text)
    });

    if compatible { spans } else { Vec::new() }
}

#[derive(Clone, Copy, Debug)]
struct NormalizedBBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    area: f32,
}

fn positioned_bbox_overlap_ratio(spans: &[ExtractedTextSpan]) -> f32 {
    let mut boxes = spans
        .iter()
        .filter_map(|span| normalized_bbox(&span.bbox))
        .collect::<Vec<_>>();
    if boxes.len() < 2 {
        return 0.0;
    }

    boxes.sort_by(|left, right| {
        left.x0
            .total_cmp(&right.x0)
            .then_with(|| left.y0.total_cmp(&right.y0))
            .then_with(|| left.x1.total_cmp(&right.x1))
            .then_with(|| left.y1.total_cmp(&right.y1))
    });

    let total_area = boxes.iter().map(|bbox| bbox.area).sum::<f32>();
    if total_area <= f32::EPSILON {
        return 0.0;
    }

    let mut overlap_area = 0.0f32;
    let mut comparisons = 0usize;
    for (index, left) in boxes.iter().enumerate() {
        for right in boxes.iter().skip(index + 1) {
            if right.x0 >= left.x1 {
                break;
            }
            overlap_area += bbox_intersection_area(*left, *right);
            comparisons += 1;
            if overlap_area >= total_area || comparisons >= MAX_BBOX_OVERLAP_COMPARISONS {
                return (overlap_area / total_area).clamp(0.0, 1.0);
            }
        }
    }

    (overlap_area / total_area).clamp(0.0, 1.0)
}

fn normalized_bbox(bbox: &BBox) -> Option<NormalizedBBox> {
    let x0 = bbox.x0.min(bbox.x1);
    let x1 = bbox.x0.max(bbox.x1);
    let y0 = bbox.y0.min(bbox.y1);
    let y1 = bbox.y0.max(bbox.y1);
    if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
        return None;
    }

    let width = x1 - x0;
    let height = y1 - y0;
    let area = width * height;
    (area > f32::EPSILON).then_some(NormalizedBBox {
        x0,
        y0,
        x1,
        y1,
        area,
    })
}

fn bbox_intersection_area(left: NormalizedBBox, right: NormalizedBBox) -> f32 {
    let width = (left.x1.min(right.x1) - left.x0.max(right.x0)).max(0.0);
    let height = (left.y1.min(right.y1) - left.y0.max(right.y0)).max(0.0);
    width * height
}

fn push_positioned_span(
    spans: &mut Vec<ExtractedTextSpan>,
    state: &mut TextGeometryState,
    page_box: &PageBox,
    matrix: PdfMatrix,
    text: String,
) {
    if text.trim().is_empty() {
        return;
    }

    let width = state.text_width(&text);
    let bbox = transformed_text_bbox(
        state.text_matrix(),
        width,
        state.font_size,
        matrix,
        page_box,
    );
    spans.push(ExtractedTextSpan { text, bbox });
    state.advance_text(width);
}

fn push_positioned_text_array_spans(
    spans: &mut Vec<ExtractedTextSpan>,
    state: &mut TextGeometryState,
    page_box: &PageBox,
    matrix: PdfMatrix,
    object: &Object,
) {
    let Ok(array) = object.as_array() else {
        return;
    };

    for item in array {
        if let Some(text) = text_operand(item) {
            push_positioned_span(spans, state, page_box, matrix, text);
        } else if let Some(adjustment) = float_operand(item) {
            state.apply_text_array_adjustment(adjustment);
        }
    }
}

fn transformed_text_bbox(
    text_matrix: PdfMatrix,
    width: f32,
    font_size: f32,
    matrix: PdfMatrix,
    page_box: &PageBox,
) -> BBox {
    let corners = [
        transformed_text_point(text_matrix, matrix, 0.0, 0.0),
        transformed_text_point(text_matrix, matrix, width, 0.0),
        transformed_text_point(text_matrix, matrix, 0.0, -font_size),
        transformed_text_point(text_matrix, matrix, width, -font_size),
    ];

    let (first_x, first_y) = corners[0];
    let mut x0 = first_x - page_box.x0;
    let mut x1 = x0;
    let mut y0 = page_box.y1 - first_y;
    let mut y1 = y0;
    for (x, pdf_y) in corners.into_iter().skip(1) {
        let page_x = x - page_box.x0;
        let page_y = page_box.y1 - pdf_y;
        x0 = x0.min(page_x);
        x1 = x1.max(page_x);
        y0 = y0.min(page_y);
        y1 = y1.max(page_y);
    }

    BBox { x0, y0, x1, y1 }
}

fn transformed_text_point(
    text_matrix: PdfMatrix,
    content_matrix: PdfMatrix,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let (text_x, text_y) = text_matrix.transform_point(x, y);
    content_matrix.transform_point(text_x, text_y)
}

fn estimate_text_width(
    text: &str,
    font_size: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scaling: f32,
) -> f32 {
    let mut glyphs = 0usize;
    let mut spaces = 0usize;
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        glyphs += 1;
        if ch == ' ' {
            spaces += 1;
        }
    }

    let glyphs = glyphs.max(1);
    let base_width = glyphs as f32 * font_size * 0.55;
    let char_spacing_width = glyphs.saturating_sub(1) as f32 * char_spacing;
    let word_spacing_width = spaces as f32 * word_spacing;
    (base_width + char_spacing_width + word_spacing_width) * horizontal_scaling
}

fn two_float_operands(operands: &[Object]) -> Option<(f32, f32)> {
    Some((
        operands.first().and_then(float_operand)?,
        operands.get(1).and_then(float_operand)?,
    ))
}

fn float_operand(object: &Object) -> Option<f32> {
    object.as_float().ok()
}

fn name_operand(object: &Object) -> Option<&[u8]> {
    object.as_name().ok()
}

fn text_operand(object: &Object) -> Option<String> {
    object
        .as_str()
        .ok()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn normalize_text_for_span_check(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn load_ocr_if_needed(
    source_path: &Path,
    ocr: OcrOptions<'_>,
    signals: &PageSignals,
) -> Result<(Option<String>, u64)> {
    if !classify_page(signals).run_ocr {
        return Ok((None, 0));
    }

    let ocr_start = Instant::now();
    let text = if let Some(ocr_sidecar) = ocr.sidecar {
        let sidecar_path = ocr_sidecar.join(sidecar_file_name(source_path, signals.page_index));
        if !sidecar_path.exists() {
            return Ok((None, 0));
        }
        fs::read_to_string(&sidecar_path)
            .with_context(|| format!("read OCR sidecar {}", sidecar_path.display()))?
    } else if let Some(command) = ocr.command {
        run_ocr_command(command, source_path, signals.page_index, ocr.timeout)?
    } else {
        return Ok((None, 0));
    };

    if text.is_empty() {
        return Ok((None, 0));
    }

    let ocr_us = ocr_start.elapsed().as_micros().max(1).min(u64::MAX as u128) as u64;
    Ok((Some(text), ocr_us))
}

fn run_ocr_command(
    command: &Path,
    source_path: &Path,
    page_index: u32,
    timeout: Duration,
) -> Result<String> {
    let mut process = ProcessCommand::new(command);
    process.arg(source_path).arg(page_index.to_string());
    let timed_output = command_output_with_timeout(process, timeout)
        .with_context(|| format!("run OCR command {}", command.display()))?;
    let output = timed_output.output;

    if timed_output.timed_out {
        bail!(
            "OCR command timed out after {} ms for page {page_index}: {}",
            duration_millis(timeout),
            command.display()
        );
    }

    if !output.status.success() {
        bail!(
            "OCR command {} failed for page {page_index}: {}",
            command.display(),
            stderr_preview(&output.stderr).unwrap_or_else(|| "no stderr".to_string())
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn sidecar_file_name(source_path: &Path, page_index: u32) -> String {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document");
    format!("{stem}.p{page_index:06}.txt")
}

fn effective_page_box(document: &Document, page_id: ObjectId) -> PageBox {
    let default = PageBox::default();
    inherited_page_array(document, page_id, b"CropBox")
        .and_then(|crop_box| page_box_from_array(crop_box))
        .or_else(|| {
            inherited_page_array(document, page_id, b"MediaBox")
                .and_then(|media_box| page_box_from_array(media_box))
        })
        .unwrap_or(default)
}

#[derive(Clone, Debug)]
struct PageBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl PageBox {
    fn default() -> Self {
        Self {
            x0: 0.0,
            y0: 0.0,
            x1: 612.0,
            y1: 792.0,
        }
    }

    fn dimensions(&self) -> PageDimensions {
        PageDimensions::new(self.x1 - self.x0, self.y1 - self.y0)
    }

    fn clipped_local_bbox(&self, bbox: &BBox) -> Option<NormalizedBBox> {
        let x0 = bbox.x0.min(bbox.x1).max(self.x0);
        let x1 = bbox.x0.max(bbox.x1).min(self.x1);
        let y0 = bbox.y0.min(bbox.y1).max(self.y0);
        let y1 = bbox.y0.max(bbox.y1).min(self.y1);
        if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
            return None;
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        let local_x0 = x0 - self.x0;
        let local_x1 = x1 - self.x0;
        let local_y0 = y0 - self.y0;
        let local_y1 = y1 - self.y0;
        let area = (local_x1 - local_x0) * (local_y1 - local_y0);
        (area > f32::EPSILON).then_some(NormalizedBBox {
            x0: local_x0,
            y0: local_y0,
            x1: local_x1,
            y1: local_y1,
            area,
        })
    }

    fn local_bbox(&self, bbox: &BBox) -> Option<BBox> {
        let x0 = bbox.x0.min(bbox.x1) - self.x0;
        let x1 = bbox.x0.max(bbox.x1) - self.x0;
        let y0 = bbox.y0.min(bbox.y1) - self.y0;
        let y1 = bbox.y0.max(bbox.y1) - self.y0;
        if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
            return None;
        }

        ((x1 - x0) > f32::EPSILON && (y1 - y0) > f32::EPSILON).then_some(BBox { x0, y0, x1, y1 })
    }
}

fn page_box_from_array(box_array: &[Object]) -> Option<PageBox> {
    if box_array.len() != 4 {
        return None;
    }

    let raw_x0 = box_array[0].as_float().ok()?;
    let raw_y0 = box_array[1].as_float().ok()?;
    let raw_x1 = box_array[2].as_float().ok()?;
    let raw_y1 = box_array[3].as_float().ok()?;
    if ![raw_x0, raw_y0, raw_x1, raw_y1]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }

    let x0 = raw_x0.min(raw_x1);
    let x1 = raw_x0.max(raw_x1);
    let y0 = raw_y0.min(raw_y1);
    let y1 = raw_y0.max(raw_y1);
    ((x1 - x0) > f32::EPSILON && (y1 - y0) > f32::EPSILON).then_some(PageBox { x0, y0, x1, y1 })
}

fn inherited_page_array<'a>(
    document: &'a Document,
    page_id: ObjectId,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    let mut current_id = page_id;
    for _ in 0..16 {
        let dict = document.get_dictionary(current_id).ok()?;
        if let Some(array) = dict.get(key).ok().and_then(|object| object.as_array().ok()) {
            return Some(array);
        }

        match dict.get(b"Parent").ok()? {
            Object::Reference(parent_id) => current_id = *parent_id,
            _ => return None,
        }
    }

    None
}

fn page_rotation(document: &Document, page_id: ObjectId) -> i16 {
    let mut current_id = page_id;
    for _ in 0..16 {
        let Ok(dict) = document.get_dictionary(current_id) else {
            return 0;
        };
        if let Some(rotation) = dict
            .get(b"Rotate")
            .ok()
            .and_then(|object| object.as_i64().ok())
        {
            return rotation as i16;
        }

        match dict.get(b"Parent").ok() {
            Some(Object::Reference(parent_id)) => current_id = *parent_id,
            _ => return 0,
        }
    }

    0
}

fn page_annotation_count(document: &Document, page_id: ObjectId) -> u32 {
    let Ok(dict) = document.get_dictionary(page_id) else {
        return 0;
    };
    let Ok(annots) = dict.get(b"Annots") else {
        return 0;
    };

    if let Some(array) = object_array(document, annots) {
        return array.len() as u32;
    }

    u32::from(object_dictionary(document, annots).is_some())
}

fn page_form_field_count(document: &Document, page_id: ObjectId) -> u32 {
    page_widget_annotation_count(document, page_id) + catalog_acroform_field_count(document)
}

fn page_widget_annotation_count(document: &Document, page_id: ObjectId) -> u32 {
    let Ok(dict) = document.get_dictionary(page_id) else {
        return 0;
    };
    let Ok(annots) = dict.get(b"Annots") else {
        return 0;
    };

    if let Some(array) = object_array(document, annots) {
        return array
            .iter()
            .filter(|annotation| is_form_annotation(document, annotation))
            .count() as u32;
    }

    u32::from(is_form_annotation(document, annots))
}

fn catalog_acroform_field_count(document: &Document) -> u32 {
    let Some(acroform) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|object| object_dictionary(document, object))
    else {
        return 0;
    };

    acroform
        .get(b"Fields")
        .ok()
        .and_then(|object| object_array(document, object))
        .map(|fields| fields.len() as u32)
        .unwrap_or_default()
}

fn is_form_annotation(document: &Document, object: &Object) -> bool {
    object_dictionary(document, object)
        .and_then(|dict| dict.get(b"Subtype").ok())
        .and_then(name_operand)
        .is_some_and(|subtype| subtype == b"Widget")
}

fn image_area_ratio_hint(
    image_artifacts: &[ExtractedImage],
    content: &[u8],
    native_text_bytes: u32,
    dimensions: &PageDimensions,
) -> f32 {
    let xobject_ratio = image_artifact_coverage_ratio(image_artifacts, dimensions);
    let fallback_ratio = if native_text_bytes == 0 && !content.is_empty() {
        0.85
    } else {
        0.0
    };

    xobject_ratio.max(fallback_ratio)
}

fn image_artifact_coverage_ratio(
    image_artifacts: &[ExtractedImage],
    dimensions: &PageDimensions,
) -> f32 {
    let page_area = dimensions.width * dimensions.height;
    if image_artifacts.is_empty() || page_area <= f32::EPSILON {
        return 0.0;
    }

    let boxes = image_artifacts
        .iter()
        .filter_map(|image| clipped_image_bbox(&image.bbox, dimensions))
        .collect::<Vec<_>>();
    if boxes.is_empty() {
        return 0.0;
    }

    let mut xs = boxes
        .iter()
        .flat_map(|bbox| [bbox.x0, bbox.x1])
        .collect::<Vec<_>>();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);

    let mut covered_area = 0.0f32;
    for window in xs.windows(2) {
        let x0 = window[0];
        let x1 = window[1];
        let width = x1 - x0;
        if width <= f32::EPSILON {
            continue;
        }

        let mut intervals = boxes
            .iter()
            .filter(|bbox| bbox.x0 < x1 && bbox.x1 > x0)
            .map(|bbox| (bbox.y0, bbox.y1))
            .collect::<Vec<_>>();
        intervals.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });

        let mut covered_y = 0.0f32;
        let mut current: Option<(f32, f32)> = None;
        for (y0, y1) in intervals {
            match current {
                Some((current_y0, current_y1)) if y0 <= current_y1 => {
                    current = Some((current_y0, current_y1.max(y1)));
                }
                Some((current_y0, current_y1)) => {
                    covered_y += current_y1 - current_y0;
                    current = Some((y0, y1));
                }
                None => current = Some((y0, y1)),
            }
        }
        if let Some((y0, y1)) = current {
            covered_y += y1 - y0;
        }

        covered_area += width * covered_y;
    }

    (covered_area / page_area).clamp(0.0, 1.0)
}

fn clipped_image_bbox(bbox: &BBox, dimensions: &PageDimensions) -> Option<NormalizedBBox> {
    let x0 = bbox.x0.min(bbox.x1).clamp(0.0, dimensions.width);
    let x1 = bbox.x0.max(bbox.x1).clamp(0.0, dimensions.width);
    let y0 = bbox.y0.min(bbox.y1).clamp(0.0, dimensions.height);
    let y1 = bbox.y0.max(bbox.y1).clamp(0.0, dimensions.height);
    let area = (x1 - x0) * (y1 - y0);

    (area > f32::EPSILON).then_some(NormalizedBBox {
        x0,
        y0,
        x1,
        y1,
        area,
    })
}

fn image_xobject_artifacts(
    document: &Document,
    page_id: ObjectId,
    content: &[u8],
    page_box: &PageBox,
) -> Vec<ExtractedImage> {
    let resources = page_resources(document, page_id);
    let Ok(content) = Content::decode(content) else {
        return Vec::new();
    };
    let dimensions = page_box.dimensions();
    let page_area = dimensions.width * dimensions.height;
    if page_area <= 0.0 {
        return Vec::new();
    }

    let mut collector = ImageArtifactCollector {
        document,
        page_box: page_box.clone(),
        page_area,
        images: Vec::new(),
    };
    collector.collect_content(&content, resources, PdfMatrix::identity(), None, 0);
    collector.images
}

struct ImageArtifactCollector<'a> {
    document: &'a Document,
    page_box: PageBox,
    page_area: f32,
    images: Vec<ExtractedImage>,
}

impl ImageArtifactCollector<'_> {
    const MAX_XOBJECT_ARTIFACT_DEPTH: u8 = 8;

    fn collect_content(
        &mut self,
        content: &Content,
        resources: Option<&Dictionary>,
        initial_state: PdfMatrix,
        source_name: Option<String>,
        depth: u8,
    ) {
        if depth >= Self::MAX_XOBJECT_ARTIFACT_DEPTH {
            return;
        }

        let mut state = initial_state;
        let mut stack = Vec::new();

        for operation in &content.operations {
            match operation.operator.as_str() {
                "q" => stack.push(state),
                "Q" => {
                    state = stack.pop().unwrap_or(initial_state);
                }
                "cm" => {
                    if let Some(matrix) = PdfMatrix::from_operands(&operation.operands) {
                        state = state.multiply(matrix);
                    }
                }
                "Do" => {
                    if let Some(resources) = resources
                        && let Some(name) = operation.operands.first().and_then(name_operand)
                    {
                        self.collect_drawn_xobject(
                            resources,
                            name,
                            state,
                            source_name.clone(),
                            depth + 1,
                        );
                    }
                }
                "BI" => {
                    self.push_image_artifact(state, "inline".to_string());
                }
                _ => {}
            }
        }
    }

    fn collect_drawn_xobject(
        &mut self,
        resources: &Dictionary,
        name: &[u8],
        state: PdfMatrix,
        source_name: Option<String>,
        depth: u8,
    ) {
        let Some(xobjects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|object| object_dictionary(self.document, object))
        else {
            return;
        };
        let Ok(object) = xobjects.get(name) else {
            return;
        };

        let Some(dict) = object_dictionary(self.document, object) else {
            return;
        };
        let Some(subtype) = dict.get(b"Subtype").ok().and_then(name_operand) else {
            return;
        };

        let source_name = source_name.unwrap_or_else(|| String::from_utf8_lossy(name).into_owned());
        if subtype == b"Image" {
            self.push_image_artifact(state, source_name);
            return;
        }
        if subtype != b"Form" {
            return;
        }

        let Some(content) = object_stream_content(self.document, object) else {
            return;
        };
        let Ok(content) = Content::decode(content) else {
            return;
        };
        let form_resources = dict
            .get(b"Resources")
            .ok()
            .and_then(|object| object_dictionary(self.document, object))
            .or(Some(resources));
        let form_matrix = dict
            .get(b"Matrix")
            .ok()
            .and_then(|object| object_array(self.document, object))
            .and_then(|array| PdfMatrix::from_operands(array))
            .unwrap_or_else(PdfMatrix::identity);

        self.collect_content(
            &content,
            form_resources,
            state.multiply(form_matrix),
            Some(source_name),
            depth,
        );
    }

    fn push_image_artifact(&mut self, state: PdfMatrix, source_name: String) {
        let raw_bbox = state.unit_square_bbox();
        let Some(bbox) = self.page_box.local_bbox(&raw_bbox) else {
            return;
        };
        let area_ratio = self
            .page_box
            .clipped_local_bbox(&raw_bbox)
            .map(|bbox| bbox.area / self.page_area)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        self.images.push(ExtractedImage {
            bbox,
            area_ratio,
            source_name: Some(source_name),
        });
    }
}

fn page_resources(document: &Document, page_id: ObjectId) -> Option<&Dictionary> {
    let mut current_id = page_id;
    for _ in 0..16 {
        let dict = document.get_dictionary(current_id).ok()?;
        if let Some(resources) = dict
            .get(b"Resources")
            .ok()
            .and_then(|object| object_dictionary(document, object))
        {
            return Some(resources);
        }

        match dict.get(b"Parent").ok()? {
            Object::Reference(parent_id) => current_id = *parent_id,
            _ => return None,
        }
    }

    None
}

fn object_stream_content<'a>(document: &'a Document, object: &'a Object) -> Option<&'a [u8]> {
    match object {
        Object::Stream(stream) => Some(&stream.content),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .ok()
            .and_then(|object| object_stream_content(document, object)),
        _ => None,
    }
}

fn object_dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .ok()
            .and_then(|object| object_dictionary(document, object)),
        _ => None,
    }
}

fn object_array<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Vec<Object>> {
    match object {
        Object::Array(array) => Some(array),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .ok()
            .and_then(|object| object_array(document, object)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct PdfMatrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl PdfMatrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn from_operands(operands: &[Object]) -> Option<Self> {
        Some(Self {
            a: operands.first().and_then(float_operand)?,
            b: operands.get(1).and_then(float_operand)?,
            c: operands.get(2).and_then(float_operand)?,
            d: operands.get(3).and_then(float_operand)?,
            e: operands.get(4).and_then(float_operand)?,
            f: operands.get(5).and_then(float_operand)?,
        })
    }

    fn multiply(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            e: self.e * next.a + self.f * next.c + next.e,
            f: self.e * next.b + self.f * next.d + next.f,
        }
    }

    fn transform_point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    fn unit_square_bbox(self) -> BBox {
        let points = [
            self.transform_point(0.0, 0.0),
            self.transform_point(1.0, 0.0),
            self.transform_point(0.0, 1.0),
            self.transform_point(1.0, 1.0),
        ];

        let (mut x0, mut y0) = points[0];
        let (mut x1, mut y1) = points[0];
        for (x, y) in points.into_iter().skip(1) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }

        BBox { x0, y0, x1, y1 }
    }
}

fn broken_encoding_ratio(text: &str) -> f32 {
    let total = text.chars().count();
    if total == 0 {
        return 0.0;
    }

    let broken = text
        .chars()
        .filter(|ch| *ch == '\u{fffd}' || (ch.is_control() && !ch.is_whitespace()))
        .count();
    broken as f32 / total as f32
}

fn duplicate_char_ratio(text: &str) -> f32 {
    let mut previous = None;
    let mut duplicate_runs = 0usize;
    let mut total = 0usize;

    for ch in text.chars().filter(|ch| !ch.is_whitespace()) {
        total += 1;
        if Some(ch) == previous {
            duplicate_runs += 1;
        }
        previous = Some(ch);
    }

    if total == 0 {
        0.0
    } else {
        duplicate_runs as f32 / total as f32
    }
}

fn table_line_density(text: &str) -> f32 {
    let total = text.chars().filter(|ch| !ch.is_whitespace()).count();
    if total == 0 {
        return 0.0;
    }

    let table_like = text
        .chars()
        .filter(|ch| matches!(ch, '|' | '\t' | '+' | '-'))
        .count();
    table_like as f32 / total as f32
}

#[derive(Default)]
struct VectorPathState {
    current: Option<(f32, f32)>,
    pending_ruling_segments: u32,
}

fn ruled_table_line_density(content: &[u8]) -> f32 {
    let Ok(content) = Content::decode(content) else {
        return 0.0;
    };

    let mut matrix = PdfMatrix::identity();
    let mut matrix_stack = Vec::new();
    let mut path = VectorPathState::default();
    let mut stroked_ruling_segments = 0u32;

    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => matrix_stack.push(matrix),
            "Q" => {
                matrix = matrix_stack.pop().unwrap_or_else(PdfMatrix::identity);
                path = VectorPathState::default();
            }
            "cm" => {
                if let Some(next) = PdfMatrix::from_operands(&operation.operands) {
                    matrix = matrix.multiply(next);
                }
            }
            "m" => {
                path.current = two_float_operands(&operation.operands)
                    .map(|(x, y)| matrix.transform_point(x, y));
            }
            "l" => {
                if let (Some(start), Some((x, y))) =
                    (path.current, two_float_operands(&operation.operands))
                {
                    let end = matrix.transform_point(x, y);
                    if is_ruling_segment(start, end) {
                        path.pending_ruling_segments += 1;
                    }
                    path.current = Some(end);
                }
            }
            "re" => {
                path.pending_ruling_segments +=
                    rectangle_ruling_segments(&operation.operands, matrix);
                path.current = None;
            }
            "S" | "s" | "B" | "B*" | "b" | "b*" => {
                stroked_ruling_segments += path.pending_ruling_segments;
                path = VectorPathState::default();
            }
            "n" | "f" | "F" | "f*" => {
                path = VectorPathState::default();
            }
            _ => {}
        }
    }

    (stroked_ruling_segments as f32 / 20.0).clamp(0.0, 1.0)
}

fn rectangle_ruling_segments(operands: &[Object], matrix: PdfMatrix) -> u32 {
    let Some(x) = operands.first().and_then(float_operand) else {
        return 0;
    };
    let Some(y) = operands.get(1).and_then(float_operand) else {
        return 0;
    };
    let Some(width) = operands.get(2).and_then(float_operand) else {
        return 0;
    };
    let Some(height) = operands.get(3).and_then(float_operand) else {
        return 0;
    };

    let lower_left = matrix.transform_point(x, y);
    let lower_right = matrix.transform_point(x + width, y);
    let upper_right = matrix.transform_point(x + width, y + height);
    let upper_left = matrix.transform_point(x, y + height);

    [
        (lower_left, lower_right),
        (lower_right, upper_right),
        (upper_right, upper_left),
        (upper_left, lower_left),
    ]
    .into_iter()
    .filter(|(start, end)| is_ruling_segment(*start, *end))
    .count() as u32
}

fn is_ruling_segment(start: (f32, f32), end: (f32, f32)) -> bool {
    let dx = (start.0 - end.0).abs();
    let dy = (start.1 - end.1).abs();
    const AXIS_TOLERANCE: f32 = 1.0;
    const MIN_RULING_LENGTH: f32 = 24.0;

    (dx <= AXIS_TOLERANCE && dy >= MIN_RULING_LENGTH)
        || (dy <= AXIS_TOLERANCE && dx >= MIN_RULING_LENGTH)
}

fn write_json(value: &impl Serialize) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    writeln!(handle)?;
    Ok(())
}

fn write_plain_text(artifact: &glyphrush_core::DocumentArtifact) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(plain_text_from_artifact(artifact).as_bytes())?;
    Ok(())
}

fn plain_text_from_artifact(artifact: &DocumentArtifact) -> String {
    let mut text = String::new();
    for page in &artifact.pages {
        text.push_str(&plain_text_from_page(page));
    }
    text
}

fn plain_text_from_page(page: &PageArtifact) -> String {
    let page_text = quality_text_from_page(page);
    if page_text.is_empty() {
        String::new()
    } else {
        format!("{page_text}\n")
    }
}

fn write_warnings(artifact: &DocumentArtifact) -> Result<()> {
    if artifact.global_diagnostics.warnings.is_empty() {
        return Ok(());
    }

    let stderr = io::stderr();
    let mut handle = stderr.lock();
    for warning in &artifact.global_diagnostics.warnings {
        writeln!(handle, "warning: {warning}")?;
    }
    Ok(())
}

fn write_markdown(artifact: &DocumentArtifact) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for (page_offset, page) in artifact.pages.iter().enumerate() {
        if page_offset > 0 {
            writeln!(handle)?;
            writeln!(handle, "---")?;
            writeln!(handle)?;
        }

        let blocks = markdown_blocks(page);
        for (block_offset, block) in blocks.iter().enumerate() {
            if block_offset > 0 {
                writeln!(handle)?;
            }
            writeln!(handle, "{block}")?;
        }
    }
    Ok(())
}

fn markdown_blocks(page: &PageArtifact) -> Vec<String> {
    if page.layout_blocks.is_empty() {
        return page
            .native_spans
            .iter()
            .chain(page.ocr_spans.iter())
            .map(|span| span.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect();
    }

    page.layout_blocks
        .iter()
        .map(|block| match block.kind {
            LayoutBlockKind::Heading => format!("# {}", block.text.trim()),
            LayoutBlockKind::Paragraph
            | LayoutBlockKind::List
            | LayoutBlockKind::Figure
            | LayoutBlockKind::Header
            | LayoutBlockKind::Footer => block.text.trim().to_string(),
            LayoutBlockKind::Table => {
                markdown_table_block(&block.text).unwrap_or_else(|| block.text.trim().to_string())
            }
        })
        .filter(|text| !text.is_empty())
        .collect()
}

fn markdown_table_block(text: &str) -> Option<String> {
    let rows = text
        .lines()
        .map(parse_markdown_table_row)
        .collect::<Option<Vec<_>>>()?;
    let column_count = rows.first()?.len();
    if rows.len() < 2 || column_count < 2 || rows.iter().any(|row| row.len() != column_count) {
        return None;
    }

    let mut markdown = String::new();
    markdown.push_str(&format_markdown_table_row(&rows[0]));
    markdown.push('\n');
    markdown.push_str(&format_markdown_table_row(&vec![
        "---".to_string();
        column_count
    ]));
    for row in rows.iter().skip(1) {
        markdown.push('\n');
        markdown.push_str(&format_markdown_table_row(row));
    }
    Some(markdown)
}

fn parse_markdown_table_row(line: &str) -> Option<Vec<String>> {
    parse_pipe_table_row(line).or_else(|| parse_whitespace_table_row(line))
}

fn parse_pipe_table_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return None;
    }

    let trimmed = trimmed.trim_matches('|');
    let cells = trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect::<Vec<_>>();

    (cells.len() >= 2 && cells.iter().any(|cell| !cell.is_empty())).then_some(cells)
}

fn parse_whitespace_table_row(line: &str) -> Option<Vec<String>> {
    let cells = line
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    (cells.len() >= 2).then_some(cells)
}

fn format_markdown_table_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
