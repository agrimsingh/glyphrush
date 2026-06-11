use crate::*;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Result, bail};

fn attach_corpus_bench_quality<B: PdfBackend + Sync>(
    backend: &B,
    output: &mut CorpusBenchOutput,
    run_configuration: RunConfiguration,
    eval_category_filter: Option<&str>,
    require_coverage_preset: Option<CoveragePreset>,
    eval_manifest_path: Option<&Path>,
) -> Result<()> {
    let Some(manifest) = eval_manifest_path else {
        return Ok(());
    };
    let quality = {
        let artifacts_by_path = output
            .documents
            .iter()
            .map(|document| (manifest_path_key(&document.source_path), &document.artifact))
            .collect::<BTreeMap<_, _>>();
        eval_manifest_from_artifacts(
            benchmark_run_metadata(backend),
            run_configuration,
            manifest,
            eval_category_filter,
            require_coverage_preset,
            &artifacts_by_path,
            EvalArtifactSelection::ExactManifest,
        )?
    };
    output.category_summaries = benchmark_category_summaries(&output.documents, &quality);
    if let Some(summary) = benchmark_silent_failure_summary(&quality) {
        output.silent_failure_count = Some(summary.count);
        output.silent_failure_pages = Some(summary.pages);
    }
    output.quality_status = BenchQualityStatus::Checked;
    output.quality = Some(quality);
    Ok(())
}

fn attach_pdf_bench_quality<B: PdfBackend + Sync>(
    backend: &B,
    output: &mut BenchOutput,
    run_configuration: RunConfiguration,
    eval_category_filter: Option<&str>,
    require_coverage_preset: Option<CoveragePreset>,
    pdf: &Path,
    eval_manifest_path: Option<&Path>,
) -> Result<()> {
    let Some(manifest) = eval_manifest_path else {
        return Ok(());
    };
    let quality = {
        let mut artifacts_by_path = BTreeMap::new();
        artifacts_by_path.insert(manifest_path_key(pdf), &output.artifact);
        eval_manifest_from_artifacts(
            benchmark_run_metadata(backend),
            run_configuration,
            manifest,
            eval_category_filter,
            require_coverage_preset,
            &artifacts_by_path,
            EvalArtifactSelection::MatchingArtifacts,
        )?
    };
    if let Some(summary) = benchmark_silent_failure_summary(&quality) {
        output.silent_failure_count = Some(summary.count);
        output.silent_failure_pages = Some(summary.pages);
    }
    output.quality_status = BenchQualityStatus::Checked;
    output.quality = Some(quality);
    Ok(())
}

fn enforce_corpus_bench_gates(
    output: &CorpusBenchOutput,
    require_quality: bool,
    require_baselines: bool,
    require_baseline_quality: bool,
    require_coverage_preset: Option<CoveragePreset>,
    require_speedup: &[BenchmarkSpeedupRequirement],
    require_speedup_claim: &[BenchmarkSpeedupRequirement],
) -> Result<()> {
    let failed_checks = output
        .quality
        .as_ref()
        .map(|quality| quality.failed_checks)
        .unwrap_or_default();
    if let Some(error) =
        benchmark_coverage_requirement_error(output.quality.as_ref(), require_coverage_preset)
    {
        bail!("{error}");
    }
    if failed_checks > 0 {
        bail!("bench quality failed: {failed_checks} check(s) failed");
    }
    if require_quality && !matches!(output.quality_status, BenchQualityStatus::Checked) {
        bail!("bench quality required: no eval manifest quality report was checked");
    }
    if require_baselines && let Some(error) = corpus_baseline_requirement_error(&output.baselines) {
        bail!("{error}");
    }
    if require_baseline_quality
        && let Some(error) = corpus_baseline_quality_requirement_error(&output.baselines)
    {
        bail!("{error}");
    }
    if let Some(error) =
        corpus_baseline_speedup_requirement_error(&output.baselines, require_speedup)
    {
        bail!("{error}");
    }
    if let Some(error) =
        speedup_claim_requirement_error(&output.speedup_claims, require_speedup_claim)
    {
        bail!("{error}");
    }
    Ok(())
}

fn enforce_pdf_bench_gates(
    output: &BenchOutput,
    require_quality: bool,
    require_baselines: bool,
    require_baseline_quality: bool,
    require_coverage_preset: Option<CoveragePreset>,
    require_speedup: &[BenchmarkSpeedupRequirement],
    require_speedup_claim: &[BenchmarkSpeedupRequirement],
) -> Result<()> {
    let failed_checks = output
        .quality
        .as_ref()
        .map(|quality| quality.failed_checks)
        .unwrap_or_default();
    if let Some(error) =
        benchmark_coverage_requirement_error(output.quality.as_ref(), require_coverage_preset)
    {
        bail!("{error}");
    }
    if failed_checks > 0 {
        bail!("bench quality failed: {failed_checks} check(s) failed");
    }
    if require_quality && !matches!(output.quality_status, BenchQualityStatus::Checked) {
        bail!("bench quality required: no eval manifest quality report was checked");
    }
    if require_baselines && let Some(error) = baseline_requirement_error(&output.baselines) {
        bail!("{error}");
    }
    if require_baseline_quality
        && let Some(error) = baseline_quality_requirement_error(&output.baselines)
    {
        bail!("{error}");
    }
    if let Some(error) = baseline_speedup_requirement_error(&output.baselines, require_speedup) {
        bail!("{error}");
    }
    if let Some(error) =
        speedup_claim_requirement_error(&output.speedup_claims, require_speedup_claim)
    {
        bail!("{error}");
    }
    Ok(())
}

#[expect(clippy::too_many_arguments)]
pub(crate) fn run_bench<B: PdfBackend + Sync>(
    backend: &B,
    pdf: PathBuf,
    ocr_sidecar: Option<PathBuf>,
    ocr_command: Option<PathBuf>,
    ocr_http_url: Option<String>,
    ocr_command_input: OcrCommandInput,
    ocr_timeout_ms: u64,
    cache_dir: Option<PathBuf>,
    eval_manifest_path: Option<PathBuf>,
    eval_category: Option<String>,
    eval_category_preset: Option<CoveragePreset>,
    require_quality: bool,
    require_baselines: bool,
    require_baseline_quality: bool,
    require_coverage_preset: Option<CoveragePreset>,
    require_speedup: Vec<BenchmarkSpeedupRequirement>,
    require_speedup_claim: Vec<BenchmarkSpeedupRequirement>,
    cache_probe: bool,
    span_geometry: bool,
    baseline: Vec<BaselineSpec>,
    baseline_preset: Option<BaselinePreset>,
    baseline_timeout_ms: u64,
    jobs: usize,
) -> Result<()> {
    let page_jobs = if pdf.is_dir() { 1 } else { jobs.max(1) };
    let options = ExtractionOptions {
        span_geometry,
        page_jobs,
    };
    let ocr = OcrOptions::new(
        ocr_sidecar.as_deref(),
        ocr_command.as_deref(),
        ocr_http_url.as_deref(),
        ocr_command_input,
        ocr_timeout_ms,
    )?;
    let requested_baseline_presets = baseline_preset_names(baseline_preset);
    let baseline_specs = baseline_specs_with_preset(&baseline, baseline_preset);
    let eval_category_filter =
        manifest_category_filter_argument(eval_category.as_deref(), eval_category_preset);
    let bench_config = BenchRunConfig {
        ocr,
        cache_dir: cache_dir.as_deref(),
        cache_probe,
        jobs,
        extraction: options,
        baselines: &baseline_specs,
        requested_baseline_presets: &requested_baseline_presets,
        baseline_timeout: Duration::from_millis(baseline_timeout_ms),
        require_quality,
        require_baselines,
        require_baseline_quality,
        require_coverage_preset,
        required_speedups: &require_speedup,
        required_speedup_claims: &require_speedup_claim,
    };
    let run_configuration = run_configuration(ocr, options);
    let baseline_quality = eval_manifest_path
        .as_deref()
        .map(|manifest| {
            load_baseline_quality_expectations(manifest, eval_category_filter.as_deref())
        })
        .transpose()?;

    if pdf.is_dir() {
        let selected_paths = eval_manifest_path
            .as_deref()
            .filter(|_| eval_category_filter.is_some())
            .map(|manifest| {
                selected_eval_manifest_path_keys(manifest, eval_category_filter.as_deref())
            })
            .transpose()?;
        let mut output = bench_corpus(
            backend,
            &pdf,
            bench_config,
            baseline_quality.as_ref(),
            selected_paths.as_ref(),
        )?;
        attach_corpus_bench_quality(
            backend,
            &mut output,
            run_configuration,
            eval_category_filter.as_deref(),
            require_coverage_preset,
            eval_manifest_path.as_deref(),
        )?;
        output.speedup_claims = corpus_speedup_claims(
            &output.baselines,
            &combined_speedup_claim_requirements(&require_speedup, &require_speedup_claim),
            &output.quality_status,
            output.quality.as_ref(),
        );
        write_json(&output)?;
        enforce_corpus_bench_gates(
            &output,
            require_quality,
            require_baselines,
            require_baseline_quality,
            require_coverage_preset,
            &require_speedup,
            &require_speedup_claim,
        )?;
    } else {
        let mut output = bench_pdf(
            backend,
            &pdf,
            bench_config,
            baseline_quality
                .as_ref()
                .and_then(|quality| quality.expectation_for_path(&pdf)),
        )?;
        attach_pdf_bench_quality(
            backend,
            &mut output,
            run_configuration,
            eval_category_filter.as_deref(),
            require_coverage_preset,
            &pdf,
            eval_manifest_path.as_deref(),
        )?;
        output.speedup_claims = speedup_claims(
            &output.baselines,
            &combined_speedup_claim_requirements(&require_speedup, &require_speedup_claim),
            &output.quality_status,
            output.quality.as_ref(),
        );
        write_json(&output)?;
        enforce_pdf_bench_gates(
            &output,
            require_quality,
            require_baselines,
            require_baseline_quality,
            require_coverage_preset,
            &require_speedup,
            &require_speedup_claim,
        )?;
    }

    Ok(())
}
