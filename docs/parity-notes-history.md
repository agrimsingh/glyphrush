# Parity notes history

Detailed capability history formerly embedded in the feature-parity matrix `notes` fields.

## native_text_extraction

PDFium is the default fast backend when the pdfium feature is enabled; lopdf remains the dependency-light explicit backend and the auto fallback in plain builds.

## page_classifier_quality_flags

Glyphrush treats uncertain extraction as a reported condition instead of silently claiming success.

## structured_json_text_markdown_exports

The structured artifact is the source of truth; text and markdown are derived views.

## quality_backed_benchmarking

This is intentionally stronger than a speed-only comparison.

## span_geometry_layout

Glyphrush avoids always-on per-character metadata, preserves full-width bands, fragmented full-width heading rows, fragmented middle cross-column bands, fragmented short section separators, leading, middle, and trailing cross-column bands, conservative short section separators, narrow academic gutters with trailing centered page numbers, column-row bands that keep centered banners, gutter-straddling rows, and trailing page numbers out of column splits, and clearly separated 2-5 column reading order when span geometry is available, seeds bounded span-bbox manifest samples, reports the per-page reading-order strategy as layout_strategy, escalates layout work when signals require it, and flags unresolved multi-column evidence as layout_uncertain with a column_layout_unresolved reason instead of silently interleaving columns. Labeled real-PDF reading-order and span-bbox fixtures gate this in test/corpus.v0.layout.json.

## ocr

OCR is adapter-based, supports sidecar, generic command, HTTP endpoint, and an explicit local Tesseract rendered-image wrapper, invokes adapters only for OCR-routed pages, exposes ocr-check preflights, and stays outside the default hot path.

## page_render_for_ocr (PDFium backend)

PDFium renders only OCR-routed pages to temporary PPM files for command or HTTP adapters, records render timing and fallback-action counts, and removes temporary image files after OCR returns.

## page_render_for_ocr (non-PDFium backends)

Rendered-image OCR handoff exists for the PDFium backend; non-rendering backends report the limitation instead of silently switching OCR input contracts.

## table_recovery

Current table support is conservative, tied to explicit uncertainty flags, preserves blank cells for delimited text, fixed-width whitespace, fixed-width wrapped descriptor fragments, key-value metadata rows, embedded pin/function tables, number-first pin-description tables, fragmented symbol/rating tables, bullet/leader spec tables, electrical-characteristics min/typ/max tables, AWINIC parameter/test-condition electrical tables with split frequency ranges, split ppm/degree-C units, ohm values, thermal shutdown rows, and footer exclusion, parameter/symbol/conditions electrical tables with condition continuations and thermal/EN threshold tail rows, reflow-profile Sn-Pb/Pb-free assembly tables, classification-temperature package/volume tables, package pin-description tables, part-number ordering tables, OMB-style budget projection tables, header-guided whitespace rows with table-header cues, same-line or wrapped multi-word descriptor cells, two-column descriptor/value rows, trailing descriptor continuations, header-guided trailing blank cells, header-guided section rows, and prefixed leading delimited/text-table captions outside table grids, aligned whitespace and positioned interior section rows, keeps positioned captions outside table grids, rejects routed description prose without table-header cues, rejects positioned-table windows that are really the page's own two-column prose lines so figure-ruling-routed academic pages keep column reading order instead of fake parallel-prose tables, recovers column-ruled grids from extracted vector ruling lines (composed through nested form XObject transforms) with text-row row structure, blank-cell preservation, wrapped-descriptor merges, and diagram-lattice rejection so filled vouchers and ruled month-grid forms produce structured cells, and aligned positioned rows including same-line fragmented positioned cells, first-column positioned section rows, fragmented first-column positioned section rows, interior positioned condition/note rows, multi-cell wrapped continuations, and same-column wrapped header rows when table recovery is routed, splits side-by-side per-column tables on two-column pages instead of mashing them into one grid, and exposes structured grids to eval text anchors. Labeled real-PDF table fixtures pass across datasheet, invoice/form, budget, and academic categories in test/corpus.v0.layout.json and test/corpus.v0.json. Two-level header groups, merged cells, and cross-page continuation stitching remain conservative and are tracked as later advanced-table-semantics work, with table_uncertain flags preserved.

## artifact_cache_snapshots

JSON cache snapshots use explicit schema/parser/backend/source provenance, reuse artifacts on warm runs, and treat unreadable or invalid snapshots as explicit misses with cache_snapshot_ignored warnings; mmap-friendly snapshots remain a later runtime optimization, not a LiteParse parity blocker.

## python_node_bindings

Dependency-free Python and Node wrappers delegate parse, text and markdown derived-output helpers, inspect-page triage, debug-page, OCR/backend/baseline preflights, feature-parity reports, eval-manifest quality gates, benchmark reports, and manifest generation to the native CLI artifact paths.

## wasm_bindings

bindings/wasm wraps glyphrush-core and the shared glyphrush-lopdf extraction crate behind wasm-bindgen: PDF bytes in, the identical JSON document artifact out, verified by a deep-equal parity test against the CLI's lopdf backend (only timing and source-mtime fields are exempt). OCR adapters are process/network seams that do not apply to the wasm surface; OCR-required pages keep their requires_ocr flags and warnings exactly like a no-OCR CLI run.

## mupdf_backend

MuPDF is AGPL-3.0 licensed while Glyphrush is MIT; wiring it as a shipped backend would constrain every downstream distribution, and the BSD-licensed PDFium adapter already provides the measured native-text fast path with rendered-image OCR handoff. Rejected deliberately rather than left as an open promise; backend-check continues to report the adapter slot so the decision stays visible.

## bundled_builtin_ocr

Bundling OCR into the default parser would violate the hot-path dependency policy.
