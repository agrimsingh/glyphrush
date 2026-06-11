use crate::*;

pub(crate) fn parameter_symbol_conditions_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = parameter_symbol_conditions_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn parameter_symbol_conditions_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header_len = parameter_symbol_conditions_table_header_len(lines)?;
    let mut rows = vec![vec![
        "Parameter".to_string(),
        "Symbol".to_string(),
        "Conditions".to_string(),
        "Min.".to_string(),
        "Typ.".to_string(),
        "Max.".to_string(),
        "Unit".to_string(),
    ]];
    let mut consumed = header_len;
    let mut pending_label: Option<ElectricalLabel> = None;
    let mut pending_parameter_parts: Vec<String> = Vec::new();
    let mut active_group_condition = String::new();
    let mut last_unit = String::new();

    for (offset, line) in lines.iter().enumerate().skip(header_len) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if looks_like_parameter_symbol_conditions_table_terminator(trimmed) {
            break;
        }

        if let Some(unit) = electrical_unit_only_line(trimmed) {
            if parameter_symbol_conditions_apply_unit_continuation(&mut rows, &mut last_unit, &unit)
            {
                consumed = offset + 1;
                continue;
            }
            break;
        }

        if let Some((prefix, mut values)) = parameter_symbol_conditions_values_from_line(trimmed) {
            let mut label = parameter_symbol_conditions_label_from_tokens(&prefix);
            if let Some(pending) = pending_label.take() {
                if label.parameter.is_empty() && label.symbol.is_empty() {
                    label.parameter = pending.parameter;
                    label.symbol = pending.symbol;
                    label.condition =
                        combine_electrical_conditions(&pending.condition, &label.condition);
                    active_group_condition = pending.condition;
                } else {
                    pending_label = Some(pending);
                }
            }

            label.parameter =
                combine_parameter_label_parts(&pending_parameter_parts, &label.parameter);
            if label.parameter.is_empty()
                && label.symbol.is_empty()
                && !active_group_condition.is_empty()
            {
                label.condition =
                    combine_electrical_conditions(&active_group_condition, &label.condition);
            }

            let starts_new_parameter = !label.parameter.is_empty() || !label.symbol.is_empty();
            if values.unit.is_empty() && !last_unit.is_empty() && !starts_new_parameter {
                values.unit = last_unit.clone();
            }
            if !values.unit.is_empty() {
                last_unit = values.unit.clone();
            }

            push_parameter_symbol_conditions_row(&mut rows, label, values);
            pending_parameter_parts.clear();
            consumed = offset + 1;
            continue;
        }

        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        let mut label = parameter_symbol_conditions_label_from_tokens(&tokens);
        if !label.symbol.is_empty() {
            label.parameter =
                combine_parameter_label_parts(&pending_parameter_parts, &label.parameter);
            pending_label = Some(label);
            pending_parameter_parts.clear();
            consumed = offset + 1;
            continue;
        }

        if !label.condition.is_empty() {
            if let Some(pending) = pending_label.as_mut() {
                pending.condition =
                    combine_electrical_conditions(&pending.condition, &label.condition);
                consumed = offset + 1;
                continue;
            }
            break;
        }

        if !label.parameter.is_empty() {
            pending_parameter_parts.push(label.parameter);
            consumed = offset + 1;
            continue;
        }

        break;
    }

    if pending_label.is_some() || !pending_parameter_parts.is_empty() {
        return None;
    }

    (rows.len() >= 4).then_some((rows, consumed))
}

pub(crate) fn parameter_symbol_conditions_table_header_len(lines: &[&str]) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    (normalize_electrical_header_line(lines[0]) == "parameter symbol conditions min typ max unit")
        .then_some(1)
}

pub(crate) fn parameter_symbol_conditions_values_from_line(
    line: &str,
) -> Option<(Vec<&str>, ElectricalValues)> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let (prefix, values) = parameter_symbol_conditions_values_from_tokens(&tokens)?;
    Some((prefix.to_vec(), values))
}

pub(crate) fn parameter_symbol_conditions_values_from_tokens<'tokens, 'text>(
    tokens: &'tokens [&'text str],
) -> Option<(&'tokens [&'text str], ElectricalValues)> {
    if tokens.len() < 2 {
        return None;
    }

    let mut value_end = tokens.len();
    let unit = tokens
        .last()
        .filter(|token| looks_like_electrical_unit_token(token))
        .copied()
        .unwrap_or_default();
    if !unit.is_empty() {
        value_end -= 1;
    }

    let mut value_start = value_end;
    while value_start > 0
        && value_end - value_start < 3
        && looks_like_parameter_test_condition_value_token(tokens[value_start - 1])
    {
        value_start -= 1;
    }

    if value_start == value_end {
        return None;
    }

    let values = &tokens[value_start..value_end];
    let prefix = &tokens[..value_start];
    let label = parameter_symbol_conditions_label_from_tokens(prefix);
    let value_cells = parameter_symbol_conditions_value_cells(&label.parameter, values)?;
    Some((
        prefix,
        ElectricalValues {
            condition: String::new(),
            min: value_cells.0,
            typ: value_cells.1,
            max: value_cells.2,
            unit: unit.to_string(),
        },
    ))
}

pub(crate) fn parameter_symbol_conditions_value_cells(
    parameter: &str,
    values: &[&str],
) -> Option<(String, String, String)> {
    let parameter = parameter.to_ascii_lowercase();
    match values {
        [only] => Some((String::new(), (*only).to_string(), String::new())),
        [left, right] if parameter.contains("range") || parameter.contains("accuracy") => {
            Some(((*left).to_string(), String::new(), (*right).to_string()))
        }
        [left, right] => Some((String::new(), (*left).to_string(), (*right).to_string())),
        [left, middle, right] => Some((
            (*left).to_string(),
            (*middle).to_string(),
            (*right).to_string(),
        )),
        _ => None,
    }
}

pub(crate) fn parameter_symbol_conditions_label_from_tokens(tokens: &[&str]) -> ElectricalLabel {
    let symbol_index = tokens
        .iter()
        .position(|token| looks_like_parameter_symbol_conditions_symbol(token));

    let Some(symbol_index) = symbol_index else {
        let (parameter_tokens, condition_tokens) =
            split_parameter_symbol_conditions_descriptor_condition(tokens);
        return ElectricalLabel {
            symbol: String::new(),
            parameter: parameter_tokens.join(" "),
            condition: condition_tokens.join(" "),
        };
    };

    ElectricalLabel {
        parameter: tokens[..symbol_index].join(" "),
        symbol: tokens[symbol_index].to_string(),
        condition: tokens[symbol_index + 1..].join(" "),
    }
}

pub(crate) fn split_parameter_symbol_conditions_descriptor_condition<'a>(
    tokens: &'a [&'a str],
) -> (&'a [&'a str], &'a [&'a str]) {
    let condition_start = tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| {
            parameter_symbol_conditions_condition_starts_at(tokens, index, token).then_some(index)
        })
        .unwrap_or(tokens.len());

    (&tokens[..condition_start], &tokens[condition_start..])
}

pub(crate) fn parameter_symbol_conditions_condition_starts_at(
    tokens: &[&str],
    index: usize,
    token: &str,
) -> bool {
    let normalized = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    let next = tokens.get(index + 1).copied().unwrap_or_default();

    normalized.contains('=')
        || normalized.contains('Ω')
        || normalized.starts_with("IOUT")
        || normalized.starts_with("COUT")
        || normalized.starts_with("BW")
        || normalized.starts_with("RLoad")
        || normalized.starts_with("VEN")
        || normalized.starts_with("VIN")
        || normalized.starts_with("fRIPPLE")
        || (index == 0 && matches!(normalized, "Start-up" | "Shutdown"))
        || (matches!(normalized, "VOUT" | "VIN" | "f") && next == "=")
}

pub(crate) fn looks_like_parameter_symbol_conditions_symbol(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    matches!(
        token,
        "VIN"
            | "IQ"
            | "ISTBY"
            | "VDROP"
            | "PSRR"
            | "VNOISE"
            | "ILIMIT"
            | "ICFB"
            | "RDIS"
            | "IEN"
            | "TSD"
            | "VEN(ON)"
            | "VEN(OFF)"
            | "VOUT"
            | "ΔVOUT"
            | "VLINE"
            | "ΔVLINE"
            | "VLOAD"
            | "ΔVLOAD"
            | "TSD"
            | "ΔTSD"
    )
}

pub(crate) fn push_parameter_symbol_conditions_row(
    rows: &mut Vec<Vec<String>>,
    label: ElectricalLabel,
    values: ElectricalValues,
) {
    rows.push(vec![
        label.parameter,
        label.symbol,
        label.condition,
        values.min,
        values.typ,
        values.max,
        values.unit,
    ]);
}

pub(crate) fn parameter_symbol_conditions_apply_unit_continuation(
    rows: &mut [Vec<String>],
    last_unit: &mut String,
    unit: &str,
) -> bool {
    let Some(unit_cell) = rows.last_mut().and_then(|row| row.get_mut(6)) else {
        return false;
    };
    if !unit_cell.is_empty() {
        return false;
    }

    *unit_cell = unit.to_string();
    *last_unit = unit.to_string();
    true
}

pub(crate) fn looks_like_parameter_symbol_conditions_table_terminator(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    looks_like_electrical_characteristics_table_terminator(line)
        || normalized.starts_with("note ")
        || normalized.contains("typical performance curves")
}

pub(crate) fn parameter_test_condition_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = parameter_test_condition_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn parameter_test_condition_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header_len = parameter_test_condition_table_header_len(lines)?;
    let mut rows = vec![vec![
        "Parameter".to_string(),
        "Test Condition".to_string(),
        "Min.".to_string(),
        "Typ.".to_string(),
        "Max.".to_string(),
        "Unit".to_string(),
    ]];
    let mut consumed = header_len;
    let mut pending_label_parts: Vec<String> = Vec::new();
    let mut pending_condition = String::new();
    let mut pending_unit_rows: Vec<usize> = Vec::new();
    let mut last_unit = String::new();
    let mut active_group_condition = String::new();

    for (offset, line) in lines.iter().enumerate().skip(header_len) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if looks_like_parameter_test_condition_table_terminator(trimmed) {
            break;
        }

        if let Some(unit) = electrical_unit_only_line(trimmed) {
            if pending_unit_rows.is_empty() {
                if append_parameter_test_condition_unit_continuation(
                    &mut rows,
                    &mut last_unit,
                    &unit,
                ) {
                    consumed = offset + 1;
                    continue;
                }
                break;
            }
            apply_parameter_test_condition_unit(&mut rows, &mut pending_unit_rows, &unit);
            last_unit = unit;
            consumed = offset + 1;
            continue;
        }

        if !pending_condition.is_empty()
            && looks_like_parameter_test_condition_continuation(&pending_condition, trimmed)
        {
            pending_condition = combine_electrical_conditions(&pending_condition, trimmed);
            consumed = offset + 1;
            continue;
        }

        if let Some((prefix, mut values)) = parameter_test_condition_values_from_line(trimmed) {
            let (line_label, line_condition) = split_parameter_test_condition_prefix(&prefix);
            let parameter = combine_parameter_label_parts(&pending_label_parts, &line_label);
            let mut condition = combine_electrical_conditions(&pending_condition, &line_condition);
            if parameter.is_empty() && !active_group_condition.is_empty() {
                condition = combine_electrical_conditions(&active_group_condition, &condition);
            }
            if parameter.is_empty() && condition.is_empty() {
                break;
            }

            if values.unit.is_empty()
                && !last_unit.is_empty()
                && parameter.is_empty()
                && !condition.is_empty()
            {
                values.unit = last_unit.clone();
            }

            let group_condition = pending_condition.clone();
            let starts_condition_group = !parameter.is_empty() && !group_condition.is_empty();
            let starts_new_parameter = !parameter.is_empty();

            push_parameter_test_condition_row(
                &mut rows,
                parameter,
                condition,
                values,
                &mut pending_unit_rows,
                &mut last_unit,
            );
            if starts_condition_group {
                active_group_condition = group_condition;
            } else if starts_new_parameter {
                active_group_condition.clear();
            }
            pending_label_parts.clear();
            pending_condition.clear();
            consumed = offset + 1;
            continue;
        }

        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        let (line_label, line_condition) = split_parameter_test_condition_prefix(&tokens);
        if line_label.is_empty() && line_condition.is_empty() {
            break;
        }
        if !line_label.is_empty() {
            pending_label_parts.push(line_label);
        }
        pending_condition = combine_electrical_conditions(&pending_condition, &line_condition);
        consumed = offset + 1;
    }

    if !pending_label_parts.is_empty()
        || !pending_condition.is_empty()
        || !pending_unit_rows.is_empty()
    {
        return None;
    }

    (rows.len() >= 4).then_some((rows, consumed))
}

pub(crate) fn parameter_test_condition_table_header_len(lines: &[&str]) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    (normalize_electrical_header_line(lines[0]) == "parameter test condition min typ max unit")
        .then_some(1)
}

pub(crate) fn parameter_test_condition_values_from_line(
    line: &str,
) -> Option<(Vec<&str>, ElectricalValues)> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let (prefix, values) = parameter_test_condition_values_from_tokens(&tokens)?;
    Some((prefix.to_vec(), values))
}

pub(crate) fn parameter_test_condition_values_from_tokens<'tokens, 'text>(
    tokens: &'tokens [&'text str],
) -> Option<(&'tokens [&'text str], ElectricalValues)> {
    if tokens.len() < 2 {
        return None;
    }

    let mut value_end = tokens.len();
    let unit = tokens
        .last()
        .filter(|token| looks_like_electrical_unit_token(token))
        .copied()
        .unwrap_or_default();
    if !unit.is_empty() {
        value_end -= 1;
    }

    let mut value_start = value_end;
    while value_start > 0
        && value_end - value_start < 3
        && looks_like_parameter_test_condition_value_token(tokens[value_start - 1])
    {
        value_start -= 1;
    }

    if value_start == value_end {
        return None;
    }

    let values = &tokens[value_start..value_end];
    let prefix = &tokens[..value_start];
    let (parameter_tokens, _) = split_parameter_test_condition_prefix(prefix);
    let value_cells = parameter_test_condition_value_cells(&parameter_tokens, values)?;
    Some((
        prefix,
        ElectricalValues {
            condition: String::new(),
            min: value_cells.0,
            typ: value_cells.1,
            max: value_cells.2,
            unit: unit.to_string(),
        },
    ))
}

pub(crate) fn looks_like_parameter_test_condition_value_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    !trimmed.is_empty()
        && (matches!(trimmed, "-" | "±")
            || trimmed
                .strip_prefix('±')
                .is_some_and(|rest| rest.chars().all(|ch| ch.is_ascii_digit() || ch == '.'))
            || trimmed
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-' | ',')))
}

pub(crate) fn parameter_test_condition_value_cells(
    parameter: &str,
    values: &[&str],
) -> Option<(String, String, String)> {
    let parameter = parameter.to_ascii_lowercase();
    match values {
        [only] => Some((String::new(), (*only).to_string(), String::new())),
        [left, right]
            if parameter.is_empty()
                || parameter.contains("range")
                || parameter.contains("accuracy") =>
        {
            Some(((*left).to_string(), String::new(), (*right).to_string()))
        }
        [left, right] => Some((String::new(), (*left).to_string(), (*right).to_string())),
        [left, middle, right] => Some((
            (*left).to_string(),
            (*middle).to_string(),
            (*right).to_string(),
        )),
        _ => None,
    }
}

pub(crate) fn split_parameter_test_condition_prefix(tokens: &[&str]) -> (String, String) {
    let condition_start = tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| {
            parameter_test_condition_starts_at(tokens, index, token).then_some(index)
        })
        .unwrap_or(tokens.len());

    (
        tokens[..condition_start].join(" "),
        tokens[condition_start..].join(" "),
    )
}

pub(crate) fn parameter_test_condition_starts_at(
    tokens: &[&str],
    index: usize,
    token: &str,
) -> bool {
    let normalized = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    let normalized_lower = normalized.to_ascii_lowercase();
    let next = tokens
        .get(index + 1)
        .map(|token| token.to_ascii_lowercase())
        .unwrap_or_default();

    normalized.contains('=')
        || normalized.contains('≤')
        || normalized.contains('≥')
        || normalized.contains('<')
        || normalized.contains('>')
        || (normalized_lower.contains("°c") && next.contains("ta"))
        || (normalized_lower.contains("℃") && next.contains("ta"))
        || (normalized_lower == "temperature" && matches!(next.as_str(), "rising" | "falling"))
}

pub(crate) fn looks_like_parameter_test_condition_continuation(previous: &str, line: &str) -> bool {
    let previous = previous.trim().to_ascii_lowercase();
    if !previous.ends_with(" to") && !previous.ends_with(" to ") {
        return false;
    }

    let token = line.trim();
    !token.is_empty()
        && token.split_whitespace().count() == 1
        && token.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '-' | 'µ' | '\u{f06d}')
        })
}

pub(crate) fn combine_parameter_label_parts(parts: &[String], suffix: &str) -> String {
    let mut labels = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if !suffix.trim().is_empty() {
        labels.push(suffix.trim());
    }
    labels.join(" ")
}

pub(crate) fn push_parameter_test_condition_row(
    rows: &mut Vec<Vec<String>>,
    parameter: String,
    condition: String,
    values: ElectricalValues,
    pending_unit_rows: &mut Vec<usize>,
    last_unit: &mut String,
) {
    let row_index = rows.len();
    let unit_is_pending = values.unit.is_empty();
    rows.push(vec![
        parameter,
        condition,
        values.min,
        values.typ,
        values.max,
        values.unit,
    ]);
    if unit_is_pending {
        pending_unit_rows.push(row_index);
    } else {
        *last_unit = rows[row_index][5].clone();
    }
}

pub(crate) fn apply_parameter_test_condition_unit(
    rows: &mut [Vec<String>],
    pending_unit_rows: &mut Vec<usize>,
    unit: &str,
) {
    for row_index in pending_unit_rows.drain(..) {
        if let Some(unit_cell) = rows.get_mut(row_index).and_then(|row| row.get_mut(5)) {
            *unit_cell = unit.to_string();
        }
    }
}

pub(crate) fn append_parameter_test_condition_unit_continuation(
    rows: &mut [Vec<String>],
    last_unit: &mut String,
    unit: &str,
) -> bool {
    if unit != "C" || last_unit != "ppm/°" {
        return false;
    }

    let Some(unit_cell) = rows.last_mut().and_then(|row| row.get_mut(5)) else {
        return false;
    };
    if unit_cell != "ppm/°" {
        return false;
    }

    unit_cell.push('C');
    last_unit.push('C');
    true
}

pub(crate) fn looks_like_parameter_test_condition_table_terminator(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    looks_like_electrical_characteristics_table_terminator(line)
        || normalized.contains("typical characteristics")
        || normalized.contains("recommended components list")
        || normalized.contains("functional block diagram")
        || normalized.contains("awinic confidential")
}

pub(crate) fn reflow_profile_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = reflow_profile_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn reflow_profile_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header = reflow_profile_table_header_cells(lines.first()?)?;
    let mut rows = vec![header];
    let mut consumed = 1;
    let mut index = 1;

    if lines
        .get(index)
        .is_some_and(|line| line.trim().eq_ignore_ascii_case("Preheat & Soak"))
    {
        rows.push(vec![
            "Preheat & Soak".to_string(),
            String::new(),
            String::new(),
        ]);
        index += 1;

        let labels = lines
            .get(index..index + 3)?
            .iter()
            .map(|line| line.trim().to_string())
            .collect::<Vec<_>>();
        if !labels
            .iter()
            .all(|label| looks_like_reflow_profile_feature_label(label))
        {
            return None;
        }
        index += labels.len();

        let values = lines
            .get(index..index + labels.len() * 2)?
            .iter()
            .map(|line| reflow_profile_single_value_line(line.trim()))
            .collect::<Option<Vec<_>>>()?;
        index += values.len();

        for (offset, label) in labels.into_iter().enumerate() {
            rows.push(vec![
                label,
                values[offset].clone(),
                values[offset + values.len() / 2].clone(),
            ]);
        }
        consumed = index;
    }

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() || looks_like_reflow_profile_table_terminator(trimmed) {
            break;
        }

        if let Some(row) = reflow_profile_inline_row(trimmed) {
            rows.push(row);
            index += 1;
            consumed = index;
            continue;
        }

        let mut label_parts = Vec::new();
        while index < lines.len() {
            let current = lines[index].trim();
            if current.is_empty()
                || looks_like_reflow_profile_table_terminator(current)
                || reflow_profile_single_value_line(current).is_some()
                || reflow_profile_value_pair_from_text(current).is_some()
            {
                break;
            }
            if !looks_like_reflow_profile_feature_label(current) {
                break;
            }
            label_parts.push(current.to_string());
            index += 1;
        }

        if label_parts.is_empty() {
            break;
        }

        if let Some((label_suffix, left, right)) = lines
            .get(index)
            .and_then(|line| reflow_profile_value_pair_from_text(line.trim()))
        {
            let label = join_reflow_profile_label_parts(&label_parts, &label_suffix);
            rows.push(vec![label, left, right]);
            index += 1;
            consumed = index;
            continue;
        }

        let mut values = Vec::new();
        while index < lines.len() {
            let current = lines[index].trim();
            let Some(value) = reflow_profile_single_value_line(current) else {
                break;
            };
            values.push(value);
            index += 1;
        }

        if values.is_empty() || values.len() % 2 != 0 {
            break;
        }

        if label_parts.len() > 1 && values.len() == label_parts.len() * 2 {
            for (offset, label) in label_parts.into_iter().enumerate() {
                rows.push(vec![
                    label,
                    values[offset].clone(),
                    values[offset + values.len() / 2].clone(),
                ]);
            }
        } else if label_parts.len() == 1 && values.len() == 2 {
            rows.push(vec![
                label_parts.remove(0),
                values[0].clone(),
                values[1].clone(),
            ]);
        } else {
            break;
        }

        consumed = index;
    }

    (rows.len() >= 4).then_some((rows, consumed))
}

pub(crate) fn reflow_profile_table_header_cells(line: &str) -> Option<Vec<String>> {
    let normalized = line.to_ascii_lowercase();
    if normalized.contains("profile feature")
        && normalized.contains("sn-pb eutectic assembly")
        && normalized.contains("pb-free assembly")
    {
        return Some(vec![
            "Profile Feature".to_string(),
            "Sn-Pb Eutectic Assembly".to_string(),
            "Pb-Free Assembly".to_string(),
        ]);
    }

    None
}

pub(crate) fn reflow_profile_inline_row(line: &str) -> Option<Vec<String>> {
    let (label, left, right) = reflow_profile_value_pair_from_text(line)?;
    (!label.trim().is_empty()).then_some(vec![label, left, right])
}

pub(crate) fn reflow_profile_value_pair_from_text(line: &str) -> Option<(String, String, String)> {
    let pairs = [
        (
            "See Classification Temp in table 1 See Classification Temp in table 2",
            "See Classification Temp in table 1",
            "See Classification Temp in table 2",
        ),
        (
            "3 °C/second max. 3°C/second max.",
            "3 °C/second max.",
            "3°C/second max.",
        ),
        (
            "6 °C/second max. 6 °C/second max.",
            "6 °C/second max.",
            "6 °C/second max.",
        ),
        ("20** seconds 30** seconds", "20** seconds", "30** seconds"),
        (
            "6 minutes max. 8 minutes max.",
            "6 minutes max.",
            "8 minutes max.",
        ),
    ];

    for (suffix, left, right) in pairs {
        if line == suffix {
            return Some((String::new(), left.to_string(), right.to_string()));
        }
        if let Some(label) = line.strip_suffix(suffix) {
            let label = label.trim();
            if !label.is_empty() {
                return Some((label.to_string(), left.to_string(), right.to_string()));
            }
        }
    }

    None
}

pub(crate) fn reflow_profile_single_value_line(line: &str) -> Option<String> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() == 2 && tokens[1] == "°C" && tokens[0].chars().all(|ch| ch.is_ascii_digit()) {
        return Some(line.to_string());
    }

    if tokens.len() == 2
        && tokens[1].eq_ignore_ascii_case("seconds")
        && tokens[0].chars().all(|ch| ch.is_ascii_digit() || ch == '-')
    {
        return Some(line.to_string());
    }

    None
}

pub(crate) fn looks_like_reflow_profile_feature_label(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.chars().any(char::is_alphabetic)
        && !trimmed.starts_with('*')
        && !trimmed.to_ascii_lowercase().starts_with("table ")
        && !trimmed
            .split_whitespace()
            .all(looks_like_electrical_value_token)
}

pub(crate) fn join_reflow_profile_label_parts(parts: &[String], suffix: &str) -> String {
    let mut label_parts = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if !suffix.trim().is_empty() {
        label_parts.push(suffix.trim());
    }
    label_parts.join(" ")
}

pub(crate) fn looks_like_reflow_profile_table_terminator(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.starts_with("* tolerance")
        || normalized.starts_with("** tolerance")
        || normalized.starts_with("table 1.")
        || normalized.starts_with("table 2.")
        || normalized.starts_with("reliability test program")
}

pub(crate) fn classification_temperature_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = classification_temperature_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn classification_temperature_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    if lines.len() < 5 {
        return None;
    }

    let first_header = lines.first()?.trim();
    let second_header = lines.get(1)?.trim();
    if !first_header.eq_ignore_ascii_case("package")
        || !second_header.eq_ignore_ascii_case("thickness")
    {
        return None;
    }

    let mut header = vec!["Package Thickness".to_string()];
    let mut index = 2;
    while index + 1 < lines.len() {
        let label = lines[index].trim();
        let value = lines[index + 1].trim();
        if !label.eq_ignore_ascii_case("volume mm3") || !looks_like_package_volume_limit(value) {
            break;
        }
        header.push(format!("Volume mm3 {value}"));
        index += 2;
    }

    let value_count = header.len().checked_sub(1)?;
    if value_count < 2 {
        return None;
    }

    let mut rows = vec![header];
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() || looks_like_classification_temperature_terminator(line) {
            break;
        }

        let Some(row) = classification_temperature_data_row(line, value_count) else {
            break;
        };
        rows.push(row);
        index += 1;
    }

    (rows.len() >= 3).then_some((rows, index))
}

pub(crate) fn looks_like_classification_temperature_caption(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.starts_with("table ")
        && normalized.contains("classification temperatures")
        && normalized.contains("(tc)")
}

pub(crate) fn looks_like_package_volume_limit(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.chars().any(|ch| ch.is_ascii_digit())
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '<' | '>' | '-' | '–' | '³' | ' '))
}

pub(crate) fn classification_temperature_data_row(
    line: &str,
    value_count: usize,
) -> Option<Vec<String>> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let value_token_count = value_count.checked_mul(2)?;
    if tokens.len() <= value_token_count {
        return None;
    }

    let label_tokens = tokens.len() - value_token_count;
    let label = tokens[..label_tokens].join(" ");
    if !label.contains("mm") {
        return None;
    }

    let mut row = vec![label];
    for pair in tokens[label_tokens..].chunks_exact(2) {
        let amount = pair[0];
        let unit = pair[1];
        if !amount.chars().all(|ch| ch.is_ascii_digit()) || unit != "°C" {
            return None;
        }
        row.push(format!("{amount} {unit}"));
    }

    (row.len() == value_count + 1).then_some(row)
}

pub(crate) fn looks_like_classification_temperature_terminator(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.starts_with("table ")
        || normalized.starts_with("reliability test program")
        || normalized.starts_with("test item ")
}

pub(crate) fn bullet_leader_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = bullet_leader_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn bullet_leader_table_rows_prefix(lines: &[&str]) -> Option<(Vec<Vec<String>>, usize)> {
    let mut rows = vec![vec!["Parameter".to_string(), "Limit".to_string()]];
    let mut consumed = 0;
    let mut pending_parameter: Vec<String> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || looks_like_bullet_leader_table_terminator(trimmed) {
            break;
        }

        if let Some((parameter, limit)) = bullet_leader_direct_row(trimmed) {
            rows.push(vec![parameter, limit]);
            pending_parameter.clear();
            consumed = index + 1;
            continue;
        }

        if let Some((parameter, limit)) = leader_continuation_row(trimmed) {
            if pending_parameter.is_empty() {
                break;
            }
            pending_parameter.push(parameter);
            rows.push(vec![pending_parameter.join(" "), limit]);
            pending_parameter.clear();
            consumed = index + 1;
            continue;
        }

        if let Some(parameter) = bullet_leader_pending_parameter(trimmed) {
            if !pending_parameter.is_empty() {
                break;
            }
            pending_parameter.push(parameter);
            consumed = index + 1;
            continue;
        }

        if !pending_parameter.is_empty() && looks_like_bullet_leader_note_fragment(trimmed) {
            pending_parameter.push(trimmed.to_string());
            consumed = index + 1;
            continue;
        }

        break;
    }

    (rows.len() >= 3 && pending_parameter.is_empty()).then_some((rows, consumed))
}

pub(crate) fn bullet_leader_direct_row(line: &str) -> Option<(String, String)> {
    let stripped = strip_bullet_marker(line)?;
    split_leader_row(stripped)
}

pub(crate) fn bullet_leader_pending_parameter(line: &str) -> Option<String> {
    let stripped = strip_bullet_marker(line)?;
    if split_leader_row(stripped).is_some() {
        return None;
    }

    let parameter = stripped.trim();
    (!parameter.is_empty()).then_some(parameter.to_string())
}

pub(crate) fn leader_continuation_row(line: &str) -> Option<(String, String)> {
    if strip_bullet_marker(line).is_some() {
        return None;
    }
    split_leader_row(line)
}

pub(crate) fn strip_bullet_marker(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '●' | '•' | '·' | '*') {
        return None;
    }
    Some(trimmed[marker.len_utf8()..].trim_start())
}

pub(crate) fn split_leader_row(line: &str) -> Option<(String, String)> {
    let (start, end) = leader_separator_range(line)?;
    let parameter = line[..start].trim();
    let limit = line[end..].trim();
    if parameter.is_empty() || limit.is_empty() {
        return None;
    }
    Some((parameter.to_string(), limit.to_string()))
}

pub(crate) fn leader_separator_range(line: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut run_start: Option<usize> = None;
    let mut previous_end = 0;

    for (index, ch) in line.char_indices() {
        if ch == '-' || ch == '‐' || ch == '‑' || ch == '–' || ch == '—' {
            run_start.get_or_insert(index);
        } else if let Some(start) = run_start.take()
            && index - start >= 5
        {
            best = Some((start, index));
        }
        previous_end = index + ch.len_utf8();
    }

    if let Some(start) = run_start
        && previous_end - start >= 5
    {
        best = Some((start, previous_end));
    }

    best
}

pub(crate) fn looks_like_bullet_leader_note_fragment(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.starts_with("(note ") || normalized.starts_with("note ")
}

pub(crate) fn looks_like_bullet_leader_table_terminator(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.starts_with("note ") || normalized.contains("operating conditions")
}
