#![allow(dead_code)]

use glyphrush_core::{
    BBox, ExtractedPage, ExtractedTextSpan, PageDimensions, PageSignals, PageTimings,
};

const DEFAULT_WIDTH: f32 = 612.0;
const DEFAULT_HEIGHT: f32 = 792.0;

pub fn dimensions() -> PageDimensions {
    PageDimensions::new(DEFAULT_WIDTH, DEFAULT_HEIGHT)
}

pub fn span(text: &str, x0: f32, y0: f32, x1: f32, y1: f32) -> ExtractedTextSpan {
    ExtractedTextSpan {
        text: text.to_string(),
        bbox: BBox { x0, y0, x1, y1 },
    }
}

pub fn signals(page_index: u32) -> PageSignals {
    PageSignals {
        page_index,
        dimensions: PageDimensions::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
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

pub fn page(page_index: u32) -> ExtractedPage {
    ExtractedPage {
        page_index,
        dimensions: PageDimensions::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
        native_text: String::new(),
        native_spans: Vec::new(),
        ruling_lines: Vec::new(),
        image_artifacts: Vec::new(),
        signals: signals(page_index),
        ocr_text: None,
        timings: PageTimings::default(),
    }
}
