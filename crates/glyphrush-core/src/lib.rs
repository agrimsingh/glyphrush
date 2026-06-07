//! Core types and pipeline primitives for Glyphrush.

use std::{
    collections::{BTreeSet, HashMap},
    fmt::Write as _,
    time::Instant,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    fn with_flags(flags: Vec<PageQuality>) -> Self {
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
            route: RouteDecision::default(),
            quality: PageQualityReport::default(),
            timings: PageTimings::default(),
        }
    }

    fn assign_artifact_id(&mut self, document_fingerprint: &str) {
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

fn document_warnings(pages: &[PageArtifact]) -> Vec<String> {
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

fn is_unsupported_feature_reason(reason: &str) -> bool {
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

fn default_worker_count() -> usize {
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
    pub fn empty(page_index: u32, dimensions: PageDimensions) -> Self {
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
    fn native_fast_path() -> Self {
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

pub fn classify_page(signals: &PageSignals) -> RouteDecision {
    let mut flags = Vec::new();
    let mut run_ocr = false;
    let mut run_heavy_layout = false;
    let mut run_table_recovery = false;
    let mut reasons = Vec::new();

    let has_native_text = signals.native_span_count > 0 && signals.native_text_bytes > 0;
    let sparse_native_text = has_native_text && signals.native_text_bytes < 128;
    let scan_like = (!has_native_text || sparse_native_text) && signals.image_area_ratio >= 0.70;
    let image_text_overlay =
        has_native_text && !sparse_native_text && signals.image_area_ratio >= 0.90;

    if scan_like {
        flags.push(PageQuality::RequiresOcr);
        flags.push(PageQuality::LowConfidenceText);
        reasons.push(if sparse_native_text {
            "high_image_coverage_with_sparse_native_text".to_string()
        } else {
            "high_image_coverage_without_native_text".to_string()
        });
        run_ocr = true;
    }

    if image_text_overlay {
        flags.push(PageQuality::LayoutUncertain);
        reasons.push("image_text_overlay".to_string());
        run_heavy_layout = true;
    }

    if signals.broken_encoding_ratio >= 0.20 {
        flags.push(PageQuality::BrokenEncoding);
        flags.push(PageQuality::LowConfidenceText);
        reasons.push("broken_encoding".to_string());
        run_heavy_layout = true;
        if signals.image_area_ratio >= 0.70 {
            flags.push(PageQuality::RequiresOcr);
            if !run_ocr {
                reasons.push("broken_encoding_with_image_coverage".to_string());
            }
            run_ocr = true;
        }
    }

    let layout_uncertain = signals.bbox_overlap_ratio >= 0.25
        || signals.duplicate_char_ratio >= 0.18
        || signals.rotation_degrees.rem_euclid(360) != 0;
    if layout_uncertain {
        flags.push(PageQuality::LayoutUncertain);
        if signals.bbox_overlap_ratio >= 0.25 {
            reasons.push("bbox_overlap".to_string());
        }
        if signals.duplicate_char_ratio >= 0.18 {
            reasons.push("duplicate_char_ratio".to_string());
        }
        if signals.rotation_degrees.rem_euclid(360) != 0 {
            reasons.push("rotated_page".to_string());
        }
        run_heavy_layout = true;
    }

    if signals.table_line_density >= 0.25 {
        flags.push(PageQuality::TableUncertain);
        reasons.push("table_line_density".to_string());
        run_table_recovery = true;
    }

    if signals.annotation_count > 0 || signals.form_field_count > 0 {
        flags.push(PageQuality::UnsupportedFeature);
        reasons.push("annotation_or_form".to_string());
    }

    if signals.huge_object_count > 64 {
        flags.push(PageQuality::UnsupportedFeature);
        reasons.push("huge_object_count".to_string());
    }

    if signals.span_geometry_capped {
        flags.push(PageQuality::UnsupportedFeature);
        reasons.push("span_geometry_capped".to_string());
    }

    dedupe_flags(&mut flags);

    if flags.is_empty() {
        return RouteDecision::native_fast_path();
    }

    let route = if flags.contains(&PageQuality::UnsupportedFeature) {
        PageRoute::Unsupported
    } else if run_ocr {
        PageRoute::OcrFallback
    } else {
        PageRoute::NeedsFallback
    };

    RouteDecision {
        route,
        run_ocr,
        run_heavy_layout,
        run_table_recovery,
        flags,
        reasons,
    }
}

pub fn quality_from_decision(decision: &RouteDecision) -> PageQualityReport {
    PageQualityReport::with_flags(decision.flags.clone())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedPage {
    pub page_index: u32,
    pub dimensions: PageDimensions,
    pub native_text: String,
    pub native_spans: Vec<ExtractedTextSpan>,
    pub image_artifacts: Vec<ExtractedImage>,
    pub signals: PageSignals,
    pub ocr_text: Option<String>,
    pub timings: PageTimings,
}

pub fn parse_extracted_pages(
    document_fingerprint: String,
    pages: Vec<ExtractedPage>,
) -> DocumentArtifact {
    let mut page_artifacts: Vec<PageArtifact> = pages
        .into_iter()
        .map(|page| {
            let classify_start = Instant::now();
            let decision = classify_page(&page.signals);
            let run_ocr = decision.run_ocr;
            let classify_us = classify_start
                .elapsed()
                .as_micros()
                .max(1)
                .min(u64::MAX as u128) as u64;
            let page_component = page_component_hash(&page);
            let mut artifact = PageArtifact::empty(
                page.page_index,
                page.dimensions.clone(),
                PageFingerprint::from_parts(
                    &document_fingerprint,
                    page.page_index,
                    &page_component,
                ),
            );

            artifact.timings = page.timings;
            artifact.quality = quality_from_decision(&decision);
            artifact.route = decision;
            artifact.signals = page.signals.clone();
            artifact.timings.classify_us = classify_us;
            artifact.image_artifacts = page
                .image_artifacts
                .into_iter()
                .enumerate()
                .map(|(image_index, image)| ImageArtifact {
                    image_id: format!("p{:06}:im{image_index:06}", page.page_index),
                    bbox: image.bbox,
                    area_ratio: image.area_ratio,
                    source_name: image.source_name,
                })
                .collect();
            let layout_start = Instant::now();
            if page.native_spans.is_empty() && !page.native_text.is_empty() {
                artifact.native_spans.push(TextSpan {
                    text: page.native_text,
                    bbox: BBox {
                        x0: 0.0,
                        y0: 0.0,
                        x1: page.dimensions.width,
                        y1: page.dimensions.height,
                    },
                    confidence: artifact.quality.text_confidence,
                    provenance: SpanProvenance::Native,
                });
            } else {
                artifact
                    .native_spans
                    .extend(page.native_spans.into_iter().map(|span| TextSpan {
                        text: span.text,
                        bbox: span.bbox,
                        confidence: artifact.quality.text_confidence,
                        provenance: SpanProvenance::Native,
                    }));
            }
            let mut layout_text = artifact
                .native_spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if run_ocr
                && let Some(ocr_text) = page.ocr_text
                && !ocr_text.trim().is_empty()
            {
                artifact.ocr_spans.push(TextSpan {
                    text: ocr_text.clone(),
                    bbox: BBox {
                        x0: 0.0,
                        y0: 0.0,
                        x1: page.dimensions.width,
                        y1: page.dimensions.height,
                    },
                    confidence: 70,
                    provenance: SpanProvenance::Ocr,
                });
                if layout_text.trim().is_empty() {
                    layout_text = ocr_text;
                }
            }
            let run_table_recovery = artifact.route.run_table_recovery;
            artifact.layout_blocks = if !artifact.native_spans.is_empty() {
                layout_blocks_from_native_spans(
                    page.page_index,
                    page.dimensions,
                    &artifact.native_spans,
                    run_table_recovery,
                )
            } else {
                layout_blocks_from_text(
                    page.page_index,
                    page.dimensions,
                    &layout_text,
                    run_table_recovery,
                )
            };
            if !artifact.layout_blocks.is_empty() {
                artifact.timings.layout_us = layout_start
                    .elapsed()
                    .as_micros()
                    .max(1)
                    .min(u64::MAX as u128) as u64;
            }

            artifact
        })
        .collect();

    classify_repeated_margin_blocks(&mut page_artifacts);

    DocumentArtifact::new(document_fingerprint, page_artifacts)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MarginRole {
    Header,
    Footer,
}

fn classify_repeated_margin_blocks(pages: &mut [PageArtifact]) {
    if pages.len() < 2 {
        return;
    }

    let mut occurrences: HashMap<(MarginRole, String), BTreeSet<u32>> = HashMap::new();
    for page in pages.iter() {
        for block in &page.layout_blocks {
            let Some(role) = margin_role(block, &page.dimensions) else {
                continue;
            };
            let normalized = normalized_layout_text(&block.text);
            if normalized.len() < 4 {
                continue;
            }
            occurrences
                .entry((role, normalized))
                .or_default()
                .insert(page.page_index);
        }
    }

    for page in pages {
        for block in &mut page.layout_blocks {
            let Some(role) = margin_role(block, &page.dimensions) else {
                continue;
            };
            let normalized = normalized_layout_text(&block.text);
            let repeated_pages = occurrences
                .get(&(role, normalized))
                .map(BTreeSet::len)
                .unwrap_or(0);
            if repeated_pages < 2 {
                continue;
            }

            block.kind = match role {
                MarginRole::Header => LayoutBlockKind::Header,
                MarginRole::Footer => LayoutBlockKind::Footer,
            };
        }
    }
}

fn margin_role(block: &LayoutBlock, dimensions: &PageDimensions) -> Option<MarginRole> {
    if block.kind == LayoutBlockKind::Table || dimensions.height <= 0.0 {
        return None;
    }

    let margin_height = dimensions.height * 0.12;
    if block.bbox.y1 <= margin_height {
        return Some(MarginRole::Header);
    }
    if block.bbox.y0 >= dimensions.height - margin_height {
        return Some(MarginRole::Footer);
    }
    None
}

fn normalized_layout_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn page_component_hash(page: &ExtractedPage) -> String {
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

fn push_component(payload: &mut String, key: &str, value: &str) {
    let _ = writeln!(payload, "{key}\0{value}");
}

fn push_u32_component(payload: &mut String, key: &str, value: u32) {
    let _ = writeln!(payload, "{key}\0{value}");
}

fn push_f32_component(payload: &mut String, key: &str, value: f32) {
    let _ = writeln!(payload, "{key}\0{:08x}", value.to_bits());
}

fn push_bbox_component(payload: &mut String, key: &str, bbox: &BBox) {
    push_f32_component(payload, &format!("{key}.x0"), bbox.x0);
    push_f32_component(payload, &format!("{key}.y0"), bbox.y0);
    push_f32_component(payload, &format!("{key}.x1"), bbox.x1);
    push_f32_component(payload, &format!("{key}.y1"), bbox.y1);
}

fn dedupe_flags(flags: &mut Vec<PageQuality>) {
    let mut deduped = Vec::with_capacity(flags.len());
    for flag in flags.drain(..) {
        if !deduped.contains(&flag) {
            deduped.push(flag);
        }
    }
    *flags = deduped;
}

fn layout_blocks_from_text(
    page_index: u32,
    dimensions: PageDimensions,
    text: &str,
    run_table_recovery: bool,
) -> Vec<LayoutBlock> {
    split_text_blocks(text, run_table_recovery)
        .into_iter()
        .enumerate()
        .map(|(block_index, block_text)| LayoutBlock {
            block_id: format!("p{page_index:06}:b{block_index:06}"),
            bbox: BBox {
                x0: 0.0,
                y0: 0.0,
                x1: dimensions.width,
                y1: dimensions.height,
            },
            kind: classify_layout_block(&block_text, run_table_recovery),
            text: block_text,
        })
        .collect()
}

fn layout_blocks_from_native_spans(
    page_index: u32,
    dimensions: PageDimensions,
    spans: &[TextSpan],
    run_table_recovery: bool,
) -> Vec<LayoutBlock> {
    if let [span] = spans
        && is_page_wide_span(span, &dimensions)
    {
        return layout_blocks_from_text(page_index, dimensions, &span.text, run_table_recovery);
    }

    if run_table_recovery
        && let Some(table_block) = table_block_from_positioned_spans(page_index, spans)
    {
        return vec![table_block];
    }

    let grouped_spans = group_spans_for_reading_order(spans, &dimensions);
    if grouped_spans.is_empty() {
        let text = spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return layout_blocks_from_text(page_index, dimensions, &text, run_table_recovery);
    }

    grouped_spans
        .into_iter()
        .enumerate()
        .filter_map(|(block_index, group)| {
            let bbox = union_span_refs_bbox(&group)?;
            let lines = group
                .iter()
                .map(|span| span.text.trim().to_string())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>();
            let text = reflow_text_block(&lines, run_table_recovery);

            Some(LayoutBlock {
                block_id: format!("p{page_index:06}:b{block_index:06}"),
                bbox,
                kind: classify_layout_block(&text, run_table_recovery),
                text,
            })
        })
        .collect()
}

fn table_block_from_positioned_spans(page_index: u32, spans: &[TextSpan]) -> Option<LayoutBlock> {
    let span_refs = spans
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();
    if span_refs.len() < 4 {
        return None;
    }

    let rows = group_positioned_table_rows(span_refs);
    if rows.len() < 2 {
        return None;
    }

    let column_count = rows.first()?.len();
    if !(2..=8).contains(&column_count) || !rows.iter().all(|row| row.len() == column_count) {
        return None;
    }
    if !table_rows_have_aligned_columns(&rows) {
        return None;
    }

    let bbox = union_span_refs_bbox(&rows.iter().flatten().copied().collect::<Vec<_>>())?;
    let text = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|span| span.text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(LayoutBlock {
        block_id: format!("p{page_index:06}:b000000"),
        bbox,
        text,
        kind: LayoutBlockKind::Table,
    })
}

fn group_positioned_table_rows(mut spans: Vec<&TextSpan>) -> Vec<Vec<&TextSpan>> {
    spans.sort_by(|left, right| {
        span_center_y(left)
            .total_cmp(&span_center_y(right))
            .then_with(|| left.bbox.x0.total_cmp(&right.bbox.x0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let tolerance = table_row_y_tolerance(&spans);
    let mut rows: Vec<Vec<&TextSpan>> = Vec::new();
    for span in spans {
        if let Some(row) = rows.last_mut()
            && (span_center_y(span) - row_center_y(row)).abs() <= tolerance
        {
            row.push(span);
            continue;
        }
        rows.push(vec![span]);
    }

    for row in &mut rows {
        row.sort_by(|left, right| {
            left.bbox
                .x0
                .total_cmp(&right.bbox.x0)
                .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
                .then_with(|| left.text.cmp(&right.text))
        });
    }
    rows
}

fn table_row_y_tolerance(spans: &[&TextSpan]) -> f32 {
    let mut heights = spans
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .filter(|height| *height > 0.0 && height.is_finite())
        .collect::<Vec<_>>();
    heights.sort_by(f32::total_cmp);

    let median_height = heights
        .get(heights.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(12.0);

    (median_height * 0.75).max(6.0)
}

fn table_rows_have_aligned_columns(rows: &[Vec<&TextSpan>]) -> bool {
    let Some(column_count) = rows.first().map(Vec::len) else {
        return false;
    };
    let tolerance = table_column_x_tolerance(rows);

    (0..column_count).all(|column_index| {
        let x0_values = rows
            .iter()
            .map(|row| row[column_index].bbox.x0)
            .collect::<Vec<_>>();
        let x1_values = rows
            .iter()
            .map(|row| row[column_index].bbox.x1)
            .collect::<Vec<_>>();

        spread(&x0_values) <= tolerance || spread(&x1_values) <= tolerance
    })
}

fn table_column_x_tolerance(rows: &[Vec<&TextSpan>]) -> f32 {
    let mut widths = rows
        .iter()
        .flatten()
        .map(|span| span.bbox.x1 - span.bbox.x0)
        .filter(|width| *width > 0.0 && width.is_finite())
        .collect::<Vec<_>>();
    widths.sort_by(f32::total_cmp);

    let median_width = widths
        .get(widths.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(48.0);

    (median_width * 0.75).max(24.0)
}

fn spread(values: &[f32]) -> f32 {
    let min = values.iter().copied().min_by(f32::total_cmp).unwrap_or(0.0);
    let max = values.iter().copied().max_by(f32::total_cmp).unwrap_or(0.0);
    max - min
}

fn span_center_y(span: &TextSpan) -> f32 {
    (span.bbox.y0 + span.bbox.y1) / 2.0
}

fn row_center_y(row: &[&TextSpan]) -> f32 {
    let top = row
        .iter()
        .map(|span| span.bbox.y0)
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    let bottom = row
        .iter()
        .map(|span| span.bbox.y1)
        .max_by(f32::total_cmp)
        .unwrap_or(0.0);
    (top + bottom) / 2.0
}

fn is_page_wide_span(span: &TextSpan, dimensions: &PageDimensions) -> bool {
    nearly_equal(span.bbox.x0, 0.0)
        && nearly_equal(span.bbox.y0, 0.0)
        && nearly_equal(span.bbox.x1, dimensions.width)
        && nearly_equal(span.bbox.y1, dimensions.height)
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.001
}

fn group_spans_for_reading_order<'a>(
    spans: &'a [TextSpan],
    dimensions: &PageDimensions,
) -> Vec<Vec<&'a TextSpan>> {
    let span_refs = spans
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .collect::<Vec<_>>();

    if let Some((left_column, right_column)) = split_two_columns(&span_refs, dimensions) {
        let mut groups = group_span_refs_by_vertical_gaps(left_column);
        groups.extend(group_span_refs_by_vertical_gaps(right_column));
        return groups;
    }

    group_span_refs_by_vertical_gaps(span_refs)
}

fn split_two_columns<'a>(
    spans: &[&'a TextSpan],
    dimensions: &PageDimensions,
) -> Option<(Vec<&'a TextSpan>, Vec<&'a TextSpan>)> {
    if spans.len() < 4 || dimensions.width <= 0.0 {
        return None;
    }

    let mut sorted_spans = spans.to_vec();
    sorted_spans.sort_by(|left, right| {
        left.bbox
            .x0
            .total_cmp(&right.bbox.x0)
            .then_with(|| left.bbox.y0.total_cmp(&right.bbox.y0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let mut best_split = None;
    let mut best_gap = 0.0_f32;
    for split_index in 2..=sorted_spans.len().saturating_sub(2) {
        let left = &sorted_spans[..split_index];
        let right = &sorted_spans[split_index..];
        let Some(left_max_x1) = left.iter().map(|span| span.bbox.x1).max_by(f32::total_cmp) else {
            continue;
        };
        let Some(right_min_x0) = right.iter().map(|span| span.bbox.x0).min_by(f32::total_cmp)
        else {
            continue;
        };
        let gap = right_min_x0 - left_max_x1;
        if gap > best_gap {
            best_gap = gap;
            best_split = Some(split_index);
        }
    }

    if best_gap < two_column_min_gap(spans, dimensions) {
        return None;
    }

    let split_index = best_split?;
    let left = sorted_spans[..split_index].to_vec();
    let right = sorted_spans[split_index..].to_vec();
    Some((left, right))
}

fn two_column_min_gap(spans: &[&TextSpan], dimensions: &PageDimensions) -> f32 {
    let mut widths = spans
        .iter()
        .map(|span| span.bbox.x1 - span.bbox.x0)
        .filter(|width| *width > 0.0 && width.is_finite())
        .collect::<Vec<_>>();
    widths.sort_by(f32::total_cmp);

    let median_width = widths
        .get(widths.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(120.0);

    (dimensions.width * 0.08).max(median_width * 0.25).max(36.0)
}

fn group_span_refs_by_vertical_gaps(mut sorted_spans: Vec<&TextSpan>) -> Vec<Vec<&TextSpan>> {
    sorted_spans.sort_by(|left, right| {
        left.bbox
            .y0
            .total_cmp(&right.bbox.y0)
            .then_with(|| left.bbox.x0.total_cmp(&right.bbox.x0))
            .then_with(|| left.text.cmp(&right.text))
    });

    let split_gap = vertical_split_gap(&sorted_spans);
    let mut groups: Vec<Vec<&TextSpan>> = Vec::new();

    for span in sorted_spans {
        let starts_new_group = groups
            .last()
            .and_then(|group| group.iter().map(|span| span.bbox.y1).max_by(f32::total_cmp))
            .map(|current_bottom| span.bbox.y0 - current_bottom > split_gap)
            .unwrap_or(true);

        if starts_new_group {
            groups.push(Vec::new());
        }

        groups.last_mut().expect("group exists").push(span);
    }

    groups
}

fn vertical_split_gap(spans: &[&TextSpan]) -> f32 {
    let mut heights = spans
        .iter()
        .map(|span| span.bbox.y1 - span.bbox.y0)
        .filter(|height| *height > 0.0 && height.is_finite())
        .collect::<Vec<_>>();
    heights.sort_by(f32::total_cmp);

    let median_height = heights
        .get(heights.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(12.0);

    (median_height * 1.5).max(12.0)
}

fn union_span_refs_bbox(spans: &[&TextSpan]) -> Option<BBox> {
    let mut spans = spans.iter().filter(|span| !span.text.trim().is_empty());
    let first = spans.next()?;
    let mut bbox = first.bbox.clone();

    for span in spans {
        bbox.x0 = bbox.x0.min(span.bbox.x0);
        bbox.y0 = bbox.y0.min(span.bbox.y0);
        bbox.x1 = bbox.x1.max(span.bbox.x1);
        bbox.y1 = bbox.y1.max(span.bbox.y1);
    }

    Some(bbox)
}

fn split_text_blocks(text: &str, run_table_recovery: bool) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();

    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(reflow_text_block(&current, run_table_recovery));
                current.clear();
            }
        } else {
            current.push(line.trim().to_string());
        }
    }

    if !current.is_empty() {
        blocks.push(reflow_text_block(&current, run_table_recovery));
    }

    merge_adjacent_fragment_blocks(blocks)
}

fn reflow_text_block(lines: &[String], run_table_recovery: bool) -> String {
    if lines.len() <= 1
        || is_table_lines(lines)
        || (run_table_recovery && is_whitespace_table_lines(lines))
        || is_list_lines(lines)
        || !should_reflow(lines)
    {
        return lines.join("\n");
    }

    let mut output = String::new();
    for fragment in lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
    {
        append_reflow_fragment(&mut output, fragment);
    }
    output
}

fn merge_adjacent_fragment_blocks(blocks: Vec<String>) -> Vec<String> {
    let mut merged = Vec::new();
    let mut current_fragments: Vec<String> = Vec::new();

    for block in blocks {
        if is_fragment_block(&block) {
            current_fragments.extend(block.lines().map(|line| line.trim().to_string()));
            continue;
        }

        flush_fragment_blocks(&mut merged, &mut current_fragments);
        merged.push(block);
    }

    flush_fragment_blocks(&mut merged, &mut current_fragments);
    merged
}

fn flush_fragment_blocks(merged: &mut Vec<String>, fragments: &mut Vec<String>) {
    if fragments.is_empty() {
        return;
    }

    if let Some(previous) = merged.last_mut()
        && let Some(absorb_count) = absorb_fragment_prefix_len(previous, fragments)
    {
        let mut reflowed = previous.clone();
        for fragment in fragments
            .iter()
            .take(absorb_count)
            .map(|fragment| fragment.trim())
        {
            append_reflow_fragment(&mut reflowed, fragment);
        }
        *previous = reflowed;
        fragments.drain(..absorb_count);
        if fragments.is_empty() {
            return;
        }
    }

    for group in split_fragment_groups(fragments) {
        if group.len() == 1 {
            merged.push(group[0].clone());
        } else {
            merged.push(reflow_text_block(&group, false));
        }
    }
    fragments.clear();
}

fn split_fragment_groups(fragments: &[String]) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for fragment in fragments {
        if let Some(previous) = current.last()
            && starts_new_fragment_group(previous.as_str(), fragment.as_str())
        {
            groups.push(current);
            current = Vec::new();
        }
        current.push(fragment.clone());
    }

    if !current.is_empty() {
        groups.push(current);
    }

    groups
}

fn starts_new_fragment_group(previous: &str, next: &str) -> bool {
    previous.chars().all(|ch| ch.is_ascii_digit())
        && next
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
}

fn absorb_fragment_prefix_len(previous: &str, fragments: &[String]) -> Option<usize> {
    if previous.contains('\n') || is_table_lines_str(&[previous]) || is_list_lines_str(&[previous])
    {
        return None;
    }

    let last_token = previous.split_whitespace().last()?;

    if fragments.is_empty()
        || !is_short_fragment(last_token)
        || !last_token.chars().any(|ch| ch.is_ascii_digit())
    {
        return None;
    }

    let count = fragments
        .iter()
        .take_while(|fragment| {
            is_short_fragment(fragment) && fragment.chars().all(|ch| ch.is_ascii_digit())
        })
        .count();

    (count > 0).then_some(count)
}

fn is_fragment_block(block: &str) -> bool {
    let lines = block.lines().map(str::to_string).collect::<Vec<_>>();
    !lines.is_empty()
        && lines.iter().all(|line| is_short_fragment(line))
        && !is_table_lines(&lines)
        && !is_list_lines(&lines)
}

fn should_reflow(lines: &[String]) -> bool {
    lines.iter().all(|line| is_short_fragment(line))
        || lines.iter().skip(1).all(|line| is_short_fragment(line))
}

fn is_short_fragment(line: &str) -> bool {
    line.trim().chars().count() <= 8
}

fn append_reflow_fragment(output: &mut String, fragment: &str) {
    if output.is_empty() {
        output.push_str(fragment);
        return;
    }

    let previous = output.chars().last().unwrap_or_default();
    let next = fragment.chars().next().unwrap_or_default();

    if matches!(next, '.' | ',' | ':' | ';' | ')' | ']')
        || matches!(fragment, "-" | "/" | "–")
        || matches!(previous, '-' | '/' | '–')
        || (previous.is_numeric() && next.is_numeric())
    {
        output.push_str(fragment);
    } else {
        output.push(' ');
        output.push_str(fragment);
    }
}

fn classify_layout_block(text: &str, run_table_recovery: bool) -> LayoutBlockKind {
    let lines = text.lines().collect::<Vec<_>>();
    if is_table_lines_str(&lines) || (run_table_recovery && is_whitespace_table_lines_str(&lines)) {
        return LayoutBlockKind::Table;
    }
    if is_list_lines_str(&lines) {
        return LayoutBlockKind::List;
    }
    if lines.len() == 1 && is_heading_line(lines[0]) {
        return LayoutBlockKind::Heading;
    }

    LayoutBlockKind::Paragraph
}

fn is_table_lines(lines: &[String]) -> bool {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    is_table_lines_str(&refs)
}

fn is_table_lines_str(lines: &[&str]) -> bool {
    lines.len() >= 2
        && lines
            .iter()
            .all(|line| line.contains('|') || line.contains('\t'))
}

fn is_whitespace_table_lines(lines: &[String]) -> bool {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    is_whitespace_table_lines_str(&refs)
}

fn is_whitespace_table_lines_str(lines: &[&str]) -> bool {
    let rows = lines
        .iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if rows.len() < 2 {
        return false;
    }

    let Some(column_count) = rows.first().map(Vec::len) else {
        return false;
    };
    (2..=8).contains(&column_count)
        && rows.iter().all(|row| {
            row.len() == column_count
                && row
                    .iter()
                    .all(|cell| !cell.is_empty() && cell.chars().count() <= 40)
        })
}

fn is_list_lines(lines: &[String]) -> bool {
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    is_list_lines_str(&refs)
}

fn is_list_lines_str(lines: &[&str]) -> bool {
    !lines.is_empty() && lines.iter().all(|line| is_list_line(line.trim_start()))
}

fn is_list_line(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || line
            .split_once(". ")
            .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
            .is_some()
}

fn is_heading_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 80
        && trimmed.chars().any(char::is_alphabetic)
        && trimmed
            .chars()
            .filter(|ch| ch.is_alphabetic())
            .all(|ch| ch.is_uppercase())
}

fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
