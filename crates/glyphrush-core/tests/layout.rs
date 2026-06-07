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
