use crate::*;

/// Shared column geometry for row-band grouping and body-column fit tests.
pub(crate) struct ColumnModel {
    center: f32,
    tolerance: f32,
    /// Spans entirely inside the center gutter are diagram labels, not body columns.
    gutter_zone: f32,
    /// Extraction imprecision lets spans extend slightly outside column bounds.
    column_fit_slack: f32,
    columns: Vec<(f32, f32)>,
}

impl ColumnModel {
    const COLUMN_FIT_SLACK: f32 = 2.0;
    const GUTTER_ZONE_FACTOR: f32 = 3.0;

    pub(crate) fn from_page_dimensions(dimensions: &PageDimensions) -> Self {
        let center = dimensions.width * 0.5;
        let tolerance = Self::center_tolerance(dimensions);
        Self {
            center,
            tolerance,
            gutter_zone: tolerance * Self::GUTTER_ZONE_FACTOR,
            column_fit_slack: Self::COLUMN_FIT_SLACK,
            columns: Vec::new(),
        }
    }

    pub(crate) fn from_body_columns(columns: &[(f32, f32)]) -> Self {
        Self {
            center: 0.0,
            tolerance: 0.0,
            gutter_zone: 0.0,
            column_fit_slack: Self::COLUMN_FIT_SLACK,
            columns: columns.to_vec(),
        }
    }

    fn center_tolerance(dimensions: &PageDimensions) -> f32 {
        (dimensions.width * 0.01).max(4.0)
    }

    fn span_straddles_center(&self, span: &TextSpan) -> bool {
        span.bbox.x0 < self.center - self.tolerance && span.bbox.x1 > self.center + self.tolerance
    }

    fn span_fits_column(&self, span: &TextSpan, column_index: usize) -> bool {
        let (column_x0, column_x1) = self.columns[column_index];
        span.bbox.x0 >= column_x0 - self.column_fit_slack
            && span.bbox.x1 <= column_x1 + self.column_fit_slack
    }

    fn span_fits_any_column(&self, span: &TextSpan) -> bool {
        self.columns
            .iter()
            .enumerate()
            .any(|(index, _)| self.span_fits_column(span, index))
    }

    fn column_index_for_span(&self, span: &TextSpan) -> Option<usize> {
        self.columns.iter().position(|&(column_x0, column_x1)| {
            span.bbox.x0 >= column_x0 - self.column_fit_slack
                && span.bbox.x1 <= column_x1 + self.column_fit_slack
        })
    }

    fn span_outside_gutter(&self, span: &TextSpan) -> bool {
        span.bbox.x0 < self.center - self.gutter_zone
            || span.bbox.x1 > self.center + self.gutter_zone
    }
}

pub(crate) enum RowClass {
    Band,
    Columnar,
    TwoSided,
}

impl ColumnModel {
    pub(crate) fn classify_row(&self, row: &[&TextSpan]) -> RowClass {
        if !self.columns.is_empty() {
            if row.iter().all(|span| self.span_fits_any_column(span)) {
                return RowClass::Columnar;
            }
            return RowClass::Band;
        }

        if row.iter().any(|span| self.span_straddles_center(span)) {
            return RowClass::Band;
        }

        let has_left = row
            .iter()
            .any(|span| span.bbox.x1 <= self.center - self.tolerance);
        let has_right = row
            .iter()
            .any(|span| span.bbox.x0 >= self.center + self.tolerance);
        if has_left && has_right {
            RowClass::TwoSided
        } else {
            RowClass::Columnar
        }
    }
}

pub(crate) fn segment_row_runs<'a>(
    rows: &'a [Vec<&'a TextSpan>],
    is_band: impl Fn(&[&TextSpan]) -> bool,
) -> Vec<(bool, std::ops::Range<usize>)> {
    let row_is_band: Vec<bool> = rows.iter().map(|row| is_band(row)).collect();
    let mut segments = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        while end < rows.len() && row_is_band[end] == row_is_band[start] {
            end += 1;
        }
        segments.push((row_is_band[start], start..end));
        start = end;
    }
    segments
}

pub(crate) type BodyColumnPartitionedRows<'a> = (
    Vec<Vec<&'a TextSpan>>,
    Vec<Vec<&'a TextSpan>>,
    Vec<Vec<&'a TextSpan>>,
);

pub(crate) fn partition_rows_by_body_columns<'a>(
    rows: &'a [Vec<&'a TextSpan>],
    columns: &[(f32, f32)],
) -> BodyColumnPartitionedRows<'a> {
    let model = ColumnModel::from_body_columns(columns);
    let mut left_rows = Vec::new();
    let mut right_rows = Vec::new();
    let mut band_rows = Vec::new();

    for row in rows {
        let mut left_spans = Vec::new();
        let mut right_spans = Vec::new();
        let mut has_unassigned = false;

        for &span in row {
            let fits_left = model.span_fits_column(span, 0);
            let fits_right = model.span_fits_column(span, 1);
            match (fits_left, fits_right) {
                (true, false) => left_spans.push(span),
                (false, true) => right_spans.push(span),
                (true, true) => {
                    let span_center = (span.bbox.x0 + span.bbox.x1) * 0.5;
                    let left_center = (columns[0].0 + columns[0].1) * 0.5;
                    let right_center = (columns[1].0 + columns[1].1) * 0.5;
                    if (span_center - left_center).abs() <= (span_center - right_center).abs() {
                        left_spans.push(span);
                    } else {
                        right_spans.push(span);
                    }
                }
                (false, false) => has_unassigned = true,
            }
        }

        if has_unassigned {
            band_rows.push(row.clone());
        } else if !left_spans.is_empty() && !right_spans.is_empty() {
            left_rows.push(left_spans);
            right_rows.push(right_spans);
        } else if !left_spans.is_empty() {
            left_rows.push(left_spans);
        } else if !right_spans.is_empty() {
            right_rows.push(right_spans);
        } else {
            band_rows.push(row.clone());
        }
    }

    (left_rows, right_rows, band_rows)
}

pub(crate) fn split_spans_by_known_columns<'a>(
    spans: &[&'a TextSpan],
    body_columns: Option<&[(f32, f32)]>,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    let columns = body_columns?;
    if columns.len() < 2 || spans.len() < 2 {
        return None;
    }

    let rows = group_positioned_text_rows(spans.to_vec());
    let model = ColumnModel::from_body_columns(columns);
    let row_is_band = |row: &[&TextSpan]| matches!(model.classify_row(row), RowClass::Band);

    let mut groups = Vec::new();
    let mut split_any = false;
    for (is_band, range) in segment_row_runs(&rows, row_is_band) {
        let segment_spans = rows[range.clone()]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        if is_band {
            groups.extend(group_span_refs_by_vertical_gaps(segment_spans));
        } else {
            let mut per_column: Vec<Vec<&TextSpan>> = vec![Vec::new(); columns.len()];
            for span in segment_spans {
                if let Some(column_index) = model.column_index_for_span(span) {
                    per_column[column_index].push(span);
                }
            }
            let occupied = per_column
                .iter()
                .filter(|column| !column.is_empty())
                .count();
            if occupied >= 2 {
                split_any = true;
            }
            for column in per_column {
                if !column.is_empty() {
                    groups.extend(group_span_refs_by_vertical_gaps(column));
                }
            }
        }
    }

    (split_any && !groups.is_empty()).then_some(groups)
}

/// Estimates the x-ranges of the page's body text columns from rows that do
/// not straddle the page-center gutter. Returns None for single-column or
/// table-dominated pages.
pub(crate) fn page_body_columns(
    rows: &[Vec<&TextSpan>],
    dimensions: &PageDimensions,
) -> Option<Vec<(f32, f32)>> {
    let model = ColumnModel::from_page_dimensions(dimensions);
    let column_spans = rows
        .iter()
        .filter(|row| !row.iter().any(|span| model.span_straddles_center(span)))
        .flatten()
        .copied()
        .filter(|span| model.span_outside_gutter(span))
        .collect::<Vec<_>>();
    if column_spans.len() < 8 {
        return None;
    }

    let columns = split_layout_columns(&column_spans, dimensions)?;
    let ranges = columns
        .iter()
        .map(|column| {
            let x0 = column
                .iter()
                .map(|span| span.bbox.x0)
                .min_by(f32::total_cmp)
                .unwrap_or(0.0);
            let x1 = column
                .iter()
                .map(|span| span.bbox.x1)
                .max_by(f32::total_cmp)
                .unwrap_or(0.0);
            (x0, x1)
        })
        .collect::<Vec<(f32, f32)>>();

    // Body text columns are made of prose lines; clusters of short table
    // cells must not be mistaken for them. Score the per-row joined text
    // length inside each inferred column and require prose-line medians.
    for (column_x0, column_x1) in &ranges {
        let mut row_lengths = rows
            .iter()
            .filter_map(|row| {
                let chars: usize = row
                    .iter()
                    .filter(|span| span.bbox.x0 >= *column_x0 && span.bbox.x1 <= *column_x1)
                    .map(|span| span.text.trim().chars().count())
                    .sum();
                (chars > 0).then_some(chars)
            })
            .collect::<Vec<_>>();
        row_lengths.sort_unstable();
        let median = row_lengths
            .get(row_lengths.len().saturating_sub(1) / 2)
            .copied()
            .unwrap_or(0);
        if median < 30 {
            return None;
        }
    }

    Some(ranges)
}

/// Detects positioned-table candidate windows that are really the page's own
/// text columns: most rows fill the detected body-column x-ranges edge to
/// edge like full text lines instead of leaving cell-sized gaps.
pub(crate) fn text_lines_from_positioned_spans(spans: &[&TextSpan]) -> Vec<String> {
    let rows = group_positioned_text_rows(spans.to_vec());
    rows.iter()
        .map(|row| text_line_from_positioned_row(row))
        .filter(|text| !text.is_empty())
        .collect()
}

pub(crate) fn group_positioned_text_rows(mut spans: Vec<&TextSpan>) -> Vec<Vec<&TextSpan>> {
    spans.retain(|span| !span.text.trim().is_empty());
    spans.sort_by(|left, right| {
        span_center_y(left)
            .total_cmp(&span_center_y(right))
            .then_with(|| left.bbox.x0.total_cmp(&right.bbox.x0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let tolerance = positioned_text_row_y_tolerance(&spans);
    let mut rows: Vec<Vec<&TextSpan>> = Vec::new();
    for span in spans {
        if let Some(row) = rows.last_mut()
            && (span_center_y(span) - row_center_y(row)).abs() <= tolerance
        {
            row.push(span);
            continue;
        }
        rows.push(vec![span]);
    }

    for row in &mut rows {
        row.sort_by(|left, right| {
            left.bbox
                .x0
                .total_cmp(&right.bbox.x0)
                .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
                .then_with(|| left.text.cmp(&right.text))
        });
    }

    rows
}

pub(crate) fn positioned_text_row_y_tolerance(spans: &[&TextSpan]) -> f32 {
    let mut heights = spans
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .filter(|height| *height > 0.0 && height.is_finite())
        .collect::<Vec<_>>();
    heights.sort_by(f32::total_cmp);

    let median_height = heights
        .get(heights.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(12.0);

    (median_height * 0.75).max(4.0)
}

pub(crate) fn text_line_from_positioned_row(row: &[&TextSpan]) -> String {
    let word_gap = positioned_word_gap_threshold(row);
    let mut output = String::new();
    let mut previous_x1 = None;

    for span in row {
        let text = span.text.trim_matches('\n');
        if text.trim().is_empty() {
            continue;
        }
        let overlaps_previous = previous_x1.is_some_and(|x1| span.bbox.x0 < x1);
        let text = if overlaps_previous {
            non_duplicate_overlapping_fragment(&output, text)
        } else {
            text.to_string()
        };
        if text.trim().is_empty() {
            previous_x1 = Some(previous_x1.map_or(span.bbox.x1, |x1: f32| x1.max(span.bbox.x1)));
            continue;
        }

        if !output.is_empty()
            && !output.ends_with(char::is_whitespace)
            && !text.starts_with(char::is_whitespace)
            && previous_x1.is_some_and(|x1| span.bbox.x0 - x1 > word_gap)
        {
            output.push(' ');
        }
        output.push_str(&text);
        previous_x1 = Some(previous_x1.map_or(span.bbox.x1, |x1: f32| x1.max(span.bbox.x1)));
    }

    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn non_duplicate_overlapping_fragment(output: &str, fragment: &str) -> String {
    let candidate = fragment.trim_start();
    let output = output.trim_end();

    let prefix_ends = candidate
        .char_indices()
        .skip(1)
        .map(|(idx, _)| idx)
        .chain(std::iter::once(candidate.len()))
        .collect::<Vec<_>>();

    for end in prefix_ends.into_iter().rev() {
        if output.ends_with(&candidate[..end]) {
            return candidate[end..].to_string();
        }
    }

    fragment.to_string()
}

pub(crate) fn positioned_word_gap_threshold(row: &[&TextSpan]) -> f32 {
    let mut widths = row
        .iter()
        .filter_map(|span| {
            let char_count = span.text.trim().chars().count();
            (char_count > 0).then_some((span.bbox.x1 - span.bbox.x0) / char_count as f32)
        })
        .filter(|width| *width > 0.0 && width.is_finite())
        .collect::<Vec<_>>();
    widths.sort_by(f32::total_cmp);

    let median_width = widths
        .get(widths.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(6.0);

    (median_width * 0.75).max(3.0)
}
pub(crate) fn columns_from_row(row: &[&TextSpan]) -> Vec<(f32, f32)> {
    row.iter()
        .map(|span| (span.bbox.x0, span.bbox.x1))
        .collect()
}

pub(crate) fn table_row_y_tolerance(spans: &[&TextSpan]) -> f32 {
    let mut heights = spans
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .filter(|height| *height > 0.0 && height.is_finite())
        .collect::<Vec<_>>();
    heights.sort_by(f32::total_cmp);

    let median_height = heights
        .get(heights.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(12.0);

    (median_height * 0.75).max(6.0)
}

pub(crate) fn table_column_x_tolerance(rows: &[Vec<&TextSpan>]) -> f32 {
    let mut widths = rows
        .iter()
        .flatten()
        .map(|span| span.bbox.x1 - span.bbox.x0)
        .filter(|width| *width > 0.0 && width.is_finite())
        .collect::<Vec<_>>();
    widths.sort_by(f32::total_cmp);

    let median_width = widths
        .get(widths.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(48.0);

    (median_width * 0.75).max(24.0)
}

pub(crate) fn span_center_y(span: &TextSpan) -> f32 {
    (span.bbox.y0 + span.bbox.y1) / 2.0
}

pub(crate) fn row_center_y(row: &[&TextSpan]) -> f32 {
    let top = row
        .iter()
        .map(|span| span.bbox.y0)
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    let bottom = row
        .iter()
        .map(|span| span.bbox.y1)
        .max_by(f32::total_cmp)
        .unwrap_or(0.0);
    (top + bottom) / 2.0
}

pub(crate) fn is_page_wide_span(span: &TextSpan, dimensions: &PageDimensions) -> bool {
    nearly_equal(span.bbox.x0, 0.0)
        && nearly_equal(span.bbox.y0, 0.0)
        && nearly_equal(span.bbox.x1, dimensions.width)
        && nearly_equal(span.bbox.y1, dimensions.height)
}

pub(crate) fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.001
}
