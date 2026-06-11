use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadingOrderStrategy {
    FullWidthBands,
    ColumnSplit,
    ColumnRowBands,
    VerticalGapColumnSplits,
    VerticalGaps,
}

impl ReadingOrderStrategy {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ReadingOrderStrategy::FullWidthBands => "full_width_bands",
            ReadingOrderStrategy::ColumnSplit => "column_split",
            ReadingOrderStrategy::ColumnRowBands => "column_row_bands",
            ReadingOrderStrategy::VerticalGapColumnSplits => "vertical_gap_column_splits",
            ReadingOrderStrategy::VerticalGaps => "vertical_gaps",
        }
    }
}

pub(crate) fn group_spans_for_reading_order<'a>(
    spans: &'a [TextSpan],
    dimensions: &PageDimensions,
) -> (Vec<Vec<&'a TextSpan>>, ReadingOrderStrategy) {
    let span_refs = spans
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();

    group_spans_for_reading_order_from_refs_with_strategy(span_refs, dimensions)
}

pub(crate) fn group_spans_for_reading_order_from_refs<'a>(
    span_refs: Vec<&'a TextSpan>,
    dimensions: &PageDimensions,
) -> Vec<Vec<&'a TextSpan>> {
    group_spans_for_reading_order_from_refs_with_strategy(span_refs, dimensions).0
}

pub(crate) fn group_spans_for_reading_order_from_refs_with_strategy<'a>(
    span_refs: Vec<&'a TextSpan>,
    dimensions: &PageDimensions,
) -> (Vec<Vec<&'a TextSpan>>, ReadingOrderStrategy) {
    if let Some(groups) = group_span_refs_by_full_width_bands(&span_refs, dimensions) {
        return (groups, ReadingOrderStrategy::FullWidthBands);
    }

    if let Some(columns) = split_layout_columns(&span_refs, dimensions) {
        let mut groups = Vec::new();
        for column in columns {
            groups.extend(group_span_refs_by_vertical_gaps(column));
        }
        return (groups, ReadingOrderStrategy::ColumnSplit);
    }

    if let Some(groups) = group_span_refs_by_column_row_bands(&span_refs, dimensions) {
        return (groups, ReadingOrderStrategy::ColumnRowBands);
    }

    if let Some(groups) =
        group_span_refs_by_vertical_gaps_with_column_splits(span_refs.clone(), dimensions)
    {
        return (groups, ReadingOrderStrategy::VerticalGapColumnSplits);
    }

    (
        group_span_refs_by_vertical_gaps(span_refs),
        ReadingOrderStrategy::VerticalGaps,
    )
}

/// Groups spans for multi-column pages where banner rows (titles, authors,
/// footers) or centered gutter-straddling rows (page numbers) prevent a clean
/// whole-page column split. Rows that straddle the page center become their own
/// reading-order bands, while runs of column-fitting rows between them are
/// split into columns.
pub(crate) fn group_span_refs_by_column_row_bands<'a>(
    span_refs: &[&'a TextSpan],
    dimensions: &PageDimensions,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    if span_refs.len() < 8 || dimensions.width <= 0.0 {
        return None;
    }

    let rows = group_positioned_text_rows(span_refs.to_vec());
    if rows.len() < 6 {
        return None;
    }

    let model = ColumnModel::from_page_dimensions(dimensions);
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
        } else if let Some(columns) = split_layout_columns(&segment_spans, dimensions) {
            split_any = true;
            for column in columns {
                groups.extend(group_span_refs_by_vertical_gaps(column));
            }
        } else if range.len() <= 3 {
            groups.extend(group_span_refs_by_vertical_gaps(segment_spans));
        } else {
            return None;
        }
    }

    split_any.then_some(groups)
}

/// Detects pages that look multi-column (many rows with text on both sides of
/// the page center) but could not be split into columns, so vertical-gap
/// grouping likely interleaved their reading order.
pub(crate) fn has_unresolved_column_evidence(
    spans: &[TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    if dimensions.width <= 0.0 {
        return false;
    }
    let span_refs = spans
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();
    if span_refs.len() < 12 {
        return false;
    }

    let rows = group_positioned_text_rows(span_refs);
    if rows.len() < 8 {
        return false;
    }

    let model = ColumnModel::from_page_dimensions(dimensions);
    let two_sided_rows = rows
        .iter()
        .filter(|row| matches!(model.classify_row(row), RowClass::TwoSided))
        .count();

    two_sided_rows >= 6 && two_sided_rows * 2 >= rows.len()
}

pub(crate) fn group_span_refs_by_vertical_gaps_with_column_splits<'a>(
    span_refs: Vec<&'a TextSpan>,
    dimensions: &PageDimensions,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    let vertical_groups = group_span_refs_by_vertical_gaps(span_refs);
    if vertical_groups.len() < 2 {
        return None;
    }

    let mut split_any = false;
    let mut groups = Vec::new();
    for group in vertical_groups {
        if let Some(columns) = split_layout_columns(&group, dimensions) {
            split_any = true;
            for column in columns {
                groups.extend(group_span_refs_by_vertical_gaps(column));
            }
        } else {
            groups.push(group);
        }
    }

    split_any.then_some(groups)
}

pub(crate) fn group_span_refs_by_full_width_bands<'a>(
    span_refs: &[&'a TextSpan],
    dimensions: &PageDimensions,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    if let Some(groups) = group_span_refs_by_fragmented_leading_band(span_refs, dimensions) {
        return Some(groups);
    }

    if let Some(groups) = group_span_refs_by_fragmented_cross_column_bands(span_refs, dimensions) {
        return Some(groups);
    }

    if span_refs.len() < 5
        || !span_refs.iter().any(|span| {
            is_full_width_layout_span(span, dimensions)
                || is_cross_column_layout_span_candidate(span, dimensions)
        })
    {
        return None;
    }

    let mut sorted_spans = span_refs.to_vec();
    sorted_spans.sort_by(|left, right| {
        left.bbox
            .y0
            .total_cmp(&right.bbox.y0)
            .then_with(|| left.bbox.x0.total_cmp(&right.bbox.x0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let mut groups = Vec::new();
    let mut pending_band = Vec::new();
    let mut split_columns = false;

    for (index, span) in sorted_spans.iter().copied().enumerate() {
        if is_full_width_layout_span(span, dimensions)
            || is_cross_column_leading_band_span(
                span,
                &pending_band,
                &sorted_spans[index + 1..],
                dimensions,
            )
            || is_cross_column_middle_band_span(
                span,
                &pending_band,
                &sorted_spans[index + 1..],
                dimensions,
            )
            || is_cross_column_trailing_band_span(
                span,
                &pending_band,
                &sorted_spans[index + 1..],
                dimensions,
            )
            || is_column_section_separator_span(
                span,
                &pending_band,
                &sorted_spans[index + 1..],
                dimensions,
            )
        {
            append_column_aware_band_groups(
                &mut groups,
                std::mem::take(&mut pending_band),
                dimensions,
                &mut split_columns,
            );
            groups.push(vec![span]);
        } else {
            pending_band.push(span);
        }
    }

    append_column_aware_band_groups(&mut groups, pending_band, dimensions, &mut split_columns);

    split_columns.then_some(groups)
}

pub(crate) fn group_span_refs_by_fragmented_cross_column_bands<'a>(
    span_refs: &[&'a TextSpan],
    dimensions: &PageDimensions,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    if span_refs.len() < 8 || dimensions.width <= 0.0 {
        return None;
    }

    let rows = group_positioned_text_rows(span_refs.to_vec());
    let mut groups = Vec::new();
    let mut pending_band = Vec::new();
    let mut split_columns = false;
    let mut saw_fragmented_band = false;

    for (row_index, row) in rows.iter().enumerate() {
        if row.len() == 1 && is_full_width_layout_span(row[0], dimensions) {
            append_column_aware_band_groups(
                &mut groups,
                std::mem::take(&mut pending_band),
                dimensions,
                &mut split_columns,
            );
            groups.push(row.to_vec());
            continue;
        } else if is_fragmented_layout_band_row(row, dimensions) {
            let following_band =
                following_fragmented_cross_column_context(&rows[row_index + 1..], dimensions);
            if fragmented_cross_column_band_has_context(
                row,
                &pending_band,
                &following_band,
                dimensions,
            ) {
                append_column_aware_band_groups(
                    &mut groups,
                    std::mem::take(&mut pending_band),
                    dimensions,
                    &mut split_columns,
                );
                groups.push(row.to_vec());
                split_columns = true;
                saw_fragmented_band = true;
                continue;
            }
        }

        pending_band.extend(row.iter().copied());
    }

    append_column_aware_band_groups(&mut groups, pending_band, dimensions, &mut split_columns);

    (saw_fragmented_band && split_columns).then_some(groups)
}

pub(crate) fn following_fragmented_cross_column_context<'a>(
    rows: &[Vec<&'a TextSpan>],
    dimensions: &PageDimensions,
) -> Vec<&'a TextSpan> {
    let mut spans = Vec::new();
    for row in rows {
        if row.iter().any(|span| {
            is_full_width_layout_span(span, dimensions)
                || is_cross_column_layout_span_candidate(span, dimensions)
        }) || is_fragmented_layout_band_row(row, dimensions)
        {
            break;
        }
        spans.extend(row.iter().copied());
    }

    spans
}

pub(crate) fn fragmented_cross_column_band_has_context(
    row: &[&TextSpan],
    previous_band: &[&TextSpan],
    following_band: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    let previous_columns =
        previous_band.len() >= 4 && split_layout_columns(previous_band, dimensions).is_some();
    let following_columns =
        following_band.len() >= 4 && split_layout_columns(following_band, dimensions).is_some();

    if previous_band.is_empty() {
        return following_columns && has_leading_row_band_vertical_gap(row, following_band);
    }

    if following_band.is_empty() {
        return previous_columns && has_trailing_row_band_vertical_gap(row, previous_band);
    }

    previous_columns
        && following_columns
        && has_fragmented_row_band_vertical_gap(row, previous_band, following_band)
}

pub(crate) fn group_span_refs_by_fragmented_leading_band<'a>(
    span_refs: &[&'a TextSpan],
    dimensions: &PageDimensions,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    if span_refs.len() < 6 || dimensions.width <= 0.0 {
        return None;
    }

    let rows = group_positioned_text_rows(span_refs.to_vec());
    let first_row = rows.first()?;
    let following_spans = rows.iter().skip(1).flatten().copied().collect::<Vec<_>>();
    if !is_fragmented_full_width_heading_row(first_row, dimensions)
        || !has_leading_row_band_vertical_gap(first_row, &following_spans)
    {
        return None;
    }

    let columns = split_layout_columns(&following_spans, dimensions)?;
    let mut groups = vec![first_row.to_vec()];
    for column in columns {
        groups.extend(group_span_refs_by_vertical_gaps(column));
    }

    Some(groups)
}

pub(crate) fn is_fragmented_full_width_heading_row(
    row: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    if !(2..=4).contains(&row.len()) || !fragmented_row_spans_are_tightly_joined(row) {
        return false;
    }

    let Some(bbox) = union_span_refs_bbox(row) else {
        return false;
    };
    let width = bbox.x1 - bbox.x0;
    let text = text_line_from_positioned_row(row);

    width >= dimensions.width * 0.45
        && bbox.x0 <= dimensions.width * 0.25
        && bbox.x1 >= dimensions.width * 0.65
        && is_heading_line(text.trim())
}

pub(crate) fn fragmented_row_spans_are_tightly_joined(row: &[&TextSpan]) -> bool {
    row.windows(2)
        .all(|window| window[1].bbox.x0 - window[0].bbox.x1 <= 36.0)
}

pub(crate) fn has_leading_row_band_vertical_gap(
    row: &[&TextSpan],
    following_spans: &[&TextSpan],
) -> bool {
    let Some(row_bottom) = row.iter().map(|span| span.bbox.y1).max_by(f32::total_cmp) else {
        return false;
    };
    let Some(following_top) = following_spans
        .iter()
        .map(|following| following.bbox.y0)
        .min_by(f32::total_cmp)
    else {
        return false;
    };

    let height = row
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .max_by(f32::total_cmp)
        .unwrap_or(12.0);
    following_top - row_bottom > (height * 0.75).max(8.0)
}

pub(crate) fn has_trailing_row_band_vertical_gap(
    row: &[&TextSpan],
    previous_spans: &[&TextSpan],
) -> bool {
    let Some(row_top) = row.iter().map(|span| span.bbox.y0).min_by(f32::total_cmp) else {
        return false;
    };
    let Some(previous_bottom) = previous_spans
        .iter()
        .map(|previous| previous.bbox.y1)
        .max_by(f32::total_cmp)
    else {
        return false;
    };

    let height = row
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .max_by(f32::total_cmp)
        .unwrap_or(12.0);
    row_top - previous_bottom > (height * 0.75).max(8.0)
}

pub(crate) fn has_fragmented_row_band_vertical_gap(
    row: &[&TextSpan],
    previous_spans: &[&TextSpan],
    following_spans: &[&TextSpan],
) -> bool {
    has_trailing_row_band_vertical_gap(row, previous_spans)
        && has_leading_row_band_vertical_gap(row, following_spans)
}

pub(crate) fn is_fragmented_layout_band_row(
    row: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    is_fragmented_cross_column_band_row(row, dimensions)
        || is_fragmented_column_section_separator_row(row, dimensions)
}

pub(crate) fn is_fragmented_cross_column_band_row(
    row: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    if !(2..=4).contains(&row.len()) || !fragmented_row_spans_are_tightly_joined(row) {
        return false;
    }

    let Some(bbox) = union_span_refs_bbox(row) else {
        return false;
    };
    let text = text_line_from_positioned_row(row);
    let trimmed = text.trim();
    let width = bbox.x1 - bbox.x0;

    !trimmed.is_empty()
        && !is_standalone_list_marker(trimmed)
        && trimmed.chars().count() <= 120
        && width >= dimensions.width * 0.45
        && bbox.x0 <= dimensions.width * 0.25
        && bbox.x1 >= dimensions.width * 0.65
}

pub(crate) fn is_fragmented_column_section_separator_row(
    row: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    if !(2..=4).contains(&row.len()) || !fragmented_row_spans_are_tightly_joined(row) {
        return false;
    }

    let Some(bbox) = union_span_refs_bbox(row) else {
        return false;
    };
    let text = text_line_from_positioned_row(row);
    let trimmed = text.trim();

    dimensions.width > 0.0
        && bbox.x0 <= dimensions.width * 0.25
        && !is_standalone_list_marker(trimmed)
        && is_heading_line(trimmed)
}

pub(crate) fn is_column_section_separator_span(
    span: &TextSpan,
    previous_band: &[&TextSpan],
    following_spans: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    if !is_left_aligned_section_heading_span(span, dimensions) {
        return false;
    }

    let following_band = following_spans
        .iter()
        .copied()
        .take_while(|following| !is_full_width_layout_span(following, dimensions))
        .collect::<Vec<_>>();

    previous_band.len() >= 4
        && following_band.len() >= 4
        && !has_same_row_neighbor(
            span,
            previous_band.iter().chain(following_band.iter()).copied(),
        )
        && has_section_separator_vertical_gap(span, previous_band, &following_band)
        && split_layout_columns(previous_band, dimensions).is_some()
        && split_layout_columns(&following_band, dimensions).is_some()
}

pub(crate) fn is_left_aligned_section_heading_span(
    span: &TextSpan,
    dimensions: &PageDimensions,
) -> bool {
    dimensions.width > 0.0
        && span.bbox.x0 <= dimensions.width * 0.25
        && is_heading_line(span.text.trim())
}

pub(crate) fn has_same_row_neighbor<'a>(
    span: &TextSpan,
    spans: impl IntoIterator<Item = &'a TextSpan>,
) -> bool {
    let tolerance = ((span.bbox.y1 - span.bbox.y0) * 0.75).max(4.0);
    spans.into_iter().any(|other| {
        !std::ptr::eq(other, span)
            && (span_center_y(other) - span_center_y(span)).abs() <= tolerance
    })
}

pub(crate) fn has_section_separator_vertical_gap(
    span: &TextSpan,
    previous_band: &[&TextSpan],
    following_band: &[&TextSpan],
) -> bool {
    let Some(previous_bottom) = previous_band
        .iter()
        .map(|previous| previous.bbox.y1)
        .max_by(f32::total_cmp)
    else {
        return false;
    };
    let Some(following_top) = following_band
        .iter()
        .map(|following| following.bbox.y0)
        .min_by(f32::total_cmp)
    else {
        return false;
    };

    let height = span.bbox.y1 - span.bbox.y0;
    let minimum_gap = (height * 0.75).max(8.0);
    span.bbox.y0 - previous_bottom > minimum_gap && following_top - span.bbox.y1 > minimum_gap
}

pub(crate) fn append_column_aware_band_groups<'a>(
    groups: &mut Vec<Vec<&'a TextSpan>>,
    spans: Vec<&'a TextSpan>,
    dimensions: &PageDimensions,
    split_columns: &mut bool,
) {
    if spans.is_empty() {
        return;
    }

    if let Some(columns) = split_layout_columns(&spans, dimensions) {
        *split_columns = true;
        for column in columns {
            groups.extend(group_span_refs_by_vertical_gaps(column));
        }
    } else {
        groups.extend(group_span_refs_by_vertical_gaps(spans));
    }
}

pub(crate) fn is_full_width_layout_span(span: &TextSpan, dimensions: &PageDimensions) -> bool {
    if dimensions.width <= 0.0 {
        return false;
    }

    let width = span.bbox.x1 - span.bbox.x0;
    width >= dimensions.width * 0.6
        && span.bbox.x0 <= dimensions.width * 0.2
        && span.bbox.x1 >= dimensions.width * 0.8
}

pub(crate) fn is_cross_column_trailing_band_span(
    span: &TextSpan,
    previous_band: &[&TextSpan],
    following_spans: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    is_cross_column_layout_span_candidate(span, dimensions)
        && previous_band.len() >= 4
        && following_spans.is_empty()
        && !has_same_row_neighbor(span, previous_band.iter().copied())
        && has_trailing_band_vertical_gap(span, previous_band)
        && split_layout_columns(previous_band, dimensions).is_some()
}

pub(crate) fn is_cross_column_leading_band_span(
    span: &TextSpan,
    previous_band: &[&TextSpan],
    following_spans: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    is_cross_column_layout_span_candidate(span, dimensions)
        && previous_band.is_empty()
        && following_spans.len() >= 4
        && !has_same_row_neighbor(span, following_spans.iter().copied())
        && has_leading_band_vertical_gap(span, following_spans)
        && split_layout_columns(following_spans, dimensions).is_some()
}

pub(crate) fn is_cross_column_middle_band_span(
    span: &TextSpan,
    previous_band: &[&TextSpan],
    following_spans: &[&TextSpan],
    dimensions: &PageDimensions,
) -> bool {
    if !is_cross_column_layout_span_candidate(span, dimensions) {
        return false;
    }

    let following_band = following_spans
        .iter()
        .copied()
        .take_while(|following| {
            !is_full_width_layout_span(following, dimensions)
                && !is_cross_column_layout_span_candidate(following, dimensions)
        })
        .collect::<Vec<_>>();

    previous_band.len() >= 4
        && following_band.len() >= 4
        && !has_same_row_neighbor(
            span,
            previous_band.iter().chain(following_band.iter()).copied(),
        )
        && has_section_separator_vertical_gap(span, previous_band, &following_band)
        && split_layout_columns(previous_band, dimensions).is_some()
        && split_layout_columns(&following_band, dimensions).is_some()
}

pub(crate) fn is_cross_column_layout_span_candidate(
    span: &TextSpan,
    dimensions: &PageDimensions,
) -> bool {
    if dimensions.width <= 0.0 {
        return false;
    }

    let width = span.bbox.x1 - span.bbox.x0;
    width >= dimensions.width * 0.45
        && span.bbox.x0 <= dimensions.width * 0.25
        && span.bbox.x1 >= dimensions.width * 0.65
}

pub(crate) fn has_trailing_band_vertical_gap(span: &TextSpan, previous_band: &[&TextSpan]) -> bool {
    let Some(previous_bottom) = previous_band
        .iter()
        .map(|previous| previous.bbox.y1)
        .max_by(f32::total_cmp)
    else {
        return false;
    };

    let height = span.bbox.y1 - span.bbox.y0;
    span.bbox.y0 - previous_bottom > (height * 0.75).max(8.0)
}

pub(crate) fn has_leading_band_vertical_gap(
    span: &TextSpan,
    following_spans: &[&TextSpan],
) -> bool {
    let Some(following_top) = following_spans
        .iter()
        .map(|following| following.bbox.y0)
        .min_by(f32::total_cmp)
    else {
        return false;
    };

    let height = span.bbox.y1 - span.bbox.y0;
    following_top - span.bbox.y1 > (height * 0.75).max(8.0)
}

pub(crate) fn split_layout_columns<'a>(
    spans: &[&'a TextSpan],
    dimensions: &PageDimensions,
) -> Option<Vec<Vec<&'a TextSpan>>> {
    if spans.len() < 4 || dimensions.width <= 0.0 {
        return None;
    }

    let mut sorted_spans = spans.to_vec();
    sorted_spans.sort_by(|left, right| {
        left.bbox
            .x0
            .total_cmp(&right.bbox.x0)
            .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let minimum_gap = layout_column_min_gap(spans, dimensions);
    let mut split_indexes = Vec::new();
    for split_index in 2..=sorted_spans.len().saturating_sub(2) {
        let left = &sorted_spans[..split_index];
        let right = &sorted_spans[split_index..];
        let Some(left_max_x1) = left.iter().map(|span| span.bbox.x1).max_by(f32::total_cmp) else {
            continue;
        };
        let Some(right_min_x0) = right.iter().map(|span| span.bbox.x0).min_by(f32::total_cmp)
        else {
            continue;
        };
        let gap = right_min_x0 - left_max_x1;
        if gap >= minimum_gap {
            split_indexes.push(split_index);
        }
    }

    if split_indexes.is_empty() || split_indexes.len() > 4 {
        return None;
    }

    let mut columns = Vec::new();
    let mut start = 0;
    for split_index in split_indexes {
        if split_index - start < 2 {
            continue;
        }
        columns.push(sorted_spans[start..split_index].to_vec());
        start = split_index;
    }

    if sorted_spans.len() - start < 2 {
        return None;
    }
    columns.push(sorted_spans[start..].to_vec());

    ((2..=5).contains(&columns.len())
        && columns
            .iter()
            .all(|column| layout_column_has_multiple_rows(column)))
    .then_some(columns)
}

pub(crate) fn layout_column_min_gap(spans: &[&TextSpan], dimensions: &PageDimensions) -> f32 {
    let mut widths = spans
        .iter()
        .map(|span| span.bbox.x1 - span.bbox.x0)
        .filter(|width| *width > 0.0 && width.is_finite())
        .collect::<Vec<_>>();
    widths.sort_by(f32::total_cmp);

    let median_width = widths
        .get(widths.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(120.0);

    (dimensions.width * 0.02).max(median_width * 0.05).max(12.0)
}

pub(crate) fn layout_column_has_multiple_rows(spans: &[&TextSpan]) -> bool {
    if spans.len() < 2 {
        return false;
    }

    let Some(min_center) = spans
        .iter()
        .map(|span| span_center_y(span))
        .min_by(f32::total_cmp)
    else {
        return false;
    };
    let Some(max_center) = spans
        .iter()
        .map(|span| span_center_y(span))
        .max_by(f32::total_cmp)
    else {
        return false;
    };
    let median_height = median_span_height(spans).unwrap_or(12.0);

    max_center - min_center > (median_height * 0.75).max(4.0)
}

pub(crate) fn median_span_height(spans: &[&TextSpan]) -> Option<f32> {
    let mut heights = spans
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .filter(|height| *height > 0.0 && height.is_finite())
        .collect::<Vec<_>>();
    heights.sort_by(f32::total_cmp);

    heights.get(heights.len().saturating_sub(1) / 2).copied()
}

pub(crate) fn group_span_refs_by_vertical_gaps(
    mut sorted_spans: Vec<&TextSpan>,
) -> Vec<Vec<&TextSpan>> {
    sorted_spans.sort_by(|left, right| {
        left.bbox
            .y0
            .total_cmp(&right.bbox.y0)
            .then_with(|| left.bbox.x0.total_cmp(&right.bbox.x0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let split_gap = vertical_split_gap(&sorted_spans);
    let mut groups: Vec<Vec<&TextSpan>> = Vec::new();

    for span in sorted_spans {
        let starts_new_group = groups
            .last()
            .and_then(|group| group.iter().map(|span| span.bbox.y1).max_by(f32::total_cmp))
            .map(|current_bottom| span.bbox.y0 - current_bottom > split_gap)
            .unwrap_or(true);

        if starts_new_group {
            groups.push(Vec::new());
        }

        groups.last_mut().expect("group exists").push(span);
    }

    groups
}

pub(crate) fn vertical_split_gap(spans: &[&TextSpan]) -> f32 {
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

    (median_height * 1.5).max(12.0)
}

pub(crate) fn union_span_refs_bbox(spans: &[&TextSpan]) -> Option<BBox> {
    let mut spans = spans.iter().filter(|span| !span.text.trim().is_empty());
    let first = spans.next()?;
    let mut bbox = first.bbox.clone();

    for span in spans {
        bbox.x0 = bbox.x0.min(span.bbox.x0);
        bbox.y0 = bbox.y0.min(span.bbox.y0);
        bbox.x1 = bbox.x1.max(span.bbox.x1);
        bbox.y1 = bbox.y1.max(span.bbox.y1);
    }

    Some(bbox)
}
