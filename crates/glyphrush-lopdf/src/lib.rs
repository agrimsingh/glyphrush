use std::thread;
use web_time::Instant;

use anyhow::{Context, Result, anyhow};
use glyphrush_core::{
    BBox, ExtractedImage, ExtractedPage, ExtractedTextSpan, NormalizedBBox, PageDimensions,
    PageSignals, PageTimings, broken_encoding_ratio, combined_table_line_density,
    duplicate_char_ratio, image_artifact_coverage_ratio, is_ruling_segment,
    normalize_text_for_span_check, positioned_bbox_overlap_ratio, ruling_density,
};
use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};

#[derive(Clone, Copy, Debug, Default)]
pub struct LopdfExtractionOptions {
    pub span_geometry: bool,
    pub page_jobs: usize,
}

pub type OcrLoader<'a> = &'a (dyn Fn(&PageSignals) -> Result<(Option<String>, u64)> + Sync);

const MAX_POSITIONED_SPAN_CONTENT_BYTES: usize = 64 * 1024;
const MAX_POSITIONED_SPAN_NATIVE_TEXT_BYTES: u32 = 4 * 1024;
const RULED_TABLE_SATURATION_SEGMENTS: u32 = 20;

pub fn extract_pages<'a>(
    document: &Document,
    options: LopdfExtractionOptions,
    ocr: OcrLoader<'a>,
) -> Result<Vec<ExtractedPage>> {
    let pages = document.get_pages().into_iter().collect::<Vec<_>>();
    let worker_count = options.page_jobs.max(1).min(pages.len().max(1));

    if worker_count == 1 {
        return pages
            .into_iter()
            .map(|(page_number, page_id)| {
                extract_lopdf_page(document, options, ocr, page_number, page_id)
            })
            .collect();
    }

    let mut extracted_pages = Vec::with_capacity(pages.len());
    for chunk in pages.chunks(worker_count) {
        let mut chunk_results = Vec::with_capacity(chunk.len());
        thread::scope(|scope| -> Result<()> {
            let handles = chunk
                .iter()
                .map(|(page_number, page_id)| {
                    scope.spawn(move || {
                        extract_lopdf_page(document, options, ocr, *page_number, *page_id)
                            .map(|page| (*page_number, page))
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                chunk_results.push(
                    handle
                        .join()
                        .map_err(|_| anyhow!("page extraction worker panicked"))??,
                );
            }

            Ok(())
        })?;
        extracted_pages.extend(chunk_results);
    }

    extracted_pages.sort_by_key(|(page_number, _)| *page_number);
    Ok(extracted_pages.into_iter().map(|(_, page)| page).collect())
}

pub fn extract_page_by_index<'a>(
    document: &Document,
    options: LopdfExtractionOptions,
    ocr: OcrLoader<'a>,
    page_index: u32,
) -> Result<ExtractedPage> {
    let page_number = page_index
        .checked_add(1)
        .with_context(|| format!("page index {page_index} is too large"))?;
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .with_context(|| format!("page index {page_index} not found"))?;

    extract_lopdf_page(document, options, ocr, page_number, page_id)
}

fn extract_lopdf_page<'a>(
    document: &Document,
    options: LopdfExtractionOptions,
    ocr: OcrLoader<'a>,
    page_number: u32,
    page_id: ObjectId,
) -> Result<ExtractedPage> {
    let page_index = page_number.saturating_sub(1);
    let page_box = effective_page_box(document, page_id);
    let dimensions = page_box.dimensions();
    let native_extract_start = Instant::now();
    let native_text = document.extract_text(&[page_number]).unwrap_or_default();
    let native_extract_us = native_extract_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;
    let content = document.get_page_content(page_id).unwrap_or_default();
    let content_len = content.len();
    let native_text_bytes = native_text.trim().len() as u32;
    let rotation_degrees = page_rotation(document, page_id);
    let can_extract_positioned_spans =
        should_extract_positioned_spans(content_len, native_text_bytes, rotation_degrees);
    let span_geometry_capped = options.span_geometry && !can_extract_positioned_spans;
    let native_spans = if options.span_geometry && can_extract_positioned_spans {
        compatible_positioned_text_spans(
            &native_text,
            extract_positioned_text_spans(&content, &page_box),
        )
    } else {
        Vec::new()
    };
    let bbox_overlap_ratio = positioned_bbox_overlap_ratio(&native_spans);
    let glyph_count = native_text.chars().filter(|ch| !ch.is_whitespace()).count() as u32;
    let native_span_count = native_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        .max(native_spans.len())
        .max((native_text_bytes > 0) as usize) as u32;

    let image_artifacts = image_xobject_artifacts(document, page_id, &content, &page_box);
    let image_area_ratio =
        image_area_ratio_hint(&image_artifacts, &content, native_text_bytes, &dimensions);
    let table_start = Instant::now();
    let table_line_density =
        combined_table_line_density(&native_text, || ruled_table_line_density(&content));
    let table_us = table_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;
    let signals = PageSignals {
        page_index,
        dimensions: dimensions.clone(),
        native_span_count,
        native_text_bytes,
        glyph_count,
        image_area_ratio,
        duplicate_char_ratio: duplicate_char_ratio(&native_text),
        bbox_overlap_ratio,
        broken_encoding_ratio: broken_encoding_ratio(&native_text),
        rotation_degrees,
        table_line_density,
        annotation_count: page_annotation_count(document, page_id),
        form_field_count: page_form_field_count(document, page_id),
        huge_object_count: if content_len > 16 * 1024 * 1024 {
            65
        } else {
            0
        },
        span_geometry_capped,
    };
    let (ocr_text, ocr_us) = ocr(&signals)?;

    Ok(ExtractedPage {
        page_index,
        dimensions,
        native_text,
        native_spans,
        ruling_lines: Vec::new(),
        image_artifacts,
        signals,
        ocr_text,
        timings: PageTimings {
            native_extract_us,
            table_us,
            ocr_us,
            ..PageTimings::default()
        },
    })
}

#[derive(Clone, Copy, Debug)]
struct TextGeometryState {
    line_x: f32,
    line_y: f32,
    x: f32,
    y: f32,
    axis_a: f32,
    axis_b: f32,
    axis_c: f32,
    axis_d: f32,
    font_size: f32,
    leading: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scaling: f32,
    text_rise: f32,
}

impl Default for TextGeometryState {
    fn default() -> Self {
        Self {
            line_x: 0.0,
            line_y: 0.0,
            x: 0.0,
            y: 0.0,
            axis_a: 1.0,
            axis_b: 0.0,
            axis_c: 0.0,
            axis_d: 1.0,
            font_size: 12.0,
            leading: 12.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            text_rise: 0.0,
        }
    }
}

impl TextGeometryState {
    fn begin_text_object(&mut self) {
        self.line_x = 0.0;
        self.line_y = 0.0;
        self.x = 0.0;
        self.y = 0.0;
        self.axis_a = 1.0;
        self.axis_b = 0.0;
        self.axis_c = 0.0;
        self.axis_d = 1.0;
    }

    fn move_text_position(&mut self, tx: f32, ty: f32) {
        self.line_x += tx * self.axis_a + ty * self.axis_c;
        self.line_y += tx * self.axis_b + ty * self.axis_d;
        self.x = self.line_x;
        self.y = self.line_y;
    }

    fn set_text_matrix(&mut self, matrix: PdfMatrix) {
        self.axis_a = matrix.a;
        self.axis_b = matrix.b;
        self.axis_c = matrix.c;
        self.axis_d = matrix.d;
        self.line_x = matrix.e;
        self.line_y = matrix.f;
        self.x = matrix.e;
        self.y = matrix.f;
    }

    fn move_to_next_line(&mut self) {
        let leading = if self.leading == 0.0 {
            self.font_size
        } else {
            self.leading
        };
        self.move_text_position(0.0, -leading);
    }

    fn text_matrix(&self) -> PdfMatrix {
        PdfMatrix {
            a: self.axis_a,
            b: self.axis_b,
            c: self.axis_c,
            d: self.axis_d,
            e: self.x + self.axis_c * self.text_rise,
            f: self.y + self.axis_d * self.text_rise,
        }
    }

    fn advance_text(&mut self, width: f32) {
        self.x += width * self.axis_a;
        self.y += width * self.axis_b;
    }

    fn apply_text_array_adjustment(&mut self, adjustment: f32) {
        self.advance_text((-adjustment / 1000.0) * self.font_size * self.horizontal_scaling);
    }

    fn text_width(&self, text: &str) -> f32 {
        estimate_text_width(
            text,
            self.font_size,
            self.char_spacing,
            self.word_spacing,
            self.horizontal_scaling,
        )
    }
}

fn should_extract_positioned_spans(
    content_len: usize,
    native_text_bytes: u32,
    rotation_degrees: i16,
) -> bool {
    content_len <= MAX_POSITIONED_SPAN_CONTENT_BYTES
        && native_text_bytes <= MAX_POSITIONED_SPAN_NATIVE_TEXT_BYTES
        && rotation_degrees.rem_euclid(360) == 0
}

fn extract_positioned_text_spans(
    content_data: &[u8],
    page_box: &PageBox,
) -> Vec<ExtractedTextSpan> {
    let Ok(content) = Content::decode(content_data) else {
        return Vec::new();
    };

    let mut state = TextGeometryState::default();
    let mut matrix = PdfMatrix::identity();
    let mut matrix_stack = Vec::new();
    let mut spans = Vec::new();

    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => matrix_stack.push(matrix),
            "Q" => matrix = matrix_stack.pop().unwrap_or_else(PdfMatrix::identity),
            "cm" => {
                if let Some(next) = PdfMatrix::from_operands(&operation.operands) {
                    matrix = matrix.multiply(next);
                }
            }
            "BT" => state.begin_text_object(),
            "Tf" => {
                if let Some(size) = operation.operands.get(1).and_then(float_operand) {
                    state.font_size = size.abs().max(1.0);
                    if state.leading == 0.0 {
                        state.leading = state.font_size;
                    }
                }
            }
            "Tc" => {
                if let Some(spacing) = operation.operands.first().and_then(float_operand) {
                    state.char_spacing = spacing;
                }
            }
            "Tw" => {
                if let Some(spacing) = operation.operands.first().and_then(float_operand) {
                    state.word_spacing = spacing;
                }
            }
            "Tz" => {
                if let Some(scaling) = operation.operands.first().and_then(float_operand) {
                    state.horizontal_scaling = (scaling / 100.0).max(0.01);
                }
            }
            "Ts" => {
                if let Some(rise) = operation.operands.first().and_then(float_operand) {
                    state.text_rise = rise;
                }
            }
            "TL" => {
                if let Some(leading) = operation.operands.first().and_then(float_operand) {
                    state.leading = leading;
                }
            }
            "Td" => {
                if let Some((tx, ty)) = two_float_operands(&operation.operands) {
                    state.move_text_position(tx, ty);
                }
            }
            "TD" => {
                if let Some((tx, ty)) = two_float_operands(&operation.operands) {
                    state.leading = -ty;
                    state.move_text_position(tx, ty);
                }
            }
            "Tm" => {
                if let Some(text_matrix) = PdfMatrix::from_operands(&operation.operands) {
                    state.set_text_matrix(text_matrix);
                }
            }
            "T*" => state.move_to_next_line(),
            "Tj" => {
                if let Some(text) = operation.operands.first().and_then(text_operand) {
                    push_positioned_span(&mut spans, &mut state, page_box, matrix, text);
                }
            }
            "TJ" => {
                if let Some(text_array) = operation.operands.first() {
                    push_positioned_text_array_spans(
                        &mut spans, &mut state, page_box, matrix, text_array,
                    );
                }
            }
            "'" => {
                state.move_to_next_line();
                if let Some(text) = operation.operands.first().and_then(text_operand) {
                    push_positioned_span(&mut spans, &mut state, page_box, matrix, text);
                }
            }
            "\"" => {
                if let Some(word_spacing) = operation.operands.first().and_then(float_operand) {
                    state.word_spacing = word_spacing;
                }
                if let Some(char_spacing) = operation.operands.get(1).and_then(float_operand) {
                    state.char_spacing = char_spacing;
                }
                state.move_to_next_line();
                if let Some(text) = operation.operands.get(2).and_then(text_operand) {
                    push_positioned_span(&mut spans, &mut state, page_box, matrix, text);
                }
            }
            _ => {}
        }
    }

    spans
}

fn compatible_positioned_text_spans(
    native_text: &str,
    spans: Vec<ExtractedTextSpan>,
) -> Vec<ExtractedTextSpan> {
    let spans = spans
        .into_iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();
    if spans.is_empty() {
        return Vec::new();
    }

    let native = normalize_text_for_span_check(native_text);
    let compatible = spans.iter().all(|span| {
        let text = normalize_text_for_span_check(&span.text);
        !text.is_empty() && native.contains(&text)
    });

    if compatible { spans } else { Vec::new() }
}
fn push_positioned_span(
    spans: &mut Vec<ExtractedTextSpan>,
    state: &mut TextGeometryState,
    page_box: &PageBox,
    matrix: PdfMatrix,
    text: String,
) {
    if text.trim().is_empty() {
        return;
    }

    let width = state.text_width(&text);
    let bbox = transformed_text_bbox(
        state.text_matrix(),
        width,
        state.font_size,
        matrix,
        page_box,
    );
    spans.push(ExtractedTextSpan { text, bbox });
    state.advance_text(width);
}

fn push_positioned_text_array_spans(
    spans: &mut Vec<ExtractedTextSpan>,
    state: &mut TextGeometryState,
    page_box: &PageBox,
    matrix: PdfMatrix,
    object: &Object,
) {
    let Ok(array) = object.as_array() else {
        return;
    };

    for item in array {
        if let Some(text) = text_operand(item) {
            push_positioned_span(spans, state, page_box, matrix, text);
        } else if let Some(adjustment) = float_operand(item) {
            state.apply_text_array_adjustment(adjustment);
        }
    }
}

fn transformed_text_bbox(
    text_matrix: PdfMatrix,
    width: f32,
    font_size: f32,
    matrix: PdfMatrix,
    page_box: &PageBox,
) -> BBox {
    let corners = [
        transformed_text_point(text_matrix, matrix, 0.0, 0.0),
        transformed_text_point(text_matrix, matrix, width, 0.0),
        transformed_text_point(text_matrix, matrix, 0.0, -font_size),
        transformed_text_point(text_matrix, matrix, width, -font_size),
    ];

    let (first_x, first_y) = corners[0];
    let mut x0 = first_x - page_box.x0;
    let mut x1 = x0;
    let mut y0 = page_box.y1 - first_y;
    let mut y1 = y0;
    for (x, pdf_y) in corners.into_iter().skip(1) {
        let page_x = x - page_box.x0;
        let page_y = page_box.y1 - pdf_y;
        x0 = x0.min(page_x);
        x1 = x1.max(page_x);
        y0 = y0.min(page_y);
        y1 = y1.max(page_y);
    }

    BBox { x0, y0, x1, y1 }
}

fn transformed_text_point(
    text_matrix: PdfMatrix,
    content_matrix: PdfMatrix,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let (text_x, text_y) = text_matrix.transform_point(x, y);
    content_matrix.transform_point(text_x, text_y)
}

fn estimate_text_width(
    text: &str,
    font_size: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scaling: f32,
) -> f32 {
    let mut glyphs = 0usize;
    let mut spaces = 0usize;
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        glyphs += 1;
        if ch == ' ' {
            spaces += 1;
        }
    }

    let glyphs = glyphs.max(1);
    let base_width = glyphs as f32 * font_size * 0.55;
    let char_spacing_width = glyphs.saturating_sub(1) as f32 * char_spacing;
    let word_spacing_width = spaces as f32 * word_spacing;
    (base_width + char_spacing_width + word_spacing_width) * horizontal_scaling
}

fn two_float_operands(operands: &[Object]) -> Option<(f32, f32)> {
    Some((
        operands.first().and_then(float_operand)?,
        operands.get(1).and_then(float_operand)?,
    ))
}

fn float_operand(object: &Object) -> Option<f32> {
    object.as_float().ok()
}

fn name_operand(object: &Object) -> Option<&[u8]> {
    object.as_name().ok()
}

fn text_operand(object: &Object) -> Option<String> {
    object
        .as_str()
        .ok()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn effective_page_box(document: &Document, page_id: ObjectId) -> PageBox {
    let default = PageBox::default();
    inherited_page_array(document, page_id, b"CropBox")
        .and_then(|crop_box| page_box_from_array(crop_box))
        .or_else(|| {
            inherited_page_array(document, page_id, b"MediaBox")
                .and_then(|media_box| page_box_from_array(media_box))
        })
        .unwrap_or(default)
}

#[derive(Clone, Debug)]
struct PageBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl PageBox {
    fn default() -> Self {
        Self {
            x0: 0.0,
            y0: 0.0,
            x1: 612.0,
            y1: 792.0,
        }
    }

    fn dimensions(&self) -> PageDimensions {
        PageDimensions::new(self.x1 - self.x0, self.y1 - self.y0)
    }

    fn clipped_local_bbox(&self, bbox: &BBox) -> Option<NormalizedBBox> {
        let x0 = bbox.x0.min(bbox.x1).max(self.x0);
        let x1 = bbox.x0.max(bbox.x1).min(self.x1);
        let y0 = bbox.y0.min(bbox.y1).max(self.y0);
        let y1 = bbox.y0.max(bbox.y1).min(self.y1);
        if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
            return None;
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        let local_x0 = x0 - self.x0;
        let local_x1 = x1 - self.x0;
        let local_y0 = y0 - self.y0;
        let local_y1 = y1 - self.y0;
        let area = (local_x1 - local_x0) * (local_y1 - local_y0);
        (area > f32::EPSILON).then_some(NormalizedBBox {
            x0: local_x0,
            y0: local_y0,
            x1: local_x1,
            y1: local_y1,
            area,
        })
    }

    fn local_bbox(&self, bbox: &BBox) -> Option<BBox> {
        let x0 = bbox.x0.min(bbox.x1) - self.x0;
        let x1 = bbox.x0.max(bbox.x1) - self.x0;
        let y0 = bbox.y0.min(bbox.y1) - self.y0;
        let y1 = bbox.y0.max(bbox.y1) - self.y0;
        if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
            return None;
        }

        ((x1 - x0) > f32::EPSILON && (y1 - y0) > f32::EPSILON).then_some(BBox { x0, y0, x1, y1 })
    }
}

fn page_box_from_array(box_array: &[Object]) -> Option<PageBox> {
    if box_array.len() != 4 {
        return None;
    }

    let raw_x0 = box_array[0].as_float().ok()?;
    let raw_y0 = box_array[1].as_float().ok()?;
    let raw_x1 = box_array[2].as_float().ok()?;
    let raw_y1 = box_array[3].as_float().ok()?;
    if ![raw_x0, raw_y0, raw_x1, raw_y1]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }

    let x0 = raw_x0.min(raw_x1);
    let x1 = raw_x0.max(raw_x1);
    let y0 = raw_y0.min(raw_y1);
    let y1 = raw_y0.max(raw_y1);
    ((x1 - x0) > f32::EPSILON && (y1 - y0) > f32::EPSILON).then_some(PageBox { x0, y0, x1, y1 })
}

fn inherited_page_array<'a>(
    document: &'a Document,
    page_id: ObjectId,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    let mut current_id = page_id;
    for _ in 0..16 {
        let dict = document.get_dictionary(current_id).ok()?;
        if let Some(array) = dict.get(key).ok().and_then(|object| object.as_array().ok()) {
            return Some(array);
        }

        match dict.get(b"Parent").ok()? {
            Object::Reference(parent_id) => current_id = *parent_id,
            _ => return None,
        }
    }

    None
}

fn page_rotation(document: &Document, page_id: ObjectId) -> i16 {
    let mut current_id = page_id;
    for _ in 0..16 {
        let Ok(dict) = document.get_dictionary(current_id) else {
            return 0;
        };
        if let Some(rotation) = dict
            .get(b"Rotate")
            .ok()
            .and_then(|object| object.as_i64().ok())
        {
            return rotation as i16;
        }

        match dict.get(b"Parent").ok() {
            Some(Object::Reference(parent_id)) => current_id = *parent_id,
            _ => return 0,
        }
    }

    0
}

fn page_annotation_count(document: &Document, page_id: ObjectId) -> u32 {
    let Ok(dict) = document.get_dictionary(page_id) else {
        return 0;
    };
    let Ok(annots) = dict.get(b"Annots") else {
        return 0;
    };

    if let Some(array) = object_array(document, annots) {
        return array.len() as u32;
    }

    u32::from(object_dictionary(document, annots).is_some())
}

fn page_form_field_count(document: &Document, page_id: ObjectId) -> u32 {
    page_widget_annotation_count(document, page_id) + catalog_acroform_field_count(document)
}

fn page_widget_annotation_count(document: &Document, page_id: ObjectId) -> u32 {
    let Ok(dict) = document.get_dictionary(page_id) else {
        return 0;
    };
    let Ok(annots) = dict.get(b"Annots") else {
        return 0;
    };

    if let Some(array) = object_array(document, annots) {
        return array
            .iter()
            .filter(|annotation| is_form_annotation(document, annotation))
            .count() as u32;
    }

    u32::from(is_form_annotation(document, annots))
}

fn catalog_acroform_field_count(document: &Document) -> u32 {
    let Some(acroform) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|object| object_dictionary(document, object))
    else {
        return 0;
    };

    acroform
        .get(b"Fields")
        .ok()
        .and_then(|object| object_array(document, object))
        .map(|fields| fields.len() as u32)
        .unwrap_or_default()
}

fn is_form_annotation(document: &Document, object: &Object) -> bool {
    object_dictionary(document, object)
        .and_then(|dict| dict.get(b"Subtype").ok())
        .and_then(name_operand)
        .is_some_and(|subtype| subtype == b"Widget")
}

fn image_area_ratio_hint(
    image_artifacts: &[ExtractedImage],
    content: &[u8],
    native_text_bytes: u32,
    dimensions: &PageDimensions,
) -> f32 {
    let xobject_ratio = image_artifact_coverage_ratio(image_artifacts, dimensions);
    let fallback_ratio = if native_text_bytes == 0 && !content.is_empty() {
        0.85
    } else {
        0.0
    };

    xobject_ratio.max(fallback_ratio)
}
fn image_xobject_artifacts(
    document: &Document,
    page_id: ObjectId,
    content: &[u8],
    page_box: &PageBox,
) -> Vec<ExtractedImage> {
    let resources = page_resources(document, page_id);
    let dimensions = page_box.dimensions();
    let page_area = dimensions.width * dimensions.height;
    if page_area <= 0.0 {
        return Vec::new();
    }

    let mut collector = ImageArtifactCollector {
        document,
        page_box: page_box.clone(),
        page_area,
        images: Vec::new(),
    };

    if let Some(draws) = raw_image_draw_ops(content) {
        if let Some(resources) = resources {
            for draw in draws {
                collector.collect_drawn_xobject(resources, draw.name, draw.state, None, 1);
            }
        }
        return collector.images;
    }

    let Ok(content) = Content::decode(content) else {
        return Vec::new();
    };
    collector.collect_content(&content, resources, PdfMatrix::identity(), None, 0);
    collector.images
}

struct RawImageDraw<'a> {
    name: &'a [u8],
    state: PdfMatrix,
}

enum RawImageOperand<'a> {
    Number(f32),
    Name(&'a [u8]),
}

enum RawImageToken<'a> {
    Number(f32),
    Name(&'a [u8]),
    Operator(&'a [u8]),
}

fn raw_image_draw_ops(content: &[u8]) -> Option<Vec<RawImageDraw<'_>>> {
    let mut state = PdfMatrix::identity();
    let mut stack = Vec::new();
    let mut operands = Vec::with_capacity(8);
    let mut draws = Vec::new();

    for token in RawImageTokens::new(content) {
        match token {
            RawImageToken::Number(value) => operands.push(RawImageOperand::Number(value)),
            RawImageToken::Name(name) => operands.push(RawImageOperand::Name(name)),
            RawImageToken::Operator(operator) => {
                match operator {
                    b"BI" | b"ID" => return None,
                    b"q" => stack.push(state),
                    b"Q" => {
                        state = stack.pop().unwrap_or_else(PdfMatrix::identity);
                    }
                    b"cm" => {
                        if let Some(matrix) = raw_image_matrix_from_operands(&operands) {
                            state = state.multiply(matrix);
                        }
                    }
                    b"Do" => {
                        let name = raw_image_name_operand(&operands)?;
                        if name.contains(&b'#') {
                            return None;
                        }
                        draws.push(RawImageDraw { name, state });
                    }
                    _ => {}
                }
                operands.clear();
            }
        }
    }

    Some(draws)
}

fn raw_image_matrix_from_operands(operands: &[RawImageOperand<'_>]) -> Option<PdfMatrix> {
    let start = operands.len().checked_sub(6)?;
    let number = |offset: usize| match operands.get(start + offset)? {
        RawImageOperand::Number(value) => Some(*value),
        RawImageOperand::Name(_) => None,
    };

    Some(PdfMatrix {
        a: number(0)?,
        b: number(1)?,
        c: number(2)?,
        d: number(3)?,
        e: number(4)?,
        f: number(5)?,
    })
}

fn raw_image_name_operand<'a>(operands: &[RawImageOperand<'a>]) -> Option<&'a [u8]> {
    operands.last().and_then(|operand| match operand {
        RawImageOperand::Name(name) => Some(*name),
        RawImageOperand::Number(_) => None,
    })
}

struct RawImageTokens<'a> {
    content: &'a [u8],
    offset: usize,
}

impl<'a> RawImageTokens<'a> {
    fn new(content: &'a [u8]) -> Self {
        Self { content, offset: 0 }
    }

    fn skip_delimiters(&mut self) {
        while self.offset < self.content.len() {
            match self.content[self.offset] {
                b'%' => {
                    self.offset += 1;
                    while self.offset < self.content.len()
                        && !matches!(self.content[self.offset], b'\n' | b'\r')
                    {
                        self.offset += 1;
                    }
                }
                byte if byte.is_ascii_whitespace()
                    || matches!(byte, b'[' | b']' | b'{' | b'}' | b'<' | b'>') =>
                {
                    self.offset += 1;
                }
                _ => break,
            }
        }
    }

    fn skip_literal_string(&mut self) {
        let mut depth = 1usize;
        self.offset += 1;
        while self.offset < self.content.len() && depth > 0 {
            match self.content[self.offset] {
                b'\\' => {
                    self.offset = (self.offset + 2).min(self.content.len());
                }
                b'(' => {
                    depth += 1;
                    self.offset += 1;
                }
                b')' => {
                    depth -= 1;
                    self.offset += 1;
                }
                _ => self.offset += 1,
            }
        }
    }

    fn skip_hex_string(&mut self) {
        self.offset += 1;
        while self.offset < self.content.len() {
            let byte = self.content[self.offset];
            self.offset += 1;
            if byte == b'>' {
                break;
            }
        }
    }
}

impl<'a> Iterator for RawImageTokens<'a> {
    type Item = RawImageToken<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.skip_delimiters();
            if self.offset >= self.content.len() {
                return None;
            }

            match self.content[self.offset] {
                b'(' => {
                    self.skip_literal_string();
                    continue;
                }
                b'<' if self
                    .content
                    .get(self.offset + 1)
                    .is_none_or(|byte| *byte != b'<') =>
                {
                    self.skip_hex_string();
                    continue;
                }
                b'/' => {
                    self.offset += 1;
                    let start = self.offset;
                    while self.offset < self.content.len()
                        && !self.content[self.offset].is_ascii_whitespace()
                        && !matches!(
                            self.content[self.offset],
                            b'[' | b']' | b'{' | b'}' | b'/' | b'(' | b')' | b'<' | b'>' | b'%'
                        )
                    {
                        self.offset += 1;
                    }
                    if self.offset > start {
                        return Some(RawImageToken::Name(&self.content[start..self.offset]));
                    }
                    continue;
                }
                _ => {}
            }

            let start = self.offset;
            while self.offset < self.content.len()
                && !self.content[self.offset].is_ascii_whitespace()
                && !matches!(
                    self.content[self.offset],
                    b'[' | b']' | b'{' | b'}' | b'/' | b'(' | b')' | b'<' | b'>' | b'%'
                )
            {
                self.offset += 1;
            }

            if self.offset > start {
                let token = &self.content[start..self.offset];
                if let Some(number) = raw_number_token(token) {
                    return Some(RawImageToken::Number(number));
                }
                return Some(RawImageToken::Operator(token));
            }

            self.offset += 1;
        }
    }
}

struct ImageArtifactCollector<'a> {
    document: &'a Document,
    page_box: PageBox,
    page_area: f32,
    images: Vec<ExtractedImage>,
}

impl ImageArtifactCollector<'_> {
    const MAX_XOBJECT_ARTIFACT_DEPTH: u8 = 8;

    fn collect_content(
        &mut self,
        content: &Content,
        resources: Option<&Dictionary>,
        initial_state: PdfMatrix,
        source_name: Option<String>,
        depth: u8,
    ) {
        if depth >= Self::MAX_XOBJECT_ARTIFACT_DEPTH {
            return;
        }

        let mut state = initial_state;
        let mut stack = Vec::new();

        for operation in &content.operations {
            match operation.operator.as_str() {
                "q" => stack.push(state),
                "Q" => {
                    state = stack.pop().unwrap_or(initial_state);
                }
                "cm" => {
                    if let Some(matrix) = PdfMatrix::from_operands(&operation.operands) {
                        state = state.multiply(matrix);
                    }
                }
                "Do" => {
                    if let Some(resources) = resources
                        && let Some(name) = operation.operands.first().and_then(name_operand)
                    {
                        self.collect_drawn_xobject(
                            resources,
                            name,
                            state,
                            source_name.clone(),
                            depth + 1,
                        );
                    }
                }
                "BI" => {
                    self.push_image_artifact(state, "inline".to_string());
                }
                _ => {}
            }
        }
    }

    fn collect_drawn_xobject(
        &mut self,
        resources: &Dictionary,
        name: &[u8],
        state: PdfMatrix,
        source_name: Option<String>,
        depth: u8,
    ) {
        let Some(xobjects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|object| object_dictionary(self.document, object))
        else {
            return;
        };
        let Ok(object) = xobjects.get(name) else {
            return;
        };

        let Some(dict) = object_dictionary(self.document, object) else {
            return;
        };
        let Some(subtype) = dict.get(b"Subtype").ok().and_then(name_operand) else {
            return;
        };

        let source_name = source_name.unwrap_or_else(|| String::from_utf8_lossy(name).into_owned());
        if subtype == b"Image" {
            self.push_image_artifact(state, source_name);
            return;
        }
        if subtype != b"Form" {
            return;
        }

        let Some(content) = object_stream_content(self.document, object) else {
            return;
        };
        let Ok(content) = Content::decode(content) else {
            return;
        };
        let form_resources = dict
            .get(b"Resources")
            .ok()
            .and_then(|object| object_dictionary(self.document, object))
            .or(Some(resources));
        let form_matrix = dict
            .get(b"Matrix")
            .ok()
            .and_then(|object| object_array(self.document, object))
            .and_then(|array| PdfMatrix::from_operands(array))
            .unwrap_or_else(PdfMatrix::identity);

        self.collect_content(
            &content,
            form_resources,
            state.multiply(form_matrix),
            Some(source_name),
            depth,
        );
    }

    fn push_image_artifact(&mut self, state: PdfMatrix, source_name: String) {
        let raw_bbox = state.unit_square_bbox();
        let Some(bbox) = self.page_box.local_bbox(&raw_bbox) else {
            return;
        };
        let area_ratio = self
            .page_box
            .clipped_local_bbox(&raw_bbox)
            .map(|bbox| bbox.area / self.page_area)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        self.images.push(ExtractedImage {
            bbox,
            area_ratio,
            source_name: Some(source_name),
        });
    }
}

fn page_resources(document: &Document, page_id: ObjectId) -> Option<&Dictionary> {
    let mut current_id = page_id;
    for _ in 0..16 {
        let dict = document.get_dictionary(current_id).ok()?;
        if let Some(resources) = dict
            .get(b"Resources")
            .ok()
            .and_then(|object| object_dictionary(document, object))
        {
            return Some(resources);
        }

        match dict.get(b"Parent").ok()? {
            Object::Reference(parent_id) => current_id = *parent_id,
            _ => return None,
        }
    }

    None
}

fn object_stream_content<'a>(document: &'a Document, object: &'a Object) -> Option<&'a [u8]> {
    match object {
        Object::Stream(stream) => Some(&stream.content),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .ok()
            .and_then(|object| object_stream_content(document, object)),
        _ => None,
    }
}

fn object_dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .ok()
            .and_then(|object| object_dictionary(document, object)),
        _ => None,
    }
}

fn object_array<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Vec<Object>> {
    match object {
        Object::Array(array) => Some(array),
        Object::Reference(object_id) => document
            .get_object(*object_id)
            .ok()
            .and_then(|object| object_array(document, object)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct PdfMatrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl PdfMatrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn from_operands(operands: &[Object]) -> Option<Self> {
        Some(Self {
            a: operands.first().and_then(float_operand)?,
            b: operands.get(1).and_then(float_operand)?,
            c: operands.get(2).and_then(float_operand)?,
            d: operands.get(3).and_then(float_operand)?,
            e: operands.get(4).and_then(float_operand)?,
            f: operands.get(5).and_then(float_operand)?,
        })
    }

    fn multiply(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            e: self.e * next.a + self.f * next.c + next.e,
            f: self.e * next.b + self.f * next.d + next.f,
        }
    }

    fn transform_point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    fn unit_square_bbox(self) -> BBox {
        let points = [
            self.transform_point(0.0, 0.0),
            self.transform_point(1.0, 0.0),
            self.transform_point(0.0, 1.0),
            self.transform_point(1.0, 1.0),
        ];

        let (mut x0, mut y0) = points[0];
        let (mut x1, mut y1) = points[0];
        for (x, y) in points.into_iter().skip(1) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }

        BBox { x0, y0, x1, y1 }
    }
}
#[derive(Default)]
struct VectorPathState {
    current: Option<(f32, f32)>,
    pending_ruling_segments: u32,
}

fn ruled_table_line_density(content: &[u8]) -> f32 {
    if let Some(density) = raw_ruled_table_line_density_hint(content) {
        return density;
    }

    let Ok(content) = Content::decode(content) else {
        return 0.0;
    };

    let mut matrix = PdfMatrix::identity();
    let mut matrix_stack = Vec::new();
    let mut path = VectorPathState::default();
    let mut stroked_ruling_segments = 0u32;

    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => matrix_stack.push(matrix),
            "Q" => {
                matrix = matrix_stack.pop().unwrap_or_else(PdfMatrix::identity);
                path = VectorPathState::default();
            }
            "cm" => {
                if let Some(next) = PdfMatrix::from_operands(&operation.operands) {
                    matrix = matrix.multiply(next);
                }
            }
            "m" => {
                path.current = two_float_operands(&operation.operands)
                    .map(|(x, y)| matrix.transform_point(x, y));
            }
            "l" => {
                if let (Some(start), Some((x, y))) =
                    (path.current, two_float_operands(&operation.operands))
                {
                    let end = matrix.transform_point(x, y);
                    if is_ruling_segment(start, end) {
                        path.pending_ruling_segments += 1;
                    }
                    path.current = Some(end);
                }
            }
            "re" => {
                path.pending_ruling_segments +=
                    rectangle_ruling_segments(&operation.operands, matrix);
                path.current = None;
            }
            "S" | "s" | "B" | "B*" | "b" | "b*" => {
                stroked_ruling_segments += path.pending_ruling_segments;
                path = VectorPathState::default();
            }
            "n" | "f" | "F" | "f*" => {
                path = VectorPathState::default();
            }
            _ => {}
        }
    }

    ruling_density(stroked_ruling_segments)
}
fn raw_ruled_table_line_density_hint(content: &[u8]) -> Option<f32> {
    let mut matrix = PdfMatrix::identity();
    let mut matrix_stack = Vec::new();
    let mut path = VectorPathState::default();
    let mut stroked_ruling_segments = 0u32;
    let mut operands = Vec::with_capacity(8);

    for token in RawContentTokens::new(content) {
        if let Some(number) = raw_number_token(token) {
            operands.push(number);
            continue;
        }

        match token {
            b"BI" | b"ID" => return None,
            b"q" => matrix_stack.push(matrix),
            b"Q" => {
                matrix = matrix_stack.pop().unwrap_or_else(PdfMatrix::identity);
                path = VectorPathState::default();
            }
            b"cm" => {
                if let Some(next) = raw_matrix_from_operands(&operands) {
                    matrix = matrix.multiply(next);
                }
            }
            b"m" => {
                path.current =
                    raw_two_float_operands(&operands).map(|(x, y)| matrix.transform_point(x, y));
            }
            b"l" => {
                if let (Some(start), Some((x, y))) =
                    (path.current, raw_two_float_operands(&operands))
                {
                    let end = matrix.transform_point(x, y);
                    if is_ruling_segment(start, end) {
                        path.pending_ruling_segments += 1;
                    }
                    path.current = Some(end);
                }
            }
            b"re" => {
                if let Some((x, y, width, height)) = raw_rectangle_operands(&operands) {
                    path.pending_ruling_segments +=
                        rectangle_ruling_segments_from_values(x, y, width, height, matrix);
                }
            }
            b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*" => {
                stroked_ruling_segments += path.pending_ruling_segments;
                if stroked_ruling_segments >= RULED_TABLE_SATURATION_SEGMENTS {
                    return Some(1.0);
                }
                path = VectorPathState::default();
            }
            b"n" | b"f" | b"F" | b"f*" => {
                path = VectorPathState::default();
            }
            _ => {}
        }
        operands.clear();
    }

    Some(ruling_density(stroked_ruling_segments))
}

fn raw_number_token(token: &[u8]) -> Option<f32> {
    let text = std::str::from_utf8(token).ok()?;
    let number = text.parse::<f32>().ok()?;
    number.is_finite().then_some(number)
}

fn raw_two_float_operands(operands: &[f32]) -> Option<(f32, f32)> {
    let start = operands.len().checked_sub(2)?;
    Some((operands[start], operands[start + 1]))
}

fn raw_rectangle_operands(operands: &[f32]) -> Option<(f32, f32, f32, f32)> {
    let start = operands.len().checked_sub(4)?;
    Some((
        operands[start],
        operands[start + 1],
        operands[start + 2],
        operands[start + 3],
    ))
}

fn raw_matrix_from_operands(operands: &[f32]) -> Option<PdfMatrix> {
    let start = operands.len().checked_sub(6)?;
    Some(PdfMatrix {
        a: operands[start],
        b: operands[start + 1],
        c: operands[start + 2],
        d: operands[start + 3],
        e: operands[start + 4],
        f: operands[start + 5],
    })
}

struct RawContentTokens<'a> {
    content: &'a [u8],
    offset: usize,
}

impl<'a> RawContentTokens<'a> {
    fn new(content: &'a [u8]) -> Self {
        Self { content, offset: 0 }
    }

    fn skip_delimiters(&mut self) {
        while self.offset < self.content.len() {
            match self.content[self.offset] {
                b'%' => {
                    self.offset += 1;
                    while self.offset < self.content.len()
                        && !matches!(self.content[self.offset], b'\n' | b'\r')
                    {
                        self.offset += 1;
                    }
                }
                byte if byte.is_ascii_whitespace()
                    || matches!(byte, b'[' | b']' | b'{' | b'}' | b'/') =>
                {
                    self.offset += 1;
                }
                _ => break,
            }
        }
    }

    fn skip_literal_string(&mut self) {
        let mut depth = 1usize;
        self.offset += 1;
        while self.offset < self.content.len() && depth > 0 {
            match self.content[self.offset] {
                b'\\' => {
                    self.offset = (self.offset + 2).min(self.content.len());
                }
                b'(' => {
                    depth += 1;
                    self.offset += 1;
                }
                b')' => {
                    depth -= 1;
                    self.offset += 1;
                }
                _ => self.offset += 1,
            }
        }
    }

    fn skip_hex_string(&mut self) {
        self.offset += 1;
        while self.offset < self.content.len() {
            let byte = self.content[self.offset];
            self.offset += 1;
            if byte == b'>' {
                break;
            }
        }
    }
}

impl<'a> Iterator for RawContentTokens<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.skip_delimiters();
            if self.offset >= self.content.len() {
                return None;
            }

            match self.content[self.offset] {
                b'(' => {
                    self.skip_literal_string();
                    continue;
                }
                b'<' if self
                    .content
                    .get(self.offset + 1)
                    .is_none_or(|byte| *byte != b'<') =>
                {
                    self.skip_hex_string();
                    continue;
                }
                _ => {}
            }

            let start = self.offset;
            while self.offset < self.content.len()
                && !self.content[self.offset].is_ascii_whitespace()
                && !matches!(
                    self.content[self.offset],
                    b'[' | b']' | b'{' | b'}' | b'/' | b'(' | b')' | b'<' | b'>' | b'%'
                )
            {
                self.offset += 1;
            }

            if self.offset > start {
                return Some(&self.content[start..self.offset]);
            }

            self.offset += 1;
        }
    }
}
fn rectangle_ruling_segments(operands: &[Object], matrix: PdfMatrix) -> u32 {
    let Some(x) = operands.first().and_then(float_operand) else {
        return 0;
    };
    let Some(y) = operands.get(1).and_then(float_operand) else {
        return 0;
    };
    let Some(width) = operands.get(2).and_then(float_operand) else {
        return 0;
    };
    let Some(height) = operands.get(3).and_then(float_operand) else {
        return 0;
    };

    rectangle_ruling_segments_from_values(x, y, width, height, matrix)
}

fn rectangle_ruling_segments_from_values(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    matrix: PdfMatrix,
) -> u32 {
    let lower_left = matrix.transform_point(x, y);
    let lower_right = matrix.transform_point(x + width, y);
    let upper_right = matrix.transform_point(x + width, y + height);
    let upper_left = matrix.transform_point(x, y + height);

    [
        (lower_left, lower_right),
        (lower_right, upper_right),
        (upper_right, upper_left),
        (upper_left, lower_left),
    ]
    .into_iter()
    .filter(|(start, end)| is_ruling_segment(*start, *end))
    .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruled_table_line_density_saturates_from_raw_ruling_hint_before_full_decode() {
        let mut content = String::new();
        for y in 0..20 {
            content.push_str(&format!("0 {y} m 120 {y} l S\n"));
        }
        content.push_str("BT /F1 12 Tf 72 720 Td (unterminated text");

        assert_eq!(
            raw_ruled_table_line_density_hint(content.as_bytes()),
            Some(1.0)
        );
    }

    #[test]
    fn raw_ruled_table_line_density_hint_detects_non_saturated_rulings() {
        let content = [
            "72 600 m 360 600 l S",
            "72 560 m 360 560 l S",
            "72 520 m 360 520 l S",
            "72 480 m 360 480 l S",
            "72 480 m 72 600 l S",
            "216 480 m 216 600 l S",
            "360 480 m 360 600 l S",
        ]
        .join("\n");

        assert_eq!(
            raw_ruled_table_line_density_hint(content.as_bytes()),
            Some(0.35)
        );
    }

    #[test]
    fn raw_ruled_table_line_density_hint_returns_zero_for_text_only_streams() {
        let content = b"BT /F1 12 Tf 72 720 Td (line l S re text) Tj ET";

        assert_eq!(raw_ruled_table_line_density_hint(content), Some(0.0));
    }

    #[test]
    fn raw_image_draw_ops_preserve_xobject_names_and_transforms() {
        let content = b"q 10 0 0 20 30 40 cm /Im1 Do Q";
        let draws = raw_image_draw_ops(content).expect("simple image ops are supported");

        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].name, b"Im1");
        assert_eq!(draws[0].state.transform_point(1.0, 1.0), (40.0, 60.0));
    }

    #[test]
    fn raw_image_draw_ops_defers_inline_images_to_full_decoder() {
        let content = b"q BI /W 1 /H 1 /BPC 1 ID \x00 EI Q";

        assert!(raw_image_draw_ops(content).is_none());
    }
}
