use crate::*;

use std::io::{self, Write};

use anyhow::Result;
use glyphrush_core::{DocumentArtifact, LayoutBlockKind, LayoutTable, PageArtifact};
use serde::Serialize;

pub(crate) fn parse_table_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter_map(|line| {
            let row = if line.contains('|') {
                split_delimited_table_cells(line, '|')
            } else if line.contains('\t') {
                split_delimited_table_cells(line, '\t')
            } else {
                line.split_whitespace()
                    .map(str::trim)
                    .filter(|cell| !cell.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            };
            (row.len() >= 2 && !is_markdown_table_separator_row(&row)).then_some(row)
        })
        .collect()
}

pub(crate) fn write_json(value: &impl Serialize) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    writeln!(handle)?;
    Ok(())
}

pub(crate) fn write_plain_text(artifact: &glyphrush_core::DocumentArtifact) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(plain_text_from_artifact(artifact).as_bytes())?;
    Ok(())
}

pub(crate) fn plain_text_from_artifact(artifact: &DocumentArtifact) -> String {
    let mut text = String::new();
    for page in &artifact.pages {
        text.push_str(&plain_text_from_page(page));
    }
    text
}

pub(crate) fn plain_text_from_page(page: &PageArtifact) -> String {
    let page_text = quality_text_from_page(page);
    if page_text.is_empty() {
        String::new()
    } else {
        format!("{page_text}\n")
    }
}

pub(crate) fn write_warnings(artifact: &DocumentArtifact) -> Result<()> {
    if artifact.global_diagnostics.warnings.is_empty() {
        return Ok(());
    }

    let stderr = io::stderr();
    let mut handle = stderr.lock();
    for warning in &artifact.global_diagnostics.warnings {
        writeln!(handle, "warning: {warning}")?;
    }
    Ok(())
}

pub(crate) fn write_markdown(artifact: &DocumentArtifact) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for (page_offset, page) in artifact.pages.iter().enumerate() {
        if page_offset > 0 {
            writeln!(handle)?;
            writeln!(handle, "---")?;
            writeln!(handle)?;
        }

        let blocks = markdown_blocks(page);
        for (block_offset, block) in blocks.iter().enumerate() {
            if block_offset > 0 {
                writeln!(handle)?;
            }
            writeln!(handle, "{block}")?;
        }
    }
    Ok(())
}

pub(crate) fn markdown_blocks(page: &PageArtifact) -> Vec<String> {
    if page.layout_blocks.is_empty() {
        return page
            .native_spans
            .iter()
            .chain(page.ocr_spans.iter())
            .map(|span| span.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect();
    }

    page.layout_blocks
        .iter()
        .map(|block| match block.kind {
            LayoutBlockKind::Heading => format!("# {}", block.text.trim()),
            LayoutBlockKind::Paragraph
            | LayoutBlockKind::List
            | LayoutBlockKind::Figure
            | LayoutBlockKind::Header
            | LayoutBlockKind::Footer => block.text.trim().to_string(),
            LayoutBlockKind::Table => block
                .table
                .as_ref()
                .and_then(markdown_table_grid)
                .or_else(|| markdown_table_block(&block.text))
                .unwrap_or_else(|| block.text.trim().to_string()),
        })
        .filter(|text| !text.is_empty())
        .collect()
}

pub(crate) fn markdown_table_grid(table: &LayoutTable) -> Option<String> {
    markdown_table_rows(&table_rows_from_grid(table))
}

pub(crate) fn markdown_table_block(text: &str) -> Option<String> {
    let rows = text
        .lines()
        .map(parse_markdown_table_row)
        .collect::<Option<Vec<_>>>()?;
    markdown_table_rows(&rows)
}

pub(crate) fn markdown_table_rows(rows: &[Vec<String>]) -> Option<String> {
    let column_count = rows.first()?.len();
    if rows.len() < 2 || column_count < 2 || rows.iter().any(|row| row.len() != column_count) {
        return None;
    }

    let mut markdown = String::new();
    markdown.push_str(&format_markdown_table_row(&rows[0]));
    markdown.push('\n');
    markdown.push_str(&format_markdown_table_row(&vec![
        "---".to_string();
        column_count
    ]));
    for row in rows.iter().skip(1) {
        markdown.push('\n');
        markdown.push_str(&format_markdown_table_row(row));
    }
    Some(markdown)
}

pub(crate) fn parse_markdown_table_row(line: &str) -> Option<Vec<String>> {
    parse_pipe_table_row(line).or_else(|| parse_whitespace_table_row(line))
}

pub(crate) fn parse_pipe_table_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return None;
    }

    let trimmed = trimmed.trim_matches('|');
    let cells = trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect::<Vec<_>>();

    (cells.len() >= 2 && cells.iter().any(|cell| !cell.is_empty())).then_some(cells)
}

pub(crate) fn parse_whitespace_table_row(line: &str) -> Option<Vec<String>> {
    let cells = line
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    (cells.len() >= 2).then_some(cells)
}

pub(crate) fn format_markdown_table_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}
