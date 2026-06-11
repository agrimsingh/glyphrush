use crate::*;

pub(crate) fn layout_blocks_from_positioned_table_runs(
    page_index: u32,
    dimensions: PageDimensions,
    spans: &[TextSpan],
    ruling_lines: &[ExtractedRulingLine],
) -> Option<Vec<LayoutBlock>> {
    let span_refs = spans
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();
    if span_refs.len() < 4 {
        return None;
    }

    if let Some(blocks) = ruled_grid_table_blocks(page_index, &dimensions, &span_refs, ruling_lines)
    {
        return Some(blocks);
    }

    let rows = merge_wrapped_positioned_table_rows(group_positioned_table_rows(span_refs));
    let candidate_ranges = positioned_table_row_ranges(&rows);
    let outside_rows = rows
        .iter()
        .enumerate()
        .filter(|(row_index, _)| {
            !candidate_ranges
                .iter()
                .any(|(start, end)| (*start..*end).contains(row_index))
        })
        .map(|(_, row)| row.clone())
        .collect::<Vec<_>>();
    let body_columns = page_body_columns(&outside_rows, &dimensions)
        .or_else(|| page_body_columns(&rows, &dimensions));
    if let Some(blocks) = layout_side_by_side_positioned_table_blocks(
        page_index,
        &dimensions,
        &rows,
        body_columns.as_deref(),
    ) {
        return Some(blocks);
    }

    let ranges = candidate_ranges
        .into_iter()
        .filter(|(start, end)| {
            !positioned_window_is_page_column_prose(&rows[*start..*end], body_columns.as_deref())
        })
        .collect::<Vec<_>>();
    if ranges.is_empty() {
        if let Some(list_block) = list_block_from_positioned_rows(page_index, 0, &rows) {
            return Some(vec![list_block]);
        }
        return None;
    }

    let mut blocks = Vec::new();
    let mut next_block_index = 0;
    append_table_run_blocks(
        page_index,
        &dimensions,
        &rows,
        body_columns.as_deref(),
        &mut next_block_index,
        &mut blocks,
    );

    (!blocks.is_empty()).then_some(blocks)
}

pub(crate) fn append_table_run_blocks(
    page_index: u32,
    dimensions: &PageDimensions,
    rows: &[Vec<&TextSpan>],
    body_columns: Option<&[(f32, f32)]>,
    next_block_index: &mut usize,
    blocks: &mut Vec<LayoutBlock>,
) {
    let candidate_ranges = positioned_table_row_ranges(rows);
    let ranges = candidate_ranges
        .into_iter()
        .filter(|(start, end)| {
            !positioned_window_is_page_column_prose(&rows[*start..*end], body_columns)
        })
        .collect::<Vec<_>>();

    let mut row_cursor = 0;
    for (start, end) in ranges {
        append_positioned_text_blocks_from_rows(
            page_index,
            dimensions,
            &rows[row_cursor..start],
            body_columns,
            next_block_index,
            blocks,
        );
        if let Some(table_block) =
            table_block_from_positioned_rows(page_index, *next_block_index, &rows[start..end])
        {
            blocks.push(table_block);
            *next_block_index += 1;
        }
        row_cursor = end;
    }

    append_positioned_text_blocks_from_rows(
        page_index,
        dimensions,
        &rows[row_cursor..],
        body_columns,
        next_block_index,
        blocks,
    );
}

pub(crate) fn layout_side_by_side_positioned_table_blocks(
    page_index: u32,
    dimensions: &PageDimensions,
    rows: &[Vec<&TextSpan>],
    body_columns: Option<&[(f32, f32)]>,
) -> Option<Vec<LayoutBlock>> {
    let columns = body_columns?;
    if columns.len() != 2 || rows.len() < 4 {
        return None;
    }

    let (left_rows, right_rows, band_rows) = partition_rows_by_body_columns(rows, columns);
    if left_rows.len() < 2 || right_rows.len() < 2 {
        return None;
    }
    if band_rows.len() * 5 > rows.len() {
        return None;
    }

    let left_ranges = positioned_table_row_ranges(&left_rows);
    let right_ranges = positioned_table_row_ranges(&right_rows);
    if left_ranges.is_empty() && right_ranges.is_empty() {
        return None;
    }

    let content_top = left_rows
        .iter()
        .chain(right_rows.iter())
        .map(|row| positioned_row_top(row))
        .min_by(f32::total_cmp)?;
    let (band_above, band_below): (Vec<_>, Vec<_>) = band_rows
        .iter()
        .cloned()
        .partition(|row| positioned_row_bottom(row) <= content_top + 1.0);

    let mut blocks = Vec::new();
    let mut next_block_index = 0;

    append_band_rows_as_text_blocks(
        page_index,
        dimensions,
        &band_above,
        &mut next_block_index,
        &mut blocks,
    );
    append_table_run_blocks(
        page_index,
        dimensions,
        &left_rows,
        Some(columns),
        &mut next_block_index,
        &mut blocks,
    );
    append_table_run_blocks(
        page_index,
        dimensions,
        &right_rows,
        Some(columns),
        &mut next_block_index,
        &mut blocks,
    );
    append_band_rows_as_text_blocks(
        page_index,
        dimensions,
        &band_below,
        &mut next_block_index,
        &mut blocks,
    );

    (!blocks.is_empty()).then_some(blocks)
}
pub(crate) fn append_band_rows_as_text_blocks(
    page_index: u32,
    _dimensions: &PageDimensions,
    band_rows: &[Vec<&TextSpan>],
    next_block_index: &mut usize,
    blocks: &mut Vec<LayoutBlock>,
) {
    if band_rows.is_empty() {
        return;
    }

    let spans = band_rows.iter().flatten().copied().collect::<Vec<_>>();
    for group in group_span_refs_by_vertical_gaps(spans) {
        if let Some(block) =
            layout_block_from_span_group(page_index, *next_block_index, group, true)
        {
            blocks.push(block);
            *next_block_index += 1;
        }
    }
}

pub(crate) fn positioned_row_top(row: &[&TextSpan]) -> f32 {
    row.iter()
        .map(|span| span.bbox.y0)
        .min_by(f32::total_cmp)
        .unwrap_or(0.0)
}

pub(crate) fn positioned_row_bottom(row: &[&TextSpan]) -> f32 {
    row.iter()
        .map(|span| span.bbox.y1)
        .max_by(f32::total_cmp)
        .unwrap_or(0.0)
}

pub(crate) fn append_positioned_text_blocks_from_rows(
    page_index: u32,
    dimensions: &PageDimensions,
    rows: &[Vec<&TextSpan>],
    body_columns: Option<&[(f32, f32)]>,
    next_block_index: &mut usize,
    blocks: &mut Vec<LayoutBlock>,
) {
    if let Some(block) = list_block_from_positioned_rows(page_index, *next_block_index, rows) {
        blocks.push(block);
        *next_block_index += 1;
        return;
    }

    let spans = rows.iter().flatten().copied().collect::<Vec<&TextSpan>>();
    let groups = split_spans_by_known_columns(&spans, body_columns)
        .unwrap_or_else(|| group_spans_for_reading_order_from_refs(spans, dimensions));
    for group in groups {
        if let Some(block) =
            layout_block_from_span_group(page_index, *next_block_index, group, true)
        {
            blocks.push(block);
            *next_block_index += 1;
        }
    }
}

/// Splits leftover spans on a table-routed page into the page's known body
/// columns so short two-column prose segments between recovered tables keep
/// column reading order instead of interleaving into fake whitespace tables.
/// Returns None when any span does not fit a single body column.
pub(crate) fn positioned_window_is_page_column_prose(
    window: &[Vec<&TextSpan>],
    body_columns: Option<&[(f32, f32)]>,
) -> bool {
    let Some(body_columns) = body_columns else {
        return false;
    };
    if window.len() < 2 || body_columns.len() < 2 {
        return false;
    }

    let filled_rows = window
        .iter()
        .filter(|row| positioned_row_fills_body_columns(row, body_columns))
        .count();

    filled_rows * 2 >= window.len()
}

pub(crate) fn positioned_row_fills_body_columns(
    row: &[&TextSpan],
    body_columns: &[(f32, f32)],
) -> bool {
    // Prose lines never end in a standalone numeric fragment; leaderboard or
    // result-table rows that happen to span the column width do.
    if row.len() >= 2
        && let Some(last) = row.last()
    {
        let text = last.text.trim();
        let digits = text.chars().filter(|ch| ch.is_ascii_digit()).count();
        let letters = text.chars().filter(|ch| ch.is_alphabetic()).count();
        if digits > 0 && letters == 0 {
            return false;
        }
    }

    let mut filled_any = false;
    for (column_x0, column_x1) in body_columns {
        let width = column_x1 - column_x0;
        if width <= 0.0 {
            continue;
        }
        let mut covered_x0: Option<f32> = None;
        let mut covered_x1: Option<f32> = None;
        for span in row {
            let overlap_x0 = span.bbox.x0.max(*column_x0);
            let overlap_x1 = span.bbox.x1.min(*column_x1);
            if overlap_x1 <= overlap_x0 {
                continue;
            }
            covered_x0 = Some(covered_x0.map_or(overlap_x0, |value| value.min(overlap_x0)));
            covered_x1 = Some(covered_x1.map_or(overlap_x1, |value| value.max(overlap_x1)));
        }
        if let (Some(covered_x0), Some(covered_x1)) = (covered_x0, covered_x1) {
            if covered_x1 - covered_x0 < width * 0.7 {
                return false;
            }
            filled_any = true;
        }
    }

    filled_any
}

pub(crate) fn positioned_table_row_ranges(rows: &[Vec<&TextSpan>]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;

    while start < rows.len() {
        let mut best_end = None;
        for end in (start + 2)..=rows.len() {
            if positioned_rows_form_table(&rows[start..end]) {
                best_end = Some(end);
            }
        }

        if let Some(end) = best_end {
            ranges.push((start, end));
            start = end;
        } else {
            start += 1;
        }
    }

    ranges
}

pub(crate) fn positioned_rows_form_table(rows: &[Vec<&TextSpan>]) -> bool {
    if positioned_rows_form_list(rows) {
        return false;
    }

    positioned_table_columns(rows).is_some()
}

pub(crate) fn list_block_from_positioned_rows(
    page_index: u32,
    block_index: usize,
    rows: &[Vec<&TextSpan>],
) -> Option<LayoutBlock> {
    let items = positioned_list_items(rows)?;

    let bbox = union_span_refs_bbox(&rows.iter().flatten().copied().collect::<Vec<_>>())?;
    let text = items.join("\n");

    Some(LayoutBlock {
        block_id: format!("p{page_index:06}:b{block_index:06}"),
        bbox,
        text,
        kind: LayoutBlockKind::List,
        table: None,
    })
}

pub(crate) fn positioned_rows_form_list(rows: &[Vec<&TextSpan>]) -> bool {
    positioned_list_items(rows).is_some()
}

pub(crate) fn positioned_list_items(rows: &[Vec<&TextSpan>]) -> Option<Vec<String>> {
    if rows.len() < 2 {
        return None;
    }

    let mut items = Vec::new();
    let mut current_item: Option<PositionedListItem> = None;

    for row in rows {
        let first = row.first()?;
        if is_standalone_list_marker(first.text.trim()) {
            flush_positioned_list_item(&mut items, current_item.take())?;
            current_item = Some(PositionedListItem {
                marker_x1: first.bbox.x1,
                text: positioned_list_item_text(first.text.trim(), &row[1..]),
            });
        } else if let Some(item) = current_item.as_mut() {
            let continuation_x0 = row
                .iter()
                .map(|span| span.bbox.x0)
                .min_by(f32::total_cmp)
                .unwrap_or(0.0);
            if continuation_x0 <= item.marker_x1 {
                return None;
            }

            append_positioned_row_text(&mut item.text, row);
        } else {
            return None;
        }
    }

    flush_positioned_list_item(&mut items, current_item)?;

    (items.len() >= 2).then_some(items)
}

#[derive(Debug)]
pub(crate) struct PositionedListItem {
    marker_x1: f32,
    text: String,
}

pub(crate) fn flush_positioned_list_item(
    items: &mut Vec<String>,
    item: Option<PositionedListItem>,
) -> Option<()> {
    let Some(item) = item else {
        return Some(());
    };
    list_item_has_body(&item.text).then(|| items.push(item.text))
}

pub(crate) fn positioned_list_item_text(marker: &str, rest: &[&TextSpan]) -> String {
    let mut text = marker.to_string();
    append_positioned_row_text(&mut text, rest);
    text
}

pub(crate) fn append_positioned_row_text(text: &mut String, row: &[&TextSpan]) {
    for fragment in row.iter().map(|span| span.text.trim()) {
        if fragment.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(fragment);
    }
}

pub(crate) fn list_item_has_body(item: &str) -> bool {
    item.split_once(char::is_whitespace)
        .is_some_and(|(_, rest)| !rest.trim().is_empty())
}

pub(crate) fn table_block_from_positioned_rows(
    page_index: u32,
    block_index: usize,
    rows: &[Vec<&TextSpan>],
) -> Option<LayoutBlock> {
    if !positioned_rows_form_table(rows) {
        return None;
    }

    let bbox = union_span_refs_bbox(&rows.iter().flatten().copied().collect::<Vec<_>>())?;
    let table = table_payload_from_positioned_rows(rows)?;
    let text = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(LayoutBlock {
        block_id: format!("p{page_index:06}:b{block_index:06}"),
        bbox,
        text,
        kind: LayoutBlockKind::Table,
        table: Some(table),
    })
}

pub(crate) fn table_payload_from_positioned_rows(rows: &[Vec<&TextSpan>]) -> Option<LayoutTable> {
    let columns = positioned_table_columns(rows)?;
    let tolerance = table_column_x_tolerance(rows);
    let row_count = rows.len();
    let rows = rows
        .iter()
        .enumerate()
        .filter_map(|(row_index, row)| {
            let cells = positioned_table_cells_from_row(row, &columns, tolerance);
            let non_empty_cell_count = cells.iter().filter(|cell| !cell.text.is_empty()).count();
            (non_empty_cell_count >= 2
                || positioned_row_is_table_section(row, &columns, tolerance, row_index, row_count))
            .then_some(LayoutTableRow {
                row_index,
                bbox: union_span_refs_bbox(row),
                cells,
            })
        })
        .collect::<Vec<_>>();

    (rows.len() >= 2).then_some(LayoutTable { rows })
}

pub(crate) fn positioned_table_cells_from_row(
    row: &[&TextSpan],
    columns: &[(f32, f32)],
    tolerance: f32,
) -> Vec<LayoutTableCell> {
    let mut cells = (0..columns.len())
        .map(|column_index| LayoutTableCell {
            column_index,
            text: String::new(),
            bbox: None,
        })
        .collect::<Vec<_>>();

    let mut last_assigned: Option<(usize, &TextSpan)> = None;
    for span in row {
        let text = span.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(column_index) =
            positioned_column_index_for_span(span, columns, tolerance, last_assigned)
        {
            let cell = &mut cells[column_index];
            if !cell.text.is_empty() {
                cell.text.push(' ');
            }
            cell.text.push_str(text);
            cell.bbox = match cell.bbox.take() {
                Some(existing) => Some(union_bboxes(&existing, &span.bbox)),
                None => Some(span.bbox.clone()),
            };
            last_assigned = Some((column_index, span));
        }
    }

    cells
}

pub(crate) fn positioned_table_columns(rows: &[Vec<&TextSpan>]) -> Option<Vec<(f32, f32)>> {
    if rows.len() < 2 {
        return None;
    }

    let columns = rows
        .iter()
        .filter_map(|row| {
            let columns = positioned_row_column_anchors(row);
            ((2..=8).contains(&columns.len())
                && columns.windows(2).all(|window| window[0].0 < window[1].0))
            .then_some(columns)
        })
        .max_by(|left, right| {
            left.len().cmp(&right.len()).then_with(|| {
                positioned_columns_width(left).total_cmp(&positioned_columns_width(right))
            })
        })?;
    if !columns.windows(2).all(|window| window[0].0 < window[1].0) {
        return None;
    }

    let tolerance = table_column_x_tolerance(rows);
    let column_count = columns.len();
    let mut regular_row_count = 0;
    let row_count = rows.len();
    for (row_index, row) in rows.iter().enumerate() {
        if positioned_row_is_table_section(row, &columns, tolerance, row_index, row_count) {
            continue;
        }

        if row.len() < 2 {
            return None;
        }

        let mut seen: Vec<Option<&TextSpan>> = vec![None; column_count];
        let mut last_assigned: Option<(usize, &TextSpan)> = None;
        let mut distinct_columns = 0;
        for span in row {
            let column_index =
                positioned_column_index_for_span(span, &columns, tolerance, last_assigned)?;
            if let Some(previous_span) = seen[column_index] {
                if !is_same_positioned_column_cell_fragment(previous_span, span) {
                    return None;
                }
                seen[column_index] = Some(span);
                last_assigned = Some((column_index, span));
                continue;
            }
            if last_assigned.is_some_and(|(previous, _)| column_index < previous) {
                return None;
            }
            seen[column_index] = Some(span);
            last_assigned = Some((column_index, span));
            distinct_columns += 1;
        }
        if distinct_columns < 2 {
            return None;
        }
        regular_row_count += 1;
    }

    if regular_row_count < 2 {
        return None;
    }

    if positioned_rows_look_like_parallel_prose(rows) {
        return None;
    }

    Some(columns)
}

/// Rejects positioned "tables" whose rows are dominated by flowing text
/// fragments without numeric cells, the shape produced by routing body prose
/// (for example two-column academic papers with figure rulings) into
/// positioned table recovery. Real tables keep numeric value cells or short
/// label cells, so they are not rejected by this guard.
pub(crate) fn positioned_rows_look_like_parallel_prose(rows: &[Vec<&TextSpan>]) -> bool {
    if rows.len() < 3 {
        return false;
    }

    let prose_rows = rows
        .iter()
        .filter(|row| positioned_row_looks_like_prose(row))
        .count();

    prose_rows * 5 >= rows.len() * 3
}

pub(crate) fn positioned_row_looks_like_prose(row: &[&TextSpan]) -> bool {
    let mut total_chars = 0;
    let mut digit_chars = 0;
    let mut has_long_wordy_cell = false;
    for span in row {
        let text = span.text.trim();
        for character in text.chars() {
            total_chars += 1;
            if character.is_ascii_digit() {
                digit_chars += 1;
            }
        }
        if text.chars().count() >= 35 && text.matches(' ').count() >= 5 {
            has_long_wordy_cell = true;
        }
    }

    total_chars >= 60 && has_long_wordy_cell && digit_chars * 100 <= total_chars * 15
}

pub(crate) fn positioned_row_column_anchors(row: &[&TextSpan]) -> Vec<(f32, f32)> {
    let mut spans = row.to_vec();
    spans.sort_by(|left, right| {
        left.bbox
            .x0
            .total_cmp(&right.bbox.x0)
            .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let mut anchors: Vec<PositionedColumnAnchor> = Vec::new();
    for span in spans {
        if let Some(anchor) = anchors.last_mut()
            && is_same_positioned_column_cell_fragment(anchor.last_span, span)
        {
            anchor.x0 = anchor.x0.min(span.bbox.x0);
            anchor.x1 = anchor.x1.max(span.bbox.x1);
            anchor.last_span = span;
            continue;
        }

        anchors.push(PositionedColumnAnchor {
            x0: span.bbox.x0,
            x1: span.bbox.x1,
            last_span: span,
        });
    }

    anchors
        .into_iter()
        .map(|anchor| (anchor.x0, anchor.x1))
        .collect()
}

#[derive(Debug)]
pub(crate) struct PositionedColumnAnchor<'a> {
    x0: f32,
    x1: f32,
    last_span: &'a TextSpan,
}

pub(crate) fn positioned_columns_width(columns: &[(f32, f32)]) -> f32 {
    let Some((x0, _)) = columns.first() else {
        return 0.0;
    };
    let Some((_, x1)) = columns.last() else {
        return 0.0;
    };

    x1 - x0
}

pub(crate) fn positioned_row_is_spanning_table_section(
    row: &[&TextSpan],
    columns: &[(f32, f32)],
    tolerance: f32,
    row_index: usize,
    row_count: usize,
) -> bool {
    if row.len() != 1 || columns.len() < 2 || row_index == 0 || row_index + 1 >= row_count {
        return false;
    }

    let span = row[0];
    let text = span.text.trim();
    if text.is_empty() || is_standalone_list_marker(text) {
        return false;
    }

    let Some((first_x0, _)) = columns.first() else {
        return false;
    };
    let Some((_, second_x1)) = columns.get(1) else {
        return false;
    };

    (span.bbox.x0 - *first_x0).abs() <= tolerance && span.bbox.x1 >= *second_x1 - tolerance
}

pub(crate) fn positioned_row_is_table_section(
    row: &[&TextSpan],
    columns: &[(f32, f32)],
    tolerance: f32,
    row_index: usize,
    row_count: usize,
) -> bool {
    positioned_row_is_spanning_table_section(row, columns, tolerance, row_index, row_count)
        || positioned_row_is_first_column_table_section(
            row, columns, tolerance, row_index, row_count,
        )
        || positioned_row_is_interior_table_note(row, columns, tolerance, row_index, row_count)
}

pub(crate) fn positioned_row_is_first_column_table_section(
    row: &[&TextSpan],
    columns: &[(f32, f32)],
    tolerance: f32,
    row_index: usize,
    row_count: usize,
) -> bool {
    if row.is_empty() || columns.len() < 2 || row_index == 0 || row_index + 1 >= row_count {
        return false;
    }

    let cells = positioned_table_cells_from_row(row, columns, tolerance);
    let Some(first_cell) = cells.first() else {
        return false;
    };
    if cells[1..].iter().any(|cell| !cell.text.is_empty()) {
        return false;
    }

    let mut row_text = String::new();
    append_positioned_row_text(&mut row_text, row);
    let text = first_cell.text.trim();
    if text != row_text.trim() {
        return false;
    }

    if !looks_like_positioned_table_section_label(text) || is_standalone_list_marker(text) {
        return false;
    }

    let Some((first_x0, _)) = columns.first() else {
        return false;
    };
    let Some((_, second_x1)) = columns.get(1) else {
        return false;
    };
    let Some(row_bbox) = union_span_refs_bbox(row) else {
        return false;
    };

    (row_bbox.x0 - *first_x0).abs() <= tolerance && row_bbox.x1 < *second_x1 - tolerance
}

pub(crate) fn positioned_row_is_interior_table_note(
    row: &[&TextSpan],
    columns: &[(f32, f32)],
    tolerance: f32,
    row_index: usize,
    row_count: usize,
) -> bool {
    if row.is_empty() || columns.len() < 3 || row_index == 0 || row_index + 1 >= row_count {
        return false;
    }

    let cells = positioned_table_cells_from_row(row, columns, tolerance);
    let non_empty_cells = cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| !cell.text.trim().is_empty())
        .collect::<Vec<_>>();
    let [(column_index, cell)] = non_empty_cells.as_slice() else {
        return false;
    };
    if *column_index == 0 || *column_index + 1 >= columns.len() {
        return false;
    }

    let mut row_text = String::new();
    append_positioned_row_text(&mut row_text, row);
    let text = cell.text.trim();
    if text != row_text.trim() || is_standalone_list_marker(text) {
        return false;
    }

    looks_like_positioned_table_condition_note(text)
}

pub(crate) fn looks_like_positioned_table_section_label(text: &str) -> bool {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    looks_like_wrapped_descriptor_fragment(&tokens) || is_heading_line(text)
}

pub(crate) fn looks_like_positioned_table_condition_note(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 80 {
        return false;
    }

    let token_count = trimmed.split_whitespace().count();
    let has_alpha = trimmed.chars().any(char::is_alphabetic);
    let has_condition_syntax = trimmed
        .chars()
        .any(|ch| ch.is_ascii_digit() || matches!(ch, '=' | '<' | '>' | '+' | '-' | '/' | '%'));

    has_alpha && has_condition_syntax && (1..=8).contains(&token_count)
}

pub(crate) fn is_wrapped_same_column_cell(previous: &TextSpan, span: &TextSpan) -> bool {
    let previous_height = previous.bbox.y1 - previous.bbox.y0;
    let span_height = span.bbox.y1 - span.bbox.y0;
    let max_baseline_gap = (previous_height.max(span_height) * 1.25).max(8.0);
    span.bbox.y0 >= previous.bbox.y0
        && span_center_y(span) - span_center_y(previous) > previous_height.min(span_height) * 0.5
        && span.bbox.y0 - previous.bbox.y1 <= max_baseline_gap
}

pub(crate) fn is_same_positioned_column_cell_fragment(
    previous: &TextSpan,
    span: &TextSpan,
) -> bool {
    is_wrapped_same_column_cell(previous, span)
        || is_same_line_positioned_cell_fragment(previous, span)
}

pub(crate) fn is_same_line_positioned_cell_fragment(previous: &TextSpan, span: &TextSpan) -> bool {
    let previous_height = previous.bbox.y1 - previous.bbox.y0;
    let span_height = span.bbox.y1 - span.bbox.y0;
    let y_tolerance = (previous_height.max(span_height) * 0.5).max(4.0);
    let horizontal_gap = span.bbox.x0 - previous.bbox.x1;

    (span_center_y(span) - span_center_y(previous)).abs() <= y_tolerance
        && horizontal_gap >= -1.0
        && horizontal_gap <= same_line_cell_fragment_gap_threshold(previous, span)
}

pub(crate) fn same_line_cell_fragment_gap_threshold(previous: &TextSpan, span: &TextSpan) -> f32 {
    let previous_width = average_span_char_width(previous);
    let span_width = average_span_char_width(span);
    let width = match (previous_width, span_width) {
        (Some(previous_width), Some(span_width)) => previous_width.max(span_width),
        (Some(width), None) | (None, Some(width)) => width,
        (None, None) => 6.0,
    };

    (width * 1.5).clamp(6.0, 16.0)
}

pub(crate) fn average_span_char_width(span: &TextSpan) -> Option<f32> {
    let char_count = span.text.trim().chars().count();
    if char_count == 0 {
        return None;
    }

    let width = span.bbox.x1 - span.bbox.x0;
    (width > 0.0 && width.is_finite()).then_some(width / char_count as f32)
}

pub(crate) fn nearest_positioned_column_index(
    span: &TextSpan,
    columns: &[(f32, f32)],
    tolerance: f32,
) -> Option<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(column_index, (x0, x1))| {
            let distance = (span.bbox.x0 - *x0).abs().min((span.bbox.x1 - *x1).abs());
            (column_index, distance)
        })
        .min_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        })
        .and_then(|(column_index, distance)| (distance <= tolerance).then_some(column_index))
}

pub(crate) fn positioned_column_index_for_span(
    span: &TextSpan,
    columns: &[(f32, f32)],
    tolerance: f32,
    last_assigned: Option<(usize, &TextSpan)>,
) -> Option<usize> {
    nearest_positioned_column_index(span, columns, tolerance).or_else(|| {
        last_assigned.and_then(|(column_index, previous_span)| {
            is_same_positioned_column_cell_fragment(previous_span, span).then_some(column_index)
        })
    })
}

pub(crate) fn group_positioned_table_rows(mut spans: Vec<&TextSpan>) -> Vec<Vec<&TextSpan>> {
    spans.sort_by(|left, right| {
        span_center_y(left)
            .total_cmp(&span_center_y(right))
            .then_with(|| left.bbox.x0.total_cmp(&right.bbox.x0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let tolerance = table_row_y_tolerance(&spans);
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

pub(crate) fn merge_wrapped_positioned_table_rows<'a>(
    rows: Vec<Vec<&'a TextSpan>>,
) -> Vec<Vec<&'a TextSpan>> {
    let mut merged: Vec<Vec<&'a TextSpan>> = Vec::new();

    for row_index in 0..rows.len() {
        let row = rows[row_index].clone();
        let following_row = rows.get(row_index + 1).map(Vec::as_slice);
        let is_first_multi_span_row_continuation = merged
            .iter()
            .filter(|candidate| candidate.len() >= 2)
            .count()
            == 1;
        if let Some(previous) = merged.last_mut()
            && (looks_like_positioned_table_continuation_row(previous, &row)
                || (is_first_multi_span_row_continuation
                    && looks_like_same_column_positioned_table_header_continuation(
                        previous,
                        &row,
                        following_row,
                    )))
        {
            previous.extend(row);
            sort_positioned_table_row(previous);
            continue;
        }
        merged.push(row);
    }

    merged
}

pub(crate) fn looks_like_same_column_positioned_table_header_continuation(
    previous: &[&TextSpan],
    row: &[&TextSpan],
    following_row: Option<&[&TextSpan]>,
) -> bool {
    if previous.len() != row.len()
        || previous.len() < 2
        || !positioned_row_looks_like_header_fragments(previous)
        || !positioned_row_looks_like_header_fragments(row)
    {
        return false;
    }

    let gap = positioned_row_vertical_gap(previous, row);
    if gap < -1.0 || gap > positioned_table_continuation_gap_threshold(previous, row) {
        return false;
    }

    let Some(following_row) = following_row else {
        return false;
    };
    if !positioned_row_looks_like_table_values(following_row) {
        return false;
    }

    let columns = columns_from_row(previous);
    let tolerance = table_column_x_tolerance(&[previous.to_vec(), row.to_vec()]);
    let mut seen = vec![false; columns.len()];
    for (expected_column, span) in row.iter().enumerate() {
        let Some(column_index) = nearest_positioned_column_index(span, &columns, tolerance) else {
            return false;
        };
        if column_index != expected_column || seen[column_index] {
            return false;
        }
        seen[column_index] = true;
    }

    true
}

pub(crate) fn positioned_row_looks_like_table_values(row: &[&TextSpan]) -> bool {
    row.iter().any(|span| {
        let text = span.text.trim();
        text.chars().any(|ch| {
            ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | '/' | '%' | '<' | '>' | '=')
        })
    })
}

pub(crate) fn positioned_row_looks_like_header_fragments(row: &[&TextSpan]) -> bool {
    row.iter().all(|span| {
        let text = span.text.trim();
        !text.is_empty()
            && !is_standalone_list_marker(text)
            && text.chars().any(char::is_alphabetic)
            && !text.chars().any(|ch| ch.is_ascii_digit())
            && text.split_whitespace().count() <= 3
    })
}

pub(crate) fn looks_like_positioned_table_continuation_row(
    previous: &[&TextSpan],
    row: &[&TextSpan],
) -> bool {
    if row.is_empty() || previous.len() < 2 || row.len() >= previous.len() {
        return false;
    }

    if previous
        .first()
        .is_some_and(|span| is_standalone_list_marker(span.text.trim()))
        || row
            .first()
            .is_some_and(|span| is_standalone_list_marker(span.text.trim()))
    {
        return false;
    }

    let gap = positioned_row_vertical_gap(previous, row);
    if gap < -1.0 || gap > positioned_table_continuation_gap_threshold(previous, row) {
        return false;
    }

    let columns = columns_from_row(previous);
    let tolerance = table_column_x_tolerance(&[previous.to_vec(), row.to_vec()]);
    let mut previous_column_index = None;

    for span in row {
        let Some(column_index) = nearest_positioned_column_index(span, &columns, tolerance) else {
            return false;
        };
        if previous_column_index.is_some_and(|previous| column_index <= previous) {
            return false;
        }
        previous_column_index = Some(column_index);
    }

    true
}

pub(crate) fn sort_positioned_table_row(row: &mut [&TextSpan]) {
    row.sort_by(|left, right| {
        left.bbox
            .x0
            .total_cmp(&right.bbox.x0)
            .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
            .then_with(|| left.text.cmp(&right.text))
    });
}

pub(crate) fn positioned_row_vertical_gap(previous: &[&TextSpan], row: &[&TextSpan]) -> f32 {
    let previous_bottom = previous
        .iter()
        .map(|span| span.bbox.y1)
        .max_by(f32::total_cmp)
        .unwrap_or(0.0);
    let row_top = row
        .iter()
        .map(|span| span.bbox.y0)
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    row_top - previous_bottom
}

pub(crate) fn positioned_table_continuation_gap_threshold(
    previous: &[&TextSpan],
    row: &[&TextSpan],
) -> f32 {
    previous
        .iter()
        .chain(row.iter())
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .filter(|height| *height > 0.0 && height.is_finite())
        .max_by(f32::total_cmp)
        .map(|height| (height * 0.75).max(8.0))
        .unwrap_or(8.0)
}
