use crate::*;

use std::{
    path::PathBuf,
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use glyphrush_core::parse_extracted_pages;
use serde::Serialize;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "glyphrush")]
#[command(about = "Adaptive fast PDF parser with explicit quality flags")]
pub(crate) struct Cli {
    #[arg(long, value_enum, default_value_t = BackendChoice::Auto, global = true)]
    pub(crate) backend: BackendChoice,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum BackendChoice {
    Auto,
    Lopdf,
    #[cfg(feature = "pdfium")]
    Pdfium,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum CoveragePreset {
    #[value(name = "glyphrush-v0")]
    GlyphrushV0,
    #[value(name = "glyphrush-v0-native-text")]
    GlyphrushV0NativeText,
}

pub(crate) const GLYPHRUSH_V0_COVERAGE_CATEGORIES: &[&str] = &[
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

pub(crate) const GLYPHRUSH_V0_NATIVE_TEXT_COVERAGE_CATEGORIES: &[&str] = &[
    "clean_digital",
    "hybrid",
    "academic_columns",
    "tables",
    "forms",
    "rotated",
    "weird_encoding",
    "large",
];

impl CoveragePreset {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::GlyphrushV0 => "glyphrush-v0",
            Self::GlyphrushV0NativeText => "glyphrush-v0-native-text",
        }
    }

    pub(crate) fn categories(self) -> &'static [&'static str] {
        match self {
            Self::GlyphrushV0 => GLYPHRUSH_V0_COVERAGE_CATEGORIES,
            Self::GlyphrushV0NativeText => GLYPHRUSH_V0_NATIVE_TEXT_COVERAGE_CATEGORIES,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum BaselinePreset {
    #[value(name = "glyphrush-v0")]
    GlyphrushV0,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OcrCommandInput {
    #[default]
    #[value(name = "pdf-page")]
    PdfPage,
    #[value(name = "rendered-image")]
    RenderedImage,
}

pub(crate) const GLYPHRUSH_V0_BASELINES: &[(&str, &str)] = &[
    ("liteparse", "tools/baselines/liteparse-text.sh"),
    (
        "liteparse-no-ocr",
        "tools/baselines/liteparse-no-ocr-text.sh",
    ),
    ("pymupdf", "tools/baselines/pymupdf-text.sh"),
    ("pdfplumber", "tools/baselines/pdfplumber-text.sh"),
];

impl BaselinePreset {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::GlyphrushV0 => "glyphrush-v0",
        }
    }

    pub(crate) fn specs(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::GlyphrushV0 => GLYPHRUSH_V0_BASELINES,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
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
        #[arg(long)]
        ocr_http_url: Option<String>,
        #[arg(long, value_enum, default_value_t = OcrCommandInput::PdfPage)]
        ocr_command_input: OcrCommandInput,
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
        #[arg(long)]
        ocr_http_url: Option<String>,
        #[arg(long, value_enum, default_value_t = OcrCommandInput::PdfPage)]
        ocr_command_input: OcrCommandInput,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        #[arg(long)]
        eval_manifest: Option<PathBuf>,
        #[arg(long)]
        eval_category: Option<String>,
        #[arg(long, value_enum, conflicts_with = "eval_category")]
        eval_category_preset: Option<CoveragePreset>,
        #[arg(long)]
        require_quality: bool,
        #[arg(long)]
        require_baselines: bool,
        #[arg(long)]
        require_baseline_quality: bool,
        #[arg(long, value_enum)]
        require_coverage_preset: Option<CoveragePreset>,
        #[arg(long, value_name = "BASELINE=RATIO")]
        require_speedup: Vec<BenchmarkSpeedupRequirement>,
        #[arg(long, value_name = "BASELINE=RATIO")]
        require_speedup_claim: Vec<BenchmarkSpeedupRequirement>,
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
    FeatureParity {
        #[arg(long)]
        bench_report: Option<PathBuf>,
        #[arg(long)]
        require_speed_evidence: bool,
        #[arg(long)]
        require_speed_advantage: bool,
        #[arg(long, value_enum)]
        require_coverage_preset: Option<CoveragePreset>,
    },
    BackendCheck {
        #[arg(long)]
        pdf: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
    },
    OcrCheck {
        pdf: PathBuf,
        #[arg(long)]
        page_index: u32,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long)]
        ocr_http_url: Option<String>,
        #[arg(long, value_enum, default_value_t = OcrCommandInput::PdfPage)]
        ocr_command_input: OcrCommandInput,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        strict: bool,
    },
    Manifest {
        pdf: PathBuf,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        category_from_path: bool,
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
        #[arg(long)]
        ocr_http_url: Option<String>,
        #[arg(long, value_enum, default_value_t = OcrCommandInput::PdfPage)]
        ocr_command_input: OcrCommandInput,
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
        #[arg(long)]
        ocr_http_url: Option<String>,
        #[arg(long, value_enum, default_value_t = OcrCommandInput::PdfPage)]
        ocr_command_input: OcrCommandInput,
        #[arg(long, default_value_t = DEFAULT_OCR_TIMEOUT_MS)]
        ocr_timeout_ms: u64,
        #[arg(long)]
        span_geometry: bool,
    },
    /// Benchmark utility: time in-process PDF parsing without process startup overhead.
    WarmBench {
        pdf: PathBuf,
        #[arg(long, default_value_t = 5)]
        runs: usize,
        #[arg(long, default_value_t = 1)]
        warmup: usize,
    },
    Eval {
        manifest: PathBuf,
        #[arg(long)]
        category: Option<String>,
        #[arg(long, value_enum, conflicts_with = "category")]
        category_preset: Option<CoveragePreset>,
        #[arg(long)]
        ocr_sidecar: Option<PathBuf>,
        #[arg(long)]
        ocr_command: Option<PathBuf>,
        #[arg(long)]
        ocr_http_url: Option<String>,
        #[arg(long, value_enum, default_value_t = OcrCommandInput::PdfPage)]
        ocr_command_input: OcrCommandInput,
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
pub(crate) enum OutputFormat {
    Json,
    Text,
    Markdown,
}

#[derive(Clone, Debug)]
pub(crate) struct BaselineSpec {
    pub(crate) name: String,
    pub(crate) command: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct CategoryCountSpec {
    pub(crate) category: String,
    pub(crate) count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BenchmarkSpeedupRequirement {
    pub(crate) baseline: String,
    pub(crate) min_glyphrush_speedup: f64,
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

impl FromStr for BenchmarkSpeedupRequirement {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (baseline, speedup) = value
            .split_once('=')
            .ok_or_else(|| "speedup requirement must use BASELINE=RATIO".to_string())?;
        let baseline = baseline.trim();
        let speedup = speedup.trim();

        if baseline.is_empty() {
            return Err("speedup baseline name cannot be empty".to_string());
        }
        if speedup.is_empty() {
            return Err("speedup ratio cannot be empty".to_string());
        }
        let min_glyphrush_speedup = speedup
            .parse::<f64>()
            .map_err(|_| "speedup ratio must be a number".to_string())?;
        if !min_glyphrush_speedup.is_finite() || min_glyphrush_speedup <= 0.0 {
            return Err("speedup ratio must be a positive finite number".to_string());
        }

        Ok(Self {
            baseline: baseline.to_string(),
            min_glyphrush_speedup,
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

pub(crate) fn main_impl() -> Result<()> {
    let cli = Cli::parse();

    match cli.backend {
        BackendChoice::Auto => run_auto_backend(cli.command),
        BackendChoice::Lopdf => run_command(&LopdfBackend, cli.command),
        #[cfg(feature = "pdfium")]
        BackendChoice::Pdfium => run_command(&PdfiumBackend, cli.command),
    }
}

pub(crate) fn run_auto_backend(command: Commands) -> Result<()> {
    #[cfg(feature = "pdfium")]
    {
        run_command(&PdfiumBackend, command)
    }
    #[cfg(not(feature = "pdfium"))]
    {
        run_command(&LopdfBackend, command)
    }
}

pub(crate) fn run_command<B: PdfBackend + Sync>(backend: &B, command: Commands) -> Result<()> {
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
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            cache_dir,
            span_geometry,
            jobs,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_http_url.as_deref(),
                ocr_command_input,
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
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            cache_dir,
            eval_manifest: eval_manifest_path,
            eval_category,
            eval_category_preset,
            require_quality,
            require_baselines,
            require_baseline_quality,
            require_coverage_preset,
            require_speedup,
            require_speedup_claim,
            cache_probe,
            span_geometry,
            baseline,
            baseline_preset,
            baseline_timeout_ms,
            jobs,
        } => run_bench(
            backend,
            pdf,
            ocr_sidecar,
            ocr_command,
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            cache_dir,
            eval_manifest_path,
            eval_category,
            eval_category_preset,
            require_quality,
            require_baselines,
            require_baseline_quality,
            require_coverage_preset,
            require_speedup,
            require_speedup_claim,
            cache_probe,
            span_geometry,
            baseline,
            baseline_preset,
            baseline_timeout_ms,
            jobs,
        )?,
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
        Commands::FeatureParity {
            bench_report,
            require_speed_evidence,
            require_speed_advantage,
            require_coverage_preset,
        } => {
            let output =
                feature_parity_output(backend, bench_report.as_deref(), require_coverage_preset)?;
            let speed_evidence_failed = require_speed_evidence
                && !output
                    .benchmark_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.evidence_passed);
            let speed_advantage_failed =
                require_speed_advantage && !output.readiness.native_text_speed_advantage_ready;
            let coverage_evidence_failed = require_coverage_preset.is_some()
                && !output
                    .benchmark_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.coverage_requirement.passed);
            write_json(&output)?;
            if speed_evidence_failed {
                bail!(
                    "feature-parity speed evidence did not satisfy quality-backed LiteParse claims"
                );
            }
            if speed_advantage_failed {
                bail!(
                    "feature-parity speed evidence did not satisfy native-text speed advantage evidence"
                );
            }
            if coverage_evidence_failed {
                let preset = require_coverage_preset
                    .map(CoveragePreset::name)
                    .unwrap_or("requested");
                bail!("feature-parity benchmark evidence did not satisfy coverage preset {preset}");
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
        Commands::OcrCheck {
            pdf,
            page_index,
            ocr_sidecar,
            ocr_command,
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            strict,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_http_url.as_deref(),
                ocr_command_input,
                ocr_timeout_ms,
            )?;
            let output = ocr_check_output(backend, &pdf, page_index, ocr, strict);
            let error = ocr_check_error(&output);
            write_json(&output)?;
            if let Some(error) = error {
                bail!("{error}");
            }
        }
        Commands::Manifest {
            pdf,
            category,
            category_from_path,
            coverage_preset,
            required_category,
            min_category_count,
            ocr_sidecar,
            ocr_command,
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            cache_dir,
            span_geometry,
            jobs,
        } => {
            let page_jobs = if pdf.is_dir() { 1 } else { jobs.max(1) };
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_http_url.as_deref(),
                ocr_command_input,
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
                    category_from_path,
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
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            span_geometry,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_http_url.as_deref(),
                ocr_command_input,
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
                layout_strategy: page.layout_strategy.clone(),
                timings: page.timings.clone(),
                image_artifacts: page.image_artifacts.clone(),
                warnings,
                decision: page.route.clone(),
            })?;
        }
        Commands::WarmBench { pdf, runs, warmup } => {
            run_warm_bench(backend, &pdf, runs, warmup)?;
        }
        Commands::Eval {
            manifest,
            category,
            category_preset,
            ocr_sidecar,
            ocr_command,
            ocr_http_url,
            ocr_command_input,
            ocr_timeout_ms,
            cache_dir,
            span_geometry,
            jobs,
        } => {
            let ocr = OcrOptions::new(
                ocr_sidecar.as_deref(),
                ocr_command.as_deref(),
                ocr_http_url.as_deref(),
                ocr_command_input,
                ocr_timeout_ms,
            )?;
            let category_filter =
                manifest_category_filter_argument(category.as_deref(), category_preset);
            let output = eval_manifest(
                backend,
                &manifest,
                category_filter.as_deref(),
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

impl BaselineSpec {
    pub(crate) fn command_label(&self) -> String {
        self.command.to_string_lossy().into_owned()
    }
}
