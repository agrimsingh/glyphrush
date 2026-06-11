use crate::*;

pub(crate) fn pin_function_table_rows_prefix(lines: &[&str]) -> Option<(Vec<Vec<String>>, usize)> {
    if lines.len() < 3 || !is_pin_function_table_header(lines[0]) {
        return None;
    }

    let mut rows = vec![vec![
        "Pin Name".to_string(),
        "Pin No.".to_string(),
        "Pin Function".to_string(),
    ]];
    let mut consumed = 1;
    let mut data_row_count = 0;
    let mut pending_name_tokens: Vec<String> = Vec::new();

    for (offset, line) in lines.iter().enumerate().skip(1) {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            break;
        }

        let last_function_empty = rows
            .last()
            .and_then(|row| row.get(2))
            .is_some_and(|function| function.trim().is_empty());
        if data_row_count > 0
            && last_function_empty
            && pin_function_continuation_line(&tokens, true)
            && let Some(function_cell) = rows.last_mut().and_then(|row| row.get_mut(2))
        {
            function_cell.push_str(&tokens.join(" "));
            consumed = offset + 1;
            continue;
        }

        if let Some(row) = pin_function_data_row(&tokens, &mut pending_name_tokens) {
            rows.push(row);
            data_row_count += 1;
            consumed = offset + 1;
            continue;
        }

        if !pending_name_tokens.is_empty() && pin_name_fragment_line(&tokens, true) {
            pending_name_tokens.extend(tokens.iter().map(|token| (*token).to_string()));
            consumed = offset + 1;
            continue;
        }

        if data_row_count > 0
            && pin_function_continuation_line(&tokens, false)
            && let Some(function_cell) = rows.last_mut().and_then(|row| row.get_mut(2))
        {
            if !function_cell.is_empty() {
                function_cell.push(' ');
            }
            function_cell.push_str(&tokens.join(" "));
            consumed = offset + 1;
            continue;
        }

        if pin_name_fragment_line(&tokens, false)
            && following_lines_complete_pin_name(&lines[offset + 1..])
        {
            pending_name_tokens.extend(tokens.iter().map(|token| (*token).to_string()));
            consumed = offset + 1;
            continue;
        }

        break;
    }

    let all_functions_populated = rows
        .iter()
        .skip(1)
        .all(|row| row.get(2).is_some_and(|cell| !cell.trim().is_empty()));

    (data_row_count >= 2 && pending_name_tokens.is_empty() && all_functions_populated)
        .then_some((rows, consumed))
}

pub(crate) fn is_pin_function_table_header(line: &str) -> bool {
    normalize_pin_table_header(line) == "pin name pin no pin function"
}

pub(crate) fn normalize_pin_table_header(line: &str) -> String {
    line.split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn looks_like_pin_function_table_caption(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    normalized.contains("pin") && normalized.contains("description")
}

pub(crate) fn pin_function_data_row(
    tokens: &[&str],
    pending_name_tokens: &mut Vec<String>,
) -> Option<Vec<String>> {
    let pin_search_start = usize::from(pending_name_tokens.is_empty());
    let pin_index = tokens
        .iter()
        .enumerate()
        .skip(pin_search_start)
        .take(4)
        .find_map(|(index, token)| looks_like_pin_number_token(token).then_some(index))?;

    let mut name_tokens = std::mem::take(pending_name_tokens);
    name_tokens.extend(tokens[..pin_index].iter().map(|token| (*token).to_string()));
    if name_tokens.is_empty() || !pin_name_tokens_look_plausible(&name_tokens) {
        return None;
    }

    let pin_number = tokens[pin_index]
        .trim_matches(|ch: char| matches!(ch, ',' | ';'))
        .to_string();
    let function = tokens
        .get(pin_index + 1..)
        .map(|tokens| tokens.join(" "))
        .unwrap_or_default();

    Some(vec![name_tokens.join(" "), pin_number, function])
}

pub(crate) fn looks_like_pin_number_token(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    if token.is_empty() || token.chars().count() > 6 {
        return false;
    }

    let has_digit = token.chars().any(|ch| ch.is_ascii_digit());
    let digit_pin = has_digit
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '/' | ','));
    let exposed_pad = matches!(token, "EP" | "EPAD" | "PAD");

    digit_pin || exposed_pad
}

pub(crate) fn pin_name_fragment_line(tokens: &[&str], has_pending_name: bool) -> bool {
    if tokens.is_empty() || tokens.len() > 3 {
        return false;
    }

    if !has_pending_name && tokens.iter().any(|token| starts_with_lowercase(token)) {
        return false;
    }

    tokens.iter().all(|token| {
        let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
        !token.is_empty()
            && token.chars().count() <= 24
            && token
                .chars()
                .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/' | '+'))
            && token.chars().any(char::is_alphabetic)
    })
}

pub(crate) fn pin_name_tokens_look_plausible(tokens: &[String]) -> bool {
    tokens.len() <= 4
        && tokens.iter().all(|token| {
            let token = token.trim();
            !token.is_empty()
                && token.chars().count() <= 24
                && token
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/' | '+'))
        })
}

pub(crate) fn following_lines_complete_pin_name(lines: &[&str]) -> bool {
    for line in lines.iter().take(3) {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            return false;
        }
        if tokens
            .iter()
            .take(2)
            .any(|token| looks_like_pin_number_token(token))
        {
            return true;
        }
        if !pin_name_fragment_line(&tokens, true) {
            return false;
        }
    }

    false
}

pub(crate) fn pin_function_continuation_line(
    tokens: &[&str],
    required_for_empty_function: bool,
) -> bool {
    if tokens.is_empty() || tokens.len() > 24 {
        return false;
    }

    let has_lowercase = tokens.iter().any(|token| {
        token
            .chars()
            .any(|ch| ch.is_alphabetic() && ch.is_lowercase())
    });
    if !has_lowercase {
        return false;
    }

    if tokens
        .first()
        .is_some_and(|token| starts_with_lowercase(token))
    {
        return true;
    }

    let line = tokens.join(" ");
    required_for_empty_function || (tokens.len() >= 4 && !is_title_case_heading_line(&line))
}

pub(crate) fn package_pin_description_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = package_pin_description_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn package_pin_description_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    if lines.len() < 6 || !is_package_pin_description_table_start(lines[0]) {
        return None;
    }
    if normalize_pin_table_header(lines[1]) != "pin name function" {
        return None;
    }

    let (package_headers, data_start) = package_pin_description_headers(lines)?;
    let mut rows = vec![
        package_headers
            .into_iter()
            .chain(["Pin Name".to_string(), "Function".to_string()])
            .collect::<Vec<_>>(),
    ];
    let mut consumed = data_start;
    let mut data_row_count = 0;

    for (offset, line) in lines.iter().enumerate().skip(data_start) {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            break;
        }

        if let Some(row) = package_pin_description_data_row(&tokens, rows[0].len() - 2) {
            rows.push(row);
            data_row_count += 1;
            consumed = offset + 1;
            continue;
        }

        if data_row_count > 0
            && pin_function_continuation_line(
                &tokens,
                rows.last().is_some_and(|row| {
                    row.last()
                        .is_some_and(|function| function.trim().is_empty())
                }),
            )
            && let Some(function_cell) = rows.last_mut().and_then(|row| row.last_mut())
        {
            if !function_cell.is_empty() {
                function_cell.push(' ');
            }
            function_cell.push_str(&tokens.join(" "));
            consumed = offset + 1;
            continue;
        }

        break;
    }

    let all_functions_populated = rows
        .iter()
        .skip(1)
        .all(|row| row.last().is_some_and(|cell| !cell.trim().is_empty()));

    (data_row_count >= 2 && all_functions_populated).then_some((rows, consumed))
}

pub(crate) fn is_package_pin_description_table_start(line: &str) -> bool {
    normalize_pin_table_header(line) == "pin number"
}

pub(crate) fn package_pin_description_headers(lines: &[&str]) -> Option<(Vec<String>, usize)> {
    let mut headers = lines
        .get(2)?
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if headers.len() < 2 {
        return None;
    }

    let third_header = lines.get(3)?.trim();
    if third_header.is_empty() || !third_header.chars().any(char::is_alphabetic) {
        return None;
    }
    headers.push(third_header.to_string());
    let mut consumed = 4;

    if let Some(parenthetical) = lines.get(consumed).map(|line| line.trim())
        && parenthetical.starts_with('(')
        && parenthetical.ends_with(')')
    {
        if let Some(last) = headers.last_mut() {
            last.push(' ');
            last.push_str(parenthetical);
        }
        consumed += 1;
    }

    (headers.len() >= 3).then_some((headers, consumed))
}

pub(crate) fn package_pin_description_data_row(
    tokens: &[&str],
    package_column_count: usize,
) -> Option<Vec<String>> {
    if tokens.len() < package_column_count + 1 || !(2..=4).contains(&package_column_count) {
        return None;
    }

    let mut row = Vec::with_capacity(package_column_count + 2);
    let mut cursor = 0;
    for _ in 0..package_column_count.saturating_sub(1) {
        let token = tokens.get(cursor)?;
        if !looks_like_package_pin_cell(token) {
            return None;
        }
        row.push(normalize_package_pin_cell(token));
        cursor += 1;
    }

    let final_package_cell =
        if tokens
            .get(cursor)
            .zip(tokens.get(cursor + 1))
            .is_some_and(|(first, second)| {
                first.eq_ignore_ascii_case("center") && second.eq_ignore_ascii_case("pad")
            })
        {
            cursor += 2;
            "Center Pad".to_string()
        } else {
            let token = tokens.get(cursor)?;
            if !looks_like_package_pin_cell(token) {
                return None;
            }
            cursor += 1;
            normalize_package_pin_cell(token)
        };
    row.push(final_package_cell);

    let pin_name = tokens.get(cursor)?;
    if !looks_like_pin_name_or_placeholder(pin_name) {
        return None;
    }
    row.push(normalize_package_pin_cell(pin_name));
    cursor += 1;

    row.push(
        tokens
            .get(cursor..)
            .map(|tokens| tokens.join(" "))
            .unwrap_or_default(),
    );
    Some(row)
}

pub(crate) fn looks_like_package_pin_cell(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    is_dash_placeholder(token) || token.chars().all(|ch| ch.is_ascii_digit())
}

pub(crate) fn looks_like_pin_name_or_placeholder(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    is_dash_placeholder(token)
        || (!token.is_empty()
            && token.chars().count() <= 24
            && token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '+'))
            && token.chars().any(char::is_alphabetic))
}

pub(crate) fn normalize_package_pin_cell(token: &str) -> String {
    token
        .trim_matches(|ch: char| matches!(ch, ',' | ';'))
        .to_string()
}

pub(crate) fn is_dash_placeholder(token: &str) -> bool {
    matches!(token, "-" | "–" | "—")
}

pub(crate) fn part_number_ordering_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    if lines.len() < 3
        || normalize_pin_table_header(lines[0]) != "part number vout package identification code"
    {
        return None;
    }

    let mut rows = vec![vec![
        "Part Number".to_string(),
        "VOUT".to_string(),
        "Package".to_string(),
        "Identification Code".to_string(),
    ]];

    for line in lines.iter().skip(1) {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        let row = part_number_ordering_data_row(&tokens)?;
        rows.push(row);
    }

    (rows.len() >= 3).then_some(rows)
}

pub(crate) fn part_number_ordering_data_row(tokens: &[&str]) -> Option<Vec<String>> {
    if tokens.len() < 4 {
        return None;
    }

    let part_number = *tokens.first()?;
    let vout = *tokens.get(1)?;
    let identification_code = *tokens.last()?;
    let package_tokens = &tokens[2..tokens.len() - 1];
    if !looks_like_ordering_part_number(part_number)
        || !looks_like_voltage_cell(vout)
        || !looks_like_identification_code(identification_code)
        || package_tokens.is_empty()
    {
        return None;
    }

    Some(vec![
        part_number.to_string(),
        vout.to_string(),
        package_tokens.join(" "),
        identification_code.to_string(),
    ])
}

pub(crate) fn looks_like_ordering_part_number(token: &str) -> bool {
    let token = token.trim();
    token.len() >= 6
        && token.contains('-')
        && token.chars().any(|ch| ch.is_ascii_digit())
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/'))
}

pub(crate) fn looks_like_voltage_cell(token: &str) -> bool {
    let Some(value) = token.strip_suffix('V') else {
        return false;
    };
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

pub(crate) fn looks_like_identification_code(token: &str) -> bool {
    let token = token.trim();
    (2..=8).contains(&token.len())
        && token.chars().all(|ch| ch.is_ascii_alphanumeric())
        && token.chars().any(|ch| ch.is_ascii_digit())
        && token.chars().any(|ch| ch.is_ascii_alphabetic())
}

pub(crate) fn budget_projection_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = budget_projection_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn budget_projection_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header_len = budget_projection_table_header_len(lines)?;
    let header = budget_projection_table_header_cells(&lines[..header_len])?;
    let value_count = header.len().checked_sub(3)?;

    let mut rows = vec![header.clone()];
    let mut consumed = header_len;
    let mut data_row_count = 0;

    for line in lines.iter().skip(header_len) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            consumed += 1;
            continue;
        }
        if looks_like_budget_projection_footer(trimmed) {
            break;
        }

        if let Some(row) = budget_projection_data_row(trimmed, value_count) {
            rows.push(row);
            data_row_count += 1;
            consumed += 1;
            continue;
        }

        if looks_like_budget_projection_section_line(trimmed) {
            rows.push(budget_projection_section_row(trimmed, header.len()));
            consumed += 1;
            continue;
        }

        if data_row_count >= 2 {
            break;
        }
        return None;
    }

    (data_row_count >= 2).then_some((rows, consumed))
}

pub(crate) fn budget_projection_table_header_len(lines: &[&str]) -> Option<usize> {
    if lines.len() < 3 {
        return None;
    }

    budget_projection_table_header_cells(&lines[..3]).map(|_| 3)
}

pub(crate) fn budget_projection_table_header_cells(lines: &[&str]) -> Option<Vec<String>> {
    let [account_line, years_line, estimate_line] = lines else {
        return None;
    };

    if normalize_pin_table_header(account_line) != "account and subfunction code" {
        return None;
    }

    let year_tokens = years_line.split_whitespace().collect::<Vec<_>>();
    let [actual_label, years @ ..] = year_tokens.as_slice() else {
        return None;
    };
    if !actual_label.eq_ignore_ascii_case("actual")
        || years.len() < 2
        || !years.iter().all(|year| is_budget_projection_year(year))
    {
        return None;
    }

    let estimate_tokens = estimate_line.split_whitespace().collect::<Vec<_>>();
    let [actual_year, estimate_label] = estimate_tokens.as_slice() else {
        return None;
    };
    if !is_budget_projection_year(actual_year) || !estimate_label.eq_ignore_ascii_case("estimate") {
        return None;
    }

    let mut cells = vec![
        "Account and Subfunction".to_string(),
        "Code".to_string(),
        "Type".to_string(),
        format!("Actual {actual_year}"),
        format!("{} Estimate", years[0]),
    ];
    cells.extend(years.iter().skip(1).map(|year| (*year).to_string()));
    Some(cells)
}

pub(crate) fn budget_projection_data_row(line: &str, value_count: usize) -> Option<Vec<String>> {
    if value_count < 2 {
        return None;
    }

    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < value_count + 2 {
        return None;
    }

    let values_start = tokens.len().checked_sub(value_count)?;
    let values = &tokens[values_start..];
    if !values.iter().all(|value| is_budget_projection_value(value)) {
        return None;
    }

    let type_index = values_start.checked_sub(1)?;
    let budget_type = tokens[type_index];
    if !is_budget_projection_type(budget_type) {
        return None;
    }

    let mut descriptor_end = type_index;
    let code = if type_index > 0 && is_budget_projection_code(tokens[type_index - 1]) {
        descriptor_end -= 1;
        tokens[type_index - 1].to_string()
    } else {
        String::new()
    };

    let descriptor = tokens[..descriptor_end].join(" ");
    if !looks_like_budget_projection_descriptor(&descriptor) {
        return None;
    }

    let mut row = Vec::with_capacity(value_count + 3);
    row.push(descriptor);
    row.push(code);
    row.push(budget_type.to_string());
    row.extend(values.iter().map(|value| (*value).to_string()));
    Some(row)
}

pub(crate) fn budget_projection_section_row(line: &str, column_count: usize) -> Vec<String> {
    let mut row = vec![String::new(); column_count];
    if let Some(first) = row.first_mut() {
        *first = line.to_string();
    }
    row
}

pub(crate) fn is_budget_projection_year(token: &str) -> bool {
    token.len() == 4 && token.chars().all(|ch| ch.is_ascii_digit())
}

pub(crate) fn is_budget_projection_value(token: &str) -> bool {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return false;
    }
    if matches!(trimmed, "---" | "-") {
        return true;
    }

    let numeric = trimmed.strip_prefix('-').unwrap_or(trimmed);
    numeric
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '.'))
        && numeric.chars().any(|ch| ch.is_ascii_digit())
}

pub(crate) fn is_budget_projection_type(token: &str) -> bool {
    matches!(token, "BA" | "O" | "BA/O")
}

pub(crate) fn is_budget_projection_code(token: &str) -> bool {
    token.len() == 3 && token.chars().all(|ch| ch.is_ascii_digit())
}

pub(crate) fn looks_like_budget_projection_descriptor(descriptor: &str) -> bool {
    let trimmed = descriptor.trim();
    !trimmed.is_empty()
        && trimmed.chars().any(char::is_alphabetic)
        && trimmed.chars().count() <= 160
}

pub(crate) fn looks_like_budget_projection_section_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.chars().any(char::is_alphabetic)
        && trimmed.chars().count() <= 180
        && !trimmed.contains('|')
        && !trimmed.contains('\t')
}

pub(crate) fn looks_like_budget_projection_footer(line: &str) -> bool {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    matches!(tokens.as_slice(), ["Page", page, "/", total]
        if page.chars().all(|ch| ch.is_ascii_digit())
            && total.chars().all(|ch| ch.is_ascii_digit()))
}

pub(crate) fn header_guided_whitespace_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    if lines.len() < 2
        || lines
            .iter()
            .any(|line| line.contains('|') || line.contains('\t') || has_wide_space_gap(line))
    {
        return None;
    }

    let header = lines.first()?.split_whitespace().collect::<Vec<_>>();
    let column_count = header.len();
    if !(2..=8).contains(&column_count)
        || header
            .iter()
            .any(|cell| cell.chars().count() > 24 || !looks_like_table_header_cell(cell))
        || !header.iter().any(|cell| is_table_header_cue(cell))
    {
        return None;
    }

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(lines.len());
    rows.push(header.iter().map(|cell| (*cell).to_string()).collect());
    let mut rows_with_table_value_cells = 0;
    let mut data_row_count = 0;
    let mut merged_descriptor_rows = 0;
    let mut pending_descriptor_tokens: Vec<&str> = Vec::new();

    for line in lines.iter().skip(1) {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < column_count {
            if !pending_descriptor_tokens.is_empty()
                || !looks_like_wrapped_descriptor_fragment(&tokens)
            {
                return None;
            }
            if data_row_count > 0
                && tokens
                    .first()
                    .is_some_and(|token| starts_with_lowercase(token))
            {
                let continuation = tokens.join(" ");
                if let Some(previous_row) = rows.last_mut() {
                    if !previous_row[0].is_empty() {
                        previous_row[0].push(' ');
                    }
                    previous_row[0].push_str(&continuation);
                    merged_descriptor_rows += 1;
                    continue;
                }
            }
            pending_descriptor_tokens.extend(tokens);
            continue;
        }
        if tokens.len() > column_count + 3 {
            return None;
        }

        let merge_pending_descriptor = pending_descriptor_tokens.first().is_some_and(|_| {
            tokens
                .first()
                .is_some_and(|token| starts_with_lowercase(token))
        });
        if !pending_descriptor_tokens.is_empty() && !merge_pending_descriptor {
            let mut section_row = vec![String::new(); column_count];
            section_row[0] = pending_descriptor_tokens.join(" ");
            rows.push(section_row);
            pending_descriptor_tokens.clear();
        }

        let mut overflow = tokens.len() - column_count;
        let trailing_blank_cells = if overflow == 0
            && header_guided_row_has_trailing_blank_descriptor(&tokens, column_count)
        {
            overflow = 1;
            1
        } else {
            0
        };
        if overflow > 0 || trailing_blank_cells > 0 || merge_pending_descriptor {
            merged_descriptor_rows += 1;
        }

        let mut row = Vec::with_capacity(column_count);
        let mut descriptor = pending_descriptor_tokens
            .drain(..)
            .map(str::to_string)
            .collect::<Vec<_>>();
        descriptor.extend(tokens[..=overflow].iter().map(|token| (*token).to_string()));
        row.push(descriptor.join(" "));
        row.extend(
            tokens
                .iter()
                .skip(overflow + 1)
                .map(|token| (*token).to_string()),
        );
        for _ in 0..trailing_blank_cells {
            row.push(String::new());
        }

        if row.len() != column_count {
            return None;
        }
        if row
            .iter()
            .skip(1)
            .any(|cell| looks_like_table_value_cell(cell))
        {
            rows_with_table_value_cells += 1;
        }
        data_row_count += 1;
        rows.push(row);
    }

    if !pending_descriptor_tokens.is_empty() {
        return None;
    }

    (data_row_count >= 2
        && merged_descriptor_rows > 0
        && rows_with_table_value_cells == data_row_count)
        .then_some(rows)
}

pub(crate) fn key_value_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    if lines.len() < 3 {
        return None;
    }

    let mut rows = Vec::with_capacity(lines.len() + 1);
    rows.push(vec!["Field".to_string(), "Value".to_string()]);

    for line in lines {
        let (label, value) = key_value_table_row(line)?;
        rows.push(vec![label, value]);
    }

    Some(rows)
}

pub(crate) fn key_value_table_row(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return None;
    }

    let (label, value) = trimmed.split_once(':')?;
    let label = label.trim();
    let value = value.trim();
    if !looks_like_key_value_table_label(label) || !looks_like_key_value_table_value(value) {
        return None;
    }

    Some((label.to_string(), value.to_string()))
}

pub(crate) fn looks_like_key_value_table_label(label: &str) -> bool {
    !label.is_empty()
        && label.chars().count() <= 80
        && label.split_whitespace().count() <= 8
        && label.chars().any(char::is_alphabetic)
        && label.chars().all(|ch| {
            ch.is_alphanumeric() || ch.is_whitespace() || matches!(ch, '-' | '/' | '(' | ')' | '%')
        })
}

pub(crate) fn looks_like_key_value_table_value(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 160
        && value
            .chars()
            .any(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '+' | '.'))
}

pub(crate) fn header_guided_row_has_trailing_blank_descriptor(
    tokens: &[&str],
    column_count: usize,
) -> bool {
    tokens.len() == column_count
        && column_count >= 3
        && looks_like_wrapped_descriptor_fragment(&tokens[..2])
        && tokens
            .iter()
            .skip(2)
            .all(|token| looks_like_table_value_cell(token))
}

pub(crate) fn looks_like_wrapped_descriptor_fragment(tokens: &[&str]) -> bool {
    !tokens.is_empty()
        && tokens.len() <= 3
        && tokens.iter().all(|token| {
            let trimmed = token.trim();
            !trimmed.is_empty()
                && trimmed.chars().count() <= 24
                && trimmed
                    .chars()
                    .all(|ch| ch.is_alphabetic() || matches!(ch, '-' | '/'))
                && !trimmed.chars().all(|ch| ch.is_uppercase())
        })
}

pub(crate) fn looks_like_table_header_cell(cell: &str) -> bool {
    let mut saw_alphanumeric = false;
    for ch in cell.chars() {
        if ch.is_alphanumeric() {
            saw_alphanumeric = true;
        } else if !matches!(ch, '_' | '-' | '/' | '%' | '(' | ')') {
            return false;
        }
    }

    saw_alphanumeric
}

pub(crate) fn is_table_header_cue(cell: &str) -> bool {
    let normalized = cell
        .trim_matches(|ch: char| !ch.is_alphanumeric())
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "parameter"
            | "symbol"
            | "min"
            | "typ"
            | "typical"
            | "max"
            | "unit"
            | "condition"
            | "conditions"
            | "value"
            | "rating"
            | "ratings"
            | "part"
            | "number"
            | "name"
            | "no"
            | "pin"
            | "function"
            | "package"
            | "code"
    )
}

pub(crate) fn looks_like_table_value_cell(cell: &str) -> bool {
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed
        .chars()
        .any(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ',' | '%' | '$' | '/' | '+' | '-'))
    {
        return true;
    }

    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    let char_count = 1 + chars.count();
    let uppercase_count = trimmed.chars().filter(|ch| ch.is_uppercase()).count();
    (first.is_uppercase() && char_count <= 24) || uppercase_count >= 2
}

#[derive(Debug)]
pub(crate) struct AlignedTableSegment {
    start: usize,
    text: String,
}

pub(crate) fn aligned_whitespace_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    if lines.len() < 2
        || lines
            .iter()
            .any(|line| line.contains('|') || line.contains('\t'))
        || !lines.iter().any(|line| has_wide_space_gap(line))
    {
        return None;
    }

    let header_segments = wide_space_segments(lines.first()?);
    let column_count = header_segments.len();
    if !(2..=8).contains(&column_count) {
        return None;
    }

    let column_starts = header_segments
        .iter()
        .map(|segment| segment.start)
        .collect::<Vec<_>>();
    if !column_starts.windows(2).all(|window| window[0] < window[1]) {
        return None;
    }

    let mut rows = Vec::with_capacity(lines.len());
    let mut regular_row_count = 0;
    let mut pending_section_row: Option<Vec<String>> = None;
    for line in lines {
        let segments = wide_space_segments(line);
        if segments.is_empty() {
            return None;
        }

        let mut cells = vec![String::new(); column_count];
        for segment in &segments {
            let column_index = nearest_column_index(segment.start, &column_starts);
            if !cells[column_index].is_empty() {
                cells[column_index].push(' ');
            }
            cells[column_index].push_str(&segment.text);
        }

        if cells.iter().filter(|cell| !cell.is_empty()).count() < 2 {
            if !aligned_whitespace_row_is_section(&segments, &column_starts) {
                return None;
            }
            if let Some(section_row) = pending_section_row.replace(cells) {
                rows.push(section_row);
            }
            continue;
        } else {
            if let Some(section_row) = pending_section_row.take() {
                if aligned_whitespace_should_merge_descriptor(&section_row[0], &cells[0]) {
                    cells[0] = format!("{} {}", section_row[0], cells[0]);
                } else {
                    rows.push(section_row);
                }
            }
            regular_row_count += 1;
        }
        rows.push(cells);
    }

    if let Some(section_row) = pending_section_row {
        rows.push(section_row);
    }

    if regular_row_count >= 2
        && rows
            .first()
            .is_some_and(|header| header.iter().all(|cell| !cell.is_empty()))
    {
        Some(rows)
    } else {
        None
    }
}

pub(crate) fn aligned_whitespace_row_is_section(
    segments: &[AlignedTableSegment],
    column_starts: &[usize],
) -> bool {
    let Some(segment) = segments.first() else {
        return false;
    };
    segments.len() == 1
        && column_starts
            .first()
            .is_some_and(|start| segment.start == *start)
        && segment.text.chars().count() <= 80
        && segment.text.chars().any(char::is_alphabetic)
}

pub(crate) fn aligned_whitespace_should_merge_descriptor(prefix: &str, first_cell: &str) -> bool {
    let prefix = prefix.trim();
    let first_cell = first_cell.trim();
    !prefix.is_empty() && !first_cell.is_empty() && starts_with_lowercase(first_cell)
}

pub(crate) fn starts_with_lowercase(text: &str) -> bool {
    text.chars().next().is_some_and(|ch| ch.is_lowercase())
}

pub(crate) fn wide_space_segments(line: &str) -> Vec<AlignedTableSegment> {
    let chars = line.chars().collect::<Vec<_>>();
    let mut segments = Vec::new();
    let mut start = None;
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == ' ' {
            let gap_start = index;
            while index < chars.len() && chars[index] == ' ' {
                index += 1;
            }
            if index - gap_start >= 2 {
                push_aligned_table_segment(&chars, start.take(), gap_start, &mut segments);
            }
            continue;
        }

        if start.is_none() {
            start = Some(index);
        }
        index += 1;
    }

    push_aligned_table_segment(&chars, start, chars.len(), &mut segments);
    segments
}

pub(crate) fn push_aligned_table_segment(
    chars: &[char],
    start: Option<usize>,
    end: usize,
    segments: &mut Vec<AlignedTableSegment>,
) {
    let Some(start) = start else {
        return;
    };
    let text = chars[start..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    if !text.is_empty() {
        segments.push(AlignedTableSegment { start, text });
    }
}

pub(crate) fn has_wide_space_gap(line: &str) -> bool {
    let mut run = 0;
    for ch in line.chars() {
        if ch == ' ' {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

pub(crate) fn nearest_column_index(start: usize, column_starts: &[usize]) -> usize {
    column_starts
        .iter()
        .enumerate()
        .min_by_key(|(index, column_start)| (start.abs_diff(**column_start), *index))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(crate) fn is_list_lines(lines: &[String]) -> bool {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    is_list_lines_str(&refs)
}

pub(crate) fn is_list_lines_str(lines: &[&str]) -> bool {
    normalized_list_items(lines).is_some()
}

pub(crate) fn normalized_list_text(lines: &[String]) -> Option<String> {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    normalized_list_items(&refs).map(|items| items.join("\n"))
}

pub(crate) fn normalized_list_items(lines: &[&str]) -> Option<Vec<String>> {
    if lines.is_empty() {
        return None;
    }

    let mut items = Vec::new();
    let mut current_item: Option<String> = None;

    for line in lines.iter().map(|line| line.trim()) {
        if line.is_empty() {
            return None;
        }

        if let Some(item) = list_item_start_text(line) {
            flush_list_text_item(&mut items, current_item.take())?;
            current_item = Some(item);
        } else if let Some(item) = current_item.as_mut() {
            append_text_fragment(item, line);
        } else {
            return None;
        }
    }

    flush_list_text_item(&mut items, current_item)?;

    (!items.is_empty()).then_some(items)
}

pub(crate) fn flush_list_text_item(items: &mut Vec<String>, item: Option<String>) -> Option<()> {
    let Some(item) = item else {
        return Some(());
    };
    list_item_has_body(&item).then(|| items.push(item))
}

pub(crate) fn list_item_start_text(line: &str) -> Option<String> {
    if let Some((marker, rest)) = line.split_once(char::is_whitespace)
        && is_standalone_list_marker(marker)
    {
        let rest = rest.trim();
        return Some(if rest.is_empty() {
            marker.to_string()
        } else {
            format!("{marker} {rest}")
        });
    }

    if is_standalone_list_marker(line) {
        return Some(line.to_string());
    }

    line.split_once(". ")
        .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
        .map(|_| line.to_string())
}

pub(crate) fn append_text_fragment(text: &mut String, fragment: &str) {
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(fragment);
}

pub(crate) fn is_standalone_list_marker(marker: &str) -> bool {
    matches!(marker, "-" | "*" | "+" | "·" | "•" | "–")
}

pub(crate) fn is_heading_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 80
        && trimmed.chars().any(char::is_alphabetic)
        && trimmed
            .chars()
            .filter(|ch| ch.is_alphabetic())
            .all(|ch| ch.is_uppercase())
}

pub(crate) fn is_title_case_heading_line(line: &str) -> bool {
    if line.ends_with(['.', ':', ';', ',']) {
        return false;
    }

    let words = line
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .collect::<Vec<_>>();

    (2..=8).contains(&words.len())
        && words.iter().all(|word| {
            word.chars()
                .find(|ch| ch.is_alphabetic())
                .is_some_and(|ch| ch.is_uppercase())
        })
}
