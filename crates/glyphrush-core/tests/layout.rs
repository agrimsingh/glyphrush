use glyphrush_core::{
    BBox, ExtractedPage, ExtractedTextSpan, LayoutBlockKind, PageDimensions, PageQuality,
    PageSignals, PageTimings, parse_extracted_pages,
};

#[test]
fn native_text_is_split_into_deterministic_layout_blocks() {
    let artifact = parse_extracted_pages(
        "doc-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "INTRODUCTION\n\n",
                "Glyphrush keeps layout artifacts explicit.\n",
                "Second paragraph line.\n\n",
                "- first item\n",
                "- second item\n\n",
                "| Part | Value |\n",
                "| A | 1 |\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 4);
    assert_eq!(page.layout_blocks[0].block_id, "p000000:b000000");
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "INTRODUCTION");
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Paragraph);
    assert_eq!(
        page.layout_blocks[1].text,
        "Glyphrush keeps layout artifacts explicit.\nSecond paragraph line."
    );
    assert_eq!(page.layout_blocks[2].kind, LayoutBlockKind::List);
    assert_eq!(page.layout_blocks[3].kind, LayoutBlockKind::Table);
    assert!(page.timings.layout_us > 0);
}

#[test]
fn pipe_table_payload_preserves_empty_cells_and_column_indexes() {
    let artifact = parse_extracted_pages(
        "doc-table-empty-cells".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "| Part | Value | Note |\n",
                "| A | | missing value |\n",
                "| B | 2 | |"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let table = artifact.pages[0].layout_blocks[0]
        .table
        .as_ref()
        .expect("table payload");
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[1].cells.len(), 3);
    assert_eq!(table.rows[1].cells[0].column_index, 0);
    assert_eq!(table.rows[1].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[1].column_index, 1);
    assert_eq!(table.rows[1].cells[1].text, "");
    assert_eq!(table.rows[1].cells[2].column_index, 2);
    assert_eq!(table.rows[1].cells[2].text, "missing value");
    assert_eq!(table.rows[2].cells[2].column_index, 2);
    assert_eq!(table.rows[2].cells[2].text, "");
}

#[test]
fn pipe_table_payload_ignores_markdown_separator_rows() {
    let artifact = parse_extracted_pages(
        "doc-table-markdown-separator".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!("| Part | Value |\n", "| --- | --- |\n", "| A | 1 |").to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let table = artifact.pages[0].layout_blocks[0]
        .table
        .as_ref()
        .expect("table payload");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Part");
    assert_eq!(table.rows[0].cells[1].text, "Value");
    assert_eq!(table.rows[1].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[1].text, "1");
}

#[test]
fn pipe_table_recovery_keeps_leading_caption_outside_table_grid() {
    let artifact = parse_extracted_pages(
        "doc-pipe-table-leading-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "REGISTER MAP\n",
                "| Address | Name | Default |\n",
                "| --- | --- | --- |\n",
                "| 0x00 | CTRL | 0x01 |\n",
                "| 0x01 | STATUS | 0x00 |"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 5,
                native_text_bytes: 124,
                glyph_count: 92,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "REGISTER MAP");
    assert!(page.layout_blocks[0].table.is_none());
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Table);

    let table = page.layout_blocks[1].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Address", "Name", "Default"],
            vec!["0x00", "CTRL", "0x01"],
            vec!["0x01", "STATUS", "0x00"],
        ]
    );
}

#[test]
fn pipe_table_recovery_keeps_leading_caption_for_two_row_table() {
    let artifact = parse_extracted_pages(
        "doc-pipe-table-short-leading-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "REGISTER SUMMARY\n",
                "| Address | Name |\n",
                "| 0x00 | CTRL |"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 3,
                native_text_bytes: 64,
                glyph_count: 48,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "REGISTER SUMMARY");
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Table);

    let table = page.layout_blocks[1].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(rows, vec![vec!["Address", "Name"], vec!["0x00", "CTRL"]]);
}

#[test]
fn pipe_table_recovery_splits_prefix_caption_and_table() {
    let artifact = parse_extracted_pages(
        "doc-pipe-table-prefix-leading-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "The following registers are implemented:\n",
                "REGISTER MAP\n",
                "| Address | Name |\n",
                "| 0x00 | CTRL |"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 4,
                native_text_bytes: 108,
                glyph_count: 82,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 3);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Paragraph);
    assert_eq!(
        page.layout_blocks[0].text,
        "The following registers are implemented:"
    );
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[1].text, "REGISTER MAP");
    assert_eq!(page.layout_blocks[2].kind, LayoutBlockKind::Table);

    let table = page.layout_blocks[2].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(rows, vec![vec!["Address", "Name"], vec!["0x00", "CTRL"]]);
}

#[test]
fn aligned_whitespace_table_payload_preserves_empty_cells_and_column_indexes() {
    let artifact = parse_extracted_pages(
        "doc-aligned-table-empty-cells".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Part          Value        Note\n",
                "A                          missing value\n",
                "B             2\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 1,
                native_text_bytes: 84,
                glyph_count: 60,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[1].cells.len(), 3);
    assert_eq!(table.rows[1].cells[0].column_index, 0);
    assert_eq!(table.rows[1].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[1].column_index, 1);
    assert_eq!(table.rows[1].cells[1].text, "");
    assert_eq!(table.rows[1].cells[2].column_index, 2);
    assert_eq!(table.rows[1].cells[2].text, "missing value");
    assert_eq!(table.rows[2].cells[0].text, "B");
    assert_eq!(table.rows[2].cells[1].text, "2");
    assert_eq!(table.rows[2].cells[2].text, "");
}

#[test]
fn aligned_whitespace_table_payload_preserves_section_rows() {
    let artifact = parse_extracted_pages(
        "doc-aligned-table-section-row".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter           Symbol      Typ     Max     Unit\n",
                "Input voltage       VIN         3.3     5.5     V\n",
                "Protection features\n",
                "Current limit       ILIM        650     900     mA\n",
                "Thermal shutdown    TSD         150     175     C\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 1,
                native_text_bytes: 224,
                glyph_count: 154,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Protection features", "", "", "", ""],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
            vec!["Thermal shutdown", "TSD", "150", "175", "C"],
        ]
    );
}

#[test]
fn aligned_whitespace_table_payload_merges_wrapped_descriptor_rows() {
    let artifact = parse_extracted_pages(
        "doc-aligned-table-wrapped-descriptor-row".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter           Symbol      Typ     Max     Unit\n",
                "Output voltage\n",
                "accuracy            VOUT        -1      1       %\n",
                "Current limit       ILIM        650     900     mA\n",
                "Thermal shutdown    TSD         150     175     C\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 1,
                native_text_bytes: 224,
                glyph_count: 154,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Output voltage accuracy", "VOUT", "-1", "1", "%"],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
            vec!["Thermal shutdown", "TSD", "150", "175", "C"],
        ]
    );
}

#[test]
fn ocr_text_can_produce_layout_blocks_when_native_text_is_missing() {
    let artifact = parse_extracted_pages(
        "doc-ocr-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: String::new(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                native_span_count: 0,
                native_text_bytes: 0,
                glyph_count: 0,
                image_area_ratio: 0.95,
                ..native_signals(0)
            },
            ocr_text: Some("OCR HEADING\n\nRecovered paragraph.".to_string()),
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert!(page.native_spans.is_empty());
    assert_eq!(page.ocr_spans.len(), 1);
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Paragraph);
}

#[test]
fn applied_ocr_replaces_low_confidence_native_text_for_layout() {
    let artifact = parse_extracted_pages(
        "doc-ocr-over-native-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "x".to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                native_span_count: 1,
                native_text_bytes: 1,
                glyph_count: 1,
                image_area_ratio: 0.95,
                ..native_signals(0)
            },
            ocr_text: Some("OCR HEADING\n\nRecovered OCR paragraph.".to_string()),
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.native_spans.len(), 1);
    assert_eq!(page.native_spans[0].text, "x");
    assert_eq!(page.ocr_spans.len(), 1);
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "OCR HEADING");
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Paragraph);
    assert_eq!(page.layout_blocks[1].text, "Recovered OCR paragraph.");
}

#[test]
fn layout_reflows_short_pdf_fragments_inside_paragraph_blocks() {
    let artifact = parse_extracted_pages(
        "doc-fragments".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "AP735\n",
                "4\n\n",
                "Document number: DS3\n",
                "9918\n\n",
                "Rev\n",
                ".\n",
                "4\n",
                "-\n",
                "2\n",
                "1\n\n",
                "Normal paragraph line.\n",
                "Second normal line.\n\n",
                "- item one\n",
                "- item two\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks[0].text, "AP7354");
    assert_eq!(blocks[1].text, "Document number: DS39918");
    assert_eq!(blocks[2].text, "Rev. 4-21");
    assert_eq!(
        blocks[3].text,
        "Normal paragraph line.\nSecond normal line."
    );
    assert_eq!(blocks[4].kind, LayoutBlockKind::List);
    assert_eq!(blocks[4].text, "- item one\n- item two");
}

#[test]
fn layout_reflows_adjacent_short_fragment_blocks() {
    let artifact = parse_extracted_pages(
        "doc-cross-block-fragments".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "AP735\n\n",
                "4\n\n",
                "Document number: DS3\n\n",
                "9918\n\n",
                "Rev\n\n",
                ".\n\n",
                "4\n\n",
                "-\n\n",
                "2\n\n",
                "1\n\n",
                "of\n\n",
                "18\n\n",
                "The\n\n",
                "AP7354\n\n",
                "November 2019\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks[0].text, "AP7354");
    assert_eq!(blocks[1].text, "Document number: DS39918");
    assert_eq!(blocks[2].text, "Rev. 4-21 of 18");
    assert_eq!(blocks[3].text, "The AP7354");
    assert_eq!(blocks[4].text, "November 2019");
}

#[test]
fn positioned_native_spans_preserve_two_column_reading_order() {
    let artifact = parse_extracted_pages(
        "doc-two-column-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Left column starts\n",
                "Left column continues\n",
                "Right column starts\n",
                "Right column continues"
            )
            .to_string(),
            native_spans: vec![
                span("Left column starts", 72.0, 100.0, 230.0, 114.0),
                span("Right column starts", 330.0, 100.0, 500.0, 114.0),
                span("Left column continues", 72.0, 118.0, 248.0, 132.0),
                span("Right column continues", 330.0, 118.0, 520.0, 132.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "Left column starts\nLeft column continues");
    assert_eq!(
        blocks[1].text,
        "Right column starts\nRight column continues"
    );
    assert_eq!(blocks[0].bbox.x0, 72.0);
    assert_eq!(blocks[0].bbox.x1, 248.0);
    assert_eq!(blocks[1].bbox.x0, 330.0);
    assert_eq!(blocks[1].bbox.x1, 520.0);
}

#[test]
fn positioned_native_spans_preserve_three_column_reading_order() {
    let artifact = parse_extracted_pages(
        "doc-three-column-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Left column starts\n",
                "Left column continues\n",
                "Middle column starts\n",
                "Middle column continues\n",
                "Right column starts\n",
                "Right column continues"
            )
            .to_string(),
            native_spans: vec![
                span("Left column starts", 48.0, 100.0, 156.0, 114.0),
                span("Middle column starts", 230.0, 100.0, 350.0, 114.0),
                span("Right column starts", 430.0, 100.0, 552.0, 114.0),
                span("Left column continues", 48.0, 118.0, 178.0, 132.0),
                span("Middle column continues", 230.0, 118.0, 370.0, 132.0),
                span("Right column continues", 430.0, 118.0, 570.0, 132.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].text, "Left column starts\nLeft column continues");
    assert_eq!(
        blocks[1].text,
        "Middle column starts\nMiddle column continues"
    );
    assert_eq!(
        blocks[2].text,
        "Right column starts\nRight column continues"
    );
    assert_eq!(blocks[0].bbox.x0, 48.0);
    assert_eq!(blocks[1].bbox.x0, 230.0);
    assert_eq!(blocks[2].bbox.x0, 430.0);
}

#[test]
fn positioned_native_spans_preserve_five_column_reading_order() {
    let artifact = parse_extracted_pages(
        "doc-five-column-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(900.0, 792.0),
            native_text: concat!(
                "Column one starts\n",
                "Column one continues\n",
                "Column two starts\n",
                "Column two continues\n",
                "Column three starts\n",
                "Column three continues\n",
                "Column four starts\n",
                "Column four continues\n",
                "Column five starts\n",
                "Column five continues"
            )
            .to_string(),
            native_spans: vec![
                span("Column one starts", 40.0, 100.0, 120.0, 114.0),
                span("Column two starts", 220.0, 100.0, 300.0, 114.0),
                span("Column three starts", 400.0, 100.0, 480.0, 114.0),
                span("Column four starts", 580.0, 100.0, 660.0, 114.0),
                span("Column five starts", 760.0, 100.0, 840.0, 114.0),
                span("Column one continues", 40.0, 118.0, 128.0, 132.0),
                span("Column two continues", 220.0, 118.0, 308.0, 132.0),
                span("Column three continues", 400.0, 118.0, 488.0, 132.0),
                span("Column four continues", 580.0, 118.0, 668.0, 132.0),
                span("Column five continues", 760.0, 118.0, 848.0, 132.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 5);
    assert_eq!(blocks[0].text, "Column one starts\nColumn one continues");
    assert_eq!(blocks[1].text, "Column two starts\nColumn two continues");
    assert_eq!(
        blocks[2].text,
        "Column three starts\nColumn three continues"
    );
    assert_eq!(blocks[3].text, "Column four starts\nColumn four continues");
    assert_eq!(blocks[4].text, "Column five starts\nColumn five continues");
    assert_eq!(blocks[0].bbox.x0, 40.0);
    assert_eq!(blocks[1].bbox.x0, 220.0);
    assert_eq!(blocks[2].bbox.x0, 400.0);
    assert_eq!(blocks[3].bbox.x0, 580.0);
    assert_eq!(blocks[4].bbox.x0, 760.0);
}

#[test]
fn positioned_native_spans_preserve_trailing_cross_column_note_after_two_columns() {
    let artifact = parse_extracted_pages(
        "doc-two-column-trailing-note".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Left column starts\n",
                "Left column continues\n",
                "Right column starts\n",
                "Right column continues\n",
                "Note: output voltage measured after startup"
            )
            .to_string(),
            native_spans: vec![
                span("Left column starts", 72.0, 100.0, 230.0, 114.0),
                span("Right column starts", 330.0, 100.0, 500.0, 114.0),
                span("Left column continues", 72.0, 118.0, 248.0, 132.0),
                span("Right column continues", 330.0, 118.0, 520.0, 132.0),
                span(
                    "Note: output voltage measured after startup",
                    72.0,
                    172.0,
                    430.0,
                    186.0,
                ),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].text, "Left column starts\nLeft column continues");
    assert_eq!(
        blocks[1].text,
        "Right column starts\nRight column continues"
    );
    assert_eq!(
        blocks[2].text,
        "Note: output voltage measured after startup"
    );
    assert_eq!(blocks[2].bbox.x0, 72.0);
    assert_eq!(blocks[2].bbox.x1, 430.0);
}

#[test]
fn positioned_native_spans_preserve_leading_cross_column_subtitle_before_two_columns() {
    let artifact = parse_extracted_pages(
        "doc-two-column-leading-subtitle".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Typical application conditions\n",
                "Left column starts\n",
                "Left column continues\n",
                "Right column starts\n",
                "Right column continues"
            )
            .to_string(),
            native_spans: vec![
                span("Typical application conditions", 72.0, 72.0, 430.0, 86.0),
                span("Left column starts", 72.0, 120.0, 230.0, 134.0),
                span("Right column starts", 330.0, 120.0, 500.0, 134.0),
                span("Left column continues", 72.0, 138.0, 248.0, 152.0),
                span("Right column continues", 330.0, 138.0, 520.0, 152.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].text, "Typical application conditions");
    assert_eq!(blocks[1].text, "Left column starts\nLeft column continues");
    assert_eq!(
        blocks[2].text,
        "Right column starts\nRight column continues"
    );
    assert_eq!(blocks[0].bbox.x0, 72.0);
    assert_eq!(blocks[0].bbox.x1, 430.0);
}

#[test]
fn positioned_native_spans_preserve_full_width_heading_before_two_columns() {
    let artifact = parse_extracted_pages(
        "doc-heading-two-column-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "FULL WIDTH TITLE\n",
                "Left column starts\n",
                "Left column continues\n",
                "Right column starts\n",
                "Right column continues"
            )
            .to_string(),
            native_spans: vec![
                span("FULL WIDTH TITLE", 72.0, 72.0, 540.0, 88.0),
                span("Left column starts", 72.0, 120.0, 230.0, 134.0),
                span("Right column starts", 330.0, 120.0, 500.0, 134.0),
                span("Left column continues", 72.0, 138.0, 248.0, 152.0),
                span("Right column continues", 330.0, 138.0, 520.0, 152.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(blocks[0].text, "FULL WIDTH TITLE");
    assert_eq!(blocks[1].text, "Left column starts\nLeft column continues");
    assert_eq!(
        blocks[2].text,
        "Right column starts\nRight column continues"
    );
    assert_eq!(blocks[0].bbox.x0, 72.0);
    assert_eq!(blocks[0].bbox.x1, 540.0);
    assert_eq!(blocks[1].bbox.x0, 72.0);
    assert_eq!(blocks[2].bbox.x0, 330.0);
}

#[test]
fn positioned_native_spans_preserve_fragmented_full_width_heading_before_two_columns() {
    let artifact = parse_extracted_pages(
        "doc-fragmented-heading-two-column-layout".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "APPLICATION INFORMATION\n",
                "Left column starts\n",
                "Left column continues\n",
                "Right column starts\n",
                "Right column continues"
            )
            .to_string(),
            native_spans: vec![
                span("APPLICATION", 72.0, 72.0, 210.0, 88.0),
                span("INFORMATION", 220.0, 72.0, 430.0, 88.0),
                span("Left column starts", 72.0, 120.0, 230.0, 134.0),
                span("Right column starts", 330.0, 120.0, 500.0, 134.0),
                span("Left column continues", 72.0, 138.0, 248.0, 152.0),
                span("Right column continues", 330.0, 138.0, 520.0, 152.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(blocks[0].text, "APPLICATION INFORMATION");
    assert_eq!(blocks[1].text, "Left column starts\nLeft column continues");
    assert_eq!(
        blocks[2].text,
        "Right column starts\nRight column continues"
    );
    assert_eq!(blocks[0].bbox.x0, 72.0);
    assert_eq!(blocks[0].bbox.x1, 430.0);
}

#[test]
fn positioned_native_spans_preserve_short_section_heading_between_two_column_regions() {
    let artifact = parse_extracted_pages(
        "doc-short-section-between-columns".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "FULL WIDTH TITLE\n",
                "Left intro starts\n",
                "Left intro continues\n",
                "Right intro starts\n",
                "Right intro continues\n",
                "ELECTRICAL CHARACTERISTICS\n",
                "Left specs starts\n",
                "Left specs continues\n",
                "Right specs starts\n",
                "Right specs continues"
            )
            .to_string(),
            native_spans: vec![
                span("FULL WIDTH TITLE", 72.0, 72.0, 540.0, 88.0),
                span("Left intro starts", 72.0, 120.0, 230.0, 134.0),
                span("Right intro starts", 330.0, 120.0, 500.0, 134.0),
                span("Left intro continues", 72.0, 138.0, 248.0, 152.0),
                span("Right intro continues", 330.0, 138.0, 520.0, 152.0),
                span("ELECTRICAL CHARACTERISTICS", 72.0, 196.0, 250.0, 212.0),
                span("Left specs starts", 72.0, 238.0, 230.0, 252.0),
                span("Right specs starts", 330.0, 238.0, 500.0, 252.0),
                span("Left specs continues", 72.0, 256.0, 248.0, 270.0),
                span("Right specs continues", 330.0, 256.0, 520.0, 270.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 6);
    assert_eq!(blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(blocks[0].text, "FULL WIDTH TITLE");
    assert_eq!(blocks[1].text, "Left intro starts\nLeft intro continues");
    assert_eq!(blocks[2].text, "Right intro starts\nRight intro continues");
    assert_eq!(blocks[3].kind, LayoutBlockKind::Heading);
    assert_eq!(blocks[3].text, "ELECTRICAL CHARACTERISTICS");
    assert_eq!(blocks[4].text, "Left specs starts\nLeft specs continues");
    assert_eq!(blocks[5].text, "Right specs starts\nRight specs continues");
}

#[test]
fn positioned_native_spans_preserve_fragmented_short_section_heading_between_two_column_regions() {
    let artifact = parse_extracted_pages(
        "doc-fragmented-short-section-between-columns".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "FULL WIDTH TITLE\n",
                "Left intro starts\n",
                "Left intro continues\n",
                "Right intro starts\n",
                "Right intro continues\n",
                "ELECTRICAL CHARACTERISTICS\n",
                "Left specs starts\n",
                "Left specs continues\n",
                "Right specs starts\n",
                "Right specs continues"
            )
            .to_string(),
            native_spans: vec![
                span("FULL WIDTH TITLE", 72.0, 72.0, 540.0, 88.0),
                span("Left intro starts", 72.0, 120.0, 230.0, 134.0),
                span("Right intro starts", 330.0, 120.0, 500.0, 134.0),
                span("Left intro continues", 72.0, 138.0, 248.0, 152.0),
                span("Right intro continues", 330.0, 138.0, 520.0, 152.0),
                span("ELECTRICAL", 72.0, 196.0, 150.0, 212.0),
                span("CHARACTERISTICS", 158.0, 196.0, 280.0, 212.0),
                span("Left specs starts", 72.0, 238.0, 230.0, 252.0),
                span("Right specs starts", 330.0, 238.0, 500.0, 252.0),
                span("Left specs continues", 72.0, 256.0, 248.0, 270.0),
                span("Right specs continues", 330.0, 256.0, 520.0, 270.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 6);
    assert_eq!(blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(blocks[0].text, "FULL WIDTH TITLE");
    assert_eq!(blocks[1].text, "Left intro starts\nLeft intro continues");
    assert_eq!(blocks[2].text, "Right intro starts\nRight intro continues");
    assert_eq!(blocks[3].kind, LayoutBlockKind::Heading);
    assert_eq!(blocks[3].text, "ELECTRICAL CHARACTERISTICS");
    assert_eq!(blocks[4].text, "Left specs starts\nLeft specs continues");
    assert_eq!(blocks[5].text, "Right specs starts\nRight specs continues");
}

#[test]
fn positioned_native_spans_preserve_middle_cross_column_caption_between_two_column_regions() {
    let artifact = parse_extracted_pages(
        "doc-middle-cross-column-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Left intro starts\n",
                "Left intro continues\n",
                "Right intro starts\n",
                "Right intro continues\n",
                "Typical performance curves\n",
                "Left details starts\n",
                "Left details continues\n",
                "Right details starts\n",
                "Right details continues"
            )
            .to_string(),
            native_spans: vec![
                span("Left intro starts", 72.0, 120.0, 230.0, 134.0),
                span("Right intro starts", 330.0, 120.0, 500.0, 134.0),
                span("Left intro continues", 72.0, 138.0, 248.0, 152.0),
                span("Right intro continues", 330.0, 138.0, 520.0, 152.0),
                span("Typical performance curves", 72.0, 196.0, 430.0, 212.0),
                span("Left details starts", 72.0, 238.0, 230.0, 252.0),
                span("Right details starts", 330.0, 238.0, 500.0, 252.0),
                span("Left details continues", 72.0, 256.0, 248.0, 270.0),
                span("Right details continues", 330.0, 256.0, 520.0, 270.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 5);
    assert_eq!(blocks[0].text, "Left intro starts\nLeft intro continues");
    assert_eq!(blocks[1].text, "Right intro starts\nRight intro continues");
    assert_eq!(blocks[2].text, "Typical performance curves");
    assert_eq!(
        blocks[3].text,
        "Left details starts\nLeft details continues"
    );
    assert_eq!(
        blocks[4].text,
        "Right details starts\nRight details continues"
    );
    assert_eq!(blocks[2].bbox.x0, 72.0);
    assert_eq!(blocks[2].bbox.x1, 430.0);
}

#[test]
fn positioned_native_spans_preserve_fragmented_middle_cross_column_caption_between_two_column_regions()
 {
    let artifact = parse_extracted_pages(
        "doc-fragmented-middle-cross-column-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Left intro starts\n",
                "Left intro continues\n",
                "Right intro starts\n",
                "Right intro continues\n",
                "Typical performance curves\n",
                "Left details starts\n",
                "Left details continues\n",
                "Right details starts\n",
                "Right details continues"
            )
            .to_string(),
            native_spans: vec![
                span("Left intro starts", 72.0, 120.0, 230.0, 134.0),
                span("Right intro starts", 330.0, 120.0, 500.0, 134.0),
                span("Left intro continues", 72.0, 138.0, 248.0, 152.0),
                span("Right intro continues", 330.0, 138.0, 520.0, 152.0),
                span("Typical performance", 72.0, 196.0, 250.0, 212.0),
                span("curves", 258.0, 196.0, 430.0, 212.0),
                span("Left details starts", 72.0, 238.0, 230.0, 252.0),
                span("Right details starts", 330.0, 238.0, 500.0, 252.0),
                span("Left details continues", 72.0, 256.0, 248.0, 270.0),
                span("Right details continues", 330.0, 256.0, 520.0, 270.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 5);
    assert_eq!(blocks[0].text, "Left intro starts\nLeft intro continues");
    assert_eq!(blocks[1].text, "Right intro starts\nRight intro continues");
    assert_eq!(blocks[2].text, "Typical performance curves");
    assert_eq!(
        blocks[3].text,
        "Left details starts\nLeft details continues"
    );
    assert_eq!(
        blocks[4].text,
        "Right details starts\nRight details continues"
    );
    assert_eq!(blocks[2].bbox.x0, 72.0);
    assert_eq!(blocks[2].bbox.x1, 430.0);
}

#[test]
fn positioned_native_spans_preserve_narrow_gutter_columns_before_centered_page_number() {
    let artifact = parse_extracted_pages(
        "doc-narrow-gutter-columns-footer".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(595.276, 841.89),
            native_text: concat!(
                "4172\n",
                "Left column first line\n",
                "Left column second line\n",
                "Left column third line\n",
                "Right column first line\n",
                "Right column second line\n",
                "Right column third line"
            )
            .to_string(),
            native_spans: vec![
                span("4172", 287.0, 778.0, 310.0, 786.0),
                span("Left column first line", 72.0, 66.0, 289.0, 76.0),
                span("Left column second line", 72.0, 80.0, 290.0, 90.0),
                span("Left column third line", 72.0, 94.0, 288.0, 104.0),
                span("Right column first line", 306.0, 66.0, 522.0, 76.0),
                span("Right column second line", 306.0, 80.0, 524.0, 90.0),
                span("Right column third line", 306.0, 94.0, 520.0, 104.0),
            ],
            image_artifacts: Vec::new(),
            signals: native_signals(0),
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(
        blocks[0].text,
        "Left column first line\nLeft column second line\nLeft column third line"
    );
    assert_eq!(
        blocks[1].text,
        "Right column first line\nRight column second line\nRight column third line"
    );
    assert_eq!(blocks[2].text, "4172");
}

#[test]
fn positioned_character_spans_reflow_into_readable_words() {
    let artifact = parse_extracted_pages(
        "doc-positioned-character-spans".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Adjustable Low Dropout 300mA Linear Regulator\nFeatures".to_string(),
            native_spans: vec![
                span("A", 72.0, 90.0, 78.0, 98.0),
                span("d", 78.5, 90.4, 84.0, 98.0),
                span("j", 84.5, 90.1, 87.0, 98.0),
                span("u", 88.0, 91.8, 94.0, 99.5),
                span("s", 95.0, 91.6, 100.0, 99.5),
                span("t", 101.0, 90.6, 104.0, 98.0),
                span("a", 105.0, 91.6, 111.0, 99.5),
                span("b", 112.0, 90.4, 118.0, 98.0),
                span("l", 119.0, 90.4, 121.0, 98.0),
                span("e ", 122.0, 91.5, 130.0, 99.5),
                span("L", 137.0, 90.2, 143.0, 98.0),
                span("o", 144.0, 91.5, 150.0, 99.5),
                span("w ", 151.0, 91.8, 160.0, 99.5),
                span("D", 167.0, 90.2, 174.0, 98.0),
                span("r", 175.0, 91.8, 179.0, 99.5),
                span("o", 180.0, 91.5, 186.0, 99.5),
                span("p", 187.0, 91.5, 193.0, 99.5),
                span("o", 194.0, 91.5, 200.0, 99.5),
                span("u", 201.0, 91.8, 207.0, 99.5),
                span("t ", 208.0, 90.6, 213.0, 98.0),
                span("3", 220.0, 90.1, 226.0, 98.0),
                span("0", 227.0, 90.1, 233.0, 98.0),
                span("0", 234.0, 90.1, 240.0, 98.0),
                span("m", 241.0, 91.5, 250.0, 99.5),
                span("A ", 251.0, 90.2, 260.0, 98.0),
                span("Linear", 267.0, 91.5, 306.0, 99.5),
                span("Regulator", 313.0, 91.5, 370.0, 99.5),
                span("F", 72.0, 129.0, 79.0, 137.0),
                span("e", 80.0, 131.1, 87.0, 139.0),
                span("a", 88.0, 131.1, 95.0, 139.0),
                span("t", 96.0, 129.3, 101.0, 137.0),
                span("u", 102.0, 131.3, 109.0, 139.0),
                span("r", 110.0, 131.2, 116.0, 139.0),
                span("e", 117.0, 131.1, 124.0, 139.0),
                span("s", 125.0, 131.1, 132.0, 139.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                native_span_count: 35,
                native_text_bytes: 62,
                glyph_count: 58,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let text = artifact.pages[0]
        .layout_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Adjustable Low Dropout 300mA Linear Regulator"));
    assert!(text.contains("Features"));
    assert!(!text.contains("A d j"));
    assert!(!text.contains("F e a"));
}

#[test]
fn positioned_overlapping_fragments_do_not_duplicate_prefix_text() {
    let artifact = parse_extracted_pages(
        "doc-positioned-overlap-duplicates".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Wide Operating Voltage Typical dropout for load".to_string(),
            native_spans: vec![
                span("Wide Operating ", 72.0, 100.0, 160.0, 112.0),
                span("Vo", 164.0, 100.0, 170.0, 112.0),
                span("Vo", 169.6, 101.0, 174.5, 113.0),
                span("ltage ", 175.0, 100.0, 198.0, 112.0),
                span("Ty", 212.0, 100.0, 219.0, 112.0),
                span("Typical ", 218.5, 101.0, 248.0, 113.0),
                span("dropout ", 252.0, 100.0, 290.0, 112.0),
                span("fo", 294.0, 100.0, 301.0, 112.0),
                span("for ", 300.5, 101.0, 314.0, 113.0),
                span("load", 318.0, 100.0, 338.0, 112.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                native_span_count: 10,
                native_text_bytes: 48,
                glyph_count: 43,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let text = artifact.pages[0]
        .layout_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Wide Operating Voltage Typical dropout for load"));
    assert!(!text.contains("VoVoltage"));
    assert!(!text.contains("TyTypical"));
    assert!(!text.contains("fofor"));
}

#[test]
fn positioned_table_spans_preserve_rows_when_table_recovery_runs() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Item\nTotal\nAlpha\n10\nBeta\n20".to_string(),
            native_spans: vec![
                span("Item", 72.0, 100.0, 130.0, 114.0),
                span("Total", 220.0, 100.0, 280.0, 114.0),
                span("Alpha", 72.0, 132.0, 140.0, 146.0),
                span("10", 220.0, 132.0, 246.0, 146.0),
                span("Beta", 72.0, 164.0, 132.0, 178.0),
                span("20", 220.0, 164.0, 246.0, 178.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 6,
                native_text_bytes: 28,
                glyph_count: 22,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert!(page.quality.flags.contains(&PageQuality::TableUncertain));
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(page.layout_blocks[0].text, "Item Total\nAlpha 10\nBeta 20");
    assert_eq!(page.layout_blocks[0].bbox.x0, 72.0);
    assert_eq!(page.layout_blocks[0].bbox.y0, 100.0);
    assert_eq!(page.layout_blocks[0].bbox.x1, 280.0);
    assert_eq!(page.layout_blocks[0].bbox.y1, 178.0);
}

#[test]
fn positioned_table_spans_preserve_empty_cells_when_rows_omit_blank_columns() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-empty-cells".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Part\nValue\nNote\nA\nmissing value\nB\n2".to_string(),
            native_spans: vec![
                span("Part", 72.0, 100.0, 130.0, 114.0),
                span("Value", 220.0, 100.0, 280.0, 114.0),
                span("Note", 360.0, 100.0, 420.0, 114.0),
                span("A", 72.0, 132.0, 92.0, 146.0),
                span("missing value", 360.0, 132.0, 470.0, 146.0),
                span("B", 72.0, 164.0, 92.0, 178.0),
                span("2", 220.0, 164.0, 240.0, 178.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 7,
                native_text_bytes: 39,
                glyph_count: 32,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        "Part Value Note\nA missing value\nB 2"
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[1].cells.len(), 3);
    assert_eq!(table.rows[1].cells[0].text, "A");
    assert_eq!(table.rows[1].cells[1].text, "");
    assert_eq!(table.rows[1].cells[2].text, "missing value");
    assert_eq!(table.rows[2].cells[0].text, "B");
    assert_eq!(table.rows[2].cells[1].text, "2");
    assert_eq!(table.rows[2].cells[2].text, "");
    assert!(table.rows[1].cells[1].bbox.is_none());
    assert!(table.rows[2].cells[2].bbox.is_none());
}

#[test]
fn positioned_table_recovery_merges_same_line_fragmented_cells() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-same-line-fragments".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Typ\n",
                "Max\n",
                "Unit\n",
                "Input\n",
                "voltage\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "V\n",
                "Quiescent\n",
                "current\n",
                "IQ\n",
                "35\n",
                "60\n",
                "uA"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 220.0, 100.0, 270.0, 114.0),
                span("Typ", 300.0, 100.0, 330.0, 114.0),
                span("Max", 360.0, 100.0, 390.0, 114.0),
                span("Unit", 420.0, 100.0, 450.0, 114.0),
                span("Input", 72.0, 132.0, 110.0, 146.0),
                span("voltage", 114.0, 132.0, 168.0, 146.0),
                span("VIN", 220.0, 132.0, 248.0, 146.0),
                span("3.3", 300.0, 132.0, 326.0, 146.0),
                span("5.5", 360.0, 132.0, 386.0, 146.0),
                span("V", 420.0, 132.0, 430.0, 146.0),
                span("Quiescent", 72.0, 164.0, 138.0, 178.0),
                span("current", 142.0, 164.0, 194.0, 178.0),
                span("IQ", 220.0, 164.0, 238.0, 178.0),
                span("35", 300.0, 164.0, 318.0, 178.0),
                span("60", 360.0, 164.0, 378.0, 178.0),
                span("uA", 420.0, 164.0, 440.0, 178.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 17,
                native_text_bytes: 92,
                glyph_count: 75,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        "Parameter Symbol Typ Max Unit\nInput voltage VIN 3.3 5.5 V\nQuiescent current IQ 35 60 uA"
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Quiescent current", "IQ", "35", "60", "uA"],
        ]
    );
    assert_eq!(table.rows[1].cells[0].bbox.as_ref().unwrap().x0, 72.0);
    assert_eq!(table.rows[1].cells[0].bbox.as_ref().unwrap().x1, 168.0);
}

#[test]
fn positioned_table_recovery_merges_wrapped_descriptor_cells() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-wrapped-descriptors".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Typ\n",
                "Max\n",
                "Unit\n",
                "Input\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "V\n",
                "voltage\n",
                "Quiescent\n",
                "IQ\n",
                "35\n",
                "60\n",
                "uA\n",
                "current"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 220.0, 100.0, 270.0, 114.0),
                span("Typ", 300.0, 100.0, 330.0, 114.0),
                span("Max", 360.0, 100.0, 390.0, 114.0),
                span("Unit", 420.0, 100.0, 450.0, 114.0),
                span("Input", 72.0, 132.0, 110.0, 146.0),
                span("VIN", 220.0, 132.0, 248.0, 146.0),
                span("3.3", 300.0, 132.0, 326.0, 146.0),
                span("5.5", 360.0, 132.0, 386.0, 146.0),
                span("V", 420.0, 132.0, 430.0, 146.0),
                span("voltage", 72.0, 148.0, 126.0, 162.0),
                span("Quiescent", 72.0, 188.0, 138.0, 202.0),
                span("IQ", 220.0, 188.0, 238.0, 202.0),
                span("35", 300.0, 188.0, 318.0, 202.0),
                span("60", 360.0, 188.0, 378.0, 202.0),
                span("uA", 420.0, 188.0, 440.0, 202.0),
                span("current", 72.0, 204.0, 124.0, 218.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 17,
                native_text_bytes: 92,
                glyph_count: 75,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        "Parameter Symbol Typ Max Unit\nInput voltage VIN 3.3 5.5 V\nQuiescent current IQ 35 60 uA"
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Quiescent current", "IQ", "35", "60", "uA"],
        ]
    );
    assert_eq!(table.rows[1].cells[0].bbox.as_ref().unwrap().y0, 132.0);
    assert_eq!(table.rows[1].cells[0].bbox.as_ref().unwrap().y1, 162.0);
}

#[test]
fn positioned_table_recovery_merges_multi_cell_wrapped_continuations() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-multi-cell-continuation".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Condition\n",
                "Typ\n",
                "Max\n",
                "Input\n",
                "VIN\n",
                "No\n",
                "3.3\n",
                "5.5\n",
                "voltage\n",
                "load"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 180.0, 100.0, 230.0, 114.0),
                span("Condition", 260.0, 100.0, 330.0, 114.0),
                span("Typ", 380.0, 100.0, 410.0, 114.0),
                span("Max", 440.0, 100.0, 470.0, 114.0),
                span("Input", 72.0, 132.0, 110.0, 146.0),
                span("VIN", 180.0, 132.0, 208.0, 146.0),
                span("No", 260.0, 132.0, 278.0, 146.0),
                span("3.3", 380.0, 132.0, 406.0, 146.0),
                span("5.5", 440.0, 132.0, 466.0, 146.0),
                span("voltage", 72.0, 148.0, 126.0, 162.0),
                span("load", 260.0, 148.0, 294.0, 162.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 12,
                native_text_bytes: 72,
                glyph_count: 58,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        "Parameter Symbol Condition Typ Max\nInput voltage VIN No load 3.3 5.5"
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Condition", "Typ", "Max"],
            vec!["Input voltage", "VIN", "No load", "3.3", "5.5"],
        ]
    );
    assert_eq!(table.rows[1].cells[0].bbox.as_ref().unwrap().y1, 162.0);
    assert_eq!(table.rows[1].cells[2].bbox.as_ref().unwrap().y1, 162.0);
}

#[test]
fn positioned_table_recovery_preserves_interior_condition_note_rows() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-interior-condition-note".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Condition\n",
                "Typ\n",
                "Max\n",
                "Input voltage\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "VIN = VOUT + 1V\n",
                "Output current\n",
                "IOUT\n",
                "100\n",
                "150\n",
                "Shutdown current\n",
                "ISD\n",
                "0.1\n",
                "1.0"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 180.0, 100.0, 230.0, 114.0),
                span("Condition", 260.0, 100.0, 330.0, 114.0),
                span("Typ", 380.0, 100.0, 410.0, 114.0),
                span("Max", 440.0, 100.0, 470.0, 114.0),
                span("Input voltage", 72.0, 132.0, 160.0, 146.0),
                span("VIN", 180.0, 132.0, 208.0, 146.0),
                span("3.3", 380.0, 132.0, 406.0, 146.0),
                span("5.5", 440.0, 132.0, 466.0, 146.0),
                span("VIN = VOUT + 1V", 260.0, 164.0, 366.0, 178.0),
                span("Output current", 72.0, 196.0, 168.0, 210.0),
                span("IOUT", 180.0, 196.0, 216.0, 210.0),
                span("100", 380.0, 196.0, 410.0, 210.0),
                span("150", 440.0, 196.0, 470.0, 210.0),
                span("Shutdown current", 72.0, 228.0, 184.0, 242.0),
                span("ISD", 180.0, 228.0, 208.0, 242.0),
                span("0.1", 380.0, 228.0, 406.0, 242.0),
                span("1.0", 440.0, 228.0, 466.0, 242.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 18,
                native_text_bytes: 150,
                glyph_count: 118,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        concat!(
            "Parameter Symbol Condition Typ Max\n",
            "Input voltage VIN 3.3 5.5\n",
            "VIN = VOUT + 1V\n",
            "Output current IOUT 100 150\n",
            "Shutdown current ISD 0.1 1.0"
        )
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Condition", "Typ", "Max"],
            vec!["Input voltage", "VIN", "", "3.3", "5.5"],
            vec!["", "", "VIN = VOUT + 1V", "", ""],
            vec!["Output current", "IOUT", "", "100", "150"],
            vec!["Shutdown current", "ISD", "", "0.1", "1.0"],
        ]
    );
    assert_eq!(table.rows[2].cells[2].bbox.as_ref().unwrap().x0, 260.0);
    assert!(table.rows[2].cells[0].bbox.is_none());
    assert!(table.rows[2].cells[4].bbox.is_none());
}

#[test]
fn positioned_table_recovery_merges_same_column_wrapped_header_rows() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-wrapped-header".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Output\n",
                "Load\n",
                "Current\n",
                "Unit\n",
                "Voltage\n",
                "Regulation\n",
                "Limit\n",
                "mA\n",
                "3.3V\n",
                "0-100mA\n",
                "150\n",
                "mA\n",
                "5.0V\n",
                "0-50mA\n",
                "100\n",
                "mA"
            )
            .to_string(),
            native_spans: vec![
                span("Output", 72.0, 100.0, 120.0, 114.0),
                span("Load", 180.0, 100.0, 220.0, 114.0),
                span("Current", 300.0, 100.0, 360.0, 114.0),
                span("Unit", 420.0, 100.0, 450.0, 114.0),
                span("Voltage", 72.0, 116.0, 128.0, 130.0),
                span("Regulation", 180.0, 116.0, 250.0, 130.0),
                span("Limit", 300.0, 116.0, 340.0, 130.0),
                span("mA", 420.0, 116.0, 442.0, 130.0),
                span("3.3V", 72.0, 152.0, 108.0, 166.0),
                span("0-100mA", 180.0, 152.0, 246.0, 166.0),
                span("150", 300.0, 152.0, 330.0, 166.0),
                span("mA", 420.0, 152.0, 442.0, 166.0),
                span("5.0V", 72.0, 184.0, 108.0, 198.0),
                span("0-50mA", 180.0, 184.0, 238.0, 198.0),
                span("100", 300.0, 184.0, 330.0, 198.0),
                span("mA", 420.0, 184.0, 442.0, 198.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 16,
                native_text_bytes: 112,
                glyph_count: 91,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        concat!(
            "Output Voltage Load Regulation Current Limit Unit mA\n",
            "3.3V 0-100mA 150 mA\n",
            "5.0V 0-50mA 100 mA"
        )
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "Output Voltage",
                "Load Regulation",
                "Current Limit",
                "Unit mA"
            ],
            vec!["3.3V", "0-100mA", "150", "mA"],
            vec!["5.0V", "0-50mA", "100", "mA"],
        ]
    );
    assert_eq!(table.rows[0].cells[0].bbox.as_ref().unwrap().y0, 100.0);
    assert_eq!(table.rows[0].cells[0].bbox.as_ref().unwrap().y1, 130.0);
}

#[test]
fn positioned_table_recovery_does_not_merge_compact_text_data_rows_into_header() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-text-data-rows".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Name\nStatus\nAlpha\nGood\nBeta\nBetter".to_string(),
            native_spans: vec![
                span("Name", 72.0, 100.0, 112.0, 114.0),
                span("Status", 220.0, 100.0, 270.0, 114.0),
                span("Alpha", 72.0, 116.0, 120.0, 130.0),
                span("Good", 220.0, 116.0, 260.0, 130.0),
                span("Beta", 72.0, 148.0, 112.0, 162.0),
                span("Better", 220.0, 148.0, 270.0, 162.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 6,
                native_text_bytes: 41,
                glyph_count: 35,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        "Name Status\nAlpha Good\nBeta Better"
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Name", "Status"],
            vec!["Alpha", "Good"],
            vec!["Beta", "Better"],
        ]
    );
}

#[test]
fn positioned_table_recovery_preserves_cross_column_section_rows() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-section-row".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Typ\n",
                "Max\n",
                "Unit\n",
                "Input voltage\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "V\n",
                "Protection features\n",
                "Current limit\n",
                "ILIM\n",
                "650\n",
                "900\n",
                "mA\n",
                "Thermal shutdown\n",
                "TSD\n",
                "150\n",
                "175\n",
                "C"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 220.0, 100.0, 270.0, 114.0),
                span("Typ", 300.0, 100.0, 330.0, 114.0),
                span("Max", 360.0, 100.0, 390.0, 114.0),
                span("Unit", 420.0, 100.0, 450.0, 114.0),
                span("Input voltage", 72.0, 132.0, 160.0, 146.0),
                span("VIN", 220.0, 132.0, 248.0, 146.0),
                span("3.3", 300.0, 132.0, 326.0, 146.0),
                span("5.5", 360.0, 132.0, 386.0, 146.0),
                span("V", 420.0, 132.0, 430.0, 146.0),
                span("Protection features", 72.0, 164.0, 450.0, 178.0),
                span("Current limit", 72.0, 196.0, 160.0, 210.0),
                span("ILIM", 220.0, 196.0, 252.0, 210.0),
                span("650", 300.0, 196.0, 326.0, 210.0),
                span("900", 360.0, 196.0, 386.0, 210.0),
                span("mA", 420.0, 196.0, 440.0, 210.0),
                span("Thermal shutdown", 72.0, 220.0, 184.0, 234.0),
                span("TSD", 220.0, 220.0, 248.0, 234.0),
                span("150", 300.0, 220.0, 326.0, 234.0),
                span("175", 360.0, 220.0, 386.0, 234.0),
                span("C", 420.0, 220.0, 430.0, 234.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 21,
                native_text_bytes: 142,
                glyph_count: 112,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        concat!(
            "Parameter Symbol Typ Max Unit\n",
            "Input voltage VIN 3.3 5.5 V\n",
            "Protection features\n",
            "Current limit ILIM 650 900 mA\n",
            "Thermal shutdown TSD 150 175 C"
        )
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Protection features", "", "", "", ""],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
            vec!["Thermal shutdown", "TSD", "150", "175", "C"],
        ]
    );
    assert_eq!(table.rows[2].cells[0].bbox.as_ref().unwrap().x0, 72.0);
    assert_eq!(table.rows[2].cells[0].bbox.as_ref().unwrap().x1, 450.0);
    assert!(table.rows[2].cells[1].bbox.is_none());
}

#[test]
fn positioned_table_recovery_preserves_first_column_section_rows() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-first-column-section-row".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Typ\n",
                "Max\n",
                "Unit\n",
                "Input voltage\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "V\n",
                "Protection features\n",
                "Current limit\n",
                "ILIM\n",
                "650\n",
                "900\n",
                "mA\n",
                "Thermal shutdown\n",
                "TSD\n",
                "150\n",
                "175\n",
                "C"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 220.0, 100.0, 270.0, 114.0),
                span("Typ", 300.0, 100.0, 330.0, 114.0),
                span("Max", 360.0, 100.0, 390.0, 114.0),
                span("Unit", 420.0, 100.0, 450.0, 114.0),
                span("Input voltage", 72.0, 132.0, 160.0, 146.0),
                span("VIN", 220.0, 132.0, 248.0, 146.0),
                span("3.3", 300.0, 132.0, 326.0, 146.0),
                span("5.5", 360.0, 132.0, 386.0, 146.0),
                span("V", 420.0, 132.0, 430.0, 146.0),
                span("Protection features", 72.0, 164.0, 184.0, 178.0),
                span("Current limit", 72.0, 196.0, 160.0, 210.0),
                span("ILIM", 220.0, 196.0, 252.0, 210.0),
                span("650", 300.0, 196.0, 326.0, 210.0),
                span("900", 360.0, 196.0, 386.0, 210.0),
                span("mA", 420.0, 196.0, 440.0, 210.0),
                span("Thermal shutdown", 72.0, 220.0, 184.0, 234.0),
                span("TSD", 220.0, 220.0, 248.0, 234.0),
                span("150", 300.0, 220.0, 326.0, 234.0),
                span("175", 360.0, 220.0, 386.0, 234.0),
                span("C", 420.0, 220.0, 430.0, 234.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 21,
                native_text_bytes: 142,
                glyph_count: 112,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        concat!(
            "Parameter Symbol Typ Max Unit\n",
            "Input voltage VIN 3.3 5.5 V\n",
            "Protection features\n",
            "Current limit ILIM 650 900 mA\n",
            "Thermal shutdown TSD 150 175 C"
        )
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Protection features", "", "", "", ""],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
            vec!["Thermal shutdown", "TSD", "150", "175", "C"],
        ]
    );
    assert_eq!(table.rows[2].cells[0].bbox.as_ref().unwrap().x0, 72.0);
    assert_eq!(table.rows[2].cells[0].bbox.as_ref().unwrap().x1, 184.0);
    assert!(table.rows[2].cells[1].bbox.is_none());
}

#[test]
fn positioned_table_recovery_preserves_fragmented_first_column_section_rows() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-fragmented-first-column-section-row".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter\n",
                "Symbol\n",
                "Typ\n",
                "Max\n",
                "Unit\n",
                "Input voltage\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "V\n",
                "Protection\n",
                "features\n",
                "Current limit\n",
                "ILIM\n",
                "650\n",
                "900\n",
                "mA\n",
                "Thermal shutdown\n",
                "TSD\n",
                "150\n",
                "175\n",
                "C"
            )
            .to_string(),
            native_spans: vec![
                span("Parameter", 72.0, 100.0, 140.0, 114.0),
                span("Symbol", 220.0, 100.0, 270.0, 114.0),
                span("Typ", 300.0, 100.0, 330.0, 114.0),
                span("Max", 360.0, 100.0, 390.0, 114.0),
                span("Unit", 420.0, 100.0, 450.0, 114.0),
                span("Input voltage", 72.0, 132.0, 160.0, 146.0),
                span("VIN", 220.0, 132.0, 248.0, 146.0),
                span("3.3", 300.0, 132.0, 326.0, 146.0),
                span("5.5", 360.0, 132.0, 386.0, 146.0),
                span("V", 420.0, 132.0, 430.0, 146.0),
                span("Protection", 72.0, 164.0, 138.0, 178.0),
                span("features", 142.0, 164.0, 194.0, 178.0),
                span("Current limit", 72.0, 196.0, 160.0, 210.0),
                span("ILIM", 220.0, 196.0, 252.0, 210.0),
                span("650", 300.0, 196.0, 326.0, 210.0),
                span("900", 360.0, 196.0, 386.0, 210.0),
                span("mA", 420.0, 196.0, 440.0, 210.0),
                span("Thermal shutdown", 72.0, 220.0, 184.0, 234.0),
                span("TSD", 220.0, 220.0, 248.0, 234.0),
                span("150", 300.0, 220.0, 326.0, 234.0),
                span("175", 360.0, 220.0, 386.0, 234.0),
                span("C", 420.0, 220.0, 430.0, 234.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 22,
                native_text_bytes: 142,
                glyph_count: 112,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[0].text,
        concat!(
            "Parameter Symbol Typ Max Unit\n",
            "Input voltage VIN 3.3 5.5 V\n",
            "Protection features\n",
            "Current limit ILIM 650 900 mA\n",
            "Thermal shutdown TSD 150 175 C"
        )
    );
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Protection features", "", "", "", ""],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
            vec!["Thermal shutdown", "TSD", "150", "175", "C"],
        ]
    );
    assert_eq!(table.rows[2].cells[0].bbox.as_ref().unwrap().x0, 72.0);
    assert_eq!(table.rows[2].cells[0].bbox.as_ref().unwrap().x1, 194.0);
    assert!(table.rows[2].cells[1].bbox.is_none());
}

#[test]
fn positioned_table_recovery_preserves_surrounding_text_blocks() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-with-context".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "SUMMARY TABLE\nItem\nTotal\nAlpha\n10\nBeta\n20\nSource note".to_string(),
            native_spans: vec![
                span("SUMMARY TABLE", 72.0, 72.0, 210.0, 86.0),
                span("Item", 72.0, 120.0, 130.0, 134.0),
                span("Total", 220.0, 120.0, 280.0, 134.0),
                span("Alpha", 72.0, 146.0, 140.0, 160.0),
                span("10", 220.0, 146.0, 246.0, 160.0),
                span("Beta", 72.0, 172.0, 132.0, 186.0),
                span("20", 220.0, 172.0, 246.0, 186.0),
                span("Source note", 72.0, 230.0, 190.0, 244.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 8,
                native_text_bytes: 61,
                glyph_count: 47,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 3);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "SUMMARY TABLE");
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Table);
    assert_eq!(page.layout_blocks[1].text, "Item Total\nAlpha 10\nBeta 20");
    assert_eq!(page.layout_blocks[1].bbox.x0, 72.0);
    assert_eq!(page.layout_blocks[1].bbox.y0, 120.0);
    assert_eq!(page.layout_blocks[1].bbox.x1, 280.0);
    assert_eq!(page.layout_blocks[1].bbox.y1, 186.0);
    assert_eq!(page.layout_blocks[2].kind, LayoutBlockKind::Paragraph);
    assert_eq!(page.layout_blocks[2].text, "Source note");
}

#[test]
fn positioned_table_recovery_keeps_top_caption_outside_table_grid() {
    let artifact = parse_extracted_pages(
        "doc-positioned-table-top-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "ELECTRICAL CHARACTERISTICS\n",
                "Parameter\n",
                "Symbol\n",
                "Typ\n",
                "Max\n",
                "Unit\n",
                "Input voltage\n",
                "VIN\n",
                "3.3\n",
                "5.5\n",
                "V\n",
                "Current limit\n",
                "ILIM\n",
                "650\n",
                "900\n",
                "mA"
            )
            .to_string(),
            native_spans: vec![
                span("ELECTRICAL CHARACTERISTICS", 72.0, 72.0, 450.0, 86.0),
                span("Parameter", 72.0, 120.0, 140.0, 134.0),
                span("Symbol", 220.0, 120.0, 270.0, 134.0),
                span("Typ", 300.0, 120.0, 330.0, 134.0),
                span("Max", 360.0, 120.0, 390.0, 134.0),
                span("Unit", 420.0, 120.0, 450.0, 134.0),
                span("Input voltage", 72.0, 152.0, 160.0, 166.0),
                span("VIN", 220.0, 152.0, 248.0, 166.0),
                span("3.3", 300.0, 152.0, 326.0, 166.0),
                span("5.5", 360.0, 152.0, 386.0, 166.0),
                span("V", 420.0, 152.0, 430.0, 166.0),
                span("Current limit", 72.0, 184.0, 160.0, 198.0),
                span("ILIM", 220.0, 184.0, 252.0, 198.0),
                span("650", 300.0, 184.0, 326.0, 198.0),
                span("900", 360.0, 184.0, 386.0, 198.0),
                span("mA", 420.0, 184.0, 440.0, 198.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 16,
                native_text_bytes: 138,
                glyph_count: 108,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "ELECTRICAL CHARACTERISTICS");
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[1].text,
        concat!(
            "Parameter Symbol Typ Max Unit\n",
            "Input voltage VIN 3.3 5.5 V\n",
            "Current limit ILIM 650 900 mA"
        )
    );

    let table = page.layout_blocks[1].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
        ]
    );
}

#[test]
fn text_table_recovery_keeps_leading_caption_outside_table_grid() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-text-table-leading-caption".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "ELECTRICAL CHARACTERISTICS\n",
                "Parameter Symbol Typ Max Unit\n",
                "Input voltage VIN 3.3 5.5 V\n",
                "Current limit ILIM 650 900 mA"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 4,
                native_text_bytes: 124,
                glyph_count: 99,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[0].text, "ELECTRICAL CHARACTERISTICS");
    assert!(page.layout_blocks[0].table.is_none());
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Table);

    let table = page.layout_blocks[1].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
        ]
    );
}

#[test]
fn text_table_recovery_merges_leading_descriptor_cells_from_header_columns() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-text-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter Symbol Typ Max Unit\n",
                "Input voltage VIN 3.3 5.5 V\n",
                "Quiescent current IQ 35 60 uA"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 3,
                native_text_bytes: 90,
                glyph_count: 70,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Quiescent current", "IQ", "35", "60", "uA"],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_embedded_pin_function_tables() {
    let artifact = parse_extracted_pages(
        "doc-embedded-pin-function-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Figure 2. Typical Application Circuit of FP6183\n",
                "Note1: To prevent oscillation, use minimum 1uF capacitors.\n",
                "Functional Pin Description\n",
                "Pin Name Pin No. Pin Function\n",
                "VOUT 1 The FP6183 is stable with an output capacitor 1uF or greater.\n",
                "GND 2 Common ground pin.\n",
                "EN 3 Pull this pin high to enable IC.\n",
                "VIN 4 Power is supplied to this device from this pin.\n",
                "Exposed\n",
                "pad EP The exposed pad must be soldered to a large PCB area and connected to GND."
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 10,
                native_text_bytes: 372,
                glyph_count: 295,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 3);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Paragraph);
    assert_eq!(
        page.layout_blocks[0].text,
        "Figure 2. Typical Application Circuit of FP6183\nNote1: To prevent oscillation, use minimum 1uF capacitors."
    );
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[1].text, "Functional Pin Description");
    assert_eq!(page.layout_blocks[2].kind, LayoutBlockKind::Table);

    let table = page.layout_blocks[2].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Pin Name", "Pin No.", "Pin Function"],
            vec![
                "VOUT",
                "1",
                "The FP6183 is stable with an output capacitor 1uF or greater."
            ],
            vec!["GND", "2", "Common ground pin."],
            vec!["EN", "3", "Pull this pin high to enable IC."],
            vec![
                "VIN",
                "4",
                "Power is supplied to this device from this pin."
            ],
            vec![
                "Exposed pad",
                "EP",
                "The exposed pad must be soldered to a large PCB area and connected to GND."
            ],
        ]
    );
}

#[test]
fn text_table_recovery_merges_split_pin_function_rows_from_pdfium_text() {
    let artifact = parse_extracted_pages(
        "doc-pdfium-split-pin-function-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Figure 2. Typical Application Circuit of FP6183\n",
                "Note1: To prevent oscillation, it is recommended to use minimum 1uF capacitors.\n",
                "Functional Pin Description\n",
                "Pin Name Pin No. Pin Function\n",
                "VOUT 1\n",
                "The FP6183 is stable with an output capacitor 1uF or greater. The larger output capacitor will be\n",
                "required for application with larger load transients.\n",
                "GND 2 Common ground pin.\n",
                "EN 3\n",
                "Pull this pin high to enable IC, pull this pin low to shutdown IC.\n",
                "VIN 4\n",
                "Power is supplied to this device from this pin.\n",
                "Exposed\n",
                "pad\n",
                "EP\n",
                "The exposed pad must be soldered to a large PCB area and connected to GND for maximum power\n",
                "dissipation.\n",
                "Block Diagram\n",
                "VIN\n",
                "Error Amp Current Limit"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 20,
                native_text_bytes: 560,
                glyph_count: 440,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("pin function table block");
    assert!(!table_block.text.contains("Block Diagram"));

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Pin Name", "Pin No.", "Pin Function"],
            vec![
                "VOUT",
                "1",
                "The FP6183 is stable with an output capacitor 1uF or greater. The larger output capacitor will be required for application with larger load transients."
            ],
            vec!["GND", "2", "Common ground pin."],
            vec![
                "EN",
                "3",
                "Pull this pin high to enable IC, pull this pin low to shutdown IC."
            ],
            vec![
                "VIN",
                "4",
                "Power is supplied to this device from this pin."
            ],
            vec![
                "Exposed pad",
                "EP",
                "The exposed pad must be soldered to a large PCB area and connected to GND for maximum power dissipation."
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_split_pin_number_name_function_tables() {
    let artifact = parse_extracted_pages(
        "doc-split-pin-number-name-function-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Operating Waveforms (Cont.)\n",
                "Pin Description\n",
                "PIN\n",
                "NO. NAME\n",
                "FUNCTION\n",
                "1 VIN Voltage supply input pin.\n",
                "2 GND Ground pin.\n",
                "3 SHDN Shutdown control pin, logic high: enable; logic low: shutdown.\n",
                "4 SET Connect this pin to an external resistor divider to adjust output voltage.\n",
                "5 VOUT Regulator output pin.\n",
                "\n",
                "Power On\n",
                "CH1 : VIN , 2V/div\n",
                "CH2 : VOUT , 2V/div"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 14,
                native_text_bytes: 430,
                glyph_count: 335,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 4);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Paragraph);
    assert_eq!(page.layout_blocks[0].text, "Operating Waveforms (Cont.)");
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Heading);
    assert_eq!(page.layout_blocks[1].text, "Pin Description");
    assert_eq!(page.layout_blocks[2].kind, LayoutBlockKind::Table);
    assert_eq!(page.layout_blocks[3].kind, LayoutBlockKind::Paragraph);
    assert!(page.layout_blocks[3].text.starts_with("Power On"));

    let table = page.layout_blocks[2].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Pin No.", "Name", "Function"],
            vec!["1", "VIN", "Voltage supply input pin."],
            vec!["2", "GND", "Ground pin."],
            vec![
                "3",
                "SHDN",
                "Shutdown control pin, logic high: enable; logic low: shutdown."
            ],
            vec![
                "4",
                "SET",
                "Connect this pin to an external resistor divider to adjust output voltage."
            ],
            vec!["5", "VOUT", "Regulator output pin."],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_fragmented_symbol_rating_tables() {
    let artifact = parse_extracted_pages(
        "doc-fragmented-symbol-rating-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Copyright ANPEC Electronics Corp.\n",
                "Rev. A.1 - Jan., 2013\n",
                "APL5324\n",
                "2 www.anpec.com.tw\n",
                "Symbol Parameter Rating Unit\n",
                "VIN\n",
                " \n",
                " VIN Supply Voltage (VIN to GND) -0.3 ~ 6.5 V\n",
                "VSHDN\n",
                " \n",
                " SHDN Input Voltage (SHDN to GND) -0.3 ~ 6.5 V\n",
                "PD\n",
                " \n",
                " Power Dissipation Internally Limited W\n",
                "TJ\n",
                " \n",
                " Junction Temperature -40 ~ 150\n",
                "oC\n",
                "TSTG\n",
                " \n",
                " Storage Temperature -65 ~ 150\n",
                "oC\n",
                "TSDR\n",
                " \n",
                " Maximum Lead Soldering Temperature, 10 Seconds 260\n",
                "oC\n",
                " \n",
                "Absolute Maximum Ratings (Note 1)"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.38,
                native_span_count: 22,
                native_text_bytes: 430,
                glyph_count: 360,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 3);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Paragraph);
    assert!(
        page.layout_blocks[0]
            .text
            .starts_with("Copyright ANPEC Electronics Corp.")
    );
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Table);
    assert_eq!(
        page.layout_blocks[2].text,
        "Absolute Maximum Ratings (Note 1)"
    );

    let table = page.layout_blocks[1].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Symbol", "Parameter", "Rating", "Unit"],
            vec!["VIN", "Supply Voltage (VIN to GND)", "-0.3 ~ 6.5", "V"],
            vec![
                "VSHDN",
                "SHDN Input Voltage (SHDN to GND)",
                "-0.3 ~ 6.5",
                "V"
            ],
            vec!["PD", "Power Dissipation Internally Limited", "", "W"],
            vec!["TJ", "Junction Temperature", "-40 ~ 150", "oC"],
            vec!["TSTG", "Storage Temperature", "-65 ~ 150", "oC"],
            vec![
                "TSDR",
                "Maximum Lead Soldering Temperature, 10 Seconds",
                "260",
                "oC"
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_bullet_leader_spec_tables() {
    let artifact = parse_extracted_pages(
        "doc-bullet-leader-spec-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "FP6183\n",
                "Absolute Maximum Ratings\n",
                "(Note 2)\n",
                "● Input Voltage VIN ------------------------------------------------------------------------------------------- -0.3V to +6.5V\n",
                "● Output Voltage VOUT -------------------------------------------------------------------------------------- -0.3V to +6.5V\n",
                "● EN Voltage VEN -------------------------------------------------------------------------------------------- -0.3V to VIN +0.3V\n",
                "● Power Dissipation @ TA=25°C & TJ=125°C (PD)\n",
                "UTDFN-4L (1.0mmx1.0mm) ---------------------------------------------------------------- 0.5W\n",
                "● Package Thermal Resistance (θJA)\n",
                "(Note 3)\n",
                " \n",
                "UTDFN-4L (1.0mmx1.0mm) ---------------------------------------------------------------- 195°C/W\n",
                "● Package Thermal Resistance (θJC)\n",
                "UTDFN-4L (1.0mmx1.0mm) ---------------------------------------------------------------- 65°C/W\n",
                "● Lead Temperature (Soldering, 10sec.) -------------------------------------------------------------- +260°C\n",
                "● Junction Temperature (TJ) ------------------------------------------------------------------------------ -40°C to +150°C\n",
                "● Storage Temperature (TSTG) ---------------------------------------------------------------------------- -65°C to +150°C\n",
                "Note 2: Stresses beyond this listed under Absolute Maximum Ratings may cause permanent damage.\n",
                "Recommended Operating Conditions"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.46,
                native_span_count: 18,
                native_text_bytes: 1040,
                glyph_count: 890,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("bullet leader spec table block");
    assert!(!table_block.text.contains("Note 2:"));
    assert!(
        !table_block
            .text
            .contains("Recommended Operating Conditions")
    );

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Limit"],
            vec!["Input Voltage VIN", "-0.3V to +6.5V"],
            vec!["Output Voltage VOUT", "-0.3V to +6.5V"],
            vec!["EN Voltage VEN", "-0.3V to VIN +0.3V"],
            vec![
                "Power Dissipation @ TA=25°C & TJ=125°C (PD) UTDFN-4L (1.0mmx1.0mm)",
                "0.5W"
            ],
            vec![
                "Package Thermal Resistance (θJA) (Note 3) UTDFN-4L (1.0mmx1.0mm)",
                "195°C/W"
            ],
            vec![
                "Package Thermal Resistance (θJC) UTDFN-4L (1.0mmx1.0mm)",
                "65°C/W"
            ],
            vec!["Lead Temperature (Soldering, 10sec.)", "+260°C"],
            vec!["Junction Temperature (TJ)", "-40°C to +150°C"],
            vec!["Storage Temperature (TSTG)", "-65°C to +150°C"],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_electrical_characteristics_tables() {
    let artifact = parse_extracted_pages(
        "doc-electrical-characteristics-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "APL5324\n",
                "Electrical Characteristics\n",
                "Unless otherwise specified, these specifications apply over VIN = VOUT+1V.\n",
                "APL5324\n",
                "Symbol Parameter Test Conditions\n",
                "Min. Typ. Max.\n",
                "Unit\n",
                "VIN Input Voltage 2.7 - 6 V\n",
                "VOUT Output Voltage Range 0.8 - 5.5 V\n",
                "IQ Quiescent Current IOUT =10mA ~300mA - 135 160 mA\n",
                "VREF Reference Voltage Measured on SET, VIN=3V, IOUT=10mA - 0.8 - V\n",
                " Output Voltage Accuracy IOUT=10mA -2 - +2 %\n",
                "REGLINE Line Regulation DVOUT%/DVIN, IOUT=10mA -0.06 - +0.06 %/V\n",
                "REGLOAD Load Regulation DVOUT%/DIOUT -0.2 - +0.2 %/A\n",
                "VOUT = 2.5V, IOUT = 300mA - 500 650\n",
                "VDROP Dropout Voltage\n",
                "VOUT = 3.3V, IOUT = 300mA - 300 400\n",
                "mV\n",
                "PSRR Power Supply Ripple Rejection Ratio f = 10kHz, IOUT = 300mA - 45 - dB\n",
                " Noise f = 80Hz to 100kHz, IOUT = 300mA - 160 - mVRMS\n",
                "ILIMIT Current Limit 450 550 - mA\n",
                "ISHORT Foldback Current VOUT = 0V - 80 - mA\n",
                "SHDN Input Voltage High 1.6 - -\n",
                " \n",
                "SHDN Input Voltage Low\n",
                " \n",
                "- - 0.4\n",
                "V\n",
                " VOUT Discharge MOSFET RDS(ON) SHDN = Low - 60 - W\n",
                " Shutdown VIN Supply Current SHDN = Low, VIN = 6V - 0.1 1 mA\n",
                " \n",
                "SHDN Pull Low Resistance - 3 - MW\n",
                " Over Temperature Threshold - 160 -\n",
                "oC\n",
                " Over Temperature Hysteresis - 40 -\n",
                "oC\n",
                " SET Input Bias Current VSET=0.8V -100 - 100 nA\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.52,
                native_span_count: 40,
                native_text_bytes: 1800,
                glyph_count: 1400,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("electrical characteristics table block");
    assert!(!table_block.text.contains("Unless otherwise specified"));

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "Symbol",
                "Parameter",
                "Test Conditions",
                "Min.",
                "Typ.",
                "Max.",
                "Unit"
            ],
            vec!["VIN", "Input Voltage", "", "2.7", "-", "6", "V"],
            vec!["VOUT", "Output Voltage Range", "", "0.8", "-", "5.5", "V"],
            vec![
                "IQ",
                "Quiescent Current",
                "IOUT =10mA ~300mA",
                "-",
                "135",
                "160",
                "mA"
            ],
            vec![
                "VREF",
                "Reference Voltage",
                "Measured on SET, VIN=3V, IOUT=10mA",
                "-",
                "0.8",
                "-",
                "V"
            ],
            vec![
                "",
                "Output Voltage Accuracy",
                "IOUT=10mA",
                "-2",
                "-",
                "+2",
                "%"
            ],
            vec![
                "REGLINE",
                "Line Regulation",
                "DVOUT%/DVIN, IOUT=10mA",
                "-0.06",
                "-",
                "+0.06",
                "%/V"
            ],
            vec![
                "REGLOAD",
                "Load Regulation",
                "DVOUT%/DIOUT",
                "-0.2",
                "-",
                "+0.2",
                "%/A"
            ],
            vec![
                "VDROP",
                "Dropout Voltage",
                "VOUT = 2.5V, IOUT = 300mA",
                "-",
                "500",
                "650",
                "mV"
            ],
            vec!["", "", "VOUT = 3.3V, IOUT = 300mA", "-", "300", "400", "mV"],
            vec![
                "PSRR",
                "Power Supply Ripple Rejection Ratio",
                "f = 10kHz, IOUT = 300mA",
                "-",
                "45",
                "-",
                "dB"
            ],
            vec![
                "",
                "Noise",
                "f = 80Hz to 100kHz, IOUT = 300mA",
                "-",
                "160",
                "-",
                "mVRMS"
            ],
            vec!["ILIMIT", "Current Limit", "", "450", "550", "-", "mA"],
            vec![
                "ISHORT",
                "Foldback Current",
                "VOUT = 0V",
                "-",
                "80",
                "-",
                "mA"
            ],
            vec!["", "SHDN Input Voltage High", "", "1.6", "-", "-", "V"],
            vec!["", "SHDN Input Voltage Low", "", "-", "-", "0.4", "V"],
            vec![
                "",
                "VOUT Discharge MOSFET RDS(ON)",
                "SHDN = Low",
                "-",
                "60",
                "-",
                "W"
            ],
            vec![
                "",
                "Shutdown VIN Supply Current",
                "SHDN = Low, VIN = 6V",
                "-",
                "0.1",
                "1",
                "mA"
            ],
            vec!["", "SHDN Pull Low Resistance", "", "-", "3", "-", "MW"],
            vec!["", "Over Temperature Threshold", "", "-", "160", "-", "oC"],
            vec!["", "Over Temperature Hysteresis", "", "-", "40", "-", "oC"],
            vec![
                "",
                "SET Input Bias Current",
                "VSET=0.8V",
                "-100",
                "-",
                "100",
                "nA"
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_parameter_symbol_conditions_tables() {
    let artifact = parse_extracted_pages(
        "doc-parameter-symbol-conditions-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "FP6183\n",
                "Electrical Characteristics\n",
                "(VIN=VOUT+1V,EN pin connected to VIN, CIN=1µF, COUT=1µF, TA=25ºC, unless otherwise specified.)\n",
                "Parameter Symbol Conditions Min Typ Max Unit\n",
                "Input Voltage Range VIN 1.75 5.5 V\n",
                "Quiescent Current\n",
                "(Note 4)\n",
                "IQ IOUT=0A 2 4 µA\n",
                "Standby Current ISTBY EN Pin Connected to GND 0.1 1 µA\n",
                "Output Voltage Accuracy VOUT IOUT=1mA -1 +1 %\n",
                "Dropout Voltage (Note 5) VDROP IOUT=300mA\n",
                "VOUT=1.0V 650 850\n",
                "mV\n",
                "VOUT=1.05V 590 770\n",
                "Line Regulation VLINE IOUT=1mA, VIN=VOUT +1V to 5V 1 8 mV\n",
                "Ripple Rejection (Note 7) PSRR\n",
                "VIN=VOUT+1VDC+0.2VP-P(AC),\n",
                "fRIPPLE=1KHz,VOUT=1.2V,\n",
                "IOUT=30mA\n",
                "65 dB\n",
                "Output Noise Voltage (Note 7) VNOISE\n",
                "COUT=1μF, IOUT=30mA\n",
                "BW=10Hz ~ 100KHz\n",
                "65 μVRMS\n",
                "Current Foldback ICFB RLoad=1Ω 100 mA\n",
                "Thermal Shutdown Threshold\n",
                "(Note 7)\n",
                "TSD 160 ºC\n",
                "Thermal Shutdown Threshold\n",
                "Hysteresis (Note 7)\n",
                "TSD 30 ºC\n",
                "EN Pin Threshold\n",
                "VEN(ON) Start-up 1.0 V\n",
                "VEN(OFF) Shutdown 0.4 V\n",
                "Note 4: except EN pull down current (IEN).\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.50,
                native_span_count: 48,
                native_text_bytes: 1650,
                glyph_count: 1250,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("parameter/symbol electrical characteristics table block");
    assert!(!table_block.text.contains("Note 4:"));

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "Parameter",
                "Symbol",
                "Conditions",
                "Min.",
                "Typ.",
                "Max.",
                "Unit"
            ],
            vec!["Input Voltage Range", "VIN", "", "1.75", "", "5.5", "V"],
            vec![
                "Quiescent Current (Note 4)",
                "IQ",
                "IOUT=0A",
                "",
                "2",
                "4",
                "µA"
            ],
            vec![
                "Standby Current",
                "ISTBY",
                "EN Pin Connected to GND",
                "",
                "0.1",
                "1",
                "µA"
            ],
            vec![
                "Output Voltage Accuracy",
                "VOUT",
                "IOUT=1mA",
                "-1",
                "",
                "+1",
                "%"
            ],
            vec![
                "Dropout Voltage (Note 5)",
                "VDROP",
                "IOUT=300mA VOUT=1.0V",
                "",
                "650",
                "850",
                "mV"
            ],
            vec!["", "", "IOUT=300mA VOUT=1.05V", "", "590", "770", "mV"],
            vec![
                "Line Regulation",
                "VLINE",
                "IOUT=1mA, VIN=VOUT +1V to 5V",
                "",
                "1",
                "8",
                "mV"
            ],
            vec![
                "Ripple Rejection (Note 7)",
                "PSRR",
                "VIN=VOUT+1VDC+0.2VP-P(AC), fRIPPLE=1KHz,VOUT=1.2V, IOUT=30mA",
                "",
                "65",
                "",
                "dB"
            ],
            vec![
                "Output Noise Voltage (Note 7)",
                "VNOISE",
                "COUT=1μF, IOUT=30mA BW=10Hz ~ 100KHz",
                "",
                "65",
                "",
                "μVRMS"
            ],
            vec!["Current Foldback", "ICFB", "RLoad=1Ω", "", "100", "", "mA"],
            vec![
                "Thermal Shutdown Threshold (Note 7)",
                "TSD",
                "",
                "",
                "160",
                "",
                "ºC"
            ],
            vec![
                "Thermal Shutdown Threshold Hysteresis (Note 7)",
                "TSD",
                "",
                "",
                "30",
                "",
                "ºC"
            ],
            vec![
                "EN Pin Threshold",
                "VEN(ON)",
                "Start-up",
                "",
                "1.0",
                "",
                "V"
            ],
            vec!["", "VEN(OFF)", "Shutdown", "", "0.4", "", "V"],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_awinic_parameter_test_condition_tables() {
    let artifact = parse_extracted_pages(
        "doc-awinic-electrical-characteristics-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Electrical Characteristics\n",
                "VIN=VOUT(SET)+1V, VCE>1V, IOUT=1mA, CIN=COUT=1µF, TA=25°C\n",
                "PARAMETER TEST CONDITION MIN TYP MAX UNIT\n",
                "VIN Input Voltage Range 1.4 5.5 V\n",
                "VOUT_ACC\n",
                "Output Voltage\n",
                "Accuracy\n",
                "TA=25°C -1.3 1.3\n",
                "%\n",
                "-40°C ≤TA≤85°C -2 2\n",
                "LOADReg Load Regulation 1mA≤IOUT≤300mA 1 40 mV\n",
                "LINEReg Line Regulation VOUT(SET)+0.5V≤VIN ≤5.5V 1 5 mV\n",
                "Vdropout Dropout Voltage IOUT=300mA\n",
                "VOUT(SET)=1.8V 310\n",
                "mV\n",
                "VOUT(SET)=3.3V 158\n",
                "ISD Shutdown Current VCE<0.4V 0.1 1 A\n",
                "IQ Quiescent Current IOUT=0mA 50 80 A\n",
                "PSRR\n",
                "Power Supply Ripple\n",
                "Rejection\n",
                "IOUT=30mA, f=1kHz\n",
                "VOUT(SET)=1.8V\n",
                "90 dB\n",
                "VN Output Voltage Noise\n",
                "IOUT=30mA\n",
                "BW=10Hz to\n",
                "100kHz\n",
                "VOUT(SET)=1.8V 33\n",
                "Vrms\n",
                "VOUT(SET)=3.3V 46\n",
                "ICL Output Current Limit VOUT=90%*VOUT(SET) 300 mA\n",
                "ISC Short Current Limit VOUT<10%*VOUT(SET) 120 mA\n",
                "VTC\n",
                "Output Voltage\n",
                "Temperature\n",
                "Coefficient\n",
                "-40°C ≤TA≤85°C ±40\n",
                "ppm/°\n",
                "C\n",
                "RDISC\n",
                "Auto Discharge\n",
                "Resistance\n",
                "VIN=4V, VCE<0.4V, VOUT=2.8V 130 Ω\n",
                "ICE\n",
                "CE Pull Down\n",
                "Current\n",
                "140 nA\n",
                "TSDH\n",
                "Thermal Shutdown\n",
                "Threshold\n",
                "Temperature Rising 150 °C\n",
                "TSDL\n",
                "Thermal Shutdown\n",
                "Reset Threshold\n",
                "Temperature Falling 130 °C\n",
                "awinic Confidential\n",
                "Typical Characteristics\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.50,
                native_span_count: 44,
                native_text_bytes: 940,
                glyph_count: 720,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("AWINIC electrical characteristics table block");
    assert!(!table_block.text.contains("Electrical Characteristics"));
    assert!(!table_block.text.contains("awinic Confidential"));
    assert!(!table_block.text.contains("Typical Characteristics"));

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "Parameter",
                "Test Condition",
                "Min.",
                "Typ.",
                "Max.",
                "Unit"
            ],
            vec!["VIN Input Voltage Range", "", "1.4", "", "5.5", "V"],
            vec![
                "VOUT_ACC Output Voltage Accuracy",
                "TA=25°C",
                "-1.3",
                "",
                "1.3",
                "%"
            ],
            vec!["", "-40°C ≤TA≤85°C", "-2", "", "2", "%"],
            vec![
                "LOADReg Load Regulation",
                "1mA≤IOUT≤300mA",
                "",
                "1",
                "40",
                "mV"
            ],
            vec![
                "LINEReg Line Regulation",
                "VOUT(SET)+0.5V≤VIN ≤5.5V",
                "",
                "1",
                "5",
                "mV"
            ],
            vec![
                "Vdropout Dropout Voltage",
                "IOUT=300mA VOUT(SET)=1.8V",
                "",
                "310",
                "",
                "mV"
            ],
            vec!["", "IOUT=300mA VOUT(SET)=3.3V", "", "158", "", "mV"],
            vec!["ISD Shutdown Current", "VCE<0.4V", "", "0.1", "1", "A"],
            vec!["IQ Quiescent Current", "IOUT=0mA", "", "50", "80", "A"],
            vec![
                "PSRR Power Supply Ripple Rejection",
                "IOUT=30mA, f=1kHz VOUT(SET)=1.8V",
                "",
                "90",
                "",
                "dB"
            ],
            vec![
                "VN Output Voltage Noise",
                "IOUT=30mA BW=10Hz to 100kHz VOUT(SET)=1.8V",
                "",
                "33",
                "",
                "Vrms"
            ],
            vec![
                "",
                "IOUT=30mA BW=10Hz to 100kHz VOUT(SET)=3.3V",
                "",
                "46",
                "",
                "Vrms"
            ],
            vec![
                "ICL Output Current Limit",
                "VOUT=90%*VOUT(SET)",
                "",
                "300",
                "",
                "mA"
            ],
            vec![
                "ISC Short Current Limit",
                "VOUT<10%*VOUT(SET)",
                "",
                "120",
                "",
                "mA"
            ],
            vec![
                "VTC Output Voltage Temperature Coefficient",
                "-40°C ≤TA≤85°C",
                "",
                "±40",
                "",
                "ppm/°C"
            ],
            vec![
                "RDISC Auto Discharge Resistance",
                "VIN=4V, VCE<0.4V, VOUT=2.8V",
                "",
                "130",
                "",
                "Ω"
            ],
            vec!["ICE CE Pull Down Current", "", "", "140", "", "nA"],
            vec![
                "TSDH Thermal Shutdown Threshold",
                "Temperature Rising",
                "",
                "150",
                "",
                "°C"
            ],
            vec![
                "TSDL Thermal Shutdown Reset Threshold",
                "Temperature Falling",
                "",
                "130",
                "",
                "°C"
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_reflow_profile_tables() {
    let artifact = parse_extracted_pages(
        "doc-reflow-profile-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "APL5324\n",
                "Classification Reflow Profiles\n",
                "Profile Feature Sn-Pb Eutectic Assembly Pb-Free Assembly\n",
                "Preheat & Soak\n",
                "Temperature min (Tsmin)\n",
                "Temperature max (Tsmax)\n",
                "Time (Tsmin to Tsmax) (ts)\n",
                "100 °C\n",
                "150 °C\n",
                "60-120 seconds\n",
                "150 °C\n",
                "200 °C\n",
                "60-120 seconds\n",
                "Average ramp-up rate\n",
                "(Tsmax to TP)\n",
                "3 °C/second max. 3°C/second max.\n",
                "Liquidous temperature (TL)\n",
                "Time at liquidous (tL)\n",
                "183 °C\n",
                "60-150 seconds\n",
                "217 °C\n",
                "60-150 seconds\n",
                "Peak package body Temperature\n",
                "(Tp)*\n",
                "See Classification Temp in table 1 See Classification Temp in table 2\n",
                "Time (tP)** within 5°C of the specified\n",
                "classification temperature (Tc)\n",
                "20** seconds 30** seconds\n",
                "Average ramp-down rate (Tp to Tsmax) 6 °C/second max. 6 °C/second max.\n",
                "Time 25°C to peak temperature 6 minutes max. 8 minutes max.\n",
                "* Tolerance for peak profile Temperature (Tp) is defined as a supplier minimum and a user maximum.\n",
                "** Tolerance for time at peak profile temperature (tp) is defined as a supplier minimum and a user maximum.\n",
                "Table 1. SnPb Eutectic Process – Classification Temperatures (Tc)"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.50,
                native_span_count: 36,
                native_text_bytes: 1350,
                glyph_count: 1050,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("reflow profile table block");
    assert!(!table_block.text.contains("* Tolerance"));
    assert!(!table_block.text.contains("Table 1."));

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "Profile Feature",
                "Sn-Pb Eutectic Assembly",
                "Pb-Free Assembly"
            ],
            vec!["Preheat & Soak", "", ""],
            vec!["Temperature min (Tsmin)", "100 °C", "150 °C"],
            vec!["Temperature max (Tsmax)", "150 °C", "200 °C"],
            vec![
                "Time (Tsmin to Tsmax) (ts)",
                "60-120 seconds",
                "60-120 seconds"
            ],
            vec![
                "Average ramp-up rate (Tsmax to TP)",
                "3 °C/second max.",
                "3°C/second max."
            ],
            vec!["Liquidous temperature (TL)", "183 °C", "217 °C"],
            vec!["Time at liquidous (tL)", "60-150 seconds", "60-150 seconds"],
            vec![
                "Peak package body Temperature (Tp)*",
                "See Classification Temp in table 1",
                "See Classification Temp in table 2"
            ],
            vec![
                "Time (tP)** within 5°C of the specified classification temperature (Tc)",
                "20** seconds",
                "30** seconds"
            ],
            vec![
                "Average ramp-down rate (Tp to Tsmax)",
                "6 °C/second max.",
                "6 °C/second max."
            ],
            vec![
                "Time 25°C to peak temperature",
                "6 minutes max.",
                "8 minutes max."
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_classification_temperature_tables() {
    let artifact = parse_extracted_pages(
        "doc-classification-temperature-tables".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Table 1. SnPb Eutectic Process – Classification Temperatures (Tc)\n",
                "Package\n",
                "Thickness\n",
                "Volume mm3\n",
                "<350\n",
                "Volume mm3\n",
                "³350\n",
                "<2.5 mm 235 °C 220 °C\n",
                "³2.5 mm 220 °C 220 °C\n",
                "\n",
                "Table 2. Pb-free Process – Classification Temperatures (Tc)\n",
                "Package\n",
                "Thickness\n",
                "Volume mm3 \n",
                "<350\n",
                "Volume mm3 \n",
                "350-2000\n",
                "Volume mm3 \n",
                ">2000\n",
                "<1.6 mm 260 °C 260 °C 260 °C\n",
                "1.6 mm – 2.5 mm 260 °C 250 °C 245 °C\n",
                "³2.5 mm 250 °C 245 °C 245 °C\n",
                "\n",
                "Reliability Test Program\n",
                "Test item Method Description\n",
                "SOLDERABILITY JESD-22, B102 5 Sec, 245°C"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.50,
                native_span_count: 22,
                native_text_bytes: 760,
                glyph_count: 640,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_rows = page
        .layout_blocks
        .iter()
        .filter(|block| block.kind == LayoutBlockKind::Table)
        .map(|block| {
            assert!(!block.text.contains("Table 1."));
            assert!(!block.text.contains("Table 2."));
            assert!(!block.text.contains("Reliability Test Program"));
            block
                .table
                .as_ref()
                .expect("table payload")
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| cell.text.as_str())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        table_rows,
        vec![
            vec![
                vec!["Package Thickness", "Volume mm3 <350", "Volume mm3 ³350"],
                vec!["<2.5 mm", "235 °C", "220 °C"],
                vec!["³2.5 mm", "220 °C", "220 °C"],
            ],
            vec![
                vec![
                    "Package Thickness",
                    "Volume mm3 <350",
                    "Volume mm3 350-2000",
                    "Volume mm3 >2000"
                ],
                vec!["<1.6 mm", "260 °C", "260 °C", "260 °C"],
                vec!["1.6 mm – 2.5 mm", "260 °C", "250 °C", "245 °C"],
                vec!["³2.5 mm", "250 °C", "245 °C", "245 °C"],
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_package_pin_description_tables() {
    let artifact = parse_extracted_pages(
        "doc-package-pin-description-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "AP7354\n",
                "Pin Description\n",
                "Pin Number\n",
                "Pin Name Function\n",
                "SOT25 SOT23\n",
                "X2-DFN1010-4\n",
                "(Type B)\n",
                "3 — 3 EN\n",
                "Chip Enable — This should be driven either high or low and must not be floating.\n",
                "Driving EN high enables regulator output, while pulling it low places regulator into\n",
                "shutdown mode.\n",
                "2 3 2 GND Ground\n",
                "5 2 1 VOUT Output Voltage\n",
                "1 1 4 VIN Power Input\n",
                "— — Center Pad — No connection or ground. Note: Chip Ground must be through GND pin.\n",
                "Functional Block Diagram\n",
                "VIN\n",
                "EN GND"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 16,
                native_text_bytes: 720,
                glyph_count: 560,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    let table_block = page
        .layout_blocks
        .iter()
        .find(|block| block.kind == LayoutBlockKind::Table)
        .expect("package pin description table block");
    assert!(!table_block.text.contains("Functional Block Diagram"));

    let table = table_block.table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "SOT25",
                "SOT23",
                "X2-DFN1010-4 (Type B)",
                "Pin Name",
                "Function"
            ],
            vec![
                "3",
                "—",
                "3",
                "EN",
                "Chip Enable — This should be driven either high or low and must not be floating. Driving EN high enables regulator output, while pulling it low places regulator into shutdown mode."
            ],
            vec!["2", "3", "2", "GND", "Ground"],
            vec!["5", "2", "1", "VOUT", "Output Voltage"],
            vec!["1", "1", "4", "VIN", "Power Input"],
            vec![
                "—",
                "—",
                "Center Pad",
                "—",
                "No connection or ground. Note: Chip Ground must be through GND pin."
            ],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_part_number_ordering_tables() {
    let artifact = parse_extracted_pages(
        "doc-part-number-ordering-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Part Number VOUT Package Identification Code\n",
                "AP7354-11FS4-7 1.1V X2-DFN1010-4 (Type B) A8M\n",
                "AP7354-12FS4-7 1.2V X2-DFN1010-4 (Type B) A8A\n",
                "AP7354D-33FS4-7 3.3V X2-DFN1010-4 (Type B) A9H\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 4,
                native_text_bytes: 230,
                glyph_count: 185,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);

    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Part Number", "VOUT", "Package", "Identification Code"],
            vec!["AP7354-11FS4-7", "1.1V", "X2-DFN1010-4 (Type B)", "A8M"],
            vec!["AP7354-12FS4-7", "1.2V", "X2-DFN1010-4 (Type B)", "A8A"],
            vec!["AP7354D-33FS4-7", "3.3V", "X2-DFN1010-4 (Type B)", "A9H"],
        ]
    );
}

#[test]
fn text_table_recovery_merges_two_column_descriptor_value_rows() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-two-column-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter Max\n",
                "Input voltage 5.5\n",
                "Quiescent current 60"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 3,
                native_text_bytes: 58,
                glyph_count: 45,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Max"],
            vec!["Input voltage", "5.5"],
            vec!["Quiescent current", "60"],
        ]
    );
}

#[test]
fn text_table_recovery_merges_wrapped_descriptor_lines_from_header_columns() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-wrapped-text-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter Symbol Typ Max Unit\n",
                "Input\n",
                "voltage VIN 3.3 5.5 V\n",
                "Quiescent\n",
                "current IQ 35 60 uA"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 5,
                native_text_bytes: 92,
                glyph_count: 72,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Quiescent current", "IQ", "35", "60", "uA"],
        ]
    );
}

#[test]
fn text_table_recovery_preserves_header_guided_section_rows() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-section-row".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter Symbol Typ Max Unit\n",
                "Input voltage VIN 3.3 5.5 V\n",
                "Protection features\n",
                "Current limit ILIM 650 900 mA\n",
                "Thermal shutdown TSD 150 175 C"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 5,
                native_text_bytes: 156,
                glyph_count: 124,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Protection features", "", "", "", ""],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
            vec!["Thermal shutdown", "TSD", "150", "175", "C"],
        ]
    );
}

#[test]
fn text_table_recovery_merges_trailing_descriptor_continuations_from_header_columns() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-trailing-continuation".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter Symbol Typ Max Unit\n",
                "Output voltage VOUT 3.3 5.5 V\n",
                "accuracy over load\n",
                "Quiescent current IQ 35 60 uA"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 4,
                native_text_bytes: 114,
                glyph_count: 91,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec![
                "Output voltage accuracy over load",
                "VOUT",
                "3.3",
                "5.5",
                "V"
            ],
            vec!["Quiescent current", "IQ", "35", "60", "uA"],
        ]
    );
}

#[test]
fn text_table_recovery_preserves_trailing_blank_cells_from_header_columns() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-trailing-blank".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Parameter Symbol Typ Max Unit\n",
                "Input voltage VIN 3.3 5.5 V\n",
                "Shutdown current ISD 0.1 1\n",
                "Current limit ILIM 650 900 mA"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 4,
                native_text_bytes: 123,
                glyph_count: 99,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec!["Parameter", "Symbol", "Typ", "Max", "Unit"],
            vec!["Input voltage", "VIN", "3.3", "5.5", "V"],
            vec!["Shutdown current", "ISD", "0.1", "1", ""],
            vec!["Current limit", "ILIM", "650", "900", "mA"],
        ]
    );
}

#[test]
fn text_table_recovery_extracts_budget_projection_rows() {
    let artifact = parse_extracted_pages(
        "doc-budget-projection-table".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "Account and Subfunction Code\n",
                "Actual 2026 2027 2028\n",
                "2025 Estimate\n",
                "TABLE 16-1. FEDERAL BUDGET BY AGENCY AND ACCOUNT, FY2027 PRESIDENT'S BUDGET POLICY\n",
                "(In millions of dollars)\n",
                "Legislative Branch\n",
                "Senate\n",
                "Federal Funds\n",
                "Compensation of Members, Senate (001-05-0100):\n",
                "Appropriations, mandatory 801 BA 25 25 25 25\n",
                "Outlays, mandatory O 24 28 25 25\n",
                "Page 2 / 516"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 12,
                native_text_bytes: 420,
                glyph_count: 360,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 2);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Table);
    assert_eq!(page.layout_blocks[1].kind, LayoutBlockKind::Paragraph);
    assert_eq!(page.layout_blocks[1].text, "Page 2 / 516");

    let table = page.layout_blocks[0].table.as_ref().expect("table payload");
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            vec![
                "Account and Subfunction",
                "Code",
                "Type",
                "Actual 2025",
                "2026 Estimate",
                "2027",
                "2028",
            ],
            vec![
                "TABLE 16-1. FEDERAL BUDGET BY AGENCY AND ACCOUNT, FY2027 PRESIDENT'S BUDGET POLICY",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
            vec!["(In millions of dollars)", "", "", "", "", "", ""],
            vec!["Legislative Branch", "", "", "", "", "", ""],
            vec!["Senate", "", "", "", "", "", ""],
            vec!["Federal Funds", "", "", "", "", "", ""],
            vec![
                "Compensation of Members, Senate (001-05-0100):",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
            vec![
                "Appropriations, mandatory",
                "801",
                "BA",
                "25",
                "25",
                "25",
                "25"
            ],
            vec!["Outlays, mandatory", "", "O", "24", "28", "25", "25"],
        ]
    );
}

#[test]
fn text_table_recovery_does_not_treat_wrapped_prose_as_header_guided_table() {
    let artifact = parse_extracted_pages(
        "doc-header-guided-prose".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "The quick brown\n",
                "fox jumps over the lazy\n",
                "dog keeps running nearby"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 3,
                native_text_bytes: 72,
                glyph_count: 58,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Paragraph);
    assert!(page.layout_blocks[0].table.is_none());
}

#[test]
fn text_table_recovery_does_not_treat_datasheet_description_prose_as_table() {
    let artifact = parse_extracted_pages(
        "doc-datasheet-description-prose".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "General Description\n",
                "AW37030YXXX is a low dropout voltage regulator\n",
                "featuring low ON resistance, high PSRR, low Noise,\n",
                "good load/line transient response and smooth soft-start.\n"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 4,
                native_text_bytes: 180,
                glyph_count: 150,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert!(
        page.layout_blocks
            .iter()
            .all(|block| block.kind != LayoutBlockKind::Table)
    );
    assert!(page.layout_blocks.iter().all(|block| block.table.is_none()));
}

#[test]
fn positioned_bullet_list_rows_are_not_recovered_as_tables() {
    let artifact = parse_extracted_pages(
        "doc-positioned-bullet-list".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "·\n",
                "Portable and Battery\n",
                "Powered Equipment\n",
                "·\n",
                "Notebook and Personal Computers"
            )
            .to_string(),
            native_spans: vec![
                span("·", 72.0, 456.0, 78.0, 467.0),
                span("Portable and Battery", 96.0, 458.0, 178.0, 466.0),
                span("Powered Equipment", 178.0, 458.0, 272.0, 467.0),
                span("·", 72.0, 474.0, 78.0, 485.0),
                span("Notebook and Personal Computers", 96.0, 476.0, 244.0, 485.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 5,
                native_text_bytes: 91,
                glyph_count: 83,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::List);
    assert_eq!(
        page.layout_blocks[0].text,
        "· Portable and Battery Powered Equipment\n· Notebook and Personal Computers"
    );
    assert!(page.layout_blocks[0].table.is_none());
}

#[test]
fn positioned_bullet_marker_rows_absorb_following_text_rows() {
    let artifact = parse_extracted_pages(
        "doc-positioned-bullet-list-split-markers".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "·\n",
                "Cellular Phones\n",
                "·\n",
                "Portable and Battery\n",
                "Powered Equipment"
            )
            .to_string(),
            native_spans: vec![
                span("·", 72.0, 100.0, 78.0, 111.0),
                span("Cellular Phones", 96.0, 116.0, 180.0, 124.0),
                span("·", 72.0, 138.0, 78.0, 149.0),
                span("Portable and Battery", 96.0, 154.0, 178.0, 162.0),
                span("Powered Equipment", 96.0, 166.0, 190.0, 175.0),
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 5,
                native_text_bytes: 70,
                glyph_count: 62,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::List);
    assert_eq!(
        page.layout_blocks[0].text,
        "· Cellular Phones\n· Portable and Battery Powered Equipment"
    );
    assert!(page.layout_blocks[0].table.is_none());
}

#[test]
fn marker_only_list_lines_are_normalized_into_list_items() {
    let artifact = parse_extracted_pages(
        "doc-marker-only-list-lines".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "·\n",
                "Cellular Phones\n",
                "·\n",
                "Portable and Battery\n",
                "Powered Equipment\n",
                "·\n",
                "Notebook and Personal Computers"
            )
            .to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                table_line_density: 0.42,
                native_span_count: 1,
                native_text_bytes: 110,
                glyph_count: 98,
                ..native_signals(0)
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::List);
    assert_eq!(
        page.layout_blocks[0].text,
        "· Cellular Phones\n· Portable and Battery Powered Equipment\n· Notebook and Personal Computers"
    );
    assert!(page.layout_blocks[0].table.is_none());
}

#[test]
fn repeated_margin_blocks_are_classified_as_headers_and_footers() {
    let artifact = parse_extracted_pages(
        "doc-repeated-margins".to_string(),
        vec![
            ExtractedPage {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_text: "DATASHEET HEADER\nFirst page body\nCONFIDENTIAL FOOTER".to_string(),
                native_spans: vec![
                    span("DATASHEET HEADER", 72.0, 24.0, 240.0, 38.0),
                    span("First page body", 72.0, 120.0, 260.0, 134.0),
                    span("CONFIDENTIAL FOOTER", 72.0, 754.0, 260.0, 768.0),
                ],
                image_artifacts: Vec::new(),
                signals: native_signals(0),
                ocr_text: None,
                timings: PageTimings::default(),
            },
            ExtractedPage {
                page_index: 1,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_text: "DATASHEET HEADER\nSecond page body\nCONFIDENTIAL FOOTER".to_string(),
                native_spans: vec![
                    span("DATASHEET HEADER", 72.0, 24.0, 240.0, 38.0),
                    span("Second page body", 72.0, 120.0, 280.0, 134.0),
                    span("CONFIDENTIAL FOOTER", 72.0, 754.0, 260.0, 768.0),
                ],
                image_artifacts: Vec::new(),
                signals: native_signals(1),
                ocr_text: None,
                timings: PageTimings::default(),
            },
        ],
    );

    for page in &artifact.pages {
        assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Header);
        assert_eq!(page.layout_blocks[2].kind, LayoutBlockKind::Footer);
        assert_eq!(page.layout_blocks[0].text, "DATASHEET HEADER");
        assert_eq!(page.layout_blocks[2].text, "CONFIDENTIAL FOOTER");
    }
    assert_eq!(
        artifact.pages[0].layout_blocks[1].kind,
        LayoutBlockKind::Paragraph
    );
    assert_eq!(
        artifact.pages[1].layout_blocks[1].kind,
        LayoutBlockKind::Paragraph
    );
}

fn span(text: &str, x0: f32, y0: f32, x1: f32, y1: f32) -> ExtractedTextSpan {
    ExtractedTextSpan {
        text: text.to_string(),
        bbox: BBox { x0, y0, x1, y1 },
    }
}

fn native_signals(page_index: u32) -> PageSignals {
    PageSignals {
        page_index,
        dimensions: PageDimensions::new(612.0, 792.0),
        native_span_count: 4,
        native_text_bytes: 120,
        glyph_count: 100,
        image_area_ratio: 0.0,
        duplicate_char_ratio: 0.0,
        bbox_overlap_ratio: 0.0,
        broken_encoding_ratio: 0.0,
        rotation_degrees: 0,
        table_line_density: 0.0,
        annotation_count: 0,
        form_field_count: 0,
        huge_object_count: 0,
        span_geometry_capped: false,
    }
}
