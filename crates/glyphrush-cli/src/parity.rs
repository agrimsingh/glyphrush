use crate::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

pub(crate) const FEATURE_PARITY_REPORT_VERSION: &str = "glyphrush-feature-parity-report-v1";

pub(crate) const FEATURE_PARITY_RECOMMENDED_GATE: &str = "bench --eval-manifest <manifest> --eval-category-preset glyphrush-v0-native-text --baseline-preset glyphrush-v0 --require-coverage-preset glyphrush-v0-native-text --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5";

pub(crate) const FEATURE_PARITY_REQUIRED_SPEED_CLAIMS: [(&str, f64); 2] =
    [("liteparse", 2.0), ("liteparse-no-ocr", 1.5)];

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityOutput {
    pub(crate) report_version: &'static str,
    pub(crate) comparison_target: &'static str,
    pub(crate) selected_backend: &'static str,
    pub(crate) run_metadata: BenchmarkRunMetadata,
    pub(crate) quality_policy: &'static str,
    pub(crate) speed_policy: &'static str,
    pub(crate) recommended_gate: &'static str,
    pub(crate) summary: FeatureParitySummary,
    pub(crate) readiness: FeatureParityReadiness,
    pub(crate) capabilities: Vec<FeatureParityCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) benchmark_evidence: Option<FeatureParityBenchmarkEvidence>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct FeatureParitySummary {
    pub(crate) target_capability_count: usize,
    pub(crate) implemented: usize,
    pub(crate) partial: usize,
    pub(crate) planned: usize,
    pub(crate) not_planned: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityReadiness {
    pub(crate) native_text_speed_race_ready: bool,
    pub(crate) native_text_speed_claim_ready: bool,
    pub(crate) native_text_speed_claim_blockers: Vec<String>,
    pub(crate) native_text_speed_advantage_ready: bool,
    pub(crate) native_text_speed_advantage_blockers: Vec<String>,
    pub(crate) full_liteparse_drop_in_ready: bool,
    pub(crate) glyphrush_product_parity_ready: bool,
    pub(crate) native_text_speed_race_gate: &'static str,
    pub(crate) hot_path: FeatureParityHotPathReadiness,
    pub(crate) liteparse_capabilities: FeatureParityCapabilityCoverage,
    pub(crate) remaining_partial: Vec<&'static str>,
    pub(crate) remaining_planned: Vec<&'static str>,
    pub(crate) not_planned_by_design: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityHotPathReadiness {
    pub(crate) capability_count: usize,
    pub(crate) implemented: usize,
    pub(crate) ready: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityCapabilityCoverage {
    pub(crate) target: usize,
    pub(crate) implemented_or_partial: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FeatureParityStatus {
    Implemented,
    Partial,
    Planned,
    NotPlanned,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityCapability {
    pub(crate) id: &'static str,
    pub(crate) area: &'static str,
    pub(crate) liteparse: &'static str,
    pub(crate) glyphrush: &'static str,
    pub(crate) glyphrush_status: FeatureParityStatus,
    pub(crate) hot_path: bool,
    pub(crate) quality_guard: &'static str,
    pub(crate) notes: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkEvidence {
    pub(crate) report_path: String,
    pub(crate) report_version: Option<String>,
    pub(crate) backend: Option<String>,
    pub(crate) quality_status: Option<String>,
    pub(crate) report_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) report_error: Option<FeatureParityBenchmarkReportError>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) quality_categories: Vec<FeatureParityBenchmarkCategoryEvidence>,
    pub(crate) coverage_requirement: FeatureParityBenchmarkCoverageRequirement,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) baseline_quality_unchecked_categories:
        Vec<FeatureParityBenchmarkBaselineQualityUncheckedCategoryEvidence>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) baseline_quality_failures: Vec<FeatureParityBenchmarkBaselineQualityFailureEvidence>,
    pub(crate) required_claim_count: usize,
    pub(crate) claim_count: usize,
    pub(crate) quality_backed_claim_count: usize,
    pub(crate) claim_passed_count: usize,
    pub(crate) evidence_passed: bool,
    pub(crate) missing_required_claims: Vec<String>,
    pub(crate) failed_required_claims: Vec<FeatureParityBenchmarkClaimEvidence>,
    pub(crate) claims: Vec<FeatureParityBenchmarkClaimEvidence>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkReportError {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkCategoryEvidence {
    pub(crate) category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) document_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) page_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failed_checks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality_passed: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkCoverageRequirement {
    pub(crate) preset: String,
    pub(crate) required: bool,
    pub(crate) required_categories: Vec<String>,
    pub(crate) present_categories: Vec<String>,
    pub(crate) missing_categories: Vec<String>,
    pub(crate) passed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkBaselineQualityUncheckedCategoryEvidence {
    pub(crate) baseline: String,
    pub(crate) category: String,
    pub(crate) document_count: u64,
    pub(crate) page_count: u64,
    pub(crate) not_checked_no_expectations_documents: u64,
    pub(crate) not_checked_timed_out_documents: u64,
    pub(crate) not_checked_execution_failed_documents: u64,
}

impl FeatureParityBenchmarkBaselineQualityUncheckedCategoryEvidence {
    pub(crate) fn add_document(&mut self, page_count: u64, quality_status: &str) {
        self.document_count += 1;
        self.page_count += page_count;
        match quality_status {
            "not_checked_no_expectations" => self.not_checked_no_expectations_documents += 1,
            "not_checked_timed_out" => self.not_checked_timed_out_documents += 1,
            "not_checked_execution_failed" => self.not_checked_execution_failed_documents += 1,
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkBaselineQualityFailureEvidence {
    pub(crate) baseline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality_status: Option<String>,
    pub(crate) quality_failed_documents: u64,
    pub(crate) quality_failed_checks: u64,
    pub(crate) failed_categories: Vec<FeatureParityBenchmarkBaselineQualityFailedCategoryEvidence>,
    pub(crate) failure_samples: Vec<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkBaselineQualityFailedCategoryEvidence {
    pub(crate) category: String,
    pub(crate) document_count: u64,
    pub(crate) page_count: u64,
    pub(crate) failed_documents: u64,
    pub(crate) failed_checks: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FeatureParityBenchmarkClaimEvidence {
    pub(crate) baseline: String,
    pub(crate) required_glyphrush_speedup: Option<f64>,
    pub(crate) actual_glyphrush_speedup: Option<f64>,
    pub(crate) speed_comparable: Option<bool>,
    pub(crate) speed_passed: Option<bool>,
    pub(crate) glyphrush_quality_checked: Option<bool>,
    pub(crate) glyphrush_quality_passed: Option<bool>,
    pub(crate) baseline_quality_checked: Option<bool>,
    pub(crate) baseline_quality_passed: Option<bool>,
    pub(crate) glyphrush_quality_backed: Option<bool>,
    pub(crate) quality_backed: Option<bool>,
    pub(crate) quality_blocker: Option<String>,
    pub(crate) claim_passed: Option<bool>,
    pub(crate) status: Option<String>,
}

pub(crate) fn feature_parity_output<B: PdfBackend>(
    backend: &B,
    bench_report: Option<&Path>,
    coverage_preset: Option<CoveragePreset>,
) -> Result<FeatureParityOutput> {
    let capabilities =
        liteparse_feature_parity_capabilities(backend.supports_page_render_for_ocr());
    let summary = feature_parity_summary(&capabilities);
    let benchmark_evidence =
        bench_report.map(|path| feature_parity_benchmark_evidence(path, coverage_preset));
    let readiness = feature_parity_readiness(&capabilities, &summary, benchmark_evidence.as_ref());

    Ok(FeatureParityOutput {
        report_version: FEATURE_PARITY_REPORT_VERSION,
        comparison_target: "liteparse",
        selected_backend: backend.name(),
        run_metadata: benchmark_run_metadata(backend),
        quality_policy: "adaptive_fallback_no_silent_failure",
        speed_policy: "quality_backed_speedup_claims_required",
        recommended_gate: FEATURE_PARITY_RECOMMENDED_GATE,
        summary,
        readiness,
        capabilities,
        benchmark_evidence,
    })
}

pub(crate) fn feature_parity_benchmark_evidence(
    path: &Path,
    coverage_preset: Option<CoveragePreset>,
) -> FeatureParityBenchmarkEvidence {
    let report_bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return feature_parity_invalid_benchmark_evidence(
                path,
                coverage_preset,
                "read_error",
                error.to_string(),
            );
        }
    };
    let report: Value = match serde_json::from_slice(&report_bytes) {
        Ok(report) => report,
        Err(error) => {
            return feature_parity_invalid_benchmark_evidence(
                path,
                coverage_preset,
                "decode_error",
                error.to_string(),
            );
        }
    };
    let claims = report
        .get("speedup_claims")
        .and_then(Value::as_array)
        .map(|claims| {
            claims
                .iter()
                .map(feature_parity_benchmark_claim_evidence)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut missing_required_claims = Vec::new();
    let mut failed_required_claims = Vec::new();
    let mut required_claims = Vec::new();

    for (baseline, required_speedup) in FEATURE_PARITY_REQUIRED_SPEED_CLAIMS {
        let Some(claim) = claims.iter().find(|claim| claim.baseline == baseline) else {
            missing_required_claims.push(baseline.to_string());
            continue;
        };
        required_claims.push(claim.clone());

        let claim_passed = claim.claim_passed.unwrap_or(false);
        let quality_backed = claim.quality_backed.unwrap_or(false);
        let requested_speedup_met = claim
            .required_glyphrush_speedup
            .is_some_and(|actual_required| actual_required >= required_speedup);
        let actual_speedup_met = claim
            .actual_glyphrush_speedup
            .is_some_and(|actual_speedup| actual_speedup >= required_speedup);
        let speed_comparable = claim.speed_comparable.unwrap_or(false);
        let speed_passed = claim.speed_passed.unwrap_or(false);
        if !claim_passed
            || !quality_backed
            || !requested_speedup_met
            || !actual_speedup_met
            || !speed_comparable
            || !speed_passed
        {
            failed_required_claims.push(claim.clone());
        }
    }

    let quality_backed_claim_count = required_claims
        .iter()
        .filter(|claim| claim.quality_backed.unwrap_or(false))
        .count();
    let claim_passed_count = required_claims
        .iter()
        .filter(|claim| claim.claim_passed.unwrap_or(false))
        .count();
    let evidence_passed = missing_required_claims.is_empty()
        && failed_required_claims.is_empty()
        && quality_backed_claim_count == FEATURE_PARITY_REQUIRED_SPEED_CLAIMS.len()
        && claim_passed_count == FEATURE_PARITY_REQUIRED_SPEED_CLAIMS.len();

    let quality_categories = feature_parity_benchmark_quality_categories(&report);
    let coverage_requirement = feature_parity_benchmark_coverage_requirement(
        coverage_preset.unwrap_or(CoveragePreset::GlyphrushV0),
        coverage_preset.is_some(),
        &quality_categories,
    );
    let baseline_quality_unchecked_categories =
        feature_parity_benchmark_baseline_quality_unchecked_categories(&report);
    let baseline_quality_failures = feature_parity_benchmark_baseline_quality_failures(&report);

    FeatureParityBenchmarkEvidence {
        report_path: path.display().to_string(),
        report_version: report
            .get("report_version")
            .and_then(Value::as_str)
            .map(str::to_string),
        backend: report
            .get("backend")
            .and_then(Value::as_str)
            .map(str::to_string),
        quality_status: report
            .get("quality_status")
            .and_then(Value::as_str)
            .map(str::to_string),
        report_valid: true,
        report_error: None,
        quality_categories,
        coverage_requirement,
        baseline_quality_unchecked_categories,
        baseline_quality_failures,
        required_claim_count: FEATURE_PARITY_REQUIRED_SPEED_CLAIMS.len(),
        claim_count: claims.len(),
        quality_backed_claim_count,
        claim_passed_count,
        evidence_passed,
        missing_required_claims,
        failed_required_claims,
        claims,
    }
}

pub(crate) fn feature_parity_invalid_benchmark_evidence(
    path: &Path,
    coverage_preset: Option<CoveragePreset>,
    error_kind: &'static str,
    error_message: String,
) -> FeatureParityBenchmarkEvidence {
    let quality_categories = Vec::new();
    let coverage_requirement = feature_parity_benchmark_coverage_requirement(
        coverage_preset.unwrap_or(CoveragePreset::GlyphrushV0),
        coverage_preset.is_some(),
        &quality_categories,
    );
    let missing_required_claims = FEATURE_PARITY_REQUIRED_SPEED_CLAIMS
        .iter()
        .map(|(baseline, _)| (*baseline).to_string())
        .collect::<Vec<_>>();

    FeatureParityBenchmarkEvidence {
        report_path: path.display().to_string(),
        report_version: None,
        backend: None,
        quality_status: None,
        report_valid: false,
        report_error: Some(FeatureParityBenchmarkReportError {
            kind: error_kind,
            message: error_message,
        }),
        quality_categories,
        coverage_requirement,
        baseline_quality_unchecked_categories: Vec::new(),
        baseline_quality_failures: Vec::new(),
        required_claim_count: FEATURE_PARITY_REQUIRED_SPEED_CLAIMS.len(),
        claim_count: 0,
        quality_backed_claim_count: 0,
        claim_passed_count: 0,
        evidence_passed: false,
        missing_required_claims,
        failed_required_claims: Vec::new(),
        claims: Vec::new(),
    }
}

pub(crate) fn feature_parity_benchmark_coverage_requirement(
    preset: CoveragePreset,
    required: bool,
    quality_categories: &[FeatureParityBenchmarkCategoryEvidence],
) -> FeatureParityBenchmarkCoverageRequirement {
    let present_categories = quality_categories
        .iter()
        .map(|category| category.category.clone())
        .collect::<Vec<_>>();
    let present = present_categories
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let required_categories = preset
        .categories()
        .iter()
        .map(|category| (*category).to_string())
        .collect::<Vec<_>>();
    let missing_categories = required_categories
        .iter()
        .filter(|category| !present.contains(category.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    FeatureParityBenchmarkCoverageRequirement {
        preset: preset.name().to_string(),
        required,
        required_categories,
        present_categories,
        passed: missing_categories.is_empty(),
        missing_categories,
    }
}

pub(crate) fn feature_parity_benchmark_quality_categories(
    report: &Value,
) -> Vec<FeatureParityBenchmarkCategoryEvidence> {
    let summaries = report
        .get("quality")
        .and_then(|quality| quality.get("category_summaries"))
        .or_else(|| report.get("category_summaries"))
        .and_then(Value::as_object);
    let Some(summaries) = summaries else {
        return Vec::new();
    };

    let mut categories = summaries
        .iter()
        .map(
            |(category, summary)| FeatureParityBenchmarkCategoryEvidence {
                category: category.clone(),
                document_count: summary.get("document_count").and_then(Value::as_u64),
                page_count: summary.get("page_count").and_then(Value::as_u64),
                failed_checks: summary.get("failed_checks").and_then(Value::as_u64),
                quality_passed: summary.get("quality_passed").and_then(Value::as_bool),
            },
        )
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| left.category.cmp(&right.category));
    categories
}

#[derive(Clone, Debug)]
pub(crate) struct FeatureParityBenchmarkQualityDocument {
    pub(crate) path: Option<String>,
    pub(crate) category: String,
    pub(crate) page_count: u64,
}

pub(crate) fn feature_parity_benchmark_baseline_quality_unchecked_categories(
    report: &Value,
) -> Vec<FeatureParityBenchmarkBaselineQualityUncheckedCategoryEvidence> {
    let quality_documents = report
        .get("quality")
        .and_then(|quality| quality.get("documents"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let quality_by_fingerprint = quality_documents
        .iter()
        .filter_map(|document| {
            let fingerprint = document.get("document_fingerprint")?.as_str()?;
            Some((
                fingerprint.to_string(),
                FeatureParityBenchmarkQualityDocument {
                    path: document
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    category: document
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or("uncategorized")
                        .to_string(),
                    page_count: document
                        .get("page_count")
                        .and_then(Value::as_u64)
                        .unwrap_or_default(),
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let quality_by_path = quality_documents
        .iter()
        .filter_map(|document| {
            let path = document.get("path")?.as_str()?;
            Some(FeatureParityBenchmarkQualityDocument {
                path: Some(path.to_string()),
                category: document
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("uncategorized")
                    .to_string(),
                page_count: document
                    .get("page_count")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let mut summaries = BTreeMap::<
        (String, String),
        FeatureParityBenchmarkBaselineQualityUncheckedCategoryEvidence,
    >::new();

    let Some(documents) = report.get("documents").and_then(Value::as_array) else {
        return Vec::new();
    };
    for document in documents {
        let fingerprint = document.get("document_fingerprint").and_then(Value::as_str);
        let path = document.get("path").and_then(Value::as_str);
        let quality_document = fingerprint
            .and_then(|fingerprint| quality_by_fingerprint.get(fingerprint))
            .or_else(|| {
                path.and_then(|path| {
                    quality_by_path
                        .iter()
                        .find(|quality| feature_parity_paths_match(quality.path.as_deref(), path))
                })
            });
        let category = quality_document
            .map(|quality| quality.category.as_str())
            .or_else(|| document.get("category").and_then(Value::as_str))
            .unwrap_or("uncategorized");
        let page_count = quality_document
            .map(|quality| quality.page_count)
            .or_else(|| document.get("page_count").and_then(Value::as_u64))
            .unwrap_or_default();
        let Some(baselines) = document.get("baselines").and_then(Value::as_array) else {
            continue;
        };

        for baseline in baselines {
            if baseline
                .get("quality")
                .is_some_and(|quality| !quality.is_null())
            {
                continue;
            }
            let Some(quality_status) = baseline.get("quality_status").and_then(Value::as_str)
            else {
                continue;
            };
            if !quality_status.starts_with("not_checked_") {
                continue;
            }
            let Some(baseline_name) = baseline.get("name").and_then(Value::as_str) else {
                continue;
            };
            let key = (baseline_name.to_string(), category.to_string());
            summaries
                .entry(key.clone())
                .or_insert_with(
                    || FeatureParityBenchmarkBaselineQualityUncheckedCategoryEvidence {
                        baseline: key.0,
                        category: key.1,
                        document_count: 0,
                        page_count: 0,
                        not_checked_no_expectations_documents: 0,
                        not_checked_timed_out_documents: 0,
                        not_checked_execution_failed_documents: 0,
                    },
                )
                .add_document(page_count, quality_status);
        }
    }

    summaries.into_values().collect()
}

pub(crate) fn feature_parity_benchmark_baseline_quality_failures(
    report: &Value,
) -> Vec<FeatureParityBenchmarkBaselineQualityFailureEvidence> {
    let Some(baselines) = report.get("baselines").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut failures = baselines
        .iter()
        .filter_map(|baseline| {
            let baseline_name = baseline.get("name")?.as_str()?.to_string();
            let mut failed_categories = baseline
                .get("quality_category_summaries")
                .and_then(Value::as_object)
                .map(|summaries| {
                    summaries
                        .iter()
                        .filter_map(|(category, summary)| {
                            let failed_documents = summary
                                .get("failed_documents")
                                .and_then(Value::as_u64)
                                .unwrap_or_default();
                            let failed_checks = summary
                                .get("failed_checks")
                                .and_then(Value::as_u64)
                                .unwrap_or_default();
                            let quality_failed = summary
                                .get("quality_failed")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            if failed_documents == 0 && failed_checks == 0 && !quality_failed {
                                return None;
                            }

                            Some(
                                FeatureParityBenchmarkBaselineQualityFailedCategoryEvidence {
                                    category: category.clone(),
                                    document_count: summary
                                        .get("document_count")
                                        .and_then(Value::as_u64)
                                        .unwrap_or_default(),
                                    page_count: summary
                                        .get("page_count")
                                        .and_then(Value::as_u64)
                                        .unwrap_or_default(),
                                    failed_documents,
                                    failed_checks,
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            failed_categories.sort_by(|left, right| left.category.cmp(&right.category));

            let failure_samples = baseline
                .get("quality_failure_samples")
                .and_then(Value::as_array)
                .map(|samples| samples.iter().take(8).cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let quality_failed_documents = baseline
                .get("quality_failed_documents")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| {
                    failed_categories
                        .iter()
                        .map(|category| category.failed_documents)
                        .sum()
                });
            let quality_failed_checks = baseline
                .get("quality_failed_checks")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| {
                    failed_categories
                        .iter()
                        .map(|category| category.failed_checks)
                        .sum()
                });

            if quality_failed_documents == 0
                && quality_failed_checks == 0
                && failed_categories.is_empty()
                && failure_samples.is_empty()
            {
                return None;
            }

            Some(FeatureParityBenchmarkBaselineQualityFailureEvidence {
                baseline: baseline_name,
                target: baseline
                    .get("target")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                quality_status: baseline
                    .get("quality_status")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                quality_failed_documents,
                quality_failed_checks,
                failed_categories,
                failure_samples,
            })
        })
        .collect::<Vec<_>>();
    failures.sort_by(|left, right| left.baseline.cmp(&right.baseline));
    failures
}

pub(crate) fn feature_parity_paths_match(quality_path: Option<&str>, document_path: &str) -> bool {
    let Some(quality_path) = quality_path else {
        return false;
    };
    quality_path == document_path
        || quality_path.ends_with(&format!("/{document_path}"))
        || document_path.ends_with(&format!("/{quality_path}"))
}

pub(crate) fn feature_parity_benchmark_claim_evidence(
    value: &Value,
) -> FeatureParityBenchmarkClaimEvidence {
    let glyphrush_quality_checked = value
        .get("glyphrush_quality_checked")
        .and_then(Value::as_bool);
    let glyphrush_quality_passed = value
        .get("glyphrush_quality_passed")
        .and_then(Value::as_bool);
    let baseline_quality_checked = value
        .get("baseline_quality_checked")
        .and_then(Value::as_bool);
    let baseline_quality_passed = value
        .get("baseline_quality_passed")
        .and_then(Value::as_bool);
    let glyphrush_quality_backed = value
        .get("glyphrush_quality_backed")
        .and_then(Value::as_bool)
        .or_else(|| Some(glyphrush_quality_checked? && glyphrush_quality_passed?));
    let quality_blocker = value
        .get("quality_blocker")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if glyphrush_quality_checked == Some(false) {
                Some("glyphrush_quality_not_checked".to_string())
            } else if glyphrush_quality_checked == Some(true)
                && glyphrush_quality_passed == Some(false)
            {
                Some("glyphrush_quality_failed".to_string())
            } else if baseline_quality_checked == Some(false) {
                Some("baseline_quality_not_checked".to_string())
            } else if baseline_quality_checked == Some(true)
                && baseline_quality_passed == Some(false)
            {
                Some("baseline_quality_failed".to_string())
            } else {
                None
            }
        });

    FeatureParityBenchmarkClaimEvidence {
        baseline: value
            .get("baseline")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        required_glyphrush_speedup: value
            .get("required_glyphrush_speedup")
            .and_then(Value::as_f64),
        actual_glyphrush_speedup: value
            .get("actual_glyphrush_speedup")
            .and_then(Value::as_f64),
        speed_comparable: value.get("speed_comparable").and_then(Value::as_bool),
        speed_passed: value.get("speed_passed").and_then(Value::as_bool),
        glyphrush_quality_checked,
        glyphrush_quality_passed,
        baseline_quality_checked,
        baseline_quality_passed,
        glyphrush_quality_backed,
        quality_backed: value.get("quality_backed").and_then(Value::as_bool),
        quality_blocker,
        claim_passed: value.get("claim_passed").and_then(Value::as_bool),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

pub(crate) fn liteparse_feature_parity_capabilities(
    supports_page_render_for_ocr: bool,
) -> Vec<FeatureParityCapability> {
    let page_render_for_ocr = if supports_page_render_for_ocr {
        FeatureParityCapability {
            id: "page_render_for_ocr",
            area: "ocr",
            liteparse: "render_pages_for_ocr",
            glyphrush: "pdfium_rendered_image_command_or_http_input",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "rendered_image_ocr_check_and_render_page_fallback_counts",
            notes: "PDFium renders only OCR-routed pages to temporary PPM files for command or HTTP adapters, records render timing and fallback-action counts, and removes temporary image files after OCR returns.",
        }
    } else {
        FeatureParityCapability {
            id: "page_render_for_ocr",
            area: "ocr",
            liteparse: "render_pages_for_ocr",
            glyphrush: "pdfium_rendered_image_command_or_http_input",
            glyphrush_status: FeatureParityStatus::Partial,
            hot_path: false,
            quality_guard: "ocr_check_render_backend_required",
            notes: "Rendered-image OCR handoff exists for the PDFium backend; non-rendering backends report the limitation instead of silently switching OCR input contracts.",
        }
    };

    vec![
        FeatureParityCapability {
            id: "native_text_extraction",
            area: "extraction",
            liteparse: "pdfium_native_text",
            glyphrush: "lopdf_or_pdfium_native_text",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: true,
            quality_guard: "text_recall_and_silent_failure_eval",
            notes: "PDFium is the default fast backend when the pdfium feature is enabled; lopdf remains the dependency-light explicit backend and the auto fallback in plain builds.",
        },
        FeatureParityCapability {
            id: "page_classifier_quality_flags",
            area: "quality",
            liteparse: "implicit_parser_behavior",
            glyphrush: "explicit_page_routes_flags_and_reasons",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: true,
            quality_guard: "requires_ocr_low_confidence_unsupported_flags",
            notes: "Glyphrush treats uncertain extraction as a reported condition instead of silently claiming success.",
        },
        FeatureParityCapability {
            id: "structured_json_text_markdown_exports",
            area: "outputs",
            liteparse: "text_markdown_json_bindings",
            glyphrush: "document_artifact_plus_text_and_markdown",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: true,
            quality_guard: "derived_outputs_from_single_artifact",
            notes: "The structured artifact is the source of truth; text and markdown are derived views.",
        },
        FeatureParityCapability {
            id: "quality_backed_benchmarking",
            area: "evaluation",
            liteparse: "latency_benchmarks",
            glyphrush: "strict_speedup_claim_gate",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "require_speedup_claim_requires_glyphrush_and_baseline_quality",
            notes: "This is intentionally stronger than a speed-only comparison.",
        },
        FeatureParityCapability {
            id: "span_geometry_layout",
            area: "layout",
            liteparse: "layout_projection_and_character_geometry",
            glyphrush: "bounded_span_geometry_and_full_width_aware_layout_blocks",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "layout_uncertain_flag_reading_order_and_span_bbox_eval",
            notes: "Glyphrush avoids always-on per-character metadata, preserves full-width bands, fragmented full-width heading rows, fragmented middle cross-column bands, fragmented short section separators, leading, middle, and trailing cross-column bands, conservative short section separators, narrow academic gutters with trailing centered page numbers, column-row bands that keep centered banners, gutter-straddling rows, and trailing page numbers out of column splits, and clearly separated 2-5 column reading order when span geometry is available, seeds bounded span-bbox manifest samples, reports the per-page reading-order strategy as layout_strategy, escalates layout work when signals require it, and flags unresolved multi-column evidence as layout_uncertain with a column_layout_unresolved reason instead of silently interleaving columns. Labeled real-PDF reading-order and span-bbox fixtures gate this in test/corpus.v0.layout.json.",
        },
        FeatureParityCapability {
            id: "ocr",
            area: "ocr",
            liteparse: "tesseract_or_http_ocr",
            glyphrush: "sidecar_command_http_or_tesseract_rendered_image_wrapper_invoked_page_selectively",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "page_selective_adapter_preflight_and_requires_ocr_flag",
            notes: "OCR is adapter-based, supports sidecar, generic command, HTTP endpoint, and an explicit local Tesseract rendered-image wrapper, invokes adapters only for OCR-routed pages, exposes ocr-check preflights, and stays outside the default hot path.",
        },
        page_render_for_ocr,
        FeatureParityCapability {
            id: "table_recovery",
            area: "tables",
            liteparse: "layout_projection_tables",
            glyphrush: "table_likelihood_and_basic_structure_recovery_with_empty_cell_preservation",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "table_uncertain_flag_and_table_structure_eval",
            notes: "Current table support is conservative, tied to explicit uncertainty flags, preserves blank cells for delimited text, fixed-width whitespace, fixed-width wrapped descriptor fragments, key-value metadata rows, embedded pin/function tables, number-first pin-description tables, fragmented symbol/rating tables, bullet/leader spec tables, electrical-characteristics min/typ/max tables, AWINIC parameter/test-condition electrical tables with split frequency ranges, split ppm/degree-C units, ohm values, thermal shutdown rows, and footer exclusion, parameter/symbol/conditions electrical tables with condition continuations and thermal/EN threshold tail rows, reflow-profile Sn-Pb/Pb-free assembly tables, classification-temperature package/volume tables, package pin-description tables, part-number ordering tables, OMB-style budget projection tables, header-guided whitespace rows with table-header cues, same-line or wrapped multi-word descriptor cells, two-column descriptor/value rows, trailing descriptor continuations, header-guided trailing blank cells, header-guided section rows, and prefixed leading delimited/text-table captions outside table grids, aligned whitespace and positioned interior section rows, keeps positioned captions outside table grids, rejects routed description prose without table-header cues, rejects positioned-table windows that are really the page's own two-column prose lines so figure-ruling-routed academic pages keep column reading order instead of fake parallel-prose tables, recovers column-ruled grids from extracted vector ruling lines (composed through nested form XObject transforms) with text-row row structure, blank-cell preservation, wrapped-descriptor merges, and diagram-lattice rejection so filled vouchers and ruled month-grid forms produce structured cells, and aligned positioned rows including same-line fragmented positioned cells, first-column positioned section rows, fragmented first-column positioned section rows, interior positioned condition/note rows, multi-cell wrapped continuations, and same-column wrapped header rows when table recovery is routed, splits side-by-side per-column tables on two-column pages instead of mashing them into one grid, and exposes structured grids to eval text anchors. Labeled real-PDF table fixtures pass across datasheet, invoice/form, budget, and academic categories in test/corpus.v0.layout.json and test/corpus.v0.json. Two-level header groups, merged cells, and cross-page continuation stitching remain conservative and are tracked as later advanced-table-semantics work, with table_uncertain flags preserved.",
        },
        FeatureParityCapability {
            id: "artifact_cache_snapshots",
            area: "runtime",
            liteparse: "fresh_parse_by_default",
            glyphrush: "cache_dir_snapshot_envelope_artifact_reuse",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "cache_key_includes_parser_backend_ocr_options",
            notes: "JSON cache snapshots use explicit schema/parser/backend/source provenance, reuse artifacts on warm runs, and treat unreadable or invalid snapshots as explicit misses with cache_snapshot_ignored warnings; mmap-friendly snapshots remain a later runtime optimization, not a LiteParse parity blocker.",
        },
        FeatureParityCapability {
            id: "python_node_bindings",
            area: "bindings",
            liteparse: "python_node_bindings",
            glyphrush: "thin_python_node_parse_inspect_debug_eval_bench_manifest_preflight_wrappers",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "bindings_must_share_native_core_artifact",
            notes: "Dependency-free Python and Node wrappers delegate parse, text and markdown derived-output helpers, inspect-page triage, debug-page, OCR/backend/baseline preflights, feature-parity reports, eval-manifest quality gates, benchmark reports, and manifest generation to the native CLI artifact paths.",
        },
        FeatureParityCapability {
            id: "wasm_bindings",
            area: "bindings",
            liteparse: "wasm_bindings",
            glyphrush: "wasm_parse_pdf_bytes_over_native_core_artifact",
            glyphrush_status: FeatureParityStatus::Implemented,
            hot_path: false,
            quality_guard: "wasm_must_share_native_core_artifact",
            notes: "bindings/wasm wraps glyphrush-core and the shared glyphrush-lopdf extraction crate behind wasm-bindgen: PDF bytes in, the identical JSON document artifact out, verified by a deep-equal parity test against the CLI's lopdf backend (only timing and source-mtime fields are exempt). OCR adapters are process/network seams that do not apply to the wasm surface; OCR-required pages keep their requires_ocr flags and warnings exactly like a no-OCR CLI run.",
        },
        FeatureParityCapability {
            id: "mupdf_backend",
            area: "backend",
            liteparse: "pdfium_core",
            glyphrush: "mupdf_adapter_rejected_on_license",
            glyphrush_status: FeatureParityStatus::NotPlanned,
            hot_path: false,
            quality_guard: "backend_check_reports_adapter_status",
            notes: "MuPDF is AGPL-3.0 licensed while Glyphrush is MIT; wiring it as a shipped backend would constrain every downstream distribution, and the BSD-licensed PDFium adapter already provides the measured native-text fast path with rendered-image OCR handoff. Rejected deliberately rather than left as an open promise; backend-check continues to report the adapter slot so the decision stays visible.",
        },
        FeatureParityCapability {
            id: "bundled_builtin_ocr",
            area: "ocr",
            liteparse: "ocr_dependency_integrated_with_parser_package",
            glyphrush: "optional_external_ocr_adapters",
            glyphrush_status: FeatureParityStatus::NotPlanned,
            hot_path: false,
            quality_guard: "no_hidden_ocr_or_network_dependency",
            notes: "Bundling OCR into the default parser would violate the hot-path dependency policy.",
        },
    ]
}

pub(crate) fn feature_parity_summary(
    capabilities: &[FeatureParityCapability],
) -> FeatureParitySummary {
    let mut summary = FeatureParitySummary {
        target_capability_count: capabilities.len(),
        ..FeatureParitySummary::default()
    };
    for capability in capabilities {
        match capability.glyphrush_status {
            FeatureParityStatus::Implemented => summary.implemented += 1,
            FeatureParityStatus::Partial => summary.partial += 1,
            FeatureParityStatus::Planned => summary.planned += 1,
            FeatureParityStatus::NotPlanned => summary.not_planned += 1,
        }
    }
    summary
}

pub(crate) fn feature_parity_readiness(
    capabilities: &[FeatureParityCapability],
    summary: &FeatureParitySummary,
    benchmark_evidence: Option<&FeatureParityBenchmarkEvidence>,
) -> FeatureParityReadiness {
    let hot_path_capability_count = capabilities
        .iter()
        .filter(|capability| capability.hot_path)
        .count();
    let hot_path_implemented = capabilities
        .iter()
        .filter(|capability| {
            capability.hot_path && capability.glyphrush_status == FeatureParityStatus::Implemented
        })
        .count();
    let hot_path_ready =
        hot_path_capability_count > 0 && hot_path_implemented == hot_path_capability_count;
    let quality_gate_ready = capabilities.iter().any(|capability| {
        capability.id == "quality_backed_benchmarking"
            && capability.glyphrush_status == FeatureParityStatus::Implemented
    });
    let native_text_speed_race_ready = hot_path_ready && quality_gate_ready;
    let (native_text_speed_claim_ready, native_text_speed_claim_blockers) =
        feature_parity_speed_claim_readiness(native_text_speed_race_ready, benchmark_evidence);
    let (native_text_speed_advantage_ready, native_text_speed_advantage_blockers) =
        feature_parity_speed_advantage_readiness(native_text_speed_race_ready, benchmark_evidence);

    FeatureParityReadiness {
        native_text_speed_race_ready,
        native_text_speed_claim_ready,
        native_text_speed_claim_blockers,
        native_text_speed_advantage_ready,
        native_text_speed_advantage_blockers,
        full_liteparse_drop_in_ready: summary.partial == 0
            && summary.planned == 0
            && summary.not_planned == 0,
        glyphrush_product_parity_ready: summary.partial == 0 && summary.planned == 0,
        native_text_speed_race_gate: FEATURE_PARITY_RECOMMENDED_GATE,
        hot_path: FeatureParityHotPathReadiness {
            capability_count: hot_path_capability_count,
            implemented: hot_path_implemented,
            ready: hot_path_ready,
        },
        liteparse_capabilities: FeatureParityCapabilityCoverage {
            target: summary.target_capability_count,
            implemented_or_partial: summary.implemented + summary.partial,
        },
        remaining_partial: capabilities
            .iter()
            .filter(|capability| capability.glyphrush_status == FeatureParityStatus::Partial)
            .map(|capability| capability.id)
            .collect(),
        remaining_planned: capabilities
            .iter()
            .filter(|capability| capability.glyphrush_status == FeatureParityStatus::Planned)
            .map(|capability| capability.id)
            .collect(),
        not_planned_by_design: capabilities
            .iter()
            .filter(|capability| capability.glyphrush_status == FeatureParityStatus::NotPlanned)
            .map(|capability| capability.id)
            .collect(),
    }
}

pub(crate) fn feature_parity_speed_claim_readiness(
    capability_ready: bool,
    benchmark_evidence: Option<&FeatureParityBenchmarkEvidence>,
) -> (bool, Vec<String>) {
    let mut blockers = Vec::new();

    if !capability_ready {
        blockers.push("native_text_speed_race_capabilities_not_ready".to_string());
    }

    let Some(benchmark_evidence) = benchmark_evidence else {
        blockers.push("missing_benchmark_evidence".to_string());
        return (false, blockers);
    };

    if !benchmark_evidence.report_valid {
        blockers.push("invalid_benchmark_report".to_string());
    } else if !benchmark_evidence.evidence_passed {
        blockers.push("missing_quality_backed_liteparse_claims".to_string());
    }

    let coverage_requirement = &benchmark_evidence.coverage_requirement;
    if !coverage_requirement.required {
        blockers.push("missing_coverage_preset".to_string());
    } else if !coverage_requirement.passed {
        blockers.push("coverage_preset_missing_categories".to_string());
    }

    (blockers.is_empty(), blockers)
}

pub(crate) fn feature_parity_speed_advantage_readiness(
    capability_ready: bool,
    benchmark_evidence: Option<&FeatureParityBenchmarkEvidence>,
) -> (bool, Vec<String>) {
    let mut blockers = Vec::new();

    if !capability_ready {
        blockers.push("native_text_speed_race_capabilities_not_ready".to_string());
    }

    let Some(benchmark_evidence) = benchmark_evidence else {
        blockers.push("missing_benchmark_evidence".to_string());
        return (false, blockers);
    };

    if !benchmark_evidence.report_valid {
        blockers.push("invalid_benchmark_report".to_string());
    } else {
        let mut missing_required_claims = false;
        let mut speed_evidence_failed = false;
        let mut glyphrush_quality_not_backed = false;
        let mut baseline_quality_not_checked = false;

        for (baseline, required_speedup) in FEATURE_PARITY_REQUIRED_SPEED_CLAIMS {
            let Some(claim) = benchmark_evidence
                .claims
                .iter()
                .find(|claim| claim.baseline == baseline)
            else {
                missing_required_claims = true;
                continue;
            };

            let requested_speedup_met = claim
                .required_glyphrush_speedup
                .is_some_and(|actual_required| actual_required >= required_speedup);
            let actual_speedup_met = claim
                .actual_glyphrush_speedup
                .is_some_and(|actual_speedup| actual_speedup >= required_speedup);
            let speed_comparable = claim.speed_comparable.unwrap_or(false);
            let speed_passed = claim.speed_passed.unwrap_or(false);
            if !requested_speedup_met || !actual_speedup_met || !speed_comparable || !speed_passed {
                speed_evidence_failed = true;
            }
            if claim.glyphrush_quality_backed != Some(true) {
                glyphrush_quality_not_backed = true;
            }
            if claim.baseline_quality_checked != Some(true) {
                baseline_quality_not_checked = true;
            }
        }

        if missing_required_claims {
            blockers.push("missing_required_liteparse_claims".to_string());
        }
        if speed_evidence_failed {
            blockers.push("speed_evidence_failed".to_string());
        }
        if glyphrush_quality_not_backed {
            blockers.push("glyphrush_quality_not_backed".to_string());
        }
        if baseline_quality_not_checked {
            blockers.push("baseline_quality_not_checked".to_string());
        }
    }

    let coverage_requirement = &benchmark_evidence.coverage_requirement;
    if !coverage_requirement.required {
        blockers.push("missing_coverage_preset".to_string());
    } else if !coverage_requirement.passed {
        blockers.push("coverage_preset_missing_categories".to_string());
    }

    (blockers.is_empty(), blockers)
}
