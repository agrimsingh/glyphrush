use crate::*;

use sha2::{Digest, Sha256};

/// Converts a PDF bottom-left-origin segment into a page-local top-left
/// ruling line. Thin rectangles report their midline as the position.
pub fn ruling_line_from_segment(
    start: (f32, f32),
    end: (f32, f32),
    dimensions: &PageDimensions,
) -> Option<ExtractedRulingLine> {
    let dx = (start.0 - end.0).abs();
    let dy = (start.1 - end.1).abs();

    if dy <= dx {
        let y = dimensions.height - (start.1 + end.1) / 2.0;
        Some(ExtractedRulingLine {
            orientation: RulingOrientation::Horizontal,
            position: y,
            start: start.0.min(end.0),
            end: start.0.max(end.0),
        })
    } else {
        let x = (start.0 + end.0) / 2.0;
        let y0 = dimensions.height - start.1.max(end.1);
        let y1 = dimensions.height - start.1.min(end.1);
        Some(ExtractedRulingLine {
            orientation: RulingOrientation::Vertical,
            position: x,
            start: y0,
            end: y1,
        })
    }
}
pub fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub const TABLE_ROUTE_DENSITY_THRESHOLD: f32 = 0.25;
pub const RULED_TABLE_SATURATION_SEGMENTS: u32 = 20;
pub const MAX_EXTRACTED_RULING_LINES: usize = 512;
pub(crate) const MAX_BBOX_OVERLAP_COMPARISONS: usize = 16_384;

#[derive(Clone, Copy, Debug)]
pub struct NormalizedBBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub area: f32,
}

pub fn positioned_bbox_overlap_ratio(spans: &[ExtractedTextSpan]) -> f32 {
    let mut boxes = spans
        .iter()
        .filter_map(|span| normalized_bbox(&span.bbox))
        .collect::<Vec<_>>();
    if boxes.len() < 2 {
        return 0.0;
    }

    boxes.sort_by(|left, right| {
        left.x0
            .total_cmp(&right.x0)
            .then_with(|| left.y0.total_cmp(&right.y0))
            .then_with(|| left.x1.total_cmp(&right.x1))
            .then_with(|| left.y1.total_cmp(&right.y1))
    });

    let total_area = boxes.iter().map(|bbox| bbox.area).sum::<f32>();
    if total_area <= f32::EPSILON {
        return 0.0;
    }

    let mut overlap_area = 0.0f32;
    let mut comparisons = 0usize;
    for (index, left) in boxes.iter().enumerate() {
        for right in boxes.iter().skip(index + 1) {
            if right.x0 >= left.x1 {
                break;
            }
            overlap_area += bbox_intersection_area(*left, *right);
            comparisons += 1;
            if overlap_area >= total_area || comparisons >= MAX_BBOX_OVERLAP_COMPARISONS {
                return (overlap_area / total_area).clamp(0.0, 1.0);
            }
        }
    }

    (overlap_area / total_area).clamp(0.0, 1.0)
}

pub(crate) fn normalized_bbox(bbox: &BBox) -> Option<NormalizedBBox> {
    let x0 = bbox.x0.min(bbox.x1);
    let x1 = bbox.x0.max(bbox.x1);
    let y0 = bbox.y0.min(bbox.y1);
    let y1 = bbox.y0.max(bbox.y1);
    if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
        return None;
    }

    let width = x1 - x0;
    let height = y1 - y0;
    let area = width * height;
    (area > f32::EPSILON).then_some(NormalizedBBox {
        x0,
        y0,
        x1,
        y1,
        area,
    })
}

pub(crate) fn bbox_intersection_area(left: NormalizedBBox, right: NormalizedBBox) -> f32 {
    let width = (left.x1.min(right.x1) - left.x0.max(right.x0)).max(0.0);
    let height = (left.y1.min(right.y1) - left.y0.max(right.y0)).max(0.0);
    width * height
}

pub fn image_artifact_coverage_ratio(
    image_artifacts: &[ExtractedImage],
    dimensions: &PageDimensions,
) -> f32 {
    let page_area = dimensions.width * dimensions.height;
    if image_artifacts.is_empty() || page_area <= f32::EPSILON {
        return 0.0;
    }

    let boxes = image_artifacts
        .iter()
        .filter_map(|image| clipped_image_bbox(&image.bbox, dimensions))
        .collect::<Vec<_>>();
    if boxes.is_empty() {
        return 0.0;
    }

    let mut xs = boxes
        .iter()
        .flat_map(|bbox| [bbox.x0, bbox.x1])
        .collect::<Vec<_>>();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);

    let mut covered_area = 0.0f32;
    for window in xs.windows(2) {
        let x0 = window[0];
        let x1 = window[1];
        let width = x1 - x0;
        if width <= f32::EPSILON {
            continue;
        }

        let mut intervals = boxes
            .iter()
            .filter(|bbox| bbox.x0 < x1 && bbox.x1 > x0)
            .map(|bbox| (bbox.y0, bbox.y1))
            .collect::<Vec<_>>();
        intervals.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });

        let mut covered_y = 0.0f32;
        let mut current: Option<(f32, f32)> = None;
        for (y0, y1) in intervals {
            match current {
                Some((current_y0, current_y1)) if y0 <= current_y1 => {
                    current = Some((current_y0, current_y1.max(y1)));
                }
                Some((current_y0, current_y1)) => {
                    covered_y += current_y1 - current_y0;
                    current = Some((y0, y1));
                }
                None => current = Some((y0, y1)),
            }
        }
        if let Some((y0, y1)) = current {
            covered_y += y1 - y0;
        }

        covered_area += width * covered_y;
    }

    (covered_area / page_area).clamp(0.0, 1.0)
}

pub(crate) fn clipped_image_bbox(
    bbox: &BBox,
    dimensions: &PageDimensions,
) -> Option<NormalizedBBox> {
    let x0 = bbox.x0.min(bbox.x1).clamp(0.0, dimensions.width);
    let x1 = bbox.x0.max(bbox.x1).clamp(0.0, dimensions.width);
    let y0 = bbox.y0.min(bbox.y1).clamp(0.0, dimensions.height);
    let y1 = bbox.y0.max(bbox.y1).clamp(0.0, dimensions.height);
    let area = (x1 - x0) * (y1 - y0);

    (area > f32::EPSILON).then_some(NormalizedBBox {
        x0,
        y0,
        x1,
        y1,
        area,
    })
}

pub fn broken_encoding_ratio(text: &str) -> f32 {
    let chars = text.chars().collect::<Vec<_>>();
    let total = chars.len();
    if total == 0 {
        return 0.0;
    }

    let replacement_or_control = chars
        .iter()
        .filter(|ch| **ch == '\u{fffd}' || (ch.is_control() && !ch.is_whitespace()))
        .count();
    let mojibake_pair_chars = chars
        .windows(2)
        .filter(|pair| pair[0] == '\u{00bf}' && pair[1] == '\u{2030}')
        .count()
        * 2;
    let broken = replacement_or_control + mojibake_pair_chars;
    broken as f32 / total as f32
}

pub fn duplicate_char_ratio(text: &str) -> f32 {
    let mut previous = None;
    let mut duplicate_runs = 0usize;
    let mut total = 0usize;

    for ch in text.chars().filter(|ch| !ch.is_whitespace()) {
        total += 1;
        if Some(ch) == previous {
            duplicate_runs += 1;
        }
        previous = Some(ch);
    }

    if total == 0 {
        0.0
    } else {
        duplicate_runs as f32 / total as f32
    }
}

pub(crate) fn table_line_density(text: &str) -> f32 {
    if has_explicit_text_table_header(text) {
        return TABLE_ROUTE_DENSITY_THRESHOLD;
    }

    let total = text.chars().filter(|ch| !ch.is_whitespace()).count();
    if total == 0 {
        return 0.0;
    }

    let table_like = text
        .chars()
        .filter(|ch| matches!(ch, '|' | '\t' | '+' | '-'))
        .count();
    table_like as f32 / total as f32
}

pub(crate) fn has_explicit_text_table_header(text: &str) -> bool {
    text.lines()
        .map(normalized_text_table_header_line)
        .any(|line| {
            matches!(
                line.as_str(),
                "parameter test condition min typ max unit"
                    | "parameter symbol conditions min typ max unit"
            )
        })
}

pub(crate) fn normalized_text_table_header_line(line: &str) -> String {
    line.split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn combined_table_line_density(
    native_text: &str,
    vector_table_density: impl FnOnce() -> f32,
) -> f32 {
    let native_density = table_line_density(native_text);
    if native_density >= TABLE_ROUTE_DENSITY_THRESHOLD {
        native_density
    } else {
        native_density.max(vector_table_density())
    }
}

pub fn ruling_density(stroked_ruling_segments: u32) -> f32 {
    (stroked_ruling_segments as f32 / RULED_TABLE_SATURATION_SEGMENTS as f32).clamp(0.0, 1.0)
}

pub fn is_ruling_segment(start: (f32, f32), end: (f32, f32)) -> bool {
    let dx = (start.0 - end.0).abs();
    let dy = (start.1 - end.1).abs();
    pub(crate) const AXIS_TOLERANCE: f32 = 1.0;
    pub(crate) const MIN_RULING_LENGTH: f32 = 24.0;

    (dx <= AXIS_TOLERANCE && dy >= MIN_RULING_LENGTH)
        || (dy <= AXIS_TOLERANCE && dx >= MIN_RULING_LENGTH)
}

pub fn normalize_text_for_span_check(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    pub(crate) fn table_signal_skips_vector_scan_when_native_text_already_routes_table_fallback() {
        let calls = Cell::new(0);

        let density = combined_table_line_density("||||||||||abcdefghij", || {
            calls.set(calls.get() + 1);
            1.0
        });

        assert!(density >= TABLE_ROUTE_DENSITY_THRESHOLD);
        assert_eq!(
            calls.get(),
            0,
            "native table signal should avoid expensive vector traversal once fallback is guaranteed"
        );
    }

    #[test]
    pub(crate) fn table_signal_uses_vector_scan_when_native_text_is_below_route_threshold() {
        let calls = Cell::new(0);

        let density = combined_table_line_density("plain paragraph text", || {
            calls.set(calls.get() + 1);
            0.75
        });

        assert_eq!(calls.get(), 1);
        assert_eq!(density, 0.75);
    }

    #[test]
    pub(crate) fn table_signal_routes_obvious_datasheet_electrical_headers_without_vector_scan() {
        let calls = Cell::new(0);
        let native_text = concat!(
            "Electrical Characteristics\n",
            "VIN=VOUT(SET)+1V, VCE>1V, IOUT=1mA\n",
            "PARAMETER TEST CONDITION MIN TYP MAX UNIT\n",
            "VIN Input Voltage Range 1.4 5.5 V\n",
            "VOUT_ACC Output Voltage Accuracy TA=25°C -1.3 1.3 %\n"
        );

        let density = combined_table_line_density(native_text, || {
            calls.set(calls.get() + 1);
            0.0
        });

        assert!(density >= TABLE_ROUTE_DENSITY_THRESHOLD);
        assert_eq!(
            calls.get(),
            0,
            "obvious native-text table headers should avoid expensive vector traversal"
        );
    }

    #[test]
    pub(crate) fn table_signal_routes_parameter_symbol_conditions_headers_without_vector_scan() {
        let calls = Cell::new(0);
        let native_text = concat!(
            "Electrical Characteristics\n",
            "(VIN=VOUT+1V, CIN=1µF, COUT=1µF)\n",
            "Parameter Symbol Conditions Min Typ Max Unit\n",
            "Input Voltage Range VIN 1.75 5.5 V\n",
            "Quiescent Current IQ IOUT=0A 2 4 µA\n"
        );

        let density = combined_table_line_density(native_text, || {
            calls.set(calls.get() + 1);
            0.0
        });

        assert!(density >= TABLE_ROUTE_DENSITY_THRESHOLD);
        assert_eq!(
            calls.get(),
            0,
            "parameter/symbol electrical table headers should avoid expensive vector traversal"
        );
    }
}
