use crate::*;

pub(crate) fn split_text_blocks(text: &str, run_table_recovery: bool) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();

    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            if run_table_recovery && should_keep_fragmented_symbol_table_open(&current) {
                continue;
            }
            if run_table_recovery && should_keep_electrical_characteristics_table_open(&current) {
                continue;
            }
            if run_table_recovery && should_keep_bullet_leader_table_open(&current) {
                continue;
            }
            if !current.is_empty() {
                push_reflowed_text_blocks(&mut blocks, &current, run_table_recovery);
                current.clear();
            }
        } else {
            current.push(line.trim().to_string());
        }
    }

    if !current.is_empty() {
        push_reflowed_text_blocks(&mut blocks, &current, run_table_recovery);
    }

    merge_adjacent_fragment_blocks(blocks)
}

pub(crate) fn should_keep_fragmented_symbol_table_open(current: &[String]) -> bool {
    let refs = current.iter().map(String::as_str).collect::<Vec<_>>();
    let Some(header_index) = refs
        .iter()
        .position(|line| symbol_parameter_table_header_cells(line).is_some())
    else {
        return false;
    };

    match fragmented_symbol_parameter_table_rows_prefix(&refs[header_index..]) {
        Some((rows, consumed)) => header_index + consumed != refs.len() || rows.len() < 3,
        None => true,
    }
}

pub(crate) fn should_keep_electrical_characteristics_table_open(current: &[String]) -> bool {
    let refs = current.iter().map(String::as_str).collect::<Vec<_>>();
    (0..refs.len()).any(|index| {
        electrical_characteristics_table_header_len(&refs[index..]).is_some()
            || parameter_symbol_conditions_table_header_len(&refs[index..]).is_some()
            || parameter_test_condition_table_header_len(&refs[index..]).is_some()
    })
}

pub(crate) fn should_keep_bullet_leader_table_open(current: &[String]) -> bool {
    let refs = current.iter().map(String::as_str).collect::<Vec<_>>();
    let Some(table_start) = refs.iter().position(|line| {
        bullet_leader_direct_row(line.trim()).is_some()
            || bullet_leader_pending_parameter(line.trim()).is_some()
    }) else {
        return false;
    };

    let mut pending_parameter = false;
    let mut row_count = 0;

    for line in refs[table_start..].iter().map(|line| line.trim()) {
        if line.is_empty() || looks_like_bullet_leader_table_terminator(line) {
            break;
        }

        if bullet_leader_direct_row(line).is_some() {
            pending_parameter = false;
            row_count += 1;
            continue;
        }

        if leader_continuation_row(line).is_some() {
            if pending_parameter {
                pending_parameter = false;
                row_count += 1;
            }
            continue;
        }

        if bullet_leader_pending_parameter(line).is_some() {
            pending_parameter = true;
            continue;
        }

        if pending_parameter && looks_like_bullet_leader_note_fragment(line) {
            continue;
        }

        break;
    }

    pending_parameter && row_count > 0
}

pub(crate) type TextTableRowsPrefix = fn(&[&str]) -> Option<(Vec<Vec<String>>, usize)>;

pub(crate) enum TextTableCaptionMode {
    None,
    OptionalBeforeHeader(fn(&str) -> bool),
    EmitAtAnchor,
}

pub(crate) enum TextTableEmitMode {
    JoinVerbatim,
    ReflowBlock,
}

pub(crate) struct TextTablePattern {
    min_lines: usize,
    find_anchor: fn(&[&str]) -> Option<usize>,
    table_line_offset: usize,
    rows_prefix: TextTableRowsPrefix,
    min_table_rows: Option<usize>,
    caption: TextTableCaptionMode,
    emit: TextTableEmitMode,
}

pub(crate) fn find_embedded_pin_function_table_header(refs: &[&str]) -> Option<usize> {
    refs.iter()
        .position(|line| is_pin_function_table_header(line))
}

pub(crate) fn find_pin_number_name_function_table_header(refs: &[&str]) -> Option<usize> {
    (0..refs.len())
        .find(|index| pin_number_name_function_table_header_len(&refs[*index..]).is_some())
}

pub(crate) fn find_symbol_parameter_table_header(refs: &[&str]) -> Option<usize> {
    refs.iter()
        .position(|line| symbol_parameter_table_header_cells(line).is_some())
}

pub(crate) fn find_parameter_symbol_conditions_table_header(refs: &[&str]) -> Option<usize> {
    (0..refs.len())
        .find(|index| parameter_symbol_conditions_table_header_len(&refs[*index..]).is_some())
}

pub(crate) fn find_electrical_characteristics_table_header(refs: &[&str]) -> Option<usize> {
    (0..refs.len())
        .find(|index| electrical_characteristics_table_header_len(&refs[*index..]).is_some())
}

pub(crate) fn find_parameter_test_condition_table_header(refs: &[&str]) -> Option<usize> {
    (0..refs.len())
        .find(|index| parameter_test_condition_table_header_len(&refs[*index..]).is_some())
}

pub(crate) fn find_reflow_profile_table_header(refs: &[&str]) -> Option<usize> {
    refs.iter()
        .position(|line| reflow_profile_table_header_cells(line).is_some())
}

pub(crate) fn find_classification_temperature_caption(refs: &[&str]) -> Option<usize> {
    refs.iter()
        .position(|line| looks_like_classification_temperature_caption(line))
}

pub(crate) fn find_bullet_leader_table_start(refs: &[&str]) -> Option<usize> {
    (0..refs.len()).find(|index| bullet_leader_table_rows_prefix(&refs[*index..]).is_some())
}

pub(crate) fn find_package_pin_description_table_header(refs: &[&str]) -> Option<usize> {
    refs.iter()
        .position(|line| is_package_pin_description_table_start(line))
}

pub(crate) fn find_budget_projection_table_header(refs: &[&str]) -> Option<usize> {
    (0..refs.len()).find(|index| budget_projection_table_header_len(&refs[*index..]).is_some())
}

pub(crate) static TEXT_TABLE_PATTERNS: [TextTablePattern; 11] = [
    TextTablePattern {
        min_lines: 4,
        find_anchor: find_embedded_pin_function_table_header,
        table_line_offset: 0,
        rows_prefix: pin_function_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::OptionalBeforeHeader(looks_like_pin_function_table_caption),
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 5,
        find_anchor: find_pin_number_name_function_table_header,
        table_line_offset: 0,
        rows_prefix: pin_number_name_function_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::OptionalBeforeHeader(looks_like_pin_function_table_caption),
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 5,
        find_anchor: find_symbol_parameter_table_header,
        table_line_offset: 0,
        rows_prefix: fragmented_symbol_parameter_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 4,
        find_anchor: find_parameter_symbol_conditions_table_header,
        table_line_offset: 0,
        rows_prefix: parameter_symbol_conditions_table_rows_prefix,
        min_table_rows: Some(4),
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::ReflowBlock,
    },
    TextTablePattern {
        min_lines: 6,
        find_anchor: find_electrical_characteristics_table_header,
        table_line_offset: 0,
        rows_prefix: electrical_characteristics_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 4,
        find_anchor: find_parameter_test_condition_table_header,
        table_line_offset: 0,
        rows_prefix: parameter_test_condition_table_rows_prefix,
        min_table_rows: Some(4),
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::ReflowBlock,
    },
    TextTablePattern {
        min_lines: 6,
        find_anchor: find_reflow_profile_table_header,
        table_line_offset: 0,
        rows_prefix: reflow_profile_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 6,
        find_anchor: find_classification_temperature_caption,
        table_line_offset: 1,
        rows_prefix: classification_temperature_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::EmitAtAnchor,
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 3,
        find_anchor: find_bullet_leader_table_start,
        table_line_offset: 0,
        rows_prefix: bullet_leader_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 6,
        find_anchor: find_package_pin_description_table_header,
        table_line_offset: 0,
        rows_prefix: package_pin_description_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::OptionalBeforeHeader(looks_like_pin_function_table_caption),
        emit: TextTableEmitMode::JoinVerbatim,
    },
    TextTablePattern {
        min_lines: 5,
        find_anchor: find_budget_projection_table_header,
        table_line_offset: 0,
        rows_prefix: budget_projection_table_rows_prefix,
        min_table_rows: None,
        caption: TextTableCaptionMode::None,
        emit: TextTableEmitMode::JoinVerbatim,
    },
];

pub(crate) fn split_text_table_by_pattern(
    lines: &[String],
    run_table_recovery: bool,
    pattern: &TextTablePattern,
) -> Option<Vec<String>> {
    if !run_table_recovery || lines.len() < pattern.min_lines {
        return None;
    }

    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    let anchor_index = (pattern.find_anchor)(&refs)?;
    let table_start = anchor_index + pattern.table_line_offset;
    let (rows, consumed) = (pattern.rows_prefix)(&refs[table_start..])?;
    if let Some(min_rows) = pattern.min_table_rows
        && rows.len() < min_rows
    {
        return None;
    }
    let table_end = table_start + consumed;

    let mut blocks = Vec::new();
    match pattern.caption {
        TextTableCaptionMode::None => {
            if anchor_index > 0 {
                blocks.push(reflow_text_block(
                    &lines[..anchor_index],
                    run_table_recovery,
                ));
            }
        }
        TextTableCaptionMode::OptionalBeforeHeader(caption_pred) => {
            let caption_index = anchor_index
                .checked_sub(1)
                .filter(|index| caption_pred(refs[*index]));
            let prefix_end = caption_index.unwrap_or(anchor_index);
            if prefix_end > 0 {
                blocks.push(reflow_text_block(&lines[..prefix_end], run_table_recovery));
            }
            if let Some(caption_index) = caption_index {
                blocks.push(lines[caption_index].trim().to_string());
            }
        }
        TextTableCaptionMode::EmitAtAnchor => {
            if anchor_index > 0 {
                blocks.push(reflow_text_block(
                    &lines[..anchor_index],
                    run_table_recovery,
                ));
            }
            blocks.push(lines[anchor_index].trim().to_string());
        }
    }

    match pattern.emit {
        TextTableEmitMode::JoinVerbatim => {
            blocks.push(lines[table_start..table_end].join("\n"));
        }
        TextTableEmitMode::ReflowBlock => {
            blocks.push(reflow_text_block(
                &lines[table_start..table_end],
                run_table_recovery,
            ));
        }
    }

    if table_end < lines.len() {
        blocks.push(reflow_text_block(&lines[table_end..], run_table_recovery));
    }

    Some(blocks)
}

pub(crate) fn push_reflowed_text_blocks(
    blocks: &mut Vec<String>,
    lines: &[String],
    run_table_recovery: bool,
) {
    for pattern in &TEXT_TABLE_PATTERNS {
        if let Some(split_blocks) = split_text_table_by_pattern(lines, run_table_recovery, pattern)
        {
            blocks.extend(split_blocks);
            return;
        }
    }

    if let Some(split_blocks) = split_leading_text_table_caption_blocks(lines, run_table_recovery) {
        blocks.extend(split_blocks);
        return;
    }

    blocks.push(reflow_text_block(lines, run_table_recovery));
}

/// Standalone: caption discovery depends on prefix context and suffix shape checks
/// that do not fit the anchor/rows_prefix table template.
pub(crate) fn split_leading_text_table_caption_blocks(
    lines: &[String],
    run_table_recovery: bool,
) -> Option<Vec<String>> {
    if !run_table_recovery || lines.len() < 3 {
        return None;
    }

    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    let caption_index = (0..refs.len() - 2).find(|index| {
        looks_like_leading_text_table_caption(refs[*index])
            && !prefix_looks_like_table_context(&refs[..*index])
            && table_lines_follow_caption(&refs[*index + 1..])
    })?;

    let mut blocks = Vec::new();
    if caption_index > 0 {
        blocks.push(reflow_text_block(
            &lines[..caption_index],
            run_table_recovery,
        ));
    }
    blocks.push(lines[caption_index].trim().to_string());
    blocks.push(reflow_text_block(
        &lines[caption_index + 1..],
        run_table_recovery,
    ));

    Some(blocks)
}

pub(crate) fn table_lines_follow_caption(lines: &[&str]) -> bool {
    is_table_lines_str(lines)
        || aligned_whitespace_table_rows(lines).is_some()
        || header_guided_whitespace_table_rows(lines).is_some()
}

pub(crate) fn prefix_looks_like_table_context(lines: &[&str]) -> bool {
    lines
        .iter()
        .any(|line| line.contains('|') || line.contains('\t') || has_wide_space_gap(line))
        || is_whitespace_table_lines_str(lines)
}

pub(crate) fn looks_like_leading_text_table_caption(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= 120
        && trimmed.chars().any(char::is_alphabetic)
        && !trimmed.contains('|')
        && !trimmed.contains('\t')
        && !has_wide_space_gap(trimmed)
        && !is_list_lines_str(&[trimmed])
}

pub(crate) fn reflow_text_block(lines: &[String], run_table_recovery: bool) -> String {
    if let Some(list_text) = normalized_list_text(lines) {
        return list_text;
    }

    if lines.len() <= 1
        || is_table_lines(lines)
        || (run_table_recovery && is_whitespace_table_lines(lines))
        || !should_reflow(lines)
    {
        return lines.join("\n");
    }

    let mut output = String::new();
    for fragment in lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
    {
        append_reflow_fragment(&mut output, fragment);
    }
    output
}

pub(crate) fn merge_adjacent_fragment_blocks(blocks: Vec<String>) -> Vec<String> {
    let mut merged = Vec::new();
    let mut current_fragments: Vec<String> = Vec::new();

    for block in blocks {
        if is_fragment_block(&block) {
            current_fragments.extend(block.lines().map(|line| line.trim().to_string()));
            continue;
        }

        flush_fragment_blocks(&mut merged, &mut current_fragments);
        merged.push(block);
    }

    flush_fragment_blocks(&mut merged, &mut current_fragments);
    merged
}

pub(crate) fn flush_fragment_blocks(merged: &mut Vec<String>, fragments: &mut Vec<String>) {
    if fragments.is_empty() {
        return;
    }

    if let Some(previous) = merged.last_mut()
        && let Some(absorb_count) = absorb_fragment_prefix_len(previous, fragments)
    {
        let mut reflowed = previous.clone();
        for fragment in fragments
            .iter()
            .take(absorb_count)
            .map(|fragment| fragment.trim())
        {
            append_reflow_fragment(&mut reflowed, fragment);
        }
        *previous = reflowed;
        fragments.drain(..absorb_count);
        if fragments.is_empty() {
            return;
        }
    }

    for group in split_fragment_groups(fragments) {
        if group.len() == 1 {
            merged.push(group[0].clone());
        } else {
            merged.push(reflow_text_block(&group, false));
        }
    }
    fragments.clear();
}

pub(crate) fn split_fragment_groups(fragments: &[String]) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for fragment in fragments {
        if let Some(previous) = current.last()
            && starts_new_fragment_group(previous.as_str(), fragment.as_str())
        {
            groups.push(current);
            current = Vec::new();
        }
        current.push(fragment.clone());
    }

    if !current.is_empty() {
        groups.push(current);
    }

    groups
}

pub(crate) fn starts_new_fragment_group(previous: &str, next: &str) -> bool {
    previous.chars().all(|ch| ch.is_ascii_digit())
        && next
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
}

pub(crate) fn absorb_fragment_prefix_len(previous: &str, fragments: &[String]) -> Option<usize> {
    if previous.contains('\n') || is_table_lines_str(&[previous]) || is_list_lines_str(&[previous])
    {
        return None;
    }

    let last_token = previous.split_whitespace().last()?;

    if fragments.is_empty()
        || !is_short_fragment(last_token)
        || !last_token.chars().any(|ch| ch.is_ascii_digit())
    {
        return None;
    }

    let count = fragments
        .iter()
        .take_while(|fragment| {
            is_short_fragment(fragment) && fragment.chars().all(|ch| ch.is_ascii_digit())
        })
        .count();

    (count > 0).then_some(count)
}

pub(crate) fn is_fragment_block(block: &str) -> bool {
    let lines = block.lines().map(str::to_string).collect::<Vec<_>>();
    !lines.is_empty()
        && lines.iter().all(|line| is_short_fragment(line))
        && !is_table_lines(&lines)
        && !is_list_lines(&lines)
}

pub(crate) fn should_reflow(lines: &[String]) -> bool {
    lines.iter().all(|line| is_short_fragment(line))
        || lines.iter().skip(1).all(|line| is_short_fragment(line))
}

pub(crate) fn is_short_fragment(line: &str) -> bool {
    line.trim().chars().count() <= 8
}

pub(crate) fn append_reflow_fragment(output: &mut String, fragment: &str) {
    if output.is_empty() {
        output.push_str(fragment);
        return;
    }

    let previous = output.chars().last().unwrap_or_default();
    let next = fragment.chars().next().unwrap_or_default();

    if matches!(next, '.' | ',' | ':' | ';' | ')' | ']')
        || matches!(fragment, "-" | "/" | "–")
        || matches!(previous, '-' | '/' | '–')
        || (previous.is_numeric() && next.is_numeric())
    {
        output.push_str(fragment);
    } else {
        output.push(' ');
        output.push_str(fragment);
    }
}

pub(crate) fn classify_layout_block(text: &str, run_table_recovery: bool) -> LayoutBlockKind {
    let lines = text.lines().collect::<Vec<_>>();
    if is_list_lines_str(&lines) {
        return LayoutBlockKind::List;
    }
    if is_table_lines_str(&lines) || (run_table_recovery && is_whitespace_table_lines_str(&lines)) {
        return LayoutBlockKind::Table;
    }
    if lines.len() == 1
        && (is_heading_line(lines[0]) || looks_like_pin_function_table_caption(lines[0]))
    {
        return LayoutBlockKind::Heading;
    }

    LayoutBlockKind::Paragraph
}

pub(crate) fn table_payload_from_text(text: &str, kind: &LayoutBlockKind) -> Option<LayoutTable> {
    if *kind != LayoutBlockKind::Table {
        return None;
    }

    let lines = text.lines().collect::<Vec<_>>();
    if let Some(rows) = pin_function_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = pin_number_name_function_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = fragmented_symbol_parameter_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = parameter_symbol_conditions_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = electrical_characteristics_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = parameter_test_condition_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = reflow_profile_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = classification_temperature_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = bullet_leader_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = package_pin_description_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = part_number_ordering_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = budget_projection_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = aligned_whitespace_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = header_guided_whitespace_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    if let Some(rows) = key_value_table_rows(&lines) {
        return layout_table_from_text_rows(rows);
    }

    let rows = lines
        .iter()
        .map(|line| table_cells_from_text_line(line))
        .collect::<Vec<_>>();

    layout_table_from_text_rows(rows)
}

pub(crate) fn layout_table_from_text_rows(rows: Vec<Vec<String>>) -> Option<LayoutTable> {
    let rows = rows
        .into_iter()
        .filter(|row| !is_markdown_table_separator_row(row))
        .enumerate()
        .filter_map(|(row_index, row)| {
            let cells = row
                .into_iter()
                .enumerate()
                .map(|(column_index, text)| LayoutTableCell {
                    column_index,
                    text,
                    bbox: None,
                })
                .collect::<Vec<_>>();

            (cells.len() >= 2).then_some(LayoutTableRow {
                row_index,
                bbox: None,
                cells,
            })
        })
        .collect::<Vec<_>>();

    (rows.len() >= 2).then_some(LayoutTable { rows })
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

pub(crate) fn table_cells_from_text_line(line: &str) -> Vec<String> {
    if line.contains('|') {
        split_delimited_table_cells(line, '|')
    } else if line.contains('\t') {
        split_delimited_table_cells(line, '\t')
    } else {
        line.split_whitespace().map(ToString::to_string).collect()
    }
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

pub(crate) fn is_table_lines(lines: &[String]) -> bool {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    is_table_lines_str(&refs)
}

pub(crate) fn is_table_lines_str(lines: &[&str]) -> bool {
    lines.len() >= 2
        && lines
            .iter()
            .all(|line| line.contains('|') || line.contains('\t'))
}

pub(crate) fn is_whitespace_table_lines(lines: &[String]) -> bool {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    is_whitespace_table_lines_str(&refs)
}

pub(crate) fn is_whitespace_table_lines_str(lines: &[&str]) -> bool {
    if pin_function_table_rows(lines).is_some() {
        return true;
    }

    if pin_number_name_function_table_rows(lines).is_some() {
        return true;
    }

    if fragmented_symbol_parameter_table_rows(lines).is_some() {
        return true;
    }

    if parameter_symbol_conditions_table_rows(lines).is_some() {
        return true;
    }

    if electrical_characteristics_table_rows(lines).is_some() {
        return true;
    }

    if parameter_test_condition_table_rows(lines).is_some() {
        return true;
    }

    if reflow_profile_table_rows(lines).is_some() {
        return true;
    }

    if classification_temperature_table_rows(lines).is_some() {
        return true;
    }

    if bullet_leader_table_rows(lines).is_some() {
        return true;
    }

    if package_pin_description_table_rows(lines).is_some() {
        return true;
    }

    if part_number_ordering_table_rows(lines).is_some() {
        return true;
    }

    if budget_projection_table_rows(lines).is_some() {
        return true;
    }

    if aligned_whitespace_table_rows(lines).is_some() {
        return true;
    }

    if header_guided_whitespace_table_rows(lines).is_some() {
        return true;
    }

    if key_value_table_rows(lines).is_some() {
        return true;
    }

    let rows = lines
        .iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if rows.len() < 2 {
        return false;
    }

    let Some(column_count) = rows.first().map(Vec::len) else {
        return false;
    };
    (2..=8).contains(&column_count)
        && rows.iter().all(|row| {
            row.len() == column_count
                && row
                    .iter()
                    .all(|cell| !cell.is_empty() && cell.chars().count() <= 40)
        })
}
