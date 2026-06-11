use crate::*;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output as ProcessOutput},
    time::Duration,
};

use anyhow::Result;
use glyphrush_core::sha256_hex;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct BaselineCheckOutput {
    pub(crate) report_version: &'static str,
    pub(crate) run_metadata: BenchmarkRunMetadata,
    pub(crate) strict: bool,
    pub(crate) requested_baseline_presets: Vec<&'static str>,
    pub(crate) baseline_count: usize,
    pub(crate) describe_success_count: usize,
    pub(crate) all_described: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) smoke_pdf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) smoke_document_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) smoke_success_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) all_smoke_passed: Option<bool>,
    pub(crate) baselines: Vec<BaselineCheckResult>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BaselineCheckResult {
    pub(crate) name: String,
    pub(crate) command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<Value>,
    pub(crate) describe: BaselineDescribeCheck,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) smoke: Option<BaselineSmokeCheck>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineDescribeCheck {
    pub(crate) success: bool,
    pub(crate) exit_status: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) timeout_ms: u64,
    pub(crate) wall_us: u128,
    pub(crate) stdout_bytes: u64,
    pub(crate) stderr_bytes: u64,
    pub(crate) stderr_preview: Option<String>,
    pub(crate) valid_json_object: bool,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BaselineSmokeCheck {
    pub(crate) success: bool,
    pub(crate) exit_status: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) timeout_ms: u64,
    pub(crate) wall_us: u128,
    pub(crate) output_bytes: u64,
    pub(crate) stdout_sha256: Option<String>,
    pub(crate) stdout_line_count: usize,
    pub(crate) stdout_word_count: usize,
    pub(crate) stderr_bytes: u64,
    pub(crate) empty_output: bool,
    pub(crate) stderr_preview: Option<String>,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) document_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) successful_documents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failed_documents: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) failure_samples: Vec<BaselineSmokeFailureSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) documents: Vec<BaselineSmokeDocument>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BaselineSmokeFailureSample {
    pub(crate) path: String,
    pub(crate) exit_status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
    pub(crate) error: Option<String>,
    pub(crate) stderr_preview: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BaselineSmokeDocument {
    pub(crate) path: String,
    pub(crate) success: bool,
    pub(crate) exit_status: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) timeout_ms: u64,
    pub(crate) wall_us: u128,
    pub(crate) output_bytes: u64,
    pub(crate) stdout_sha256: Option<String>,
    pub(crate) stdout_line_count: usize,
    pub(crate) stdout_word_count: usize,
    pub(crate) stderr_bytes: u64,
    pub(crate) empty_output: bool,
    pub(crate) stderr_preview: Option<String>,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineBenchOutput {
    pub(crate) name: String,
    pub(crate) command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<Value>,
    pub(crate) description_status: BaselineDescribeCheck,
    pub(crate) comparison: BaselineComparisonOutput,
    pub(crate) success: bool,
    pub(crate) exit_status: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) timeout_ms: u64,
    pub(crate) wall_us: u128,
    pub(crate) output_bytes: u64,
    pub(crate) stdout_sha256: Option<String>,
    pub(crate) stdout_line_count: usize,
    pub(crate) stdout_word_count: usize,
    pub(crate) stderr_bytes: u64,
    pub(crate) empty_output: bool,
    pub(crate) stderr_preview: Option<String>,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
    pub(crate) quality_status: BaselineQualityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality: Option<BaselineQualityOutput>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BaselineQualityStatus {
    Checked,
    NotCheckedNoExpectations,
    NotCheckedTimedOut,
    NotCheckedExecutionFailed,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct BaselineComparisonOutput {
    pub(crate) speed_comparable: bool,
    pub(crate) glyphrush_wall_us: u128,
    pub(crate) baseline_wall_us: u128,
    pub(crate) glyphrush_speedup: f64,
    pub(crate) baseline_speedup: f64,
    pub(crate) glyphrush_text_output_bytes: u64,
    pub(crate) baseline_output_bytes: u64,
    pub(crate) baseline_to_glyphrush_output_bytes: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct BaselineQualityInputs {
    pub(crate) expectations_by_path: BTreeMap<PathBuf, BaselineQualityExpectations>,
    pub(crate) categories_by_path: BTreeMap<PathBuf, String>,
}

impl BaselineQualityInputs {
    pub(crate) fn expectation_for_path(&self, path: &Path) -> Option<&BaselineQualityExpectations> {
        self.expectations_by_path.get(&manifest_path_key(path))
    }

    pub(crate) fn category_for_path(&self, path: &Path) -> String {
        self.categories_by_path
            .get(&manifest_path_key(path))
            .cloned()
            .unwrap_or_else(|| "uncategorized".to_string())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BaselineQualityExpectations {
    pub(crate) category: Option<String>,
    pub(crate) required_text: Vec<String>,
    pub(crate) text_recall: Option<TextRecallExpectation>,
    pub(crate) reading_order: Option<ReadingOrderExpectation>,
    pub(crate) table_structure: Vec<TableStructureExpectation>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineQualityOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<String>,
    pub(crate) passed: bool,
    pub(crate) failed_checks: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) required_text: Option<BaselineRequiredTextOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text_recall: Option<BaselineTextRecallOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reading_order: Option<BaselineReadingOrderOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) table_structure: Option<Vec<BaselineTableStructureOutput>>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineRequiredTextOutput {
    pub(crate) passed: bool,
    pub(crate) expected: Vec<String>,
    pub(crate) missing: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineTextRecallOutput {
    pub(crate) passed: bool,
    pub(crate) word_recall: f64,
    pub(crate) char_recall: f64,
    pub(crate) missing_words: Vec<String>,
    pub(crate) min_word_recall: f64,
    pub(crate) min_char_recall: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineReadingOrderOutput {
    pub(crate) passed: bool,
    pub(crate) score: f64,
    pub(crate) matched: Vec<ReadingOrderMatch>,
    pub(crate) missing: Vec<String>,
    pub(crate) inversion_count: usize,
    pub(crate) inversions: Vec<ReadingOrderInversion>,
    pub(crate) min_score: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BaselineTableStructureOutput {
    pub(crate) page: u32,
    pub(crate) passed: bool,
    pub(crate) extracted_rows: Vec<Vec<String>>,
    pub(crate) row_precision: f64,
    pub(crate) row_recall: f64,
    pub(crate) row_f1: f64,
    pub(crate) missing_rows: Vec<Vec<String>>,
    pub(crate) extra_rows: Vec<Vec<String>>,
    pub(crate) cell_precision: f64,
    pub(crate) cell_recall: f64,
    pub(crate) cell_f1: f64,
    pub(crate) missing_cells: Vec<TableCell>,
    pub(crate) extra_cells: Vec<TableCell>,
    pub(crate) min_row_precision: f64,
    pub(crate) min_row_recall: f64,
    pub(crate) min_row_f1: f64,
    pub(crate) min_cell_precision: f64,
    pub(crate) min_cell_recall: f64,
    pub(crate) min_cell_f1: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusBaselineBenchOutput {
    pub(crate) name: String,
    pub(crate) command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description_status: Option<BaselineDescribeCheck>,
    pub(crate) comparison: BaselineComparisonOutput,
    pub(crate) document_count: usize,
    pub(crate) successful_documents: usize,
    pub(crate) failed_documents: usize,
    pub(crate) timed_out_documents: usize,
    pub(crate) successful_pages: usize,
    pub(crate) failed_pages: usize,
    pub(crate) timed_out_pages: usize,
    pub(crate) empty_output_documents: usize,
    pub(crate) empty_output_pages: usize,
    pub(crate) success_rate: f64,
    pub(crate) quality_status: CorpusBaselineQualityStatus,
    pub(crate) quality_documents: usize,
    pub(crate) quality_unchecked_documents: usize,
    pub(crate) quality_passed_documents: usize,
    pub(crate) quality_failed_documents: usize,
    pub(crate) quality_failed_checks: u32,
    pub(crate) quality_required_text_failed_documents: usize,
    pub(crate) quality_text_recall_failed_documents: usize,
    pub(crate) quality_reading_order_failed_documents: usize,
    pub(crate) quality_table_structure_failed_documents: usize,
    pub(crate) quality_category_summaries: BTreeMap<String, CorpusBaselineQualityCategorySummary>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) quality_unchecked_category_summaries:
        BTreeMap<String, CorpusBaselineQualityUncheckedCategorySummary>,
    pub(crate) quality_pass_rate: f64,
    pub(crate) failure_samples: Vec<CorpusBaselineFailureSample>,
    pub(crate) quality_failure_samples: Vec<CorpusBaselineQualityFailureSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) quality_unchecked_samples: Vec<CorpusBaselineQualityUncheckedSample>,
    pub(crate) wall_us: u128,
    pub(crate) pages_per_sec: f64,
    pub(crate) successful_pages_per_sec: f64,
    pub(crate) output_bytes: u64,
    pub(crate) stderr_bytes: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorpusBaselineQualityStatus {
    Checked,
    PartiallyChecked,
    NotCheckedNoExpectations,
    NotCheckedBaselineFailures,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusBaselineFailureSample {
    pub(crate) path: String,
    pub(crate) exit_status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
    pub(crate) error: Option<String>,
    pub(crate) stderr_preview: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusBaselineQualityFailureSample {
    pub(crate) path: String,
    pub(crate) failed_checks: u32,
    pub(crate) failed_check_types: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusBaselineQualityUncheckedSample {
    pub(crate) path: String,
    pub(crate) category: String,
    pub(crate) quality_status: BaselineQualityStatus,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct CorpusBaselineQualityCategorySummary {
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) passed_documents: usize,
    pub(crate) failed_documents: usize,
    pub(crate) failed_checks: u32,
    pub(crate) quality_pass_rate: f64,
    pub(crate) quality_passed: bool,
    pub(crate) quality_failed: bool,
}

impl CorpusBaselineQualityCategorySummary {
    pub(crate) fn add_document(&mut self, page_count: usize, quality: &BaselineQualityOutput) {
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

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct CorpusBaselineQualityUncheckedCategorySummary {
    pub(crate) document_count: usize,
    pub(crate) page_count: usize,
    pub(crate) not_checked_no_expectations_documents: usize,
    pub(crate) not_checked_timed_out_documents: usize,
    pub(crate) not_checked_execution_failed_documents: usize,
}

impl CorpusBaselineQualityUncheckedCategorySummary {
    pub(crate) fn add_document(&mut self, page_count: usize, status: BaselineQualityStatus) {
        self.document_count += 1;
        self.page_count += page_count;
        match status {
            BaselineQualityStatus::Checked => {}
            BaselineQualityStatus::NotCheckedNoExpectations => {
                self.not_checked_no_expectations_documents += 1;
            }
            BaselineQualityStatus::NotCheckedTimedOut => {
                self.not_checked_timed_out_documents += 1;
            }
            BaselineQualityStatus::NotCheckedExecutionFailed => {
                self.not_checked_execution_failed_documents += 1;
            }
        }
    }
}

pub(crate) fn baseline_comparison(
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

pub(crate) fn run_external_baselines(
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

pub(crate) fn baseline_requirement_error(baselines: &[BaselineBenchOutput]) -> Option<String> {
    if baselines.is_empty() {
        return Some("bench baselines required: no baselines were requested".to_string());
    }

    let failed = baselines
        .iter()
        .filter(|baseline| !baseline.success)
        .count();
    (failed > 0).then(|| format!("bench baselines required: {failed} baseline run(s) failed"))
}

pub(crate) fn corpus_baseline_requirement_error(
    baselines: &[CorpusBaselineBenchOutput],
) -> Option<String> {
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

pub(crate) fn baseline_quality_requirement_error(
    baselines: &[BaselineBenchOutput],
) -> Option<String> {
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

pub(crate) fn corpus_baseline_quality_requirement_error(
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

pub(crate) fn baseline_speedup_requirement_error(
    baselines: &[BaselineBenchOutput],
    requirements: &[BenchmarkSpeedupRequirement],
) -> Option<String> {
    for requirement in requirements {
        let Some(baseline) = baselines
            .iter()
            .find(|baseline| baseline.name == requirement.baseline)
        else {
            return Some(format!(
                "bench speedup required: baseline {} was not run",
                requirement.baseline
            ));
        };
        if !baseline.comparison.speed_comparable {
            return Some(format!(
                "bench speedup required: baseline {} is not speed-comparable",
                requirement.baseline
            ));
        }
        if baseline.comparison.glyphrush_speedup < requirement.min_glyphrush_speedup {
            return Some(format!(
                "bench speedup required: baseline {} glyphrush_speedup {:.3} below required {:.3}",
                requirement.baseline,
                baseline.comparison.glyphrush_speedup,
                requirement.min_glyphrush_speedup
            ));
        }
    }

    None
}

pub(crate) fn corpus_baseline_speedup_requirement_error(
    baselines: &[CorpusBaselineBenchOutput],
    requirements: &[BenchmarkSpeedupRequirement],
) -> Option<String> {
    for requirement in requirements {
        let Some(baseline) = baselines
            .iter()
            .find(|baseline| baseline.name == requirement.baseline)
        else {
            return Some(format!(
                "bench speedup required: baseline {} was not run",
                requirement.baseline
            ));
        };
        if !baseline.comparison.speed_comparable {
            return Some(format!(
                "bench speedup required: baseline {} is not speed-comparable",
                requirement.baseline
            ));
        }
        if baseline.comparison.glyphrush_speedup < requirement.min_glyphrush_speedup {
            return Some(format!(
                "bench speedup required: baseline {} glyphrush_speedup {:.3} below required {:.3}",
                requirement.baseline,
                baseline.comparison.glyphrush_speedup,
                requirement.min_glyphrush_speedup
            ));
        }
    }

    None
}

pub(crate) fn baseline_check<B: PdfBackend>(
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

pub(crate) fn baseline_check_error(output: &BaselineCheckOutput) -> Option<String> {
    if output.baseline_count == 0 {
        return Some("baseline-check requires at least one --baseline".to_string());
    }

    baseline_check_strict_error(output)
}

pub(crate) fn baseline_check_strict_error(output: &BaselineCheckOutput) -> Option<String> {
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

pub(crate) fn check_external_baseline(
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

pub(crate) fn baseline_smoke_targets(path: &Path) -> Result<Vec<DiscoveredPdf>> {
    if path.is_dir() {
        discover_pdfs(path)
    } else {
        Ok(vec![DiscoveredPdf {
            path: path.to_path_buf(),
            label: path.to_string_lossy().into_owned(),
            category: None,
        }])
    }
}

pub(crate) fn describe_external_baseline_probe(
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
            let error_kind =
                baseline_describe_error_kind(&output, timed_output.timed_out, valid_json_object);

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
                    error_kind,
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
                error_kind: Some("spawn_failed"),
            },
        ),
    }
}

pub(crate) fn baseline_describe_error(
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

pub(crate) fn baseline_describe_error_kind(
    output: &ProcessOutput,
    timed_out: bool,
    valid_json_object: bool,
) -> Option<&'static str> {
    if let Some(kind) = baseline_process_error_kind(output, timed_out) {
        Some(kind)
    } else if output.stdout.is_empty() {
        Some("empty_describe_output")
    } else if !valid_json_object {
        Some("invalid_describe_output")
    } else {
        None
    }
}

pub(crate) fn smoke_external_baseline_probe(
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
                error_kind: Some("invalid_smoke_target"),
                document_count: Some(0),
                successful_documents: Some(0),
                failed_documents: Some(0),
                failure_samples: Vec::new(),
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
    let error_kind = documents.iter().find_map(|document| document.error_kind);
    let failure_samples = baseline_smoke_failure_samples(&documents);

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
        error_kind,
        document_count: Some(documents.len()),
        successful_documents: Some(successful_documents),
        failed_documents: Some(failed_documents),
        failure_samples,
        documents,
    }
}

pub(crate) fn baseline_smoke_failure_samples(
    documents: &[BaselineSmokeDocument],
) -> Vec<BaselineSmokeFailureSample> {
    documents
        .iter()
        .filter(|document| !document.success)
        .take(3)
        .map(|document| BaselineSmokeFailureSample {
            path: document.path.clone(),
            exit_status: document.exit_status,
            error_kind: document.error_kind,
            error: document.error.clone(),
            stderr_preview: document.stderr_preview.clone(),
        })
        .collect()
}

pub(crate) fn smoke_external_baseline_document_probe(
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
                stdout_sha256: Some(sha256_hex(&output.stdout)),
                stdout_line_count: stdout_line_count(&output.stdout),
                stdout_word_count: stdout_word_count(&output.stdout),
                stderr_bytes: output.stderr.len() as u64,
                empty_output: output.status.success() && output.stdout.is_empty(),
                stderr_preview: stderr_preview(&output.stderr),
                error: baseline_smoke_error(&output, timed_output.timed_out),
                error_kind: baseline_process_error_kind(&output, timed_output.timed_out),
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
            error_kind: Some("spawn_failed"),
        },
    }
}

pub(crate) fn baseline_smoke_check_from_document(
    document: &BaselineSmokeDocument,
) -> BaselineSmokeCheck {
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
        error_kind: document.error_kind,
        document_count: None,
        successful_documents: None,
        failed_documents: None,
        failure_samples: Vec::new(),
        documents: Vec::new(),
    }
}

pub(crate) fn run_external_baseline(
    path: &Path,
    baseline: &BaselineSpec,
    baseline_quality: Option<&BaselineQualityExpectations>,
    glyphrush_wall_us: u128,
    glyphrush_text_output_bytes: u64,
    timeout: Duration,
) -> BaselineBenchOutput {
    let (description, description_status) = describe_external_baseline_probe(baseline, timeout);
    let target = baseline_description_target(description.as_ref());
    let mut command_process = ProcessCommand::new(&baseline.command);
    command_process.arg(path);
    eprintln!(
        "glyphrush: external baseline start baseline={} pdf={}",
        baseline.name,
        path.display()
    );
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
                target,
                description,
                description_status,
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
                stdout_sha256: Some(sha256_hex(&output.stdout)),
                stdout_line_count: stdout_line_count(&output.stdout),
                stdout_word_count: stdout_word_count(&output.stdout),
                stderr_bytes: output.stderr.len() as u64,
                empty_output: output.status.success() && output.stdout.is_empty(),
                stderr_preview: stderr_preview(&output.stderr),
                error: timed_output
                    .timed_out
                    .then(|| format!("baseline timed out after {} ms", timeout.as_millis())),
                error_kind: baseline_process_error_kind(&output, timed_output.timed_out),
                quality_status,
                quality,
            }
        }
        Err(error) => BaselineBenchOutput {
            name: baseline.name.clone(),
            command,
            target,
            description,
            description_status,
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
            error_kind: Some("spawn_failed"),
            quality_status: baseline_quality
                .map(|_| BaselineQualityStatus::NotCheckedExecutionFailed)
                .unwrap_or(BaselineQualityStatus::NotCheckedNoExpectations),
            quality: None,
        },
    }
}

pub(crate) fn baseline_description_target(description: Option<&Value>) -> Option<String> {
    description?
        .get("target")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(crate) fn baseline_quality_status(
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

pub(crate) fn baseline_required_text_expectations(expectations: &EvalExpectations) -> Vec<String> {
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
    for text in &expectations.baseline_required_text {
        if !required_text.contains(text) {
            required_text.push(text.clone());
        }
    }

    required_text
}

pub(crate) fn baseline_quality_from_stdout(
    stdout: &[u8],
    expectations: &BaselineQualityExpectations,
) -> BaselineQualityOutput {
    let actual_text = String::from_utf8_lossy(stdout);
    let required_text = (!expectations.required_text.is_empty()).then(|| {
        let missing = required_text_missing(&expectations.required_text, &actual_text);
        BaselineRequiredTextOutput {
            passed: missing.is_empty(),
            expected: expectations.required_text.clone(),
            missing,
        }
    });
    let text_recall = expectations.text_recall.as_ref().map(|expectation| {
        let score = text_recall_score(expectation, &actual_text);
        BaselineTextRecallOutput {
            passed: score.passed(expectation),
            word_recall: score.word_recall,
            char_recall: score.char_recall,
            missing_words: score.missing_words,
            min_word_recall: expectation.min_word_recall.unwrap_or(1.0),
            min_char_recall: expectation.min_char_recall.unwrap_or(1.0),
        }
    });
    let reading_order = expectations
        .reading_order
        .as_ref()
        .map(|expectation| baseline_reading_order_from_text(&actual_text, expectation));
    let baseline_table_expectations = expectations
        .table_structure
        .iter()
        .filter(|expectation| expectation.baseline)
        .collect::<Vec<_>>();
    let table_structure = (!baseline_table_expectations.is_empty()).then(|| {
        baseline_table_expectations
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

pub(crate) fn baseline_reading_order_from_text(
    actual_text: &str,
    expectation: &ReadingOrderExpectation,
) -> BaselineReadingOrderOutput {
    let outcome = reading_order_outcome(expectation, actual_text);
    let min_score = expectation.min_score.unwrap_or(1.0);

    BaselineReadingOrderOutput {
        passed: outcome.score >= min_score,
        score: outcome.score,
        matched: outcome.matched,
        missing: outcome.missing,
        inversion_count: outcome.inversion_count,
        inversions: outcome.inversions,
        min_score,
    }
}

pub(crate) fn baseline_table_structure_from_text(
    actual_text: &str,
    expectation: &TableStructureExpectation,
) -> BaselineTableStructureOutput {
    let expected_rows = normalize_table_rows(&expectation.expected_rows);
    let score = score_table_structure_rows(&expected_rows, parse_table_rows(actual_text));

    BaselineTableStructureOutput {
        page: expectation.page,
        passed: table_structure_thresholds_pass(&score, expectation),
        extracted_rows: score.actual_rows,
        row_precision: score.row_precision,
        row_recall: score.row_recall,
        row_f1: score.row_f1,
        missing_rows: score.missing_rows,
        extra_rows: score.extra_rows,
        cell_precision: score.cell_precision,
        cell_recall: score.cell_recall,
        cell_f1: score.cell_f1,
        missing_cells: score.missing_cells,
        extra_cells: score.extra_cells,
        min_row_precision: expectation.min_row_precision.unwrap_or(0.0),
        min_row_recall: expectation.min_row_recall.unwrap_or(1.0),
        min_row_f1: expectation.min_row_f1.unwrap_or(0.0),
        min_cell_precision: expectation.min_cell_precision.unwrap_or(0.0),
        min_cell_recall: expectation.min_cell_recall.unwrap_or(1.0),
        min_cell_f1: expectation.min_cell_f1.unwrap_or(0.0),
    }
}

pub(crate) fn aggregate_corpus_baselines(
    documents: &[CorpusBenchDocument],
    baselines: &[BaselineSpec],
    page_count: usize,
    baseline_quality: Option<&BaselineQualityInputs>,
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
            let quality_unchecked_category_summaries = runs
                .iter()
                .filter(|(_, run)| run.quality.is_none())
                .fold(BTreeMap::new(), |mut summaries, (document, run)| {
                    let category = baseline_quality_category_for_document(
                        baseline_quality,
                        document.source_path.as_path(),
                    );
                    summaries
                        .entry(category)
                        .or_insert_with(CorpusBaselineQualityUncheckedCategorySummary::default)
                        .add_document(document.page_count, run.quality_status);
                    summaries
                });
            let failure_samples = runs
                .iter()
                .filter(|(_, run)| !run.success)
                .take(3)
                .map(|(document, run)| CorpusBaselineFailureSample {
                    path: document.path.clone(),
                    exit_status: run.exit_status,
                    error_kind: run.error_kind,
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
            let quality_unchecked_samples = runs
                .iter()
                .filter(|(_, run)| run.quality.is_none())
                .take(3)
                .map(|(document, run)| CorpusBaselineQualityUncheckedSample {
                    path: document.path.clone(),
                    category: baseline_quality_category_for_document(
                        baseline_quality,
                        document.source_path.as_path(),
                    ),
                    quality_status: run.quality_status,
                })
                .collect();
            let description = runs.iter().find_map(|(_, run)| run.description.clone());
            let target = baseline_description_target(description.as_ref());
            let description_status = runs.first().map(|(_, run)| run.description_status.clone());

            CorpusBaselineBenchOutput {
                name: baseline.name.clone(),
                command: baseline_command,
                target,
                description,
                description_status,
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
                quality_unchecked_category_summaries,
                quality_pass_rate,
                failure_samples,
                quality_failure_samples,
                quality_unchecked_samples,
                wall_us,
                pages_per_sec: pages_per_sec(page_count, wall_us),
                successful_pages_per_sec: pages_per_sec(successful_pages, wall_us),
                output_bytes,
                stderr_bytes,
            }
        })
        .collect()
}

pub(crate) fn baseline_quality_failed_check_types(
    quality: &BaselineQualityOutput,
) -> Vec<&'static str> {
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

pub(crate) fn baseline_quality_category(quality: &BaselineQualityOutput) -> &str {
    quality.category.as_deref().unwrap_or("uncategorized")
}

pub(crate) fn baseline_quality_category_for_document(
    baseline_quality: Option<&BaselineQualityInputs>,
    path: &Path,
) -> String {
    baseline_quality
        .map(|quality| quality.category_for_path(path))
        .unwrap_or_else(|| "uncategorized".to_string())
}
