use crate::*;

use web_time::Instant;

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
            let native_layout_text = artifact
                .native_spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let mut ocr_layout_text = None;
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
                ocr_layout_text = Some(ocr_text);
            }
            let run_table_recovery = artifact.route.run_table_recovery;
            let ruling_lines = page.ruling_lines.clone();
            artifact.layout_blocks = if let Some(layout_text) = ocr_layout_text.as_deref() {
                artifact.layout_strategy = Some("ocr_text".to_string());
                layout_blocks_from_text(
                    page.page_index,
                    page.dimensions,
                    layout_text,
                    run_table_recovery,
                )
            } else if !artifact.native_spans.is_empty() {
                let (blocks, layout_diagnostics) = layout_blocks_from_native_spans(
                    page.page_index,
                    page.dimensions,
                    &artifact.native_spans,
                    &ruling_lines,
                    run_table_recovery,
                );
                artifact.layout_strategy = Some(layout_diagnostics.strategy.to_string());
                if layout_diagnostics.column_layout_unresolved {
                    artifact.route.flags.push(PageQuality::LayoutUncertain);
                    artifact
                        .route
                        .reasons
                        .push("column_layout_unresolved".to_string());
                    dedupe_flags(&mut artifact.route.flags);
                    artifact.quality = quality_from_decision(&artifact.route);
                }
                blocks
            } else {
                artifact.layout_strategy = Some("text_fallback".to_string());
                layout_blocks_from_text(
                    page.page_index,
                    page.dimensions,
                    &native_layout_text,
                    run_table_recovery,
                )
            };
            if artifact.layout_blocks.is_empty()
                && let Some(figure_block) =
                    figure_block_from_image_artifacts(page.page_index, &artifact.image_artifacts)
            {
                artifact.layout_blocks.push(figure_block);
            }
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
pub(crate) fn layout_blocks_from_text(
    page_index: u32,
    dimensions: PageDimensions,
    text: &str,
    run_table_recovery: bool,
) -> Vec<LayoutBlock> {
    split_text_blocks(text, run_table_recovery)
        .into_iter()
        .enumerate()
        .map(|(block_index, block_text)| {
            let kind = classify_layout_block(&block_text, run_table_recovery);
            let table = table_payload_from_text(&block_text, &kind);
            LayoutBlock {
                block_id: format!("p{page_index:06}:b{block_index:06}"),
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: dimensions.width,
                    y1: dimensions.height,
                },
                text: block_text,
                kind,
                table,
            }
        })
        .collect()
}

pub(crate) fn figure_block_from_image_artifacts(
    page_index: u32,
    images: &[ImageArtifact],
) -> Option<LayoutBlock> {
    let mut images = images.iter();
    let first = images.next()?;
    let bbox = images.fold(first.bbox.clone(), |bbox, image| {
        union_bboxes(&bbox, &image.bbox)
    });

    Some(LayoutBlock {
        block_id: format!("p{page_index:06}:b000000"),
        bbox,
        text: String::new(),
        kind: LayoutBlockKind::Figure,
        table: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeLayoutDiagnostics {
    strategy: &'static str,
    column_layout_unresolved: bool,
}

impl NativeLayoutDiagnostics {
    pub(crate) fn resolved(strategy: &'static str) -> Self {
        Self {
            strategy,
            column_layout_unresolved: false,
        }
    }
}

pub(crate) fn layout_blocks_from_native_spans(
    page_index: u32,
    dimensions: PageDimensions,
    spans: &[TextSpan],
    ruling_lines: &[ExtractedRulingLine],
    run_table_recovery: bool,
) -> (Vec<LayoutBlock>, NativeLayoutDiagnostics) {
    if let [span] = spans
        && is_page_wide_span(span, &dimensions)
    {
        return (
            layout_blocks_from_text(page_index, dimensions, &span.text, run_table_recovery),
            NativeLayoutDiagnostics::resolved("page_wide_text"),
        );
    }

    if run_table_recovery
        && let Some(blocks) = layout_blocks_from_positioned_table_runs(
            page_index,
            dimensions.clone(),
            spans,
            ruling_lines,
        )
    {
        return (
            blocks,
            NativeLayoutDiagnostics::resolved("positioned_table_runs"),
        );
    }

    let (grouped_spans, strategy) = group_spans_for_reading_order(spans, &dimensions);
    if grouped_spans.is_empty() {
        let text = spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return (
            layout_blocks_from_text(page_index, dimensions, &text, run_table_recovery),
            NativeLayoutDiagnostics::resolved("text_fallback"),
        );
    }

    let column_layout_unresolved = strategy == ReadingOrderStrategy::VerticalGaps
        && has_unresolved_column_evidence(spans, &dimensions);
    let blocks = grouped_spans
        .into_iter()
        .enumerate()
        .filter_map(|(block_index, group)| {
            layout_block_from_span_group(page_index, block_index, group, run_table_recovery)
        })
        .collect();

    (
        blocks,
        NativeLayoutDiagnostics {
            strategy: strategy.as_str(),
            column_layout_unresolved,
        },
    )
}
pub(crate) fn union_bboxes(left: &BBox, right: &BBox) -> BBox {
    BBox {
        x0: left.x0.min(right.x0),
        y0: left.y0.min(right.y0),
        x1: left.x1.max(right.x1),
        y1: left.y1.max(right.y1),
    }
}

pub(crate) fn layout_block_from_span_group(
    page_index: u32,
    block_index: usize,
    group: Vec<&TextSpan>,
    run_table_recovery: bool,
) -> Option<LayoutBlock> {
    let bbox = union_span_refs_bbox(&group)?;
    let lines = text_lines_from_positioned_spans(&group);
    let text = reflow_text_block(&lines, run_table_recovery);

    let kind = classify_layout_block(&text, run_table_recovery);
    let table = table_payload_from_text(&text, &kind);

    Some(LayoutBlock {
        block_id: format!("p{page_index:06}:b{block_index:06}"),
        bbox,
        text,
        kind,
        table,
    })
}
