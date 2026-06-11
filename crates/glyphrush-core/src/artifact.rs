use crate::*;

use serde::{Deserialize, Serialize};

use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageDimensions {
    pub width: f32,
    pub height: f32,
}

impl PageDimensions {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageFingerprint {
    hash_hex: String,
}

impl PageFingerprint {
    pub fn from_parts(document_fingerprint: &str, page_index: u32, page_fingerprint: &str) -> Self {
        Self {
            hash_hex: sha256_hex(format!(
                "{document_fingerprint}:{page_index}:{page_fingerprint}"
            )),
        }
    }

    pub fn as_hex(&self) -> &str {
        &self.hash_hex
    }

    pub fn short(&self) -> &str {
        &self.hash_hex[..12]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageQuality {
    RequiresOcr,
    LowConfidenceText,
    BrokenEncoding,
    LayoutUncertain,
    TableUncertain,
    UnsupportedFeature,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageQualityReport {
    pub flags: Vec<PageQuality>,
    pub text_confidence: u8,
    pub layout_confidence: u8,
}

impl PageQualityReport {
    pub(crate) fn with_flags(flags: Vec<PageQuality>) -> Self {
        let low_text = flags.contains(&PageQuality::LowConfidenceText);
        let uncertain_layout = flags.contains(&PageQuality::LayoutUncertain)
            || flags.contains(&PageQuality::TableUncertain);

        Self {
            flags,
            text_confidence: if low_text { 25 } else { 90 },
            layout_confidence: if uncertain_layout { 40 } else { 85 },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTimings {
    pub open_us: u64,
    pub classify_us: u64,
    pub native_extract_us: u64,
    pub layout_us: u64,
    pub table_us: u64,
    pub render_us: u64,
    pub ocr_us: u64,
    pub merge_us: u64,
}

impl PageTimings {
    pub fn total_us(&self) -> u64 {
        self.open_us
            + self.classify_us
            + self.native_extract_us
            + self.layout_us
            + self.table_us
            + self.render_us
            + self.ocr_us
            + self.merge_us
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageArtifact {
    pub artifact_id: String,
    pub page_index: u32,
    pub dimensions: PageDimensions,
    pub fingerprint: PageFingerprint,
    pub signals: PageSignals,
    pub native_spans: Vec<TextSpan>,
    pub ocr_spans: Vec<TextSpan>,
    pub image_artifacts: Vec<ImageArtifact>,
    pub layout_blocks: Vec<LayoutBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_strategy: Option<String>,
    pub route: RouteDecision,
    pub quality: PageQualityReport,
    pub timings: PageTimings,
}

impl PageArtifact {
    pub fn empty(
        page_index: u32,
        dimensions: PageDimensions,
        fingerprint: PageFingerprint,
    ) -> Self {
        Self {
            artifact_id: String::new(),
            page_index,
            dimensions: dimensions.clone(),
            fingerprint,
            signals: PageSignals::empty(page_index, dimensions),
            native_spans: Vec::new(),
            ocr_spans: Vec::new(),
            image_artifacts: Vec::new(),
            layout_blocks: Vec::new(),
            layout_strategy: None,
            route: RouteDecision::default(),
            quality: PageQualityReport::default(),
            timings: PageTimings::default(),
        }
    }

    pub(crate) fn assign_artifact_id(&mut self, document_fingerprint: &str) {
        self.artifact_id = format!(
            "{document_fingerprint}:p{:06}:{}",
            self.page_index,
            self.fingerprint.short()
        );
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    pub bbox: BBox,
    pub confidence: u8,
    pub provenance: SpanProvenance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedTextSpan {
    pub text: String,
    pub bbox: BBox,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedImage {
    pub bbox: BBox,
    pub area_ratio: f32,
    pub source_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageArtifact {
    pub image_id: String,
    pub bbox: BBox,
    pub area_ratio: f32,
    pub source_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutBlock {
    pub block_id: String,
    pub bbox: BBox,
    pub text: String,
    pub kind: LayoutBlockKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<LayoutTable>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutTable {
    pub rows: Vec<LayoutTableRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutTableRow {
    pub row_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    pub cells: Vec<LayoutTableCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutTableCell {
    pub column_index: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanProvenance {
    Native,
    Ocr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutBlockKind {
    Paragraph,
    Heading,
    List,
    Table,
    Figure,
    Header,
    Footer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentArtifact {
    pub document_fingerprint: String,
    #[serde(default)]
    pub metadata: DocumentMetadata,
    pub pages: Vec<PageArtifact>,
    pub global_diagnostics: GlobalDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub parser_name: String,
    pub parser_version: String,
    pub backend: String,
    pub backend_version: String,
    pub source_size_bytes: u64,
    #[serde(default)]
    pub source_modified_unix_ms: u64,
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self {
            parser_name: "glyphrush".to_string(),
            parser_version: env!("CARGO_PKG_VERSION").to_string(),
            backend: "unknown".to_string(),
            backend_version: "unknown".to_string(),
            source_size_bytes: 0,
            source_modified_unix_ms: 0,
        }
    }
}

impl DocumentArtifact {
    pub fn new(document_fingerprint: String, mut pages: Vec<PageArtifact>) -> Self {
        pages.sort_by_key(|page| page.page_index);
        for page in &mut pages {
            page.assign_artifact_id(&document_fingerprint);
        }

        let fallback_pages = pages
            .iter()
            .filter(|page| !page.quality.flags.is_empty())
            .count() as u32;
        let ocr_pages = pages
            .iter()
            .filter(|page| page.quality.flags.contains(&PageQuality::RequiresOcr))
            .count() as u32;
        let ocr_applied_pages = pages
            .iter()
            .filter(|page| !page.ocr_spans.is_empty())
            .count() as u32;
        let total_stage_time_us = pages.iter().map(|page| page.timings.total_us()).sum();
        let warnings = document_warnings(&pages);

        Self {
            document_fingerprint,
            metadata: DocumentMetadata::default(),
            pages,
            global_diagnostics: GlobalDiagnostics {
                fallback_pages,
                ocr_pages,
                ocr_required_pages: ocr_pages,
                ocr_applied_pages,
                worker_count: 1,
                cache_status: CacheStatus::Disabled,
                cache_key: None,
                total_stage_time_us,
                warnings,
            },
        }
    }

    pub fn with_metadata(mut self, metadata: DocumentMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

pub(crate) fn document_warnings(pages: &[PageArtifact]) -> Vec<String> {
    let mut warnings = Vec::new();
    for page in pages {
        if page.quality.flags.contains(&PageQuality::RequiresOcr) && page.ocr_spans.is_empty() {
            warnings.push(format!(
                "p{:06}: requires_ocr_without_ocr_output",
                page.page_index
            ));
        }

        if page
            .quality
            .flags
            .contains(&PageQuality::UnsupportedFeature)
        {
            let mut unsupported_reasons = page
                .route
                .reasons
                .iter()
                .filter(|reason| is_unsupported_feature_reason(reason))
                .peekable();
            if unsupported_reasons.peek().is_none() {
                warnings.push(format!("p{:06}: unsupported_feature", page.page_index));
            } else {
                warnings.extend(unsupported_reasons.map(|reason| {
                    format!("p{:06}: unsupported_feature: {reason}", page.page_index)
                }));
            }
        }
    }
    warnings
}

pub(crate) fn is_unsupported_feature_reason(reason: &str) -> bool {
    matches!(
        reason,
        "huge_object_count" | "span_geometry_capped" | "annotation_or_form"
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalDiagnostics {
    pub fallback_pages: u32,
    pub ocr_pages: u32,
    pub ocr_required_pages: u32,
    pub ocr_applied_pages: u32,
    #[serde(default = "default_worker_count")]
    pub worker_count: usize,
    pub cache_status: CacheStatus,
    pub cache_key: Option<String>,
    pub total_stage_time_us: u64,
    pub warnings: Vec<String>,
}

pub(crate) fn default_worker_count() -> usize {
    1
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    #[default]
    Disabled,
    Miss,
    Hit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSignals {
    pub page_index: u32,
    pub dimensions: PageDimensions,
    pub native_span_count: u32,
    pub native_text_bytes: u32,
    pub glyph_count: u32,
    pub image_area_ratio: f32,
    pub duplicate_char_ratio: f32,
    pub bbox_overlap_ratio: f32,
    pub broken_encoding_ratio: f32,
    pub rotation_degrees: i16,
    pub table_line_density: f32,
    pub annotation_count: u32,
    pub form_field_count: u32,
    pub huge_object_count: u32,
    pub span_geometry_capped: bool,
}

impl PageSignals {
    pub fn zeroed(page_index: u32, dimensions: PageDimensions) -> Self {
        Self {
            page_index,
            dimensions,
            native_span_count: 0,
            native_text_bytes: 0,
            glyph_count: 0,
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

    pub fn empty(page_index: u32, dimensions: PageDimensions) -> Self {
        Self::zeroed(page_index, dimensions)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageRoute {
    NativeFastPath,
    NeedsFallback,
    OcrFallback,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub route: PageRoute,
    pub run_ocr: bool,
    pub run_heavy_layout: bool,
    pub run_table_recovery: bool,
    pub flags: Vec<PageQuality>,
    pub reasons: Vec<String>,
}

impl RouteDecision {
    pub(crate) fn native_fast_path() -> Self {
        Self {
            route: PageRoute::NativeFastPath,
            run_ocr: false,
            run_heavy_layout: false,
            run_table_recovery: false,
            flags: Vec::new(),
            reasons: Vec::new(),
        }
    }
}

impl Default for RouteDecision {
    fn default() -> Self {
        Self::native_fast_path()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedPage {
    pub page_index: u32,
    pub dimensions: PageDimensions,
    pub native_text: String,
    pub native_spans: Vec<ExtractedTextSpan>,
    pub ruling_lines: Vec<ExtractedRulingLine>,
    pub image_artifacts: Vec<ExtractedImage>,
    pub signals: PageSignals,
    pub ocr_text: Option<String>,
    pub timings: PageTimings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulingOrientation {
    Horizontal,
    Vertical,
}

/// A stroked horizontal or vertical ruling segment in page-local top-left
/// coordinates. `position` is the constant coordinate (y for horizontal
/// lines, x for vertical lines); `start`/`end` bound the varying coordinate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedRulingLine {
    pub orientation: RulingOrientation,
    pub position: f32,
    pub start: f32,
    pub end: f32,
}
pub(crate) fn page_component_hash(page: &ExtractedPage) -> String {
    let mut payload = String::new();

    push_component(&mut payload, "native_text", &page.native_text);
    push_component(
        &mut payload,
        "ocr_text",
        page.ocr_text.as_deref().unwrap_or_default(),
    );
    push_f32_component(&mut payload, "dimensions.width", page.dimensions.width);
    push_f32_component(&mut payload, "dimensions.height", page.dimensions.height);
    push_u32_component(
        &mut payload,
        "signals.native_span_count",
        page.signals.native_span_count,
    );
    push_u32_component(
        &mut payload,
        "signals.native_text_bytes",
        page.signals.native_text_bytes,
    );
    push_u32_component(
        &mut payload,
        "signals.glyph_count",
        page.signals.glyph_count,
    );
    push_f32_component(
        &mut payload,
        "signals.image_area_ratio",
        page.signals.image_area_ratio,
    );
    push_f32_component(
        &mut payload,
        "signals.duplicate_char_ratio",
        page.signals.duplicate_char_ratio,
    );
    push_f32_component(
        &mut payload,
        "signals.bbox_overlap_ratio",
        page.signals.bbox_overlap_ratio,
    );
    push_f32_component(
        &mut payload,
        "signals.broken_encoding_ratio",
        page.signals.broken_encoding_ratio,
    );
    push_component(
        &mut payload,
        "signals.rotation_degrees",
        &page.signals.rotation_degrees.to_string(),
    );
    push_f32_component(
        &mut payload,
        "signals.table_line_density",
        page.signals.table_line_density,
    );
    push_u32_component(
        &mut payload,
        "signals.annotation_count",
        page.signals.annotation_count,
    );
    push_u32_component(
        &mut payload,
        "signals.form_field_count",
        page.signals.form_field_count,
    );
    push_u32_component(
        &mut payload,
        "signals.huge_object_count",
        page.signals.huge_object_count,
    );
    push_component(
        &mut payload,
        "signals.span_geometry_capped",
        if page.signals.span_geometry_capped {
            "true"
        } else {
            "false"
        },
    );
    push_u32_component(
        &mut payload,
        "native_spans.len",
        page.native_spans.len() as u32,
    );
    for (index, span) in page.native_spans.iter().enumerate() {
        push_component(
            &mut payload,
            &format!("native_spans.{index}.text"),
            &span.text,
        );
        push_bbox_component(
            &mut payload,
            &format!("native_spans.{index}.bbox"),
            &span.bbox,
        );
    }
    push_u32_component(
        &mut payload,
        "image_artifacts.len",
        page.image_artifacts.len() as u32,
    );
    for (index, image) in page.image_artifacts.iter().enumerate() {
        push_bbox_component(
            &mut payload,
            &format!("image_artifacts.{index}.bbox"),
            &image.bbox,
        );
        push_f32_component(
            &mut payload,
            &format!("image_artifacts.{index}.area_ratio"),
            image.area_ratio,
        );
        push_component(
            &mut payload,
            &format!("image_artifacts.{index}.source_name"),
            image.source_name.as_deref().unwrap_or_default(),
        );
    }

    sha256_hex(payload)
}

pub(crate) fn push_component(payload: &mut String, key: &str, value: &str) {
    let _ = writeln!(payload, "{key}\0{value}");
}

pub(crate) fn push_u32_component(payload: &mut String, key: &str, value: u32) {
    let _ = writeln!(payload, "{key}\0{value}");
}

pub(crate) fn push_f32_component(payload: &mut String, key: &str, value: f32) {
    let _ = writeln!(payload, "{key}\0{:08x}", value.to_bits());
}

pub(crate) fn push_bbox_component(payload: &mut String, key: &str, bbox: &BBox) {
    push_f32_component(payload, &format!("{key}.x0"), bbox.x0);
    push_f32_component(payload, &format!("{key}.y0"), bbox.y0);
    push_f32_component(payload, &format!("{key}.x1"), bbox.x1);
    push_f32_component(payload, &format!("{key}.y1"), bbox.y1);
}
