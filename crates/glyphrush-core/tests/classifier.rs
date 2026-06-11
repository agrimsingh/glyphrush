mod common;

use glyphrush_core::{PageQuality, PageRoute, PageSignals, classify_page, quality_from_decision};

fn zeroed(page_index: u32) -> PageSignals {
    PageSignals::zeroed(page_index, common::dimensions())
}

#[test]
fn confident_native_text_page_stays_on_fast_path() {
    let decision = classify_page(&PageSignals {
        native_span_count: 48,
        native_text_bytes: 4_600,
        glyph_count: 4_200,
        image_area_ratio: 0.02,
        duplicate_char_ratio: 0.01,
        bbox_overlap_ratio: 0.02,
        table_line_density: 0.01,
        ..zeroed(0)
    });

    assert_eq!(decision.route, PageRoute::NativeFastPath);
    assert!(!decision.run_ocr);
    assert!(!decision.run_heavy_layout);
    assert!(!decision.run_table_recovery);
    assert!(decision.flags.is_empty());
}

#[test]
fn scanned_image_page_requires_ocr_instead_of_silent_success() {
    let decision = classify_page(&PageSignals {
        page_index: 4,
        image_area_ratio: 0.94,
        ..zeroed(4)
    });

    assert_eq!(decision.route, PageRoute::OcrFallback);
    assert!(decision.run_ocr);
    assert!(decision.flags.contains(&PageQuality::RequiresOcr));
    assert!(decision.flags.contains(&PageQuality::LowConfidenceText));
}

#[test]
fn image_heavy_sparse_native_text_requires_ocr_review() {
    let decision = classify_page(&PageSignals {
        page_index: 1,
        native_span_count: 1,
        native_text_bytes: 18,
        glyph_count: 18,
        image_area_ratio: 0.98,
        ..zeroed(1)
    });

    assert_eq!(decision.route, PageRoute::OcrFallback);
    assert!(decision.run_ocr);
    assert!(decision.flags.contains(&PageQuality::RequiresOcr));
    assert!(decision.flags.contains(&PageQuality::LowConfidenceText));
}

#[test]
fn image_heavy_substantial_native_text_flags_layout_review_without_ocr() {
    let decision = classify_page(&PageSignals {
        page_index: 1,
        native_span_count: 12,
        native_text_bytes: 1_024,
        glyph_count: 980,
        image_area_ratio: 0.98,
        duplicate_char_ratio: 0.01,
        bbox_overlap_ratio: 0.02,
        ..zeroed(1)
    });

    assert_eq!(decision.route, PageRoute::NeedsFallback);
    assert!(!decision.run_ocr);
    assert!(decision.run_heavy_layout);
    assert_eq!(decision.flags, [PageQuality::LayoutUncertain]);
    assert_eq!(decision.reasons, ["image_text_overlay"]);
}

#[test]
fn broken_encoding_is_flagged_and_avoids_fast_path() {
    let decision = classify_page(&PageSignals {
        page_index: 2,
        native_span_count: 15,
        native_text_bytes: 320,
        glyph_count: 900,
        image_area_ratio: 0.07,
        duplicate_char_ratio: 0.04,
        bbox_overlap_ratio: 0.08,
        broken_encoding_ratio: 0.42,
        table_line_density: 0.01,
        ..zeroed(2)
    });

    assert_eq!(decision.route, PageRoute::NeedsFallback);
    assert!(!decision.run_ocr);
    assert!(decision.run_heavy_layout);
    assert!(decision.flags.contains(&PageQuality::BrokenEncoding));
    assert!(decision.flags.contains(&PageQuality::LowConfidenceText));
    assert_eq!(decision.reasons, ["broken_encoding"]);
}

#[test]
fn image_backed_broken_encoding_requires_ocr_fallback() {
    let decision = classify_page(&PageSignals {
        page_index: 2,
        native_span_count: 15,
        native_text_bytes: 520,
        glyph_count: 1_100,
        image_area_ratio: 0.76,
        duplicate_char_ratio: 0.04,
        bbox_overlap_ratio: 0.08,
        broken_encoding_ratio: 0.42,
        table_line_density: 0.01,
        ..zeroed(2)
    });

    assert_eq!(decision.route, PageRoute::OcrFallback);
    assert!(decision.run_ocr);
    assert!(decision.run_heavy_layout);
    assert!(decision.flags.contains(&PageQuality::RequiresOcr));
    assert!(decision.flags.contains(&PageQuality::BrokenEncoding));
    assert!(decision.flags.contains(&PageQuality::LowConfidenceText));
    assert_eq!(
        decision.reasons,
        ["broken_encoding", "broken_encoding_with_image_coverage"]
    );
}

#[test]
fn table_dense_pages_request_table_recovery_without_ocr() {
    let decision = classify_page(&PageSignals {
        page_index: 3,
        native_span_count: 85,
        native_text_bytes: 7_100,
        glyph_count: 6_900,
        image_area_ratio: 0.04,
        duplicate_char_ratio: 0.03,
        bbox_overlap_ratio: 0.05,
        table_line_density: 0.37,
        ..zeroed(3)
    });

    assert_eq!(decision.route, PageRoute::NeedsFallback);
    assert!(!decision.run_ocr);
    assert!(decision.run_table_recovery);
    assert!(decision.flags.contains(&PageQuality::TableUncertain));
    assert_eq!(decision.reasons, ["table_line_density"]);
}

#[test]
fn table_uncertain_pages_lower_layout_confidence_without_lowering_text_confidence() {
    let decision = classify_page(&PageSignals {
        page_index: 3,
        native_span_count: 85,
        native_text_bytes: 7_100,
        glyph_count: 6_900,
        image_area_ratio: 0.04,
        duplicate_char_ratio: 0.03,
        bbox_overlap_ratio: 0.05,
        table_line_density: 0.37,
        ..zeroed(3)
    });
    let quality = quality_from_decision(&decision);

    assert_eq!(quality.flags, [PageQuality::TableUncertain]);
    assert_eq!(quality.text_confidence, 90);
    assert_eq!(quality.layout_confidence, 40);
}

#[test]
fn page_annotations_are_unsupported_instead_of_silent_success() {
    let decision = classify_page(&PageSignals {
        page_index: 5,
        native_span_count: 12,
        native_text_bytes: 1_200,
        glyph_count: 1_100,
        image_area_ratio: 0.01,
        duplicate_char_ratio: 0.01,
        bbox_overlap_ratio: 0.01,
        annotation_count: 1,
        ..zeroed(5)
    });

    assert_eq!(decision.route, PageRoute::Unsupported);
    assert!(!decision.run_ocr);
    assert!(decision.flags.contains(&PageQuality::UnsupportedFeature));
    assert_eq!(decision.reasons, ["annotation_or_form"]);
}

#[test]
fn single_huge_object_is_unsupported_instead_of_silent_success() {
    let decision = classify_page(&PageSignals {
        page_index: 6,
        native_span_count: 24,
        native_text_bytes: 2_400,
        glyph_count: 2_200,
        image_area_ratio: 0.02,
        duplicate_char_ratio: 0.01,
        bbox_overlap_ratio: 0.01,
        huge_object_count: 1,
        ..zeroed(6)
    });

    assert_eq!(decision.route, PageRoute::Unsupported);
    assert!(!decision.run_ocr);
    assert!(decision.flags.contains(&PageQuality::UnsupportedFeature));
    assert_eq!(decision.reasons, ["huge_object_count"]);
}
