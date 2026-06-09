# Artifact Schema

Glyphrush emits structured document artifacts first. Plain text and markdown are derived views, not the source of truth.

## Document Artifact

- `document_fingerprint`: SHA-256 hash of the source bytes.
- `metadata`: parser, backend, source-size, and source modified-time provenance for the artifact.
- `pages`: page artifacts sorted by zero-based `page_index`.
- `global_diagnostics`: fallback counts, OCR-required page counts, effective parse worker count, total stage timing, and warnings.

## Metadata

- `parser_name`: parser family name, currently `glyphrush`.
- `parser_version`: CLI parser version that emitted the artifact.
- `backend`: selected PDF backend name, currently `lopdf`.
- `backend_version`: backend adapter version, currently `lopdf-adapter-v0`.
- `source_size_bytes`: source PDF byte size at parse time.
- `source_modified_unix_ms`: source PDF filesystem modified time at parse time, in Unix milliseconds.

## Page Artifact

- `artifact_id`: deterministic ID in the form `<document>:p000000:<page_hash_prefix>`.
- `page_index`: zero-based page index.
- `dimensions`: effective page width and height in PDF units, currently derived from `/CropBox` when present, otherwise `/MediaBox`, including page-tree inheritance.
- `fingerprint`: page-level SHA-256 fingerprint derived from native/OCR text, effective dimensions, route-driving page signals, native span geometry, and image artifact metadata. Timings are intentionally excluded.
- `signals`: cheap classifier evidence for this page, including native text/glyph counts, image coverage, encoding/layout/table/form risk signals, rotation, and span-geometry cap state.
- `native_spans`: spans extracted from PDF text objects. The default hot path emits a page-wide native span. With `--span-geometry`, trustworthy small/simple backend geometry can produce positioned spans with approximate bounding boxes normalized into effective page-local coordinates, including simple text-matrix/content-matrix transforms, text-state parameters preserved across text objects, spacing adjustments, text-rise adjustments, line-leading adjustments, and `'`/`"` text-showing shortcut adjustments.
- `ocr_spans`: spans produced by an OCR adapter when a page is routed to OCR and adapter output is available.
- `image_artifacts`: cheap metadata for drawn image XObjects, image-backed form XObjects, and detected inline images, currently deterministic image IDs, source XObject names when available, drawn bboxes normalized into effective page-local coordinates, and per-artifact visible page-area ratios. Pixel data is not copied or rendered. Image artifacts are preserved even when they fall outside the effective page box; those artifacts contribute zero visible area. The page-level `signals.image_area_ratio` uses unioned artifact bboxes clipped to the page so repeated or overlapping images do not double-count coverage. For form-wrapped images, nested form content and image transforms are followed before area ratios are computed. Skipped or unsupported inline image operators are still surfaced as image artifacts.
- `layout_blocks`: deterministic layout blocks. With positioned native spans, v0 can isolate table-routed aligned row runs into table blocks while preserving surrounding text blocks, otherwise groups spans by vertical gaps and assigns each block the union of only the contributing span boxes. Image-only pages with no native or OCR layout text emit an empty-text `figure` block using the union of preserved image-artifact boxes, so image-backed pages remain visible to layout diagnostics without fabricating text. Table blocks can include an optional `table` payload with ordered rows and cells; positioned-table cells include cell bounding boxes.
- `route`: classifier route, fallback booleans, quality flags, and deterministic `reasons` strings explaining the route decision.
- `quality`: flags and confidence scores. `low_confidence_text` lowers text confidence; `layout_uncertain` and `table_uncertain` lower layout confidence so table recovery pages are not reported as high-confidence layout.
- `timings`: per-stage counters in microseconds.

## Global Diagnostics

- `fallback_pages`: pages with any quality/fallback flag.
- `ocr_pages`: compatibility alias for pages that require OCR.
- `ocr_required_pages`: pages routed to OCR fallback by classifier evidence.
- `ocr_applied_pages`: pages where OCR text was actually merged into `ocr_spans`.
- `worker_count`: effective page extraction worker count for this parser invocation.
- `cache_status`: `disabled`, `miss`, or `hit`.
- `cache_key`: cache artifact key when caching is enabled.
- `total_stage_time_us`: sum of page timing counters.
- `warnings`: document-level warnings, including `p000000: requires_ocr_without_ocr_output` when a page requires OCR but no OCR span was produced, `p000000: unsupported_feature: span_geometry_capped` when an explicit parser capability request hit a bounded or unsupported v0 geometry cap, or `p000000: unsupported_feature: annotation_or_form` when page annotations, form fields, or widget annotations are present but not extracted.

## Quality Flags

- `requires_ocr`: native extraction is missing or not trustworthy and OCR should be used when available.
- `low_confidence_text`: text output should not be treated as complete.
- `broken_encoding`: native text appears damaged by encoding/CMap issues.
- `layout_uncertain`: reading order or geometry needs heavier layout work.
- `table_uncertain`: table recovery should be attempted or manually reviewed, based on text-table patterns or ruled-line geometry.
- `unsupported_feature`: the page hit a v0 cap or unsupported condition.

## Route Reasons

Route reasons are stable machine-readable strings attached to each page route decision. Current reason values include `high_image_coverage_without_native_text`, `high_image_coverage_with_sparse_native_text`, `image_text_overlay`, `broken_encoding`, `broken_encoding_with_image_coverage`, `bbox_overlap`, `duplicate_char_ratio`, `rotated_page`, `table_line_density`, `annotation_or_form`, `huge_object_count`, and `span_geometry_capped`. `image_text_overlay` means the page has high image coverage plus substantial native text, so OCR is skipped but layout/completeness should be reviewed. `broken_encoding_with_image_coverage` means native text is damaged and the page has enough image evidence to route to OCR fallback. `bbox_overlap` means accepted positioned spans overlap enough to make reading order, duplicate text, or hidden text uncertain. `rotated_page` is based on the effective page rotation, including `/Rotate` inherited from parent page-tree nodes. `annotation_or_form` means the document declares page annotations, AcroForm fields, or page widget annotations that v0 does not yet extract. `span_geometry_capped` means `--span-geometry` was requested but the page exceeded the bounded geometry extraction cap or uses unsupported geometry such as page rotation, so page-wide native spans were retained.

## Page Signals

Page signals are included in each page artifact so downstream agents can inspect the evidence behind `route`, `quality`, and warning decisions without rerunning `debug-page`. They are cheap diagnostics, not expensive rendered analysis. Current signals include text density, glyph count, image area ratio, duplicate-character and broken-encoding ratios, measured bbox-overlap ratio for accepted positioned spans, rotation, table-line density, `annotation_count` for page annotations, `form_field_count` for catalog AcroForm fields and page widget annotations, huge-object count, span-geometry cap state, and effective dimensions. The broken-encoding ratio counts replacement/control characters and repeated `¿‰` mojibake pairs so damaged native text can route to OCR when image evidence is available.

## Layout Blocks

Layout blocks are page-local and use deterministic IDs like `p000000:b000000`.

Supported v0 block kinds:

- `heading`: short uppercase single-line blocks.
- `paragraph`: default text blocks.
- `list`: consecutive bullet or numbered lines.
- `table`: simple pipe-, tab-, table-route whitespace, or table-route aligned positioned row groups. Pipe, tab, fixed-width whitespace, and aligned positioned-row tables preserve empty cells so blank columns remain visible in the structured payload. Fixed-width whitespace rows can merge lowercase wrapped descriptor fragments into the following row and preserve section rows as first-cell text with blank remaining cells; number-first pin-description rows keep split `PIN` / `NO. NAME` / `FUNCTION` headers as `Pin No.`, `Name`, and `Function`; part-number ordering rows keep compound `Part Number` / `Identification Code` headers and package suffix fragments in the package cell; header-guided whitespace rows can merge same-line or wrapped leading multi-word descriptor cells against the inferred header columns, merge lowercase trailing descriptor continuations into the preceding row, keep inferred trailing blank cells explicit, preserve section rows as first-cell text with blank remaining cells, and keep leading captions outside the table payload; positioned table rows can merge same-line fragmented cells, same-column wrapped header rows, plus near-adjacent single- or multi-cell wrapped continuations into the same structured row, keep cross-column, first-column, or fragmented first-column interior section rows in-grid, and keep top/bottom captions outside the table payload.

Fragmented datasheet symbol tables with headers such as `Symbol Parameter Rating Unit`, including PDFium streams with blank separator lines between symbols and row values, are normalized into structured `Symbol`, `Parameter`, value, and `Unit` cells when table recovery is routed.

Bullet/leader datasheet rows with repeated separator runs are normalized into two-column `Parameter` / `Limit` table payloads, including wrapped package-specific continuation rows.

Electrical-characteristics tables with multi-line `Symbol / Parameter / Test Conditions / Min. / Typ. / Max. / Unit` headers normalize into seven-column table payloads, including blank symbol cells, rows whose units arrive on a following line, and PDFium extraction order where a condition line precedes its symbol/parameter label.

AWINIC-style electrical tables with `Parameter / Test Condition / Min. / Typ. / Max. / Unit` headers normalize into six-column table payloads, including wrapped labels, inherited test conditions, symbol-font micro units, and unit-only continuation lines.

Reflow-profile datasheet tables with `Profile Feature`, `Sn-Pb Eutectic Assembly`, and `Pb-Free Assembly` columns normalize into structured rows when table recovery is routed, including PDFium value groups that arrive below their feature labels.

Classification-temperature datasheet tables with `Package Thickness` and `Volume mm3` columns normalize into structured package/temperature cells when table recovery is routed, while table captions stay outside the table payload.

The v0 layout engine is primarily text-derived. It can receive backend-provided native span boxes when `--span-geometry` is enabled: simple positioned text operations from the `lopdf` backend, including simple text/page content transforms, text-state persistence across text objects, text spacing operators, text rise, text leading, and text-showing shortcuts; or PDFium's merged text-segment rectangles from the `pdfium` backend. Current block geometry is conservative: when the classifier has already routed a page for table recovery, aligned positioned row runs, including same-line fragmented positioned cells, same-column wrapped header rows, near-adjacent single- or multi-cell wrapped positioned continuations, interior cross-column section rows, first-column interior section rows, and fragmented first-column interior section rows, simple whitespace rows with a consistent column count, fixed-width whitespace rows with header-aligned columns, lowercase wrapped descriptor fragments, and section rows, embedded pin/function tables with wrapped function descriptions, number-first pin-description tables, package pin-description tables with package-specific pin-number columns, part-number ordering tables with compound headers and package suffixes, or header-guided whitespace rows with explicit table-header cues, same-line or wrapped leading multi-word descriptor cells, trailing descriptor continuations, inferred trailing blank cells, and section rows are preserved as table blocks while surrounding text, including leading captions and pin-table titles, remains separate heading/paragraph/list blocks. Table blocks expose structured `table.rows[].cells[]`; pipe/tab rows, fixed-width whitespace rows, embedded pin/function rows, number-first pin-description rows, package pin-description rows, part-number ordering rows, and aligned positioned rows keep empty cells. Cell boxes are emitted when a positioned cell has source spans, while text-only cells and omitted positioned cells leave cell boxes absent. Otherwise, usable spans are sorted deterministically, full-width title/banner bands, fragmented full-width heading rows, leading, middle, and trailing cross-column bands, conservative short section-heading separators, and clearly separated 2-4 column reading order are preserved, remaining spans are split into blocks on large vertical gaps, and each block receives a union box over its own spans. If a page has preserved image artifacts but no text-derived or OCR-derived layout blocks, the image artifact boxes are unioned into one empty `figure` block; derived text and markdown filter that block out, while `inspect --pages`, manifest bootstrap, and eval layout-count checks can still see that the page contains figure/image content. Repeated positioned blocks in the top or bottom page margin are classified as `header` or `footer` when the same normalized text appears on multiple pages. Markdown export renders structured, pipe-delimited, or whitespace-delimited table blocks as markdown tables with a generated separator row; plain text and JSON artifacts keep the original block text alongside any structured table payload. Full layout reconstruction and advanced table reconstruction are later milestones.

The splitter includes a conservative fragment reflow pass for common native-text extraction artifacts such as `AP735\n4`, `Rev\n.\n4\n-\n2\n1`, and short adjacent blocks separated by spurious blank lines. Raw `native_spans` are preserved; reflow and geometry-aware column ordering affect `layout_blocks`, derived text, derived markdown, and eval text-quality checks.

## OCR Adapter Provenance

The v0 CLI supports `--ocr-sidecar <dir>`, `--ocr-command <executable>`, and `--ocr-http-url <url>` as adapter seams. For sidecars, page index `0` of `example.pdf` is loaded from `example.p000000.txt`. For commands, Glyphrush invokes the executable only for OCR-routed pages. The default `--ocr-command-input pdf-page` contract passes the PDF path plus zero-based page index as arguments. For HTTP OCR, Glyphrush POSTs JSON with `pdf_path` and `page_index` only for OCR-routed pages, accepts plain response bodies as OCR text, and accepts `application/json` responses with a string `text` field. With `--backend pdfium --ocr-command-input rendered-image`, Glyphrush renders an OCR-routed page to a temporary PPM image, records render timing, and removes the temporary file after OCR returns. Command OCR receives the rendered image path plus page index as arguments; HTTP OCR receives JSON with `rendered_image_path` and `page_index` instead of `pdf_path`. `tools/ocr/tesseract-rendered-image.sh` adapts the rendered-image command contract to local Tesseract without making OCR part of the default parser path. OCR commands and HTTP requests are bounded by `--ocr-timeout-ms`, defaulting to `120000`. `ocr-check <pdf> --page-index <N>` runs the same adapter contract as a strict preflight surface, including PDFium rendered-image command or HTTP input, and emits `glyphrush-ocr-check-report-v1` with non-empty output status, digest, text counts, timeout state, render timing when applicable, stderr preview, and stable failure kinds such as `empty_output`, `missing_dependency`, `spawn_failed`, `render_backend_required`, `sidecar_read_failed`, `http_request_failed`, `http_status_failed`, or `http_response_decode_failed`. OCR text is stored in `ocr_spans` with `provenance: "ocr"` and does not overwrite native spans. When OCR is applied for an OCR-routed page, derived layout/text views are built from the OCR text because the native text has already been classified as missing or low-confidence.

## Image Artifact Provenance

The v0 CLI records drawn image XObjects, image-backed form XObjects, and detected inline images as `image_artifacts` with page-local IDs such as `p000000:im000000`. Each entry stores the source XObject name when available, the transformed unit-image bbox normalized into effective page-local coordinates, and approximate visible page-area ratio. Page-level image coverage is computed as the union of artifact boxes clipped to the page, not a raw sum of per-artifact ratios. For form XObjects, Glyphrush recurses through nested form content and applies nested image transforms before recording the bbox and visible area ratio. Inline images use `source_name: "inline"` when no external XObject name exists, including skipped inline images whose bytes or colorspace are unsupported by the lightweight decoder. Glyphrush does not include image bytes in the artifact; image metadata exists to keep image-backed pages observable without adding render/copy cost to the default parser path.

## Cache Provenance

The v0 CLI supports `--cache-dir <dir>` on parse, bench, eval, `inspect --pages`, and manifest generation. Cache keys include parser name/version, parser cache schema, backend name/version, PDF byte fingerprint, OCR sidecar text fingerprint for files matching the current PDF's `<stem>.pNNNNNN.txt` convention, OCR command path/content/timeout, OCR HTTP URL/timeout, and span-geometry option. If parser identity, relevant OCR adapter state, span-geometry mode, backend adapter version, or the parser cache schema changes, the cache key changes and the parse is treated as a miss. Unrelated sidecar files for other PDFs do not invalidate this document's cache. Cache files are JSON snapshot envelopes with `snapshot_version`, `cache_schema`, `cache_key`, parser/backend metadata, `document_fingerprint`, and the cached `artifact`; older raw artifact cache files are still readable. If a matching cache snapshot is unreadable, invalid JSON, or fails envelope validation, Glyphrush ignores it, reparses as a cache miss, rewrites the snapshot, and emits a `cache_snapshot_ignored` warning in `global_diagnostics.warnings`. On cache hits, page-level parser stage timings and `global_diagnostics.total_stage_time_us` are zeroed because the page extraction pipeline did not run, while source metadata such as `source_size_bytes` and `source_modified_unix_ms` is refreshed from the current file; use `cache_status`, per-document `artifact_cache_status`, or aggregate eval/bench/inspect `cache_hits` and `cache_misses` to distinguish warm paths from cold misses.
