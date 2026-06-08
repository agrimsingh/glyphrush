use glyphrush_core::{
    BBox, ExtractedImage, ExtractedPage, ExtractedTextSpan, LayoutBlockKind, PageDimensions,
    PageQuality, PageRoute, PageSignals, PageTimings, SpanProvenance, parse_extracted_pages,
};

#[test]
fn native_text_input_becomes_a_native_span_with_fast_path_quality() {
    let artifact = parse_extracted_pages(
        "doc-native".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Glyphrush reads native text first.".to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 1,
                native_text_bytes: 35,
                glyph_count: 35,
                image_area_ratio: 0.01,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings {
                native_extract_us: 123,
                ..PageTimings::default()
            },
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.native_spans.len(), 1);
    assert_eq!(
        page.native_spans[0].text,
        "Glyphrush reads native text first."
    );
    assert_eq!(page.native_spans[0].provenance, SpanProvenance::Native);
    assert!(page.ocr_spans.is_empty());
    assert_eq!(page.route.route, PageRoute::NativeFastPath);
    assert!(page.quality.flags.is_empty());
    assert_eq!(page.timings.native_extract_us, 123);
    assert!(page.timings.classify_us > 0);
    assert_eq!(artifact.global_diagnostics.fallback_pages, 0);
    assert!(artifact.global_diagnostics.total_stage_time_us > 0);
}

#[test]
fn extracted_page_signals_are_preserved_in_page_artifact() {
    let artifact = parse_extracted_pages(
        "doc-signals".to_string(),
        vec![ExtractedPage {
            page_index: 2,
            dimensions: PageDimensions::new(300.0, 400.0),
            native_text: "Signal text".to_string(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                page_index: 2,
                dimensions: PageDimensions::new(300.0, 400.0),
                native_span_count: 3,
                native_text_bytes: 11,
                glyph_count: 10,
                image_area_ratio: 0.42,
                duplicate_char_ratio: 0.13,
                bbox_overlap_ratio: 0.07,
                broken_encoding_ratio: 0.02,
                rotation_degrees: 90,
                table_line_density: 0.31,
                annotation_count: 0,
                form_field_count: 1,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.signals.page_index, 2);
    assert_eq!(page.signals.dimensions, PageDimensions::new(300.0, 400.0));
    assert_eq!(page.signals.native_span_count, 3);
    assert_eq!(page.signals.native_text_bytes, 11);
    assert_eq!(page.signals.glyph_count, 10);
    assert_eq!(page.signals.image_area_ratio, 0.42);
    assert_eq!(page.signals.rotation_degrees, 90);
    assert_eq!(page.signals.table_line_density, 0.31);
    assert_eq!(page.signals.form_field_count, 1);
}

#[test]
fn backend_native_spans_are_preserved_with_bounding_boxes() {
    let artifact = parse_extracted_pages(
        "doc-spans".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "First line\nSecond line".to_string(),
            native_spans: vec![
                ExtractedTextSpan {
                    text: "First line".to_string(),
                    bbox: BBox {
                        x0: 72.0,
                        y0: 96.0,
                        x1: 180.0,
                        y1: 120.0,
                    },
                },
                ExtractedTextSpan {
                    text: "Second line".to_string(),
                    bbox: BBox {
                        x0: 72.0,
                        y0: 124.0,
                        x1: 210.0,
                        y1: 148.0,
                    },
                },
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 2,
                native_text_bytes: 22,
                glyph_count: 19,
                image_area_ratio: 0.01,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.native_spans.len(), 2);
    assert_eq!(page.native_spans[0].text, "First line");
    assert_eq!(page.native_spans[0].bbox.x0, 72.0);
    assert_eq!(page.native_spans[0].bbox.y0, 96.0);
    assert_eq!(page.native_spans[1].text, "Second line");
    assert_eq!(page.native_spans[1].bbox.x1, 210.0);
    assert_eq!(page.native_spans[1].provenance, SpanProvenance::Native);
    assert_eq!(page.layout_blocks[0].text, "First line\nSecond line");
    assert_eq!(page.layout_blocks[0].bbox.x0, 72.0);
    assert_eq!(page.layout_blocks[0].bbox.y0, 96.0);
    assert_eq!(page.layout_blocks[0].bbox.x1, 210.0);
    assert_eq!(page.layout_blocks[0].bbox.y1, 148.0);
}

#[test]
fn extracted_image_artifacts_are_preserved_without_pixel_payloads() {
    let artifact = parse_extracted_pages(
        "doc-image-artifacts".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: "Image-backed page".to_string(),
            native_spans: Vec::new(),
            image_artifacts: vec![ExtractedImage {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 612.0,
                    y1: 792.0,
                },
                area_ratio: 1.0,
                source_name: Some("Im1".to_string()),
            }],
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 1,
                native_text_bytes: 17,
                glyph_count: 15,
                image_area_ratio: 1.0,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let image = &artifact.pages[0].image_artifacts[0];
    assert_eq!(image.image_id, "p000000:im000000");
    assert_eq!(image.source_name.as_deref(), Some("Im1"));
    assert_eq!(image.bbox.x1, 612.0);
    assert_eq!(image.bbox.y1, 792.0);
    assert_eq!(image.area_ratio, 1.0);
}

#[test]
fn image_only_pages_expose_figure_layout_without_faking_text() {
    let artifact = parse_extracted_pages(
        "doc-image-only".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: String::new(),
            native_spans: Vec::new(),
            image_artifacts: vec![ExtractedImage {
                bbox: BBox {
                    x0: 12.0,
                    y0: 24.0,
                    x1: 580.0,
                    y1: 760.0,
                },
                area_ratio: 0.86,
                source_name: Some("ScanImage".to_string()),
            }],
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 0,
                native_text_bytes: 0,
                glyph_count: 0,
                image_area_ratio: 0.86,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert_eq!(page.route.route, PageRoute::OcrFallback);
    assert_eq!(page.layout_blocks.len(), 1);
    assert_eq!(page.layout_blocks[0].kind, LayoutBlockKind::Figure);
    assert_eq!(page.layout_blocks[0].text, "");
    assert_eq!(page.layout_blocks[0].bbox.x0, 12.0);
    assert_eq!(page.layout_blocks[0].bbox.y0, 24.0);
    assert_eq!(page.layout_blocks[0].bbox.x1, 580.0);
    assert_eq!(page.layout_blocks[0].bbox.y1, 760.0);
    assert_eq!(
        artifact.global_diagnostics.warnings,
        vec!["p000000: requires_ocr_without_ocr_output"]
    );
}

#[test]
fn page_fingerprint_changes_when_native_span_geometry_changes() {
    let without_geometry = ExtractedPage {
        page_index: 0,
        dimensions: PageDimensions::new(612.0, 792.0),
        native_text: "Same text".to_string(),
        native_spans: Vec::new(),
        image_artifacts: Vec::new(),
        signals: PageSignals {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_span_count: 1,
            native_text_bytes: 9,
            glyph_count: 8,
            image_area_ratio: 0.01,
            duplicate_char_ratio: 0.0,
            bbox_overlap_ratio: 0.0,
            broken_encoding_ratio: 0.0,
            rotation_degrees: 0,
            table_line_density: 0.0,
            annotation_count: 0,
            form_field_count: 0,
            huge_object_count: 0,
            span_geometry_capped: false,
        },
        ocr_text: None,
        timings: PageTimings::default(),
    };
    let with_geometry = ExtractedPage {
        native_spans: vec![ExtractedTextSpan {
            text: "Same text".to_string(),
            bbox: BBox {
                x0: 72.0,
                y0: 96.0,
                x1: 140.0,
                y1: 112.0,
            },
        }],
        ..without_geometry.clone()
    };

    let page_wide = parse_extracted_pages("doc-fingerprint".to_string(), vec![without_geometry]);
    let positioned = parse_extracted_pages("doc-fingerprint".to_string(), vec![with_geometry]);

    assert_ne!(
        page_wide.pages[0].fingerprint.as_hex(),
        positioned.pages[0].fingerprint.as_hex()
    );
    assert_ne!(
        page_wide.pages[0].artifact_id,
        positioned.pages[0].artifact_id
    );
}

#[test]
fn positioned_native_spans_split_layout_blocks_by_geometry_gaps() {
    let artifact = parse_extracted_pages(
        "doc-span-layout-gaps".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: concat!(
                "First paragraph line\n",
                "Second paragraph line\n",
                "New section"
            )
            .to_string(),
            native_spans: vec![
                ExtractedTextSpan {
                    text: "First paragraph line".to_string(),
                    bbox: BBox {
                        x0: 72.0,
                        y0: 96.0,
                        x1: 210.0,
                        y1: 110.0,
                    },
                },
                ExtractedTextSpan {
                    text: "Second paragraph line".to_string(),
                    bbox: BBox {
                        x0: 72.0,
                        y0: 112.0,
                        x1: 220.0,
                        y1: 126.0,
                    },
                },
                ExtractedTextSpan {
                    text: "New section".to_string(),
                    bbox: BBox {
                        x0: 72.0,
                        y0: 180.0,
                        x1: 150.0,
                        y1: 194.0,
                    },
                },
            ],
            image_artifacts: Vec::new(),
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 3,
                native_text_bytes: 54,
                glyph_count: 51,
                image_area_ratio: 0.01,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let blocks = &artifact.pages[0].layout_blocks;
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        blocks[0].text,
        "First paragraph line\nSecond paragraph line"
    );
    assert_eq!(blocks[0].bbox.x0, 72.0);
    assert_eq!(blocks[0].bbox.y0, 96.0);
    assert_eq!(blocks[0].bbox.x1, 220.0);
    assert_eq!(blocks[0].bbox.y1, 126.0);
    assert_eq!(blocks[1].text, "New section");
    assert_eq!(blocks[1].bbox.x0, 72.0);
    assert_eq!(blocks[1].bbox.y0, 180.0);
    assert_eq!(blocks[1].bbox.x1, 150.0);
    assert_eq!(blocks[1].bbox.y1, 194.0);
}

#[test]
fn scanned_like_input_is_flagged_for_ocr_without_faking_text_success() {
    let artifact = parse_extracted_pages(
        "doc-scan".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: String::new(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 0,
                native_text_bytes: 0,
                glyph_count: 0,
                image_area_ratio: 0.91,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: None,
            timings: PageTimings::default(),
        }],
    );

    let page = &artifact.pages[0];
    assert!(page.native_spans.is_empty());
    assert_eq!(page.route.route, PageRoute::OcrFallback);
    assert!(page.route.run_ocr);
    assert!(page.quality.flags.contains(&PageQuality::RequiresOcr));
    assert!(page.quality.flags.contains(&PageQuality::LowConfidenceText));
    assert_eq!(artifact.global_diagnostics.fallback_pages, 1);
    assert_eq!(artifact.global_diagnostics.ocr_pages, 1);
    assert_eq!(artifact.global_diagnostics.ocr_required_pages, 1);
    assert_eq!(artifact.global_diagnostics.ocr_applied_pages, 0);
    assert_eq!(
        artifact.global_diagnostics.warnings,
        vec!["p000000: requires_ocr_without_ocr_output"]
    );
}

#[test]
fn ocr_text_is_merged_with_provenance_for_required_pages() {
    let artifact = parse_extracted_pages(
        "doc-ocr".to_string(),
        vec![ExtractedPage {
            page_index: 0,
            dimensions: PageDimensions::new(612.0, 792.0),
            native_text: String::new(),
            native_spans: Vec::new(),
            image_artifacts: Vec::new(),
            signals: PageSignals {
                page_index: 0,
                dimensions: PageDimensions::new(612.0, 792.0),
                native_span_count: 0,
                native_text_bytes: 0,
                glyph_count: 0,
                image_area_ratio: 0.93,
                duplicate_char_ratio: 0.0,
                bbox_overlap_ratio: 0.0,
                broken_encoding_ratio: 0.0,
                rotation_degrees: 0,
                table_line_density: 0.0,
                annotation_count: 0,
                form_field_count: 0,
                huge_object_count: 0,
                span_geometry_capped: false,
            },
            ocr_text: Some("OCR recovered this page.".to_string()),
            timings: PageTimings {
                ocr_us: 456,
                ..PageTimings::default()
            },
        }],
    );

    let page = &artifact.pages[0];
    assert!(page.native_spans.is_empty());
    assert_eq!(page.ocr_spans.len(), 1);
    assert_eq!(page.ocr_spans[0].text, "OCR recovered this page.");
    assert_eq!(page.ocr_spans[0].provenance, SpanProvenance::Ocr);
    assert_eq!(page.route.route, PageRoute::OcrFallback);
    assert!(page.quality.flags.contains(&PageQuality::RequiresOcr));
    assert_eq!(page.timings.ocr_us, 456);
    assert_eq!(artifact.global_diagnostics.ocr_required_pages, 1);
    assert_eq!(artifact.global_diagnostics.ocr_applied_pages, 1);
    assert!(artifact.global_diagnostics.warnings.is_empty());
}
