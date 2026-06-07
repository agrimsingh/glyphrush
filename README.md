# Glyphrush

[![CI](https://github.com/agrimsingh/glyphrush/actions/workflows/ci.yml/badge.svg)](https://github.com/agrimsingh/glyphrush/actions/workflows/ci.yml)

Glyphrush is a native PDF parsing experiment focused on fast native-text extraction with explicit quality and fallback signals. The v0 implementation is a Rust workspace with:

- `glyphrush-core`: deterministic artifacts, page signals, classifier decisions, and extracted-page parsing.
- `glyphrush-cli`: `inspect`, `parse`, `bench`, `debug-page`, and backend/baseline preflight commands backed by a thin backend interface. `lopdf` is the only enabled backend today.

The current backend is intentionally small. It extracts native text through `lopdf`, can preserve simple positioned text spans with approximate boxes when explicitly requested, records cheap drawn-image metadata for direct image XObjects, image-backed form XObjects, and detected inline images without copying pixels, follows nested form image transforms for image coverage, routes OCR-required pages to optional sidecar or command adapters, and emits structured artifacts with parser/backend/source size and modified-time metadata. Bundled OCR engines, full table reconstruction, richer geometry-aware layout recovery, and PDFium/MuPDF comparison are later milestones. Use `backend-check` to inspect the enabled backend and the pending PDFium/MuPDF adapter candidates.

## Commands

```sh
cargo run -p glyphrush-cli -- eval test/corpus.datasheets.json --category datasheet --jobs 2
bash scripts/verify.sh
```

`scripts/verify.sh` is the shared local/GitHub CI gate. It runs formatting, the full workspace test suite, clippy with warnings denied, strict `glyphrush-v0` baseline-preset metadata preflight, and the datasheet eval gate when ignored local PDFs exist under `test/`. In a fresh GitHub checkout those PDFs are absent by design, so CI skips only that local corpus gate rather than failing on non-committed benchmark files.

```sh
cargo run -p glyphrush-cli -- inspect test/example.pdf
cargo run -p glyphrush-cli -- inspect test/example.pdf --pages
cargo run -p glyphrush-cli -- inspect test/example.pdf --pages --jobs 4
cargo run -p glyphrush-cli -- inspect test/example.pdf --pages --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- backend-check
cargo run -p glyphrush-cli -- backend-check --pdf test/example.pdf
cargo run -p glyphrush-cli -- backend-check --pdf test/
cargo run -p glyphrush-cli -- backend-check --pdf test/ --jobs 4
cargo run -p glyphrush-cli -- --backend lopdf inspect test/example.pdf
cargo run -p glyphrush-cli -- --backend lopdf backend-check
cargo run -p glyphrush-cli -- parse test/example.pdf --format json
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --jobs 4
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --span-geometry
cargo run -p glyphrush-cli -- parse test/example.pdf --format text
cargo run -p glyphrush-cli -- parse test/example.pdf --format markdown
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --ocr-command tools/ocr/my-ocr.sh --ocr-timeout-ms 120000
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- bench test/example.pdf
cargo run -p glyphrush-cli -- bench test/example.pdf --jobs 4
cargo run -p glyphrush-cli -- bench test/example.pdf --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- bench test/example.pdf --ocr-command tools/ocr/my-ocr.sh
cargo run -p glyphrush-cli -- bench test/example.pdf --eval-manifest test/corpus.json
cargo run -p glyphrush-cli -- bench test/example.pdf --eval-manifest test/corpus.json --eval-category datasheet
cargo run -p glyphrush-cli -- bench test/example.pdf --require-quality --eval-manifest test/corpus.json
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/example.pdf --require-baselines --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/example.pdf --require-baseline-quality --eval-manifest test/corpus.json --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline liteparse=tools/baselines/liteparse-text.sh
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline liteparse-no-ocr=tools/baselines/liteparse-no-ocr-text.sh
cargo run -p glyphrush-cli -- baseline-check --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- baseline-check --baseline liteparse=tools/baselines/liteparse-text.sh
cargo run -p glyphrush-cli -- baseline-check --pdf test/example.pdf --baseline liteparse=tools/baselines/liteparse-text.sh
cargo run -p glyphrush-cli -- baseline-check --pdf test/ --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- baseline-check --strict --pdf test/example.pdf --baseline liteparse=tools/baselines/liteparse-text.sh
cargo run -p glyphrush-cli -- baseline-check --baseline pymupdf=tools/baselines/pymupdf-text.sh --baseline pdfplumber=tools/baselines/pdfplumber-text.sh
cargo run -p glyphrush-cli -- baseline-check --baseline marker=tools/baselines/marker-text.sh --baseline docling=tools/baselines/docling-text.sh
cargo run -p glyphrush-cli -- bench test/example.pdf --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- bench test/example.pdf --cache-dir .glyphrush-cache --cache-probe
cargo run -p glyphrush-cli -- manifest test/example.pdf
cargo run -p glyphrush-cli -- manifest test/example.pdf --jobs 4
cargo run -p glyphrush-cli -- manifest test/example.pdf --category clean_digital
cargo run -p glyphrush-cli -- manifest test/example.pdf --category clean_digital --coverage-preset glyphrush-v0
cargo run -p glyphrush-cli -- manifest test/example.pdf --required-category clean_digital --required-category scanned
cargo run -p glyphrush-cli -- manifest test/example.pdf --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- debug-page test/example.pdf 0
cargo run -p glyphrush-cli -- debug-page test/example.pdf 0 --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- debug-page test/example.pdf 0 --ocr-command tools/ocr/my-ocr.sh
cargo run -p glyphrush-cli -- eval test/corpus.json
cargo run -p glyphrush-cli -- eval test/corpus.json --category datasheet
cargo run -p glyphrush-cli -- eval test/corpus.json --jobs 4
cargo run -p glyphrush-cli -- eval test/corpus.json --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- eval test/corpus.json --ocr-command tools/ocr/my-ocr.sh
cargo run -p glyphrush-cli -- inspect test/
cargo run -p glyphrush-cli -- inspect test/ --pages
cargo run -p glyphrush-cli -- inspect test/ --pages --jobs 4
cargo run -p glyphrush-cli -- inspect test/ --pages --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- bench test/
cargo run -p glyphrush-cli -- bench test/ --jobs 4
cargo run -p glyphrush-cli -- bench test/ --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- bench test/ --ocr-command tools/ocr/my-ocr.sh
cargo run -p glyphrush-cli -- bench test/ --eval-manifest test/corpus.json
cargo run -p glyphrush-cli -- bench test/ --eval-manifest test/corpus.json --eval-category datasheet
cargo run -p glyphrush-cli -- bench test/ --require-quality --eval-manifest test/corpus.json
cargo run -p glyphrush-cli -- bench test/ --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/ --require-baselines --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/ --require-baseline-quality --eval-manifest test/corpus.json --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/ --baseline liteparse=tools/baselines/liteparse-text.sh
cargo run -p glyphrush-cli -- bench test/ --baseline liteparse-no-ocr=tools/baselines/liteparse-no-ocr-text.sh
cargo run -p glyphrush-cli -- bench test/ --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- bench test/ --cache-dir .glyphrush-cache --cache-probe
cargo run -p glyphrush-cli -- manifest test/ > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --category datasheet > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --category clean_digital --coverage-preset glyphrush-v0 > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --category datasheet --required-category datasheet > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --category datasheet --min-category-count datasheet=5 > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --jobs 4 > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --cache-dir .glyphrush-cache > test/corpus.generated.json
```

The global `--backend` option currently accepts `lopdf`. It is already part of the command surface so PDFium/MuPDF spikes can be wired without changing the parse, inspect, bench, or debug command contracts. `backend-check` emits `glyphrush-backend-check-report-v1` with parser version, selected backend, enabled backend count, candidate backend count, and per-backend capability/limitation metadata. Add `backend-check --pdf <file-or-directory>` to smoke the selected backend against one PDF or every top-level PDF in a directory and report open/extract success, page counts, native text bytes, image artifact count, OCR-required pages, source size/fingerprints for files, wall time, stable failure `error_kind` values such as `encrypted_pdf_requires_password`, sorted per-document results for directories, and bounded directory `failure_samples` so a mixed corpus failure is visible without scanning every document. Use `--jobs <N>` with directory smoke tests to run PDFs concurrently while merging results back into stable filename order and reporting the effective worker count. Today it reports `lopdf` as enabled and PDFium/MuPDF as `not_wired`, making the backend decision gate explicit rather than hidden in docs.

Local PDFs can be dropped into `test/`. PDF files in that directory are ignored by git so benchmark corpora do not get committed accidentally. File and directory `inspect` outputs include parser/backend/source metadata. `inspect --pages <pdf-or-directory>` runs the normal extraction/classifier path and emits compact page triage summaries: page artifact IDs, page fingerprints, dimensions, per-stage timings, route, quality flags, route reasons, native/OCR span counts, native text bytes, image artifact counts, layout block counts, and page warnings. Use `inspect --pages --jobs <N>` to triage a single PDF with page workers or a directory with per-PDF workers; results are merged back into stable page or filename order and the effective worker count is reported as `worker_count`. Use `inspect --pages --cache-dir <dir>` for warm repeated triage; single-file output reports `cache_status`/`cache_key`, while directory output also reports aggregate `cache_hits` and `cache_misses` plus per-document cache status. Directory `inspect --pages`, `inspect`, and `bench` commands discover top-level `.pdf`/`.PDF` files in stable filename order and emit corpus-level aggregate JSON with a `corpus_fingerprint` over the ordered document labels, document fingerprints, and page counts.

`parse --jobs <N>` opts into page extraction workers for one PDF and reports the effective count as `global_diagnostics.worker_count`. Worker results are merged back by page number before building JSON, text, markdown, cache artifacts, or benchmark quality reports.

Bench output includes top-level `report_version` for the benchmark JSON envelope schema, `quality_status` for whether the run was eval-scored, `run_metadata` for parser/backend provenance, `run_configuration` for output-affecting options, `requirements` for strict gate switches requested by the command, `requested_baseline_presets` for any preset comparison set expanded into wrapper runs, `worker_count`, wall time, pages/sec, artifact size, parser-run allocation bytes, derived text output size/counts, peak RSS, parser stage timings, p50/p95 page latency, per-route latency, route counts, route-reason counts, image artifact counts, fallback counts, fallback action counts, OCR-required/OCR-applied counts, per-flag quality counts, and artifact warnings. Corpus reports include the same report version, quality status, run metadata/configuration, requirements, aggregate warning counts, bounded warning samples with document paths, per-document warning arrays, image artifact totals, and empty-text output counts. When a directory benchmark uses an eval manifest with document categories, top-level `category_summaries` reports Glyphrush benchmark timing, pages/sec, fallback/OCR counts, route counts, quality-flag counts, warnings, and eval failed checks by category. Single-PDF benchmarks accept `--jobs <N>` for opt-in parallel page extraction; directory benchmarks use `--jobs <N>` for opt-in parallel per-PDF runs. Both paths merge results back into stable page or filename order so artifacts, document arrays, and corpus fingerprints remain deterministic. `bench --eval-manifest <manifest.json>` embeds the full eval result under `quality`, sets `quality_status` to `checked`, includes the manifest path, SHA-256, and bounded failure samples, scores the exact artifacts that were benchmarked, and exits nonzero after writing JSON when quality gates fail. If the eval manifest includes `silent_failures` checks, bench reports also include top-level `silent_failure_count` and path-qualified `silent_failure_pages`; those fields are absent when the gate was not requested, which means "not checked", not "zero". Without `--eval-manifest`, `quality_status` is `not_checked_no_eval_manifest` so speed-only reports cannot be mistaken for quality-backed reports. Add `--require-quality` when benchmark jobs must fail instead of accepting a speed-only report; Glyphrush still writes the JSON report, then exits nonzero if no eval manifest quality report was checked. Use `bench --eval-category <name>` with an eval manifest to restrict the embedded quality report and benchmark category summaries to one normalized coverage class; if the category selects no documents, Glyphrush still writes the bench JSON with a `quality.document_count` failure before exiting nonzero. When baselines are present, manifest document-level `required_text`, page-level `required_text`, `text_recall`, `reading_order`, and `table_structure` expectations are also scored against baseline stdout under `baselines[].quality`, and each baseline result exposes `quality_status` so speed-only comparisons are explicit as `not_checked_no_expectations` instead of looking quality-backed. Page-level required-text anchors are checked as full-document stdout anchors for text-only baselines; page locality remains enforced by Glyphrush artifact eval. Corpus baseline summaries also expose `quality_status` (`checked`, `partially_checked`, `not_checked_no_expectations`, or `not_checked_baseline_failures`) plus `quality_documents` and `quality_unchecked_documents` alongside quality pass counts/rate, per-check failure counters, `quality_category_summaries` keyed by manifest category, and bounded `quality_failure_samples` naming the failed documents and check types. Category-filtered benchmark runs apply the same manifest category filter before scoring baseline quality. `bench --cache-probe --cache-dir <dir>` runs a forced cold cache miss followed by a warm cache hit in one command; corpus probe summaries include aggregate cold/warm stage timings, allocation counters, and fallback action counters so warm-cache reports can prove parser stages did not rerun. `bench --baseline NAME=EXECUTABLE` runs an external parser wrapper for each PDF and records optional `--describe` metadata, wall time, exit status, timeout status, stdout bytes, stdout SHA-256, stdout line/word counts, stderr bytes, empty-output status, bounded stderr previews, and a `comparison` object with Glyphrush-vs-baseline speed/output ratios. Use `bench --baseline-preset glyphrush-v0` for the core comparison set: LiteParse with its default OCR behavior, LiteParse with OCR disabled, PyMuPDF, and pdfplumber; saved reports then include `"requested_baseline_presets": ["glyphrush-v0"]`. Add `--require-baselines` when benchmark jobs must fail if no baseline was requested or any requested baseline execution failed or timed out; add `--require-baseline-quality` when they must also fail if baseline text quality was not checked or did not pass. Glyphrush still writes the JSON report first so failure details remain inspectable. `--baseline-timeout-ms <ms>` bounds each baseline and its `--describe` probe; timed-out baselines stay in the report as failed, non-comparable runs with `quality_status: "not_checked_timed_out"` when quality expectations existed. Corpus baseline summaries also report comparison metadata when available, success rate, successful/failed/timed-out pages, empty-output pages, and failure samples, so a fast partial failure, timeout, empty extraction, or quality miss is visible. The wrapper receives the PDF path as its only argument. Marker/Docling remain manually addable as heavier quality-context baselines, while speed comparisons stay in the same report as Glyphrush image, fallback, and OCR-required counters.

`baseline-check --baseline NAME=EXECUTABLE` validates wrapper metadata without parsing a PDF. It emits top-level `report_version`, parser/backend `run_metadata`, `requested_baseline_presets`, runs each wrapper's `--describe` mode, reports valid JSON metadata, timeouts, missing executables, stderr previews, and aggregate `describe_success_count`/`all_described` fields. `baseline-check --baseline-preset glyphrush-v0` expands to the four core baselines: LiteParse, LiteParse no-OCR, PyMuPDF, and pdfplumber, and records `["glyphrush-v0"]` in `requested_baseline_presets`. At least one explicit baseline or preset baseline is required; an empty preflight still writes JSON but exits nonzero with `all_described: false` so setup scripts cannot treat a missing LiteParse/PyMuPDF/pdfplumber comparison as passing. Add `--pdf <file>` to run each wrapper against one smoke PDF and report output size, stdout digest, line/word counts, stderr previews, stable smoke `error_kind` values such as `missing_dependency`, and `smoke_success_count`/`all_smoke_passed`. Add `--pdf <directory>` to smoke every top-level `.pdf`/`.PDF` in stable filename order; the report includes `smoke_document_count`, aggregate per-baseline document pass/fail counts, bounded `smoke.failure_samples`, and per-document smoke entries so missing parser dependencies or file-specific failures are visible before a long corpus benchmark. Add `--strict` when this should behave like a setup gate: Glyphrush still writes the JSON report, then exits nonzero if any describe probe failed or if a smoke PDF/directory was supplied and any smoke probe failed. Use it before long `bench --baseline` corpus runs to catch missing wrappers, broken comparison metadata, or missing parser dependencies quickly.

OCR remains optional. For testable OCR plumbing without installing a heavy engine, `parse --ocr-sidecar <dir>`, `bench --ocr-sidecar <dir>`, `debug-page --ocr-sidecar <dir>`, and `eval --ocr-sidecar <dir>` read page text from files named `<pdf-stem>.p000000.txt`, using zero-based page indexes. `--ocr-command <executable>` is an alternate adapter for the same commands; Glyphrush invokes it only for pages the classifier routes to OCR fallback, passing the PDF path as argument 1 and the zero-based page index as argument 2, then treating stdout as OCR text. OCR commands are bounded by `--ocr-timeout-ms`, defaulting to `120000`, so a broken adapter cannot hang parse or benchmark runs indefinitely. `--ocr-sidecar` and `--ocr-command` are mutually exclusive. High image coverage with missing, very sparse, or broken-encoding native text is treated as OCR-required instead of a successful native fast path. When OCR text is applied for an OCR-routed page, derived layout/text views use the OCR text while native spans remain in the JSON artifact for provenance. If OCR is required but no OCR text is applied, `global_diagnostics.warnings` includes a stable page warning such as `p000000: requires_ocr_without_ocr_output`; if an explicit capability request hits a v0 cap or page annotations/form fields/widget annotations are present but not extracted, warnings include values such as `p000000: unsupported_feature: span_geometry_capped` or `p000000: unsupported_feature: annotation_or_form`. `parse --format text` and `parse --format markdown` also write these warnings to stderr while keeping stdout as the derived text view.

Classifier decisions include deterministic `reasons` strings and the cheap `signals` that produced them in JSON artifacts and `debug-page` output. `debug-page` extracts only the requested page, can apply page-selective sidecar OCR with `--ocr-sidecar`, reports `document_page_count`, `extracted_page_count`, selected-page `artifact_id`, `page_fingerprint`, dimensions, page `quality`, page warnings, per-stage `timings`, derived `text_output` metrics, bounded `layout` block counts, and any drawn image artifact metadata, and includes parser/backend/source metadata so classifier investigations can be tied to the exact parser build and backend adapter. Reasons explain fallback routing with values such as `high_image_coverage_with_sparse_native_text`, `image_text_overlay`, `broken_encoding`, `broken_encoding_with_image_coverage`, `bbox_overlap`, `table_line_density`, `annotation_or_form`, and `rotated_page`; `annotation_or_form` is raised when page annotations, widget annotations, or catalog AcroForm fields are present but not extracted.

Structured page artifacts include `image_artifacts` for drawn image XObjects, image-backed form XObjects, and detected inline images. Each entry stores a deterministic page-local image ID, source XObject name when available, drawn bbox, and approximate per-artifact page-area ratio. The route-driving `signals.image_area_ratio` is computed from the union of image artifact boxes, clipped to the page, so overlapping or repeated images do not overstate scan coverage. For form-wrapped images, Glyphrush follows nested form content and transforms so small logos inside page-sized forms do not look like full-page scans. Skipped or unsupported inline image operators are still surfaced with `source_name: "inline"` so image-backed pages do not disappear from diagnostics. Glyphrush does not include image bytes in the artifact; the metadata exists so image-backed content stays visible without adding render/copy cost to the fast path.

Artifact caching is optional. `--cache-dir <dir>` keys cached JSON artifacts by parser version, backend name/version, cache schema, PDF bytes, OCR sidecar text state for files matching the current PDF stem or OCR command path/content/timeout, and span-geometry mode. Cache-schema bumps intentionally invalidate warm artifacts when output-relevant diagnostics such as warnings, quality flags, confidence scores, or artifact metadata shape changes. Cache diagnostics are emitted as `cache_status` and `cache_key`; cache-hit artifacts reset page-stage timings to zero because the page extraction pipeline did not rerun, while source metadata such as `source_size_bytes` and `source_modified_unix_ms` is refreshed from the current file.

Layout reconstruction v0 is text-derived. Glyphrush splits native or OCR text into deterministic page-local blocks, reflows common short extraction fragments, and classifies simple headings, lists, tables, and paragraphs. When opt-in span geometry produces usable boxes, the layout path can preserve a conservative two-column reading order by processing clearly separated left/right column bands before falling back to vertical-gap grouping. Repeated positioned blocks in the top or bottom page margin are classified as `header` or `footer` when the same normalized text appears on multiple pages. The page classifier also treats dense stroked horizontal/vertical ruling lines as `table_uncertain` so ruled tables are escalated even when extracted text has no pipe/tab delimiters. When that table route is active, simple consistent whitespace rows are preserved and exposed as table blocks for quality gates; if positioned spans form aligned multi-column rows, Glyphrush groups them as a table before applying the two-column reading-order heuristic. Plain text, markdown, and eval quality text prefer layout-block order when blocks are available while raw `native_spans` remain preserved in JSON artifacts. Markdown export normalizes consistently pipe-delimited or whitespace-delimited table blocks into markdown tables with a generated separator row; other table blocks remain plain text.

Native span geometry is currently conservative and opt-in. Add `--span-geometry` to `parse`, `bench`, `debug-page`, or `eval` when you want bounded/simple `lopdf` text-positioning streams to produce multiple boxed spans, approximate boxes with simple text-matrix and content-matrix transforms plus text-state parameters preserved across text objects, `TJ`/`Tc`/`Tw`/`Tz` spacing, `Ts` text rise, `TL` leading, and `'`/`"` text-showing shortcut adjustments applied, and a measured `bbox_overlap_ratio` layout-risk signal. The default hot path emits page-wide native spans. Large streams fall back to the page-wide native span and are flagged and warned with `unsupported_feature` plus `span_geometry_capped` instead of silently pretending requested bbox detail was produced. Decoded spans that do not match the backend's native text output still fall back to the page-wide native span instead of taxing the hot path or risking bad geometry.

`eval <manifest.json>` turns local PDFs into repeatable gates. Manifest paths are resolved relative to the manifest file and can assert document-level counts, required text substrings against the derived layout-aware eval text, and page-level artifact ID, page fingerprint, route, required text, flag, reason, and layout-block-count expectations. Empty manifests and category filters that select no documents are not accepted as quality passes: `eval` emits a `document_count` failure sample and exits nonzero when no documents are selected. Add optional root-level document `category` values such as `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, or `large` to track benchmark corpus coverage; eval reports include top-level `category_counts`, `category_summaries`, and per-document categories, with missing or blank categories counted as `uncategorized`. Add manifest-level `required_categories` when a corpus must cover specific benchmark classes before its speed/quality results are accepted, and `min_category_counts` when a class needs more than one fixture; missing or under-counted categories produce a top-level `category_coverage` failure. Use `eval --category <name>` to run gates for one normalized category, including `uncategorized`, when a mixed manifest needs class-specific quality checks. `category_summaries` reports document/page counts, passed/failed document counts, failed checks, and category-level quality pass/fail state so benchmark regressions can be tied to a corpus class. Use `eval --jobs <N>` to evaluate manifest documents concurrently while preserving manifest document order in the report and emitting the effective `worker_count`. Use `eval --cache-dir <dir>` for repeated quality gates; top-level `report_version`, `run_metadata`, `run_configuration`, `corpus_fingerprint`, `cache_hits`, and `cache_misses` summarize the eval report schema, parser/backend provenance, output-affecting options, evaluated source set, and warm/cold artifact reuse while each document still reports `artifact_cache_status`. `run_configuration` records `span_geometry`, OCR adapter mode booleans, and `ocr_timeout_ms`. Eval reports also include aggregate and per-document diagnostics for page count, fallback/OCR counts, image artifact counts, empty-text pages, route counts, route-reason counts, quality-flag counts, layout block counts when asserted, fallback-action counts, and warning counts, even when the manifest does not explicitly assert those checks.

`manifest <pdf-or-directory>` emits an eval-compatible JSON skeleton for the current parser output. It is useful after dropping new PDFs into `test/`: save the output next to the PDFs, run `eval` to confirm the structural gates, then tighten the manifest with human/labeled document-level `required_text`, `text_recall`, `reading_order`, `table_structure`, or `span_bbox` expectations. Use `manifest --category <name>` to stamp every generated document with a benchmark coverage class such as `datasheet`, `clean_digital`, or `scanned`; downstream eval and benchmark reports then populate category counts and category summaries without hand-editing the generated JSON. Use `manifest --coverage-preset glyphrush-v0` to add the core v0 corpus coverage gate for `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`, with each category required at least once. Use repeated `manifest --required-category <name>` to generate the top-level presence gate when a corpus must include specific classes before speed/quality claims are accepted, and repeated `manifest --min-category-count <name=count>` to require enough documents in a class before accepting a corpus benchmark. Use `manifest --jobs <N>` to generate directory manifests with parallel per-PDF workers or single-PDF manifests with page workers; results are merged back into deterministic filename/page order and the generated JSON does not include worker-count provenance, so corpus fingerprints and expectations remain stable across worker counts. Use `manifest --cache-dir <dir>` to reuse parsed artifacts across repeated manifest generation; cache status is intentionally not serialized into the manifest so cold and warm generated JSON remain comparable. The generated skeleton includes deterministic generator provenance, a corpus fingerprint, optional `required_categories`, optional `min_category_counts`, optional per-document category, per-document source fingerprints, sizes, and modified times checked by `eval`, page counts, route counts, quality-flag counts, OCR-needed classification, non-empty quality-flag classification gates, warning counts, exact required warnings, image artifact counts, `silent_failures: 0`, and per-page artifact ID, page fingerprint, route, bounded page-local required-text anchor, layout-block, image, flag, and reason acknowledgements so current fallback decisions and diagnostics are explicit instead of implicit.

```json
{
  "required_categories": ["clean_digital", "scanned"],
  "min_category_counts": {
    "clean_digital": 2,
    "scanned": 1
  },
  "documents": [
    {
      "path": "example.pdf",
      "expect": {
        "page_count": 1,
        "fallback_pages": 0,
        "ocr_required_pages": 0,
        "ocr_applied_pages": 0,
        "required_text": ["Hello Glyphrush"],
        "text_recall": {
          "expected": "Hello Glyphrush",
          "min_word_recall": 1.0,
          "min_char_recall": 1.0
        },
        "reading_order": {
          "expected_sequence": ["Hello", "Glyphrush"],
          "min_score": 1.0
        },
        "ocr_required_classification": {
          "expected_pages": [],
          "min_precision": 1.0,
          "min_recall": 1.0
        },
        "silent_failures": {
          "max_count": 0
        },
        "quality_flag_classification": [
          {
            "flag": "low_confidence_text",
            "expected_pages": [],
            "min_precision": 1.0,
            "min_recall": 1.0
          }
        ],
        "table_structure": [
          {
            "page": 0,
            "expected_rows": [["Item", "Total"], ["Widget", "$10.00"]],
            "min_row_recall": 1.0,
            "min_cell_recall": 1.0,
            "min_cell_f1": 1.0
          }
        ],
        "span_bbox": [
          {
            "page": 0,
            "text": "Hello",
            "provenance": "native",
            "min_x0": 40.0,
            "max_x0": 90.0
          }
        ],
        "pages": [
	          {
	            "index": 0,
	            "artifact_id": "example-document-fingerprint:p000000:pagehashprefix",
	            "page_fingerprint": "64-character-page-fingerprint",
	            "route": "native_fast_path",
	            "empty_text_output": false,
	            "required_text": ["Hello Glyphrush"],
	            "layout_block_counts": {
	              "block_count": 1,
	              "paragraph_blocks": 1
	            },
	            "required_flags": [],
	            "required_reasons": []
	          }
        ]
      }
    }
  ]
}
```

The eval command emits a JSON report and exits non-zero when any gate fails. Top-level `report_version`, `run_metadata`, `run_configuration`, `corpus_fingerprint`, `category_counts`, `category_summaries`, optional `category_coverage`, `passed`, `quality_passed`, `quality_failed`, `failed_checks`, `cache_hits`, and `cache_misses` fields make report schema, parser provenance, output-affecting option state, evaluated-source provenance, corpus coverage, category-level quality, coverage-gate state, quality-gate state, and warm-cache state easy to consume from scripts, while top-level diagnostic counters make fallback/OCR/warning drift visible before digging into per-document checks. `failure_samples` include up to ten failed document/check pairs or top-level coverage failures with expected and actual values for quick triage. Strict `silent_failures` checks also treat empty derived-text pages as failures unless they are explicitly expected blank pages or acknowledged OCR-required pages; use page-level `"empty_text_output": true` only for pages that are expected to be blank or intentionally textless.

## Development

```sh
cargo fmt --all
cargo test --workspace
```

The tests generate tiny valid PDFs at runtime, so the test suite does not depend on local corpus files.
