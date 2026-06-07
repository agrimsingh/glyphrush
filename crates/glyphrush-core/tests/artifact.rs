use glyphrush_core::{
    DocumentArtifact, PageArtifact, PageDimensions, PageFingerprint, PageQuality, PageTimings,
};

#[test]
fn document_artifact_sorts_pages_and_assigns_stable_page_ids() {
    let mut first = PageArtifact::empty(
        1,
        PageDimensions::new(612.0, 792.0),
        PageFingerprint::from_parts("doc-a", 1, "page-one"),
    );
    first.quality.flags.push(PageQuality::RequiresOcr);

    let second = PageArtifact::empty(
        0,
        PageDimensions::new(612.0, 792.0),
        PageFingerprint::from_parts("doc-a", 0, "page-zero"),
    );

    let artifact = DocumentArtifact::new("doc-a".to_string(), vec![first, second]);

    assert_eq!(
        artifact
            .pages
            .iter()
            .map(|page| page.page_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(artifact.pages[0].artifact_id, "doc-a:p000000:61dc6be295ee");
    assert_eq!(artifact.pages[1].artifact_id, "doc-a:p000001:104d101e6b81");
    assert_eq!(artifact.global_diagnostics.fallback_pages, 1);
}

#[test]
fn page_timings_report_total_stage_time_without_losing_stage_detail() {
    let timings = PageTimings {
        open_us: 12,
        classify_us: 20,
        native_extract_us: 40,
        layout_us: 8,
        table_us: 4,
        render_us: 0,
        ocr_us: 0,
        merge_us: 6,
    };

    assert_eq!(timings.total_us(), 90);
}
