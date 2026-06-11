use crate::*;

pub(crate) fn pin_function_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = pin_function_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn pin_number_name_function_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = pin_number_name_function_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn pin_number_name_function_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header_len = pin_number_name_function_table_header_len(lines)?;
    let mut rows = vec![vec![
        "Pin No.".to_string(),
        "Name".to_string(),
        "Function".to_string(),
    ]];
    let mut consumed = header_len;

    for (offset, line) in lines.iter().enumerate().skip(header_len) {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        let row = pin_number_name_function_data_row(&tokens)?;
        rows.push(row);
        consumed = offset + 1;
    }

    (rows.len() >= 3).then_some((rows, consumed))
}

pub(crate) fn pin_number_name_function_table_header_len(lines: &[&str]) -> Option<usize> {
    let first = normalize_pin_table_header(lines.first()?);
    if first == "pin no name function" {
        return Some(1);
    }

    if lines.len() >= 2
        && first == "pin no name"
        && normalize_pin_table_header(lines[1]) == "function"
    {
        return Some(2);
    }

    if lines.len() >= 3
        && first == "pin"
        && normalize_pin_table_header(lines[1]) == "no name"
        && normalize_pin_table_header(lines[2]) == "function"
    {
        return Some(3);
    }

    None
}

pub(crate) fn pin_number_name_function_data_row(tokens: &[&str]) -> Option<Vec<String>> {
    if tokens.len() < 3 || !looks_like_pin_number_token(tokens[0]) {
        return None;
    }

    let name = tokens[1].trim_matches(|ch: char| matches!(ch, ',' | ';'));
    let name_tokens = vec![name.to_string()];
    if !pin_name_tokens_look_plausible(&name_tokens) {
        return None;
    }

    let function = tokens[2..].join(" ");
    if function.trim().is_empty() {
        return None;
    }

    Some(vec![tokens[0].to_string(), name.to_string(), function])
}

pub(crate) fn fragmented_symbol_parameter_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = fragmented_symbol_parameter_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn fragmented_symbol_parameter_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header = symbol_parameter_table_header_cells(lines.first()?)?;
    let mut rows = vec![header];
    let mut consumed = 1;
    let mut index = 1;

    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            break;
        }
        let Some(symbol) = symbol_parameter_symbol_line(line) else {
            break;
        };
        index += 1;

        let mut fragments = Vec::new();
        while index < lines.len() {
            let next = lines[index].trim();
            if next.is_empty() || symbol_parameter_symbol_line(next).is_some() {
                break;
            }
            if !fragments.is_empty() && looks_like_symbol_parameter_table_terminator(next) {
                break;
            }
            fragments.push(next);
            index += 1;
        }

        let row = fragmented_symbol_parameter_data_row(&symbol, &fragments)?;
        rows.push(row);
        consumed = index;
    }

    (rows.len() >= 3).then_some((rows, consumed))
}

pub(crate) fn symbol_parameter_table_header_cells(line: &str) -> Option<Vec<String>> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4
        || !tokens[0].eq_ignore_ascii_case("symbol")
        || !tokens[1].eq_ignore_ascii_case("parameter")
        || !tokens.last()?.eq_ignore_ascii_case("unit")
    {
        return None;
    }

    let value_header = tokens[2..tokens.len() - 1].join(" ");
    let normalized = value_header.to_ascii_lowercase();
    if !matches!(
        normalized.as_str(),
        "rating" | "range" | "typ" | "typical" | "typical value"
    ) {
        return None;
    }

    let value_header = if normalized == "typ" {
        "Typ".to_string()
    } else {
        title_case_ascii_words(&value_header)
    };

    Some(vec![
        "Symbol".to_string(),
        "Parameter".to_string(),
        value_header,
        "Unit".to_string(),
    ])
}

pub(crate) fn title_case_ascii_words(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut output = first.to_ascii_uppercase().to_string();
            output.push_str(&chars.as_str().to_ascii_lowercase());
            output
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn symbol_parameter_symbol_line(line: &str) -> Option<String> {
    let mut tokens = line.split_whitespace();
    let token = tokens.next()?;
    if tokens.next().is_some() || !looks_like_symbol_parameter_symbol_token(token) {
        return None;
    }

    Some(
        token
            .trim_matches(|ch: char| matches!(ch, ',' | ';'))
            .to_string(),
    )
}

pub(crate) fn looks_like_symbol_parameter_symbol_token(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    if token.is_empty()
        || token.chars().count() > 12
        || looks_like_symbol_parameter_unit_token(token)
        || !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return false;
    }

    token.chars().any(|ch| ch.is_ascii_uppercase())
        && token.chars().any(|ch| ch.is_ascii_alphabetic())
}

pub(crate) fn fragmented_symbol_parameter_data_row(
    symbol: &str,
    fragments: &[&str],
) -> Option<Vec<String>> {
    let mut tokens = fragments
        .iter()
        .flat_map(|fragment| fragment.split_whitespace())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }

    if tokens
        .first()
        .is_some_and(|token| token.eq_ignore_ascii_case(symbol))
    {
        tokens.remove(0);
    }

    let unit = tokens
        .last()
        .filter(|token| looks_like_symbol_parameter_unit_token(token))
        .copied()
        .unwrap_or_default();
    if !unit.is_empty() {
        tokens.pop();
    }

    let mut rating = Vec::new();
    while tokens
        .last()
        .is_some_and(|token| looks_like_symbol_parameter_rating_token(token))
    {
        rating.push(tokens.pop().expect("last token exists"));
    }
    rating.reverse();

    if tokens.is_empty() {
        return None;
    }

    Some(vec![
        symbol.to_string(),
        tokens.join(" "),
        rating.join(" "),
        unit.to_string(),
    ])
}

pub(crate) fn looks_like_symbol_parameter_unit_token(token: &str) -> bool {
    let normalized = token
        .trim_matches(|ch: char| matches!(ch, ',' | ';'))
        .replace(['µ', 'μ', '\u{f06d}'], "u")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "v" | "a"
            | "ma"
            | "ua"
            | "w"
            | "mw"
            | "f"
            | "mf"
            | "uf"
            | "nf"
            | "pf"
            | "oc"
            | "c"
            | "oc/w"
            | "c/w"
            | "ohm"
            | "kohm"
            | "mohm"
            | "%"
    )
}

pub(crate) fn looks_like_symbol_parameter_rating_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    !trimmed.is_empty()
        && (matches!(trimmed, "~" | "-" | "to" | "To" | "TO" | "+/-" | "±")
            || trimmed
                .strip_prefix('±')
                .is_some_and(|rest| rest.chars().all(|ch| ch.is_ascii_digit() || ch == '.'))
            || trimmed
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-' | ',')))
}

pub(crate) fn looks_like_symbol_parameter_table_terminator(line: &str) -> bool {
    if symbol_parameter_table_header_cells(line).is_some()
        || symbol_parameter_symbol_line(line).is_some()
        || looks_like_symbol_parameter_unit_token(line)
    {
        return false;
    }

    if looks_like_symbol_parameter_table_caption(line) {
        return true;
    }

    let has_rating_token = line
        .split_whitespace()
        .any(looks_like_symbol_parameter_rating_token);
    if has_rating_token {
        return false;
    }

    is_heading_line(line)
}

pub(crate) fn looks_like_symbol_parameter_table_caption(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    normalized.contains("ratings")
        || normalized.contains("characteristics")
        || normalized.contains("operating conditions")
}

#[derive(Clone, Debug)]
pub(crate) struct ElectricalValues {
    pub(crate) condition: String,
    pub(crate) min: String,
    pub(crate) typ: String,
    pub(crate) max: String,
    pub(crate) unit: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ElectricalLabel {
    pub(crate) symbol: String,
    pub(crate) parameter: String,
    pub(crate) condition: String,
}

pub(crate) fn electrical_characteristics_table_rows(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    let (rows, consumed) = electrical_characteristics_table_rows_prefix(lines)?;
    (consumed == lines.len()).then_some(rows)
}

pub(crate) fn electrical_characteristics_table_rows_prefix(
    lines: &[&str],
) -> Option<(Vec<Vec<String>>, usize)> {
    let header_len = electrical_characteristics_table_header_len(lines)?;
    let mut rows = vec![vec![
        "Symbol".to_string(),
        "Parameter".to_string(),
        "Test Conditions".to_string(),
        "Min.".to_string(),
        "Typ.".to_string(),
        "Max.".to_string(),
        "Unit".to_string(),
    ]];
    let mut consumed = header_len;
    let mut pending_label: Option<ElectricalLabel> = None;
    let mut pending_values_before_label: Option<ElectricalValues> = None;
    let mut pending_unit_rows: Vec<usize> = Vec::new();

    for (offset, line) in lines.iter().enumerate().skip(header_len) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if looks_like_electrical_characteristics_table_terminator(trimmed) {
            break;
        }

        if let Some(unit) = electrical_unit_only_line(trimmed) {
            if pending_unit_rows.is_empty() {
                break;
            }
            for row_index in pending_unit_rows.drain(..) {
                if let Some(unit_cell) = rows.get_mut(row_index).and_then(|row| row.get_mut(6)) {
                    *unit_cell = unit.clone();
                }
            }
            consumed = offset + 1;
            continue;
        }

        if let Some(values) = electrical_values_only_line(trimmed) {
            let Some(label) = pending_label.take() else {
                break;
            };
            push_electrical_row(
                &mut rows,
                label.symbol,
                label.parameter,
                label.condition,
                values,
                &mut pending_unit_rows,
            );
            consumed = offset + 1;
            continue;
        }

        if let Some((label, values)) = electrical_data_line(trimmed) {
            if label.parameter.is_empty() {
                if pending_label.is_some() {
                    let label = pending_label.take().expect("pending label exists");
                    push_electrical_row(
                        &mut rows,
                        label.symbol,
                        label.parameter,
                        combine_electrical_conditions(&label.condition, &values.condition),
                        values,
                        &mut pending_unit_rows,
                    );
                } else if electrical_label_only_line(lines.get(offset + 1).copied().unwrap_or(""))
                    .is_some_and(|next_label| !next_label.symbol.is_empty())
                {
                    pending_values_before_label = Some(values);
                } else {
                    push_electrical_row(
                        &mut rows,
                        String::new(),
                        String::new(),
                        values.condition.clone(),
                        values,
                        &mut pending_unit_rows,
                    );
                }
            } else {
                pending_label = None;
                push_electrical_row(
                    &mut rows,
                    label.symbol,
                    label.parameter,
                    combine_electrical_conditions(&label.condition, &values.condition),
                    values,
                    &mut pending_unit_rows,
                );
            }
            consumed = offset + 1;
            continue;
        }

        if let Some(label) = electrical_label_only_line(trimmed) {
            if let Some(values) = pending_values_before_label.take() {
                push_electrical_row(
                    &mut rows,
                    label.symbol,
                    label.parameter,
                    combine_electrical_conditions(&label.condition, &values.condition),
                    values,
                    &mut pending_unit_rows,
                );
            } else {
                pending_label = Some(label);
            }
            consumed = offset + 1;
            continue;
        }

        break;
    }

    if pending_label.is_some()
        || pending_values_before_label.is_some()
        || !pending_unit_rows.is_empty()
    {
        return None;
    }

    (rows.len() >= 4).then_some((rows, consumed))
}

pub(crate) fn electrical_characteristics_table_header_len(lines: &[&str]) -> Option<usize> {
    if lines.len() < 3 {
        return None;
    }

    if normalize_electrical_header_line(lines[0]) == "symbol parameter test conditions"
        && normalize_electrical_header_line(lines[1]) == "min typ max"
        && normalize_electrical_header_line(lines[2]) == "unit"
    {
        return Some(3);
    }

    None
}

pub(crate) fn normalize_electrical_header_line(line: &str) -> String {
    line.split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn electrical_data_line(line: &str) -> Option<(ElectricalLabel, ElectricalValues)> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let (prefix, mut values) = electrical_values_from_tokens(&tokens)?;
    if prefix.is_empty() {
        return Some((
            ElectricalLabel {
                symbol: String::new(),
                parameter: String::new(),
                condition: String::new(),
            },
            values,
        ));
    }

    let label = electrical_label_from_tokens(prefix)?;
    if !label.parameter.is_empty() {
        values.condition.clear();
    }
    Some((label, values))
}

pub(crate) fn electrical_values_only_line(line: &str) -> Option<ElectricalValues> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() != 3
        || !tokens
            .iter()
            .all(|token| looks_like_electrical_value_token(token))
    {
        return None;
    }

    Some(ElectricalValues {
        condition: String::new(),
        min: tokens[0].to_string(),
        typ: tokens[1].to_string(),
        max: tokens[2].to_string(),
        unit: String::new(),
    })
}

pub(crate) fn electrical_values_from_tokens<'a>(
    tokens: &'a [&'a str],
) -> Option<(&'a [&'a str], ElectricalValues)> {
    if tokens.len() < 3 {
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
    if value_end < 3 {
        return None;
    }

    let value_start = value_end - 3;
    let values = &tokens[value_start..value_end];
    if !values
        .iter()
        .all(|token| looks_like_electrical_value_token(token))
    {
        return None;
    }

    let prefix = &tokens[..value_start];
    let (_, condition_tokens) = split_electrical_descriptor_condition(prefix);
    Some((
        prefix,
        ElectricalValues {
            condition: condition_tokens.join(" "),
            min: values[0].to_string(),
            typ: values[1].to_string(),
            max: values[2].to_string(),
            unit: unit.to_string(),
        },
    ))
}

pub(crate) fn electrical_label_only_line(line: &str) -> Option<ElectricalLabel> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| looks_like_electrical_value_token(token))
    {
        return None;
    }

    electrical_label_from_tokens(&tokens)
}

pub(crate) fn electrical_label_from_tokens(tokens: &[&str]) -> Option<ElectricalLabel> {
    let (symbol, descriptor_tokens) = split_electrical_symbol(tokens);
    let (parameter_tokens, condition_tokens) =
        split_electrical_descriptor_condition(descriptor_tokens);
    if parameter_tokens.is_empty() && condition_tokens.is_empty() {
        return None;
    }

    Some(ElectricalLabel {
        symbol,
        parameter: parameter_tokens.join(" "),
        condition: condition_tokens.join(" "),
    })
}

pub(crate) fn split_electrical_symbol<'a>(tokens: &'a [&'a str]) -> (String, &'a [&'a str]) {
    let Some(first) = tokens.first() else {
        return (String::new(), tokens);
    };
    let token = first.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    let next = tokens.get(1).copied().unwrap_or_default();

    let is_symbol = matches!(
        token,
        "VIN"
            | "VOUT"
            | "IQ"
            | "VREF"
            | "REGLINE"
            | "REGLOAD"
            | "VDROP"
            | "PSRR"
            | "ILIMIT"
            | "ISHORT"
    ) && !(token == "VOUT" && next != "Output");

    if is_symbol {
        (token.to_string(), &tokens[1..])
    } else {
        (String::new(), tokens)
    }
}

pub(crate) fn split_electrical_descriptor_condition<'a>(
    tokens: &'a [&'a str],
) -> (&'a [&'a str], &'a [&'a str]) {
    let condition_start = tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| {
            electrical_condition_starts_at(tokens, index, token).then_some(index)
        })
        .unwrap_or(tokens.len());

    (&tokens[..condition_start], &tokens[condition_start..])
}

pub(crate) fn electrical_condition_starts_at(tokens: &[&str], index: usize, token: &str) -> bool {
    let normalized = token.trim_matches(|ch: char| matches!(ch, ',' | ';'));
    let next = tokens.get(index + 1).copied().unwrap_or_default();

    normalized.contains('=')
        || matches!(normalized, "Measured")
        || normalized.starts_with("DVOUT")
        || normalized.starts_with("IOUT")
        || normalized.starts_with("VSET")
        || (matches!(normalized, "VOUT" | "VIN" | "SHDN" | "f") && next == "=")
}

pub(crate) fn push_electrical_row(
    rows: &mut Vec<Vec<String>>,
    symbol: String,
    parameter: String,
    condition: String,
    values: ElectricalValues,
    pending_unit_rows: &mut Vec<usize>,
) {
    let row_index = rows.len();
    let unit_is_pending = values.unit.is_empty();
    rows.push(vec![
        symbol,
        parameter,
        condition,
        values.min,
        values.typ,
        values.max,
        values.unit,
    ]);
    if unit_is_pending {
        pending_unit_rows.push(row_index);
    }
}

pub(crate) fn combine_electrical_conditions(prefix: &str, suffix: &str) -> String {
    match (prefix.trim().is_empty(), suffix.trim().is_empty()) {
        (true, true) => String::new(),
        (true, false) => suffix.trim().to_string(),
        (false, true) => prefix.trim().to_string(),
        (false, false) => format!("{} {}", prefix.trim(), suffix.trim()),
    }
}

pub(crate) fn electrical_unit_only_line(line: &str) -> Option<String> {
    let token = line.trim();
    (token.split_whitespace().count() == 1 && looks_like_electrical_unit_token(token))
        .then_some(token.to_string())
}

pub(crate) fn looks_like_electrical_unit_token(token: &str) -> bool {
    let normalized = token
        .trim_matches(|ch: char| matches!(ch, ',' | ';'))
        .replace(['µ', 'μ', '\u{f06d}'], "u")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "v" | "mv"
            | "ma"
            | "ua"
            | "a"
            | "w"
            | "mw"
            | "mvrms"
            | "uvrms"
            | "db"
            | "na"
            | "%"
            | "%/v"
            | "%/a"
            | "°c"
            | "ºc"
            | "℃"
            | "oc"
            | "c"
            | "ω"
            | "Ω"
            | "ohm"
            | "kohm"
            | "mohm"
            | "ppm/°"
            | "ppm/°c"
    )
}

pub(crate) fn looks_like_electrical_value_token(token: &str) -> bool {
    looks_like_symbol_parameter_rating_token(token)
}

pub(crate) fn looks_like_electrical_characteristics_table_terminator(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.contains("typical operating characteristics")
        || normalized.contains("application information")
        || normalized.contains("package information")
        || normalized.contains("recommended operating conditions")
        || normalized.contains("thermal characteristics")
}
