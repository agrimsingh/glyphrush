use crate::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use glyphrush_core::{
    BBox, DocumentArtifact, LayoutBlockKind, LayoutTable, PageArtifact, PageQuality, TextSpan,
};
use serde::Serialize;
use serde_json::json;

pub(crate) fn required_text_missing(expected: &[String], actual_text: &str) -> Vec<String> {
    expected
        .iter()
        .filter(|text| !required_text_anchor_matches(actual_text, text))
        .cloned()
        .collect()
}

pub(crate) fn text_recall_score(
    expectation: &TextRecallExpectation,
    actual_text: &str,
) -> TextRecallScore {
    TextRecallScore {
        word_recall: multiset_recall(
            normalize_words(&expectation.expected),
            normalize_words(actual_text),
        ),
        char_recall: multiset_recall(
            normalize_chars(&expectation.expected),
            normalize_chars(actual_text),
        ),
        missing_words: missing_multiset_items(
            normalize_words(&expectation.expected),
            normalize_words(actual_text),
        ),
    }
}

pub(crate) fn reading_order_outcome(
    expectation: &ReadingOrderExpectation,
    actual_text: &str,
) -> ReadingOrderOutcome {
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
    ReadingOrderOutcome {
        score,
        matched,
        missing,
        inversion_count,
        inversions,
    }
}

pub(crate) fn table_structure_thresholds_pass(
    score: &TableStructureScore,
    expectation: &TableStructureExpectation,
) -> bool {
    score.row_precision >= expectation.min_row_precision.unwrap_or(0.0)
        && score.row_recall >= expectation.min_row_recall.unwrap_or(1.0)
        && score.row_f1 >= expectation.min_row_f1.unwrap_or(0.0)
        && score.cell_precision >= expectation.min_cell_precision.unwrap_or(0.0)
        && score.cell_recall >= expectation.min_cell_recall.unwrap_or(1.0)
        && score.cell_f1 >= expectation.min_cell_f1.unwrap_or(0.0)
}

pub(crate) fn add_table_summary_counts(
    summary: &mut DebugLayoutSummary,
    rows: usize,
    cells: usize,
    cells_with_bbox: usize,
) {
    *summary.table_rows.get_or_insert(0) += rows;
    *summary.table_cells.get_or_insert(0) += cells;
    *summary.table_cells_with_bbox.get_or_insert(0) += cells_with_bbox;
}

pub(crate) fn normalize_min_category_counts(
    categories: &BTreeMap<String, usize>,
    filter: Option<&str>,
) -> BTreeMap<String, usize> {
    let filter = normalize_manifest_category_filter(filter);
    categories
        .iter()
        .filter_map(|(category, count)| {
            let category = normalize_manifest_category(Some(category))?;
            if *count == 0 {
                return None;
            }
            if !manifest_category_filter_matches(&filter, &category) {
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

pub(crate) fn category_coverage(
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

pub(crate) fn resolve_manifest_path(manifest_dir: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        manifest_dir.join(path)
    }
}

pub(crate) fn insert_check<T>(
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

pub(crate) fn insert_json_check(
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

pub(crate) fn insert_layout_block_counts_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    name: String,
    expected: DebugLayoutSummary,
    actual: Option<DebugLayoutSummary>,
) {
    let passed = actual
        .map(|actual| layout_summary_matches_expectation(expected, actual))
        .unwrap_or(false);
    checks.insert(
        name,
        EvalCheckOutput {
            passed,
            expected: json!(expected),
            actual: actual
                .map(|actual| json!(actual))
                .unwrap_or(serde_json::Value::Null),
        },
    );
}

pub(crate) fn layout_summary_matches_expectation(
    expected: DebugLayoutSummary,
    actual: DebugLayoutSummary,
) -> bool {
    expected.block_count == actual.block_count
        && expected.paragraph_blocks == actual.paragraph_blocks
        && expected.heading_blocks == actual.heading_blocks
        && expected.list_blocks == actual.list_blocks
        && expected.table_blocks == actual.table_blocks
        && expected.figure_blocks == actual.figure_blocks
        && expected.header_blocks == actual.header_blocks
        && expected.footer_blocks == actual.footer_blocks
        && expected
            .table_rows
            .map(|expected| actual.table_rows == Some(expected))
            .unwrap_or(true)
        && expected
            .table_cells
            .map(|expected| actual.table_cells == Some(expected))
            .unwrap_or(true)
        && expected
            .table_cells_with_bbox
            .map(|expected| actual.table_cells_with_bbox == Some(expected))
            .unwrap_or(true)
}

pub(crate) fn insert_required_text_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    required_text: &[String],
    artifact: &DocumentArtifact,
) {
    let missing = required_text_missing(required_text, &document_text(artifact));

    checks.insert(
        "required_text".to_string(),
        EvalCheckOutput {
            passed: missing.is_empty(),
            expected: json!(required_text),
            actual: json!({ "missing": missing }),
        },
    );
}

pub(crate) fn insert_required_warnings_check(
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

pub(crate) fn insert_text_recall_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &TextRecallExpectation,
    artifact: &DocumentArtifact,
) {
    let score = text_recall_score(expectation, &document_text(artifact));
    let min_word_recall = expectation.min_word_recall.unwrap_or(1.0);
    let min_char_recall = expectation.min_char_recall.unwrap_or(1.0);

    checks.insert(
        "text_recall".to_string(),
        EvalCheckOutput {
            passed: score.passed(expectation),
            expected: json!({
                "min_word_recall": min_word_recall,
                "min_char_recall": min_char_recall,
            }),
            actual: json!({
                "word_recall": score.word_recall,
                "char_recall": score.char_recall,
                "missing_words": score.missing_words,
            }),
        },
    );
}

pub(crate) fn insert_reading_order_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &ReadingOrderExpectation,
    artifact: &DocumentArtifact,
) {
    let outcome = reading_order_outcome(expectation, &document_text(artifact));
    let min_score = expectation.min_score.unwrap_or(1.0);

    checks.insert(
        "reading_order".to_string(),
        EvalCheckOutput {
            passed: outcome.score >= min_score,
            expected: json!({
                "expected_sequence": expectation.expected_sequence,
                "min_score": min_score,
            }),
            actual: json!({
                "score": outcome.score,
                "matched": outcome.matched,
                "missing": outcome.missing,
                "inversion_count": outcome.inversion_count,
                "inversions": outcome.inversions,
            }),
        },
    );
}

pub(crate) fn reading_order_score(
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

pub(crate) fn insert_ocr_required_classification_check(
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

pub(crate) fn insert_quality_flag_classification_check(
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

pub(crate) fn insert_silent_failures_check(
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

pub(crate) fn expected_empty_text_output_pages(expectations: &EvalExpectations) -> BTreeSet<u32> {
    expectations
        .pages
        .iter()
        .filter(|page| page.empty_text_output == Some(true))
        .map(|page| page.index)
        .collect()
}

pub(crate) fn expected_quality_flags_by_page(
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

pub(crate) fn insert_expected_quality_flag(
    flags_by_page: &mut BTreeMap<u32, Vec<PageQuality>>,
    page_index: u32,
    flag: PageQuality,
) {
    let flags = flags_by_page.entry(page_index).or_default();
    if !flags.contains(&flag) {
        flags.push(flag);
    }
}

pub(crate) fn page_quality_name(flag: &PageQuality) -> &'static str {
    match flag {
        PageQuality::RequiresOcr => "requires_ocr",
        PageQuality::LowConfidenceText => "low_confidence_text",
        PageQuality::BrokenEncoding => "broken_encoding",
        PageQuality::LayoutUncertain => "layout_uncertain",
        PageQuality::TableUncertain => "table_uncertain",
        PageQuality::UnsupportedFeature => "unsupported_feature",
    }
}

pub(crate) fn insert_table_structure_check(
    checks: &mut BTreeMap<String, EvalCheckOutput>,
    expectation: &TableStructureExpectation,
    expectation_index: Option<usize>,
    artifact: &DocumentArtifact,
) {
    let expected_rows = normalize_table_rows(&expectation.expected_rows);
    let actual_rows = best_table_rows_for_expectation(artifact, expectation.page, &expected_rows);
    let score = score_table_structure_rows(&expected_rows, actual_rows);
    let min_row_precision = expectation.min_row_precision.unwrap_or(0.0);
    let min_row_recall = expectation.min_row_recall.unwrap_or(1.0);
    let min_row_f1 = expectation.min_row_f1.unwrap_or(0.0);
    let min_cell_precision = expectation.min_cell_precision.unwrap_or(0.0);
    let min_cell_recall = expectation.min_cell_recall.unwrap_or(1.0);
    let min_cell_f1 = expectation.min_cell_f1.unwrap_or(0.0);
    let check_key = match expectation_index {
        Some(index) => format!(
            "table_structure.page_{:06}.expectation_{index:06}",
            expectation.page
        ),
        None => format!("table_structure.page_{:06}", expectation.page),
    };

    checks.insert(
        check_key,
        EvalCheckOutput {
            passed: table_structure_thresholds_pass(&score, expectation),
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
                "extracted_rows": score.actual_rows,
                "row_precision": score.row_precision,
                "row_recall": score.row_recall,
                "row_f1": score.row_f1,
                "missing_rows": score.missing_rows,
                "extra_rows": score.extra_rows,
                "cell_precision": score.cell_precision,
                "cell_recall": score.cell_recall,
                "cell_f1": score.cell_f1,
                "missing_cells": score.missing_cells,
                "extra_cells": score.extra_cells,
            }),
        },
    );
}

pub(crate) fn best_table_rows_for_expectation(
    artifact: &DocumentArtifact,
    page_index: u32,
    expected_rows: &[Vec<String>],
) -> Vec<Vec<String>> {
    table_row_candidates_for_page(artifact, page_index, expected_rows.len())
        .into_iter()
        .max_by_key(|candidate| table_structure_candidate_rank(expected_rows, candidate))
        .unwrap_or_default()
}

pub(crate) fn table_row_candidates_for_page(
    artifact: &DocumentArtifact,
    page_index: u32,
    expected_row_count: usize,
) -> Vec<Vec<Vec<String>>> {
    let groups = table_row_groups_for_page(artifact, page_index);
    let mut candidates = Vec::new();
    for group in &groups {
        push_table_row_candidates(&mut candidates, group, expected_row_count);
    }

    if groups.len() > 1 {
        let flattened = groups.into_iter().flatten().collect::<Vec<_>>();
        push_table_row_candidates(&mut candidates, &flattened, expected_row_count);
    }

    candidates
}

pub(crate) fn push_table_row_candidates(
    candidates: &mut Vec<Vec<Vec<String>>>,
    rows: &[Vec<String>],
    expected_row_count: usize,
) {
    if rows.is_empty() {
        return;
    }

    candidates.push(rows.to_vec());
    if expected_row_count == 0 || rows.len() < expected_row_count {
        return;
    }

    for window in rows.windows(expected_row_count) {
        candidates.push(window.to_vec());
    }
}

pub(crate) fn table_structure_candidate_rank(
    expected_rows: &[Vec<String>],
    actual_rows: &[Vec<String>],
) -> (usize, usize, usize, usize, usize, usize) {
    let score = score_table_structure_rows(expected_rows, actual_rows.to_vec());
    (
        score.matched_cell_count(expected_rows),
        scaled_f64(score.cell_f1),
        score.matched_row_count(expected_rows),
        scaled_f64(score.row_f1),
        1_000_000usize.saturating_sub(score.extra_cells.len()),
        1_000_000usize.saturating_sub(score.actual_rows.len()),
    )
}

pub(crate) fn score_table_structure_rows(
    expected_rows: &[Vec<String>],
    actual_rows: Vec<Vec<String>>,
) -> TableStructureScore {
    let expected_rows = expected_rows.to_vec();
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

    TableStructureScore {
        actual_rows,
        missing_rows,
        extra_rows,
        missing_cells,
        extra_cells,
        row_precision,
        row_recall,
        row_f1,
        cell_precision,
        cell_recall,
        cell_f1,
    }
}

pub(crate) fn scaled_f64(value: f64) -> usize {
    (value * 1_000_000.0).round() as usize
}

pub(crate) fn insert_span_bbox_check(
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

pub(crate) fn span_bbox_sample(span: &TextSpan) -> serde_json::Value {
    json!({
        "text": &span.text,
        "provenance": &span.provenance,
        "bbox": &span.bbox,
    })
}

pub(crate) fn span_bbox_bounds(expectation: &SpanBBoxExpectation) -> serde_json::Value {
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

pub(crate) fn bbox_bound_failures(bbox: &BBox, expectation: &SpanBBoxExpectation) -> Vec<String> {
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

pub(crate) fn push_min_bound_failure(
    failures: &mut Vec<String>,
    field: &str,
    actual: f32,
    min: Option<f32>,
) {
    if let Some(min) = min
        && actual < min
    {
        failures.push(format!("{field}_below_min"));
    }
}

pub(crate) fn push_max_bound_failure(
    failures: &mut Vec<String>,
    field: &str,
    actual: f32,
    max: Option<f32>,
) {
    if let Some(max) = max
        && actual > max
    {
        failures.push(format!("{field}_above_max"));
    }
}

pub(crate) fn table_rows_for_page(
    artifact: &DocumentArtifact,
    page_index: u32,
) -> Vec<Vec<String>> {
    table_row_groups_for_page(artifact, page_index)
        .into_iter()
        .flatten()
        .collect()
}

pub(crate) fn table_row_groups_for_page(
    artifact: &DocumentArtifact,
    page_index: u32,
) -> Vec<Vec<Vec<String>>> {
    artifact
        .pages
        .iter()
        .find(|page| page.page_index == page_index)
        .map(|page| {
            page.layout_blocks
                .iter()
                .filter(|block| block.kind == LayoutBlockKind::Table)
                .filter_map(|block| {
                    let rows = block
                        .table
                        .as_ref()
                        .map(table_rows_from_grid)
                        .unwrap_or_else(|| parse_table_rows(&block.text));
                    (!rows.is_empty()).then_some(rows)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn table_rows_from_grid(table: &LayoutTable) -> Vec<Vec<String>> {
    table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|row| row.len() >= 2)
        .collect()
}

pub(crate) fn split_delimited_table_cells(line: &str, delimiter: char) -> Vec<String> {
    let trimmed = line.trim_matches(|ch: char| ch.is_ascii_whitespace() && ch != delimiter);
    let trimmed = trimmed.strip_prefix(delimiter).unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix(delimiter).unwrap_or(trimmed);

    trimmed
        .split(delimiter)
        .map(|cell| cell.trim().to_string())
        .collect()
}

pub(crate) fn is_markdown_table_separator_row(row: &[String]) -> bool {
    row.len() >= 2
        && row
            .iter()
            .all(|cell| is_markdown_table_separator_cell(cell))
}

pub(crate) fn is_markdown_table_separator_cell(cell: &str) -> bool {
    let trimmed = cell.trim();
    let core = trimmed.strip_prefix(':').unwrap_or(trimmed);
    let core = core.strip_suffix(':').unwrap_or(core);

    core.len() >= 3 && core.chars().all(|ch| ch == '-')
}

pub(crate) fn normalize_table_rows(rows: &[Vec<String>]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|row| row.len() >= 2)
        .collect()
}

pub(crate) fn table_cells(rows: &[Vec<String>]) -> Vec<TableCell> {
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

pub(crate) fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 1.0;
    }

    numerator as f64 / denominator as f64
}

pub(crate) fn f1(precision: f64, recall: f64) -> f64 {
    if precision == 0.0 && recall == 0.0 {
        return 0.0;
    }

    2.0 * precision * recall / (precision + recall)
}

pub(crate) fn classification_precision(
    true_positive_count: usize,
    predicted_positive_count: usize,
) -> f64 {
    if predicted_positive_count == 0 {
        return 1.0;
    }

    true_positive_count as f64 / predicted_positive_count as f64
}

pub(crate) fn classification_recall(
    true_positive_count: usize,
    expected_positive_count: usize,
) -> f64 {
    if expected_positive_count == 0 {
        return 1.0;
    }

    true_positive_count as f64 / expected_positive_count as f64
}

pub(crate) fn required_text_anchor_matches(actual: &str, expected: &str) -> bool {
    actual.contains(expected)
        || normalize_required_text_anchor(actual)
            .contains(&normalize_required_text_anchor(expected))
        || {
            let squashed_expected = squashed_required_text_anchor(expected);
            squashed_expected.len() >= 8
                && squashed_required_text_anchor(actual).contains(&squashed_expected)
        }
}

pub(crate) fn normalize_required_text_anchor(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn squashed_required_text_anchor(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric() || ch.is_ascii_punctuation())
        .collect()
}

pub(crate) fn quality_text_from_page(page: &PageArtifact) -> String {
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
            .map(quality_text_from_layout_block)
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

pub(crate) fn quality_text_from_layout_block(block: &glyphrush_core::LayoutBlock) -> String {
    if block.kind == LayoutBlockKind::Table
        && let Some(table) = block.table.as_ref()
        && let Some(text) = structured_table_text(table)
    {
        return text;
    }

    block.text.trim().to_string()
}

pub(crate) fn structured_table_text(table: &LayoutTable) -> Option<String> {
    let rows = table_rows_from_grid(table);
    let column_count = rows.iter().map(Vec::len).max()?;
    if rows.len() < 2 || column_count < 2 {
        return None;
    }

    Some(
        rows.iter()
            .map(|row| format_structured_table_text_row(row, column_count))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(crate) fn format_structured_table_text_row(row: &[String], column_count: usize) -> String {
    let mut cells = row.to_vec();
    cells.resize(column_count, String::new());
    format!("| {} |", cells.join(" | "))
}

pub(crate) fn normalize_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

pub(crate) fn normalize_chars(text: &str) -> Vec<char> {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn multiset_recall<T>(expected: Vec<T>, actual: Vec<T>) -> f64
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

pub(crate) fn missing_multiset_items<T>(mut expected: Vec<T>, mut actual: Vec<T>) -> Vec<T>
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

pub(crate) fn insert_page_expectation_checks(
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
        insert_layout_block_counts_check(
            checks,
            format!("{prefix}.layout_block_counts"),
            expected_layout_block_counts,
            page.map(layout_summary_from_page),
        );
    }

    if !expectation.required_text.is_empty() {
        let page_text = page.map(quality_text_from_page).unwrap_or_default();
        let missing = expectation
            .required_text
            .iter()
            .filter(|text| !required_text_anchor_matches(&page_text, text))
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
