use crate::*;

use std::collections::{BTreeSet, HashMap};

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

    if signals.table_line_density >= TABLE_ROUTE_DENSITY_THRESHOLD {
        flags.push(PageQuality::TableUncertain);
        reasons.push("table_line_density".to_string());
        run_table_recovery = true;
    }

    if signals.annotation_count > 0 || signals.form_field_count > 0 {
        flags.push(PageQuality::UnsupportedFeature);
        reasons.push("annotation_or_form".to_string());
    }

    if signals.huge_object_count > 0 {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MarginRole {
    Header,
    Footer,
}

pub(crate) fn classify_repeated_margin_blocks(pages: &mut [PageArtifact]) {
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

pub(crate) fn margin_role(block: &LayoutBlock, dimensions: &PageDimensions) -> Option<MarginRole> {
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

pub(crate) fn normalized_layout_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub(crate) fn dedupe_flags(flags: &mut Vec<PageQuality>) {
    let mut deduped = Vec::with_capacity(flags.len());
    for flag in flags.drain(..) {
        if !deduped.contains(&flag) {
            deduped.push(flag);
        }
    }
    *flags = deduped;
}
