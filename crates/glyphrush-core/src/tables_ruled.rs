use crate::*;

#[derive(Clone, Debug)]
pub(crate) struct RulingCluster {
    position: f32,
    start: f32,
    end: f32,
}

pub(crate) fn cluster_ruling_lines(
    lines: &[ExtractedRulingLine],
    tolerance: f32,
) -> Vec<RulingCluster> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut sorted = lines.to_vec();
    sorted.sort_by(|left, right| left.position.total_cmp(&right.position));

    let mut members: Vec<Vec<ExtractedRulingLine>> = Vec::new();
    for line in sorted {
        if let Some(cluster) = members.last_mut() {
            let mean =
                cluster.iter().map(|entry| entry.position).sum::<f32>() / cluster.len() as f32;
            if (line.position - mean).abs() <= tolerance {
                cluster.push(line);
                continue;
            }
        }
        members.push(vec![line]);
    }

    members
        .into_iter()
        .map(|cluster| {
            let count = cluster.len() as f32;
            RulingCluster {
                position: cluster.iter().map(|entry| entry.position).sum::<f32>() / count,
                start: cluster
                    .iter()
                    .map(|entry| entry.start)
                    .fold(f32::INFINITY, f32::min),
                end: cluster
                    .iter()
                    .map(|entry| entry.end)
                    .fold(f32::NEG_INFINITY, f32::max),
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(crate) struct RuledGridLattice {
    vertical: Vec<RulingCluster>,
    region_x0: f32,
    region_x1: f32,
    region_y0: f32,
    region_y1: f32,
}

pub(crate) fn cluster_y_overlap_fraction(
    cluster: &RulingCluster,
    band_start: f32,
    band_end: f32,
) -> f32 {
    let band_span = band_end - band_start;
    if band_span <= 0.0 {
        return 0.0;
    }
    let overlap_start = cluster.start.max(band_start);
    let overlap_end = cluster.end.min(band_end);
    if overlap_end <= overlap_start {
        return 0.0;
    }
    (overlap_end - overlap_start) / band_span
}

pub(crate) fn filter_ruled_grid_clusters(
    vertical: Vec<RulingCluster>,
    horizontal: Vec<RulingCluster>,
) -> Option<RuledGridLattice> {
    let seed = vertical
        .iter()
        .max_by(|left, right| (left.end - left.start).total_cmp(&(right.end - right.start)))?;
    let seed_band_start = seed.start;
    let seed_band_end = seed.end;
    if seed_band_end <= seed_band_start {
        return None;
    }

    let mut kept_vertical = vertical
        .into_iter()
        .filter(|cluster| {
            cluster_y_overlap_fraction(cluster, seed_band_start, seed_band_end) >= 0.6
        })
        .collect::<Vec<_>>();
    if kept_vertical.len() < 3 {
        return None;
    }

    kept_vertical.sort_by(|left, right| left.position.total_cmp(&right.position));

    let vertical_band_start = kept_vertical
        .iter()
        .map(|cluster| cluster.start)
        .fold(f32::INFINITY, f32::min);
    let vertical_band_end = kept_vertical
        .iter()
        .map(|cluster| cluster.end)
        .fold(f32::NEG_INFINITY, f32::max);
    if vertical_band_end <= vertical_band_start {
        return None;
    }

    let region_x0 = kept_vertical.first()?.position;
    let region_x1 = kept_vertical.last()?.position;
    let region_x_span = region_x1 - region_x0;
    if region_x_span <= 0.0 {
        return None;
    }

    let qualifying_horizontal: Vec<&RulingCluster> = horizontal
        .iter()
        .filter(|cluster| (cluster.end - cluster.start) >= region_x_span * 0.6)
        .collect();
    let (region_y0, region_y1) = if qualifying_horizontal.len() >= 2 {
        let hy0 = qualifying_horizontal
            .iter()
            .map(|cluster| cluster.position)
            .min_by(f32::total_cmp)
            .unwrap_or(vertical_band_start);
        let hy1 = qualifying_horizontal
            .iter()
            .map(|cluster| cluster.position)
            .max_by(f32::total_cmp)
            .unwrap_or(vertical_band_end);
        (vertical_band_start.min(hy0), vertical_band_end.max(hy1))
    } else {
        (vertical_band_start, vertical_band_end)
    };

    Some(RuledGridLattice {
        vertical: kept_vertical,
        region_x0,
        region_x1,
        region_y0,
        region_y1,
    })
}

pub(crate) fn span_center_x(span: &TextSpan) -> f32 {
    (span.bbox.x0 + span.bbox.x1) / 2.0
}

pub(crate) fn grid_interval_index(value: f32, positions: &[f32]) -> Option<usize> {
    (0..positions.len().saturating_sub(1))
        .find(|&index| value > positions[index] && value < positions[index + 1])
}

pub(crate) fn span_straddles_vertical_boundary(
    span: &TextSpan,
    vertical_positions: &[f32],
) -> bool {
    vertical_positions
        .iter()
        .any(|&position| span.bbox.x0 + 4.0 < position && position < span.bbox.x1 - 4.0)
}

pub(crate) fn ruled_grid_row_has_non_overlapping_columns(row_cells: &[Vec<&TextSpan>]) -> bool {
    let occupied: Vec<(usize, f32, f32)> = row_cells
        .iter()
        .enumerate()
        .filter_map(|(column_index, cell_spans)| {
            if cell_spans.is_empty() {
                return None;
            }
            let x0 = cell_spans
                .iter()
                .map(|span| span.bbox.x0)
                .min_by(f32::total_cmp)
                .unwrap_or(0.0);
            let x1 = cell_spans
                .iter()
                .map(|span| span.bbox.x1)
                .max_by(f32::total_cmp)
                .unwrap_or(0.0);
            Some((column_index, x0, x1))
        })
        .collect();
    if occupied.len() < 2 {
        return false;
    }
    for left in 0..occupied.len() {
        for right in (left + 1)..occupied.len() {
            let (_, left_x0, left_x1) = occupied[left];
            let (_, right_x0, right_x1) = occupied[right];
            if left_x1 <= right_x0 || right_x1 <= left_x0 {
                return true;
            }
        }
    }
    false
}

pub(crate) fn ruled_grid_cell_text(spans: &[&TextSpan]) -> String {
    let mut sorted = spans.to_vec();
    sorted.sort_by(|left, right| {
        left.bbox
            .x0
            .total_cmp(&right.bbox.x0)
            .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
    });
    sorted
        .iter()
        .map(|span| span.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn looks_like_ruled_grid_wrapped_continuation_row(
    previous_cells: &[Vec<&TextSpan>],
    row_cells: &[Vec<&TextSpan>],
) -> bool {
    let non_empty_columns: Vec<usize> = row_cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| !cell.is_empty())
        .map(|(index, _)| index)
        .collect();
    if non_empty_columns.len() != 1 {
        return false;
    }
    let column_index = non_empty_columns[0];
    if previous_cells
        .get(column_index)
        .is_none_or(|cell| cell.is_empty())
    {
        return false;
    }
    let continuation_text = ruled_grid_cell_text(&row_cells[column_index]);
    continuation_text
        .chars()
        .next()
        .is_some_and(|ch| ch.is_lowercase())
}

pub(crate) fn merge_wrapped_ruled_grid_table_rows(
    rows: Vec<Vec<Vec<&TextSpan>>>,
) -> Vec<Vec<Vec<&TextSpan>>> {
    let mut merged: Vec<Vec<Vec<&TextSpan>>> = Vec::new();
    for row in rows {
        if let Some(previous) = merged.last_mut()
            && looks_like_ruled_grid_wrapped_continuation_row(previous, &row)
        {
            let column_index = row
                .iter()
                .enumerate()
                .find(|(_, cell)| !cell.is_empty())
                .map(|(index, _)| index)
                .expect("continuation row has one non-empty column");
            previous[column_index].extend(&row[column_index]);
            continue;
        }
        merged.push(row);
    }
    merged
}

pub(crate) fn ruled_grid_table_blocks(
    page_index: u32,
    dimensions: &PageDimensions,
    span_refs: &[&TextSpan],
    ruling_lines: &[ExtractedRulingLine],
) -> Option<Vec<LayoutBlock>> {
    let vertical_lines: Vec<ExtractedRulingLine> = ruling_lines
        .iter()
        .filter(|line| line.orientation == RulingOrientation::Vertical)
        .cloned()
        .collect();
    let horizontal_lines: Vec<ExtractedRulingLine> = ruling_lines
        .iter()
        .filter(|line| line.orientation == RulingOrientation::Horizontal)
        .cloned()
        .collect();

    let vertical_clusters = cluster_ruling_lines(&vertical_lines, 2.0);
    let horizontal_clusters = cluster_ruling_lines(&horizontal_lines, 2.0);
    let lattice = filter_ruled_grid_clusters(vertical_clusters, horizontal_clusters)?;
    let RuledGridLattice {
        vertical: kept_vertical,
        region_x0,
        region_x1,
        region_y0,
        region_y1,
    } = lattice;

    let vertical_positions = kept_vertical
        .iter()
        .map(|cluster| cluster.position)
        .collect::<Vec<_>>();
    let column_count = vertical_positions.len().saturating_sub(1);
    if column_count == 0 {
        return None;
    }

    let mut leading_leftovers = Vec::new();
    let mut trailing_leftovers = Vec::new();
    let mut in_region_spans = Vec::new();

    for span in span_refs {
        let center_x = span_center_x(span);
        let center_y = span_center_y(span);
        if center_x < region_x0
            || center_x > region_x1
            || center_y < region_y0
            || center_y > region_y1
        {
            if center_y < region_y0 {
                leading_leftovers.push(*span);
            } else if center_y > region_y1 {
                trailing_leftovers.push(*span);
            } else {
                leading_leftovers.push(*span);
            }
            continue;
        }

        let Some(column_index) = grid_interval_index(center_x, &vertical_positions) else {
            leading_leftovers.push(*span);
            continue;
        };
        let _ = column_index;
        in_region_spans.push(*span);
    }

    if in_region_spans.is_empty() {
        return None;
    }

    let straddling = in_region_spans
        .iter()
        .filter(|span| span_straddles_vertical_boundary(span, &vertical_positions))
        .count();
    if straddling as f32 / in_region_spans.len() as f32 > 0.3 {
        return None;
    }

    let text_rows = group_positioned_text_rows(in_region_spans.to_vec());
    let mut grid_rows: Vec<Vec<Vec<&TextSpan>>> = text_rows
        .iter()
        .map(|row_spans| {
            let mut row_cells = vec![Vec::<&TextSpan>::new(); column_count];
            for span in row_spans {
                let center_x = span_center_x(span);
                if let Some(column_index) = grid_interval_index(center_x, &vertical_positions) {
                    row_cells[column_index].push(span);
                }
            }
            row_cells
        })
        .collect();

    grid_rows = merge_wrapped_ruled_grid_table_rows(grid_rows);

    let mut non_empty_cells = 0usize;
    let mut rows_with_two_plus = 0usize;
    let mut has_non_overlapping_row = false;
    let mut dense_cells = 0usize;
    let mut max_cell_spans = 0usize;
    for row_cells in &grid_rows {
        let row_non_empty = row_cells.iter().filter(|cell| !cell.is_empty()).count();
        non_empty_cells += row_non_empty;
        if row_non_empty >= 2 {
            rows_with_two_plus += 1;
        }
        if ruled_grid_row_has_non_overlapping_columns(row_cells) {
            has_non_overlapping_row = true;
        }
        for cell in row_cells {
            if cell.len() > 4 {
                dense_cells += 1;
            }
            max_cell_spans = max_cell_spans.max(cell.len());
        }
    }

    if grid_rows.len() < 2
        || non_empty_cells < 4
        || rows_with_two_plus < 2
        || !has_non_overlapping_row
    {
        return None;
    }

    // Diagram lattices (architecture figures, flowcharts) put clouds of tiny
    // token fragments into most "cells"; real tables keep one or two spans
    // per cell outside header regions. Reject only widespread density so
    // dense header rows do not disqualify an otherwise clean grid.
    if max_cell_spans > 30 || dense_cells * 3 > non_empty_cells {
        return None;
    }

    let table_rows = grid_rows
        .iter()
        .enumerate()
        .map(|(row_index, row_cells)| {
            let cells = row_cells
                .iter()
                .enumerate()
                .map(|(column_index, cell_spans)| {
                    let text = ruled_grid_cell_text(cell_spans);
                    let bbox = if cell_spans.is_empty() {
                        None
                    } else {
                        union_span_refs_bbox(cell_spans)
                    };
                    LayoutTableCell {
                        column_index,
                        text,
                        bbox,
                    }
                })
                .collect::<Vec<_>>();
            let row_spans = row_cells.iter().flatten().copied().collect::<Vec<_>>();
            LayoutTableRow {
                row_index,
                bbox: union_span_refs_bbox(&row_spans),
                cells,
            }
        })
        .collect::<Vec<_>>();

    let table = LayoutTable { rows: table_rows };
    let table_bbox = union_span_refs_bbox(&in_region_spans)?;
    let table_text = table
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

    let table_block = LayoutBlock {
        block_id: String::new(),
        bbox: table_bbox,
        text: table_text,
        kind: LayoutBlockKind::Table,
        table: Some(table),
    };

    let mut blocks = Vec::new();
    let mut next_block_index = 0usize;

    for group in group_spans_for_reading_order_from_refs(leading_leftovers, dimensions) {
        if let Some(block) = layout_block_from_span_group(page_index, next_block_index, group, true)
        {
            blocks.push(block);
            next_block_index += 1;
        }
    }

    blocks.push(LayoutBlock {
        block_id: format!("p{page_index:06}:b{next_block_index:06}"),
        ..table_block
    });
    next_block_index += 1;

    for group in group_spans_for_reading_order_from_refs(trailing_leftovers, dimensions) {
        if let Some(block) = layout_block_from_span_group(page_index, next_block_index, group, true)
        {
            blocks.push(block);
            next_block_index += 1;
        }
    }

    Some(blocks)
}
