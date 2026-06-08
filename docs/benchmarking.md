# Benchmarking

`glyphrush bench <pdf>` runs the current parser once and emits a JSON summary:

- `report_version`: benchmark report envelope schema, currently `glyphrush-bench-report-v1`.
- `quality_status`: `checked` when the benchmark embedded an eval report, otherwise `not_checked_no_eval_manifest`. Use `--require-quality` in CI when `not_checked_no_eval_manifest` should be a nonzero benchmark result instead of a speed-only report.
- `backend`.
- `run_metadata`: parser/backend provenance for the benchmark run.
- `run_configuration`: output-affecting options for the benchmark run: `span_geometry`, `ocr_sidecar`, `ocr_command`, `ocr_http_url`, `ocr_command_input`, and `ocr_timeout_ms`.
- `requirements`: gate switches requested for the benchmark run: `require_quality`, `require_baselines`, `require_baseline_quality`, `require_coverage_preset`, `require_speedups`, and `require_speedup_claims`.
- `speedup_claims`: one entry per `--require-speedup` or `--require-speedup-claim` request. Each entry records the named baseline, required and actual Glyphrush speedup, speed comparability, whether the speed threshold passed, Glyphrush and baseline quality-check/pass status, `quality_backed`, and a conservative `claim_passed` verdict.
- `requested_baseline_presets`: preset comparison sets expanded into baseline wrappers, such as `["glyphrush-v0"]`.
- `metadata`: parser/backend/source provenance copied from the structured artifact.
- `document_fingerprint`.
- `page_count`.
- `worker_count`: the effective number of page extraction worker slots used for a single-PDF benchmark.
- `wall_us`.
- `pages_per_sec`.
- `artifact_bytes`: size of the structured Glyphrush document artifact encoded as JSON.
- Bench JSON includes `run_metadata.parser_name`, `run_metadata.parser_version`, `run_metadata.backend`, `run_metadata.backend_version`, `run_configuration`, and `requirements` so saved reports can be compared without inspecting a nested document, cache key, or original shell command. Single-file reports also include `metadata.source_size_bytes` and `metadata.source_modified_unix_ms` so speed and quality results remain tied to the source artifact that produced them.
- `allocated_bytes`: requested allocation bytes during the parser/cache run. Corpus `--jobs` uses thread-local accounting to keep per-document counters isolated; single-PDF page workers use a process-wide delta so worker-thread page extraction is included.
- `allocated_bytes_per_page`: `allocated_bytes` divided by page count.
- `text_output_bytes`, `text_output_line_count`, `text_output_word_count`, and `empty_text_output`: size and emptiness of the same derived text view emitted by `parse --format text`.
- `peak_rss_bytes`: process peak resident set size sampled with platform `getrusage` support.
- `stage_timings_us`: summed parser stage timings across pages.
- `page_latency_us`: p50, p95, and max per-page parser-stage latency.
- `route_latency_us`: p50, p95, and max per-page parser-stage latency split by route.
- `route_counts`: per-route page counts for `native_fast_path`, `needs_fallback`, `ocr_fallback`, and `unsupported`.
- `route_reason_counts`: deterministic classifier reason strings counted across pages.
- `fallback_pages`.
- `ocr_pages`.
- `ocr_required_pages`.
- `ocr_applied_pages`.
- `image_artifact_count`: total drawn image or image-backed form artifacts represented in structured page artifacts.
- `image_artifact_pages`: pages with at least one image artifact.
- `fallback_action_counts`: page counts for requested or applied fallback actions.
- `quality_flag_counts`: per-flag page counts for `requires_ocr`, `low_confidence_text`, `broken_encoding`, `layout_uncertain`, `table_uncertain`, and `unsupported_feature`.
- `warnings_count`.
- `warnings`: artifact-level diagnostics such as OCR-required pages without OCR output or unsupported feature caps.
- `cache_status`.
- `cache_key`.
- `baselines`: optional external baseline command results.
- `silent_failure_count` and `silent_failure_pages`: optional eval-derived summary emitted only when `--eval-manifest` includes `silent_failures` checks. Each page entry is path-qualified and includes page index, unexpected quality flags, and whether empty derived text was unexpected.
- `quality`: optional full eval report when `--eval-manifest` is used.
- `cache_probe`: optional cold/warm cache run comparison when `--cache-probe` is used.

`ocr_pages` is kept as a compatibility alias for `ocr_required_pages`.

Single-PDF benchmarks accept `--jobs <N>` for opt-in page extraction parallelism. Glyphrush runs at most `N` pages concurrently, then merges worker results back by page number before building the structured artifact and any embedded `--eval-manifest` quality report. This keeps page order deterministic for text, layout, and reading-order checks.

`parse <pdf> --jobs <N>` uses the same deterministic page-worker path for one PDF and records the effective count in `global_diagnostics.worker_count`. Cache hits reuse the cached page artifacts but reset `worker_count` for the current invocation.

Use `glyphrush inspect <pdf-or-directory> --pages` as a lightweight triage step before full benchmarks. It runs the same extraction/classifier path and emits per-page artifact IDs, page fingerprints, dimensions, stage timings, route, quality flags, route reasons, native/OCR span counts, native text bytes, image artifact counts, bounded layout block summaries, table row/cell counts when tables are recovered, and warnings without dumping full spans or layout blocks. Add `--jobs <N>` to triage a single PDF with page workers or a directory with per-PDF workers; worker results are merged back into deterministic page or filename order and the effective count is emitted as `worker_count`. Add `--cache-dir <dir>` to reuse structured artifacts across repeated triage runs; single-file output reports `cache_status` and `cache_key`, and directory output reports aggregate `cache_hits`/`cache_misses` plus per-document cache status. Directory output preserves stable filename order and includes aggregate fallback/OCR/warning counts. This is useful when newly dropped PDFs need a quick answer about which pages require OCR, table recovery, or manual review before adding labels to an eval manifest.

Use `glyphrush feature-parity` when the question is "how close are we to LiteParse?" rather than "how fast was this corpus?" The command emits `glyphrush-feature-parity-report-v1` with the selected backend, parser metadata, a LiteParse capability matrix, status counts, the adaptive no-silent-failure quality policy, and the recommended quality-backed speedup gate. Its `readiness` block answers the claim boundary directly: `native_text_speed_race_ready` can be true when hot-path capabilities and quality-backed speed gates exist, while `native_text_speed_claim_ready` remains false until a saved benchmark report supplies passing LiteParse and LiteParse-no-OCR speedup claims plus the recommended coverage preset. `native_text_speed_claim_blockers` explains whether the blocker is missing benchmark evidence, missing quality-backed LiteParse claims, missing coverage-preset enforcement, or missing corpus categories. `full_liteparse_drop_in_ready` and `glyphrush_product_parity_ready` remain false until partial/planned gaps close. Add `--bench-report <saved-bench-json> --require-speed-evidence` when the parity report itself should fail unless the saved benchmark contains passing, quality-backed `liteparse` and `liteparse-no-ocr` speedup claims. The embedded `benchmark_evidence.quality_categories` array lists labeled eval categories with document/page counts, failed checks, and pass state, so a speed claim is readable next to the corpus classes that backed it. `benchmark_evidence.coverage_requirement` always lists the recommended `glyphrush-v0` category target, its present and missing categories, and a `required` flag; datasheet-only seed reports can therefore show passing speed evidence while still showing `required: false` and the missing full-coverage classes. Add `--require-coverage-preset glyphrush-v0` when a release or comparison job must fail unless the saved report covers `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`; the failure payload includes the required, present, and missing categories with `required: true`. This keeps "faster than LiteParse" tied to the same benchmark verdicts that already check Glyphrush quality, baseline quality, speed comparability, and category coverage. It is intentionally conservative: native extraction, explicit quality flags, structured outputs, speed-plus-quality benchmarking, cache snapshots, page-selective OCR adapters, Python/Node wrappers, and PDFium rendered-image OCR handoff are implemented; layout and tables are partial; WASM bindings and MuPDF remain planned; bundled built-in OCR is not planned because OCR should stay optional and page-selective.

Directory benchmarks accept `--jobs <N>` for opt-in per-PDF parallelism. Glyphrush runs at most `N` documents concurrently, then merges worker results back by the original stable filename order before computing corpus aggregates and `corpus_fingerprint`. Directory runs intentionally keep each document's page extraction serial to avoid nested worker blow-up; use single-PDF `--jobs` when investigating one large PDF. This keeps output order deterministic while allowing large local corpora in `test/` to avoid fully serial document extraction when the current backend and machine can tolerate it.

`glyphrush bench <directory>` discovers top-level PDF files in stable filename order and emits:

- `report_version`: benchmark report envelope schema, currently `glyphrush-bench-report-v1`.
- `quality_status`: `checked` when the benchmark embedded an eval report, otherwise `not_checked_no_eval_manifest`.
- `run_metadata`: parser/backend provenance for the benchmark run.
- `run_configuration`: output-affecting options for the benchmark run.
- `requirements`: requested strict benchmark gates.
- `speedup_claims`: corpus-level verdicts for each `--require-speedup` or `--require-speedup-claim` request, computed from the aggregate corpus baseline comparison and corpus quality status.
- `requirements.require_coverage_preset`: optional corpus coverage gate requested with `--require-coverage-preset`, such as `"glyphrush-v0"`.
- `document_count`.
- `worker_count`: the effective number of per-PDF worker slots used for the corpus run.
- `corpus_fingerprint`: SHA-256 over stable document labels, document fingerprints, and page counts.
- aggregate `page_count`, `wall_us`, `pages_per_sec`, `artifact_bytes`, `allocated_bytes`, `allocated_bytes_per_page`, `text_output_bytes`, `text_output_line_count`, `text_output_word_count`, `empty_text_output_documents`, `empty_text_output_pages`, `peak_rss_bytes`, `stage_timings_us`, `page_latency_us`, `route_latency_us`, `route_counts`, `route_reason_counts`, `fallback_pages`, `ocr_pages`, `ocr_required_pages`, `ocr_applied_pages`, `image_artifact_count`, `image_artifact_pages`, `fallback_action_counts`, `quality_flag_counts`, `warnings_count`, `warning_samples`, `cache_hits`, and `cache_misses`. For corpus runs, top-level `wall_us` is effective Glyphrush parser wall time for the configured worker chunks and excludes external baseline child-process time; per-document `wall_us` values remain available and can sum higher than the top-level value when `--jobs` runs documents concurrently.
- `category_summaries`: optional Glyphrush benchmark summaries keyed by eval manifest `category` when `--eval-manifest` is used. Each category records document/page counts, `wall_us`, `pages_per_sec`, fallback/OCR counts, route counts, quality-flag counts, warning counts, eval failed checks, and category pass/fail state.
- `baselines`: aggregate external baseline command results, when requested.
- `silent_failure_count` and `silent_failure_pages`: optional eval-derived corpus summary emitted only when `--eval-manifest` includes `silent_failures` checks. Page entries use corpus-relative document paths so a speed-plus-quality report can expose zero-silent-failure status without traversing nested eval checks.
- `quality`: optional full eval report, when requested. Absent quality reports are paired with `quality_status: "not_checked_no_eval_manifest"` so speed-only runs stay explicit.
- `cache_probe`: aggregate cold/warm cache comparison, when requested.
- `documents`: per-PDF benchmark summaries with paths relative to the input directory, including each document's `metadata`, text output metrics, `route_latency_us`, `route_counts`, `route_reason_counts`, `image_artifact_count`, `image_artifact_pages`, `quality_flag_counts`, and raw `warnings`.

Stage timings currently include `open_us`, `classify_us`, `native_extract_us`, `layout_us`, `table_us`, `render_us`, `ocr_us`, `merge_us`, and `total_us`. They are parser-stage counters, not a replacement for wall-clock timing; both matter because a fast wall time with hidden OCR/layout work or poor fallback behavior is not a useful win. In v0, `table_us` measures cheap table-likelihood signal work, including text delimiter density and ruled-line scanning; full table reconstruction is still a later stage.

`fallback_action_counts` groups the page-level work decisions that caused fallback cost or would cause it once the heavier adapters exist: `ocr_requested_pages`, `ocr_applied_pages`, `heavy_layout_pages`, `table_recovery_pages`, and `render_pages`. `ocr_requested_pages` counts pages routed to OCR fallback; `ocr_applied_pages` counts pages where OCR text was actually merged, including sidecar OCR. `render_pages` counts pages with nonzero render-stage timing, so it remains zero for sidecar OCR or default PDF-path OCR commands and increments only when a renderer actually produced a page image, such as PDFium `--ocr-command-input rendered-image`.

`peak_rss_bytes` is sampled for the current Glyphrush process, not external baseline child processes. `allocated_bytes` is allocation pressure measured by a process-wide counting allocator around the parser/cache path only; it is not net live memory and does not include JSON serialization or external baseline child processes. `artifact_bytes` measures Glyphrush's structured artifact size, while `text_output_bytes` measures the derived text view and baseline `output_bytes` measures each wrapper's stdout size. Per-document baseline results include optional top-level `target` copied from valid `BASELINE --describe` metadata, optional full `description` metadata, `description_status` for that describe probe including stable failure `error_kind` values, `timed_out`, `timeout_ms`, `stdout_sha256`, `stdout_line_count`, `stdout_word_count`, `empty_output` for successful runs that emitted no stdout, `quality_status`, plus a bounded `stderr_preview` when the wrapper writes stderr. `quality_status` is `checked` when baseline stdout was scored against manifest text/reading/table expectations, `not_checked_no_expectations` when no eval manifest or no baseline-supported expectations matched that PDF, `not_checked_timed_out` when expectations existed but the baseline timed out, and `not_checked_execution_failed` when expectations existed but the wrapper could not be executed. Use `--require-baselines` when automation should reject missing, failed, or timed-out external comparison runs; use `--require-baseline-quality` when automation should also reject baselines whose stdout was not quality-checked or failed those checks; use `--require-speedup NAME=RATIO` when automation should reject speed reports where Glyphrush is not at least `RATIO` times faster than a named baseline such as `liteparse`. Use `--require-speedup-claim NAME=RATIO` when automation should additionally reject speed-only evidence and require both Glyphrush and baseline quality checks to pass before accepting the faster-than-baseline claim. Matching `speedup_claims` entries make this explicit in the JSON: `speed_passed` is only the numeric timing gate, while `claim_passed` is true only when the speed gate passed and both Glyphrush and the baseline have passing quality checks. Glyphrush still writes the JSON report before exiting nonzero.

Each per-document baseline result and corpus baseline summary includes `comparison`:

- `glyphrush_wall_us`.
- `baseline_wall_us`.
- `speed_comparable`: false when the baseline failed or a corpus baseline only parsed part of the corpus.
- `glyphrush_speedup`: `baseline_wall_us / glyphrush_wall_us`; values above `1.0` mean Glyphrush was faster.
- `baseline_speedup`: `glyphrush_wall_us / baseline_wall_us`; values above `1.0` mean the baseline was faster.
- `glyphrush_text_output_bytes`.
- `baseline_output_bytes`.
- `baseline_to_glyphrush_output_bytes`: baseline stdout bytes divided by Glyphrush derived text bytes.

Use `glyphrush baseline-check --baseline NAME=EXECUTABLE` before longer baseline runs to verify wrapper metadata quickly. `glyphrush baseline-check --baseline-preset glyphrush-v0` preflights the core LiteParse, LiteParse no-OCR, PyMuPDF, and pdfplumber set. It runs `EXECUTABLE --describe` for each baseline, emits `report_version` (`glyphrush-baseline-check-report-v1`), parser/backend run metadata, per-baseline description JSON when valid, stdout/stderr byte counts, stderr previews, timeout/error details, stable describe `error_kind` values for failures, `describe_success_count`, and `all_described`. Describe failure kinds include `timeout`, `missing_dependency`, `execution_failed`, `spawn_failed`, `empty_describe_output`, and `invalid_describe_output`. At least one explicit baseline or preset baseline is required; an invocation with no explicit baselines and no preset still emits the JSON envelope with `baseline_count: 0` and `all_described: false`, then exits nonzero so automation cannot confuse a missing comparison setup for a passing LiteParse/PyMuPDF/pdfplumber preflight.

Add `--pdf <file>` to run each wrapper against one smoke PDF and emit output bytes, stdout SHA-256, line/word counts, empty-output status, stderr previews, stable `error_kind` values for failures, `smoke_success_count`, and `all_smoke_passed`. Add `--pdf <directory>` to smoke every top-level PDF in stable order and emit per-baseline aggregate document counts plus up to three `smoke.failure_samples` with failed document path, exit status, `error_kind`, stderr preview, and error. Current smoke failure kinds include `timeout`, `missing_dependency`, `execution_failed`, `spawn_failed`, and `invalid_smoke_target`. Add `--strict` for CI or benchmark preflight scripts: the command still writes its JSON report, then exits nonzero if any describe probe failed or if a smoke PDF was supplied and any smoke probe failed. This catches missing wrapper paths, broken comparison metadata, and missing parser dependencies before a long corpus run.

Use `glyphrush backend-check --pdf <file-or-directory>` before backend spikes or corpus triage to verify the selected PDF backend itself. Single-PDF smoke reports include stable `error_kind` values such as `encrypted_pdf_requires_password` while preserving source size and fingerprint when the file can be read. Directory smoke reports aggregate success/failure counts and include up to three `failure_samples` with path, stable error kind, and concrete error text, so mixed corpora expose unsupported files without requiring callers to scan every child document entry.

Corpus baseline summaries include optional top-level `target` copied from the first valid baseline description, optional full `description` metadata from the first described per-document run, `description_status` from the first matching per-document baseline run, `successful_documents`, `failed_documents`, `timed_out_documents`, `successful_pages`, `failed_pages`, `timed_out_pages`, `empty_output_documents`, `empty_output_pages`, `success_rate`, aggregate `pages_per_sec`, `successful_pages_per_sec`, and up to three `failure_samples` with stable baseline `error_kind` values when a wrapper failed. When baseline quality summaries are available, corpus summaries also include `quality_status`, `quality_documents`, `quality_unchecked_documents`, `quality_passed_documents`, `quality_failed_documents`, `quality_failed_checks`, `quality_required_text_failed_documents`, `quality_text_recall_failed_documents`, `quality_reading_order_failed_documents`, `quality_table_structure_failed_documents`, `quality_category_summaries`, `quality_pass_rate`, and up to three `quality_failure_samples` with the failed document path, failed-check count, and failed check type names. Corpus `quality_status` is `checked` only when every baseline run in that corpus summary was quality-scored, `partially_checked` when only a subset was scored, `not_checked_no_expectations` when no baseline-supported expectations matched any document, and `not_checked_baseline_failures` when expectations existed but no baseline run could be scored because the wrapper failed or timed out. `quality_unchecked_documents` is the number of baseline document runs not scored by quality expectations. `quality_category_summaries` is keyed by eval manifest `category`, with missing or blank categories reported as `uncategorized`, and records document/page counts, passed/failed document counts, failed checks, category pass rate, and pass/fail state for the baseline's quality-checked documents. This avoids treating a fast wrapper that fails part of the corpus, times out, exits successfully with empty output, hides broken comparison metadata, or misses labeled text gates as equivalent to one that actually parsed every page.

`--cache-dir <dir>` can be used with file or directory benchmarks, `inspect --pages` triage, and `manifest` generation. Cache hits reuse structured artifacts; cache misses parse and then write a JSON snapshot envelope into the cache directory. Cache keys include parser name/version, parser cache schema, and backend adapter version, so artifact-shape, page-fingerprint behavior, fallback-behavior, warning/diagnostic behavior, derived text ordering, parser-version changes, adapter-version changes, or cache snapshot schema changes intentionally force fresh misses instead of reusing stale JSON. Snapshot files include `snapshot_version`, `cache_schema`, `cache_key`, parser/backend metadata, `document_fingerprint`, and the cached `artifact`, so warm-run reuse is inspectable from the cache file itself instead of only from the filename. If a matching snapshot is unreadable, corrupt, or fails envelope validation, Glyphrush treats it as a miss, emits a `cache_snapshot_ignored` warning, reparses, and overwrites the snapshot; cache state is never allowed to be the only reason parsing fails. OCR sidecar cache fingerprints are scoped to files matching the current PDF's `<stem>.pNNNNNN.txt` names; adding sidecar files for other PDFs does not invalidate this document's warm path. OCR command cache fingerprints include the command input mode, command path, command file content when the executable path is a file, and OCR timeout, so PDF-path and rendered-image OCR runs cannot reuse each other's artifacts. OCR HTTP cache fingerprints include the command input mode, endpoint URL, and OCR timeout, so PDF-path and rendered-image HTTP OCR runs cannot reuse each other's artifacts, and reports do not reuse sidecar/command artifacts for HTTP adapter runs. On cache hits, page-level parser stage timings and artifact `total_stage_time_us` are reset to zero because native extraction, layout, table-signal scanning, render, and OCR stages did not run during the warm path; source metadata such as `source_size_bytes` and `source_modified_unix_ms` is refreshed from the current file before the artifact is emitted.

Use `--cache-probe --cache-dir <dir>` to force a cold miss followed by a warm cache hit in one command. The cold run clears the target cache artifact first, parses the PDF, and writes the artifact. The warm run immediately reuses that artifact. Per-document reports include run statuses, timings, pages/sec, artifact size, allocation counters, RSS, stage timings, page latency, route diagnostics, image artifact counts, quality flag counts, fallback action counts, warnings, fallback counts, and a speedup ratio. Corpus probe summaries include aggregate cold and warm stage timings, allocation counters, and fallback action counters so a warm-cache report can show that parser stages did not rerun.

Use `--span-geometry` on `bench` or `eval` only when measuring or gating positioned native spans. The default benchmark path skips span-geometry extraction to represent the native-text hot path.

Use `--eval-manifest <manifest.json>` on `bench` to pair speed metrics with quality gates in the same JSON report. The benchmark timing fields measure the parser benchmark run, and the embedded eval report is computed from the same in-memory artifacts that were benchmarked instead of reparsing the PDFs. A manifest-backed run sets top-level `quality_status` to `checked`; without an eval manifest, the top-level status remains `not_checked_no_eval_manifest`. Use `--require-quality` when automation should reject speed-only benchmark evidence; Glyphrush still writes the JSON report first, then exits nonzero with `bench quality required` if no eval manifest quality report was checked. The embedded quality report includes top-level `report_version`, `run_metadata`, `run_configuration`, `manifest_path`, `manifest_sha256`, and `corpus_fingerprint` so a benchmark result can be tied to the exact eval report schema, parser/backend build, output-affecting options, quality-label file, and evaluated source set, not only a mutable path. It also includes up to ten top-level `failure_samples` with document path, check name, expected value, and actual value so failed reports can be triaged without traversing every document check. Each quality document reports `metadata` and `artifact_cache_status` so parser/backend/source provenance and cache miss/hit evidence match the benchmarked artifact. Use `--eval-category <name>` with `--eval-manifest` to restrict the embedded quality report and benchmark category summaries to one normalized coverage class, including `uncategorized` for blank or missing manifest categories. If the selected category has no manifest documents, the benchmark report is still written and the embedded quality report fails with `document_count` instead of silently passing or exiting before JSON output. If quality gates fail, Glyphrush writes the JSON report first and then exits nonzero with `bench quality failed`, matching the standalone `eval` failure pattern. A single-file benchmark can reuse a full corpus manifest; Glyphrush evaluates only the manifest document matching the benchmarked PDF and fails if none match. Directory benchmarks remain strict: every manifest document in the selected category must correspond to a discovered directory PDF so corpus gates cannot be accidentally skipped.

When matching eval documents include `silent_failures` expectations, benchmark reports also include top-level `silent_failure_count` and `silent_failure_pages`. These are copied from the existing eval check results and are intentionally absent when the manifest did not request that gate; absence means "not checked", not "zero".

When `--eval-manifest` is combined with `--baseline`, each baseline run also receives a compact `quality` summary when that document has document-level `required_text`, page-level `required_text`, `text_recall`, `reading_order`, or `table_structure` expectations. This checks the baseline wrapper stdout against the same text anchors, recall math, snippet-order scoring, and simple table row/cell scoring used by Glyphrush eval, so LiteParse/PyMuPDF/pdfplumber timing can be read next to basic text-quality evidence instead of output bytes alone. Page-level required-text anchors are flattened into full-document stdout checks for text-only baselines because those wrappers do not expose page artifacts; page-locality remains a Glyphrush artifact eval guarantee. Baseline `required_text` output includes the full `expected` anchor list plus the `missing` subset so saved comparison reports show exactly what was checked. Baseline table checks parse pipe-, tab-, or whitespace-separated rows from the wrapper's full stdout because text-only wrappers do not expose Glyphrush page artifacts; markdown separator rows such as `| --- | --- |` are ignored instead of counted as data rows. If no such expectations match, `quality_status` remains `not_checked_no_expectations`; treat that as a speed-only baseline run, not a quality comparison. If the eval manifest document has a root-level `category`, baseline quality summaries preserve it so corpus baseline aggregates can report `quality_category_summaries`; `--eval-category` applies before baseline quality expectations are attached.

Use `--ocr-sidecar <dir>` on `parse`, `bench`, `debug-page`, `eval`, or `manifest` when measuring or investigating OCR fallback plumbing without installing a full OCR engine. Sidecar text is loaded page-selectively, participates in benchmark cache keys, updates `ocr_applied_pages`, and is passed through to any embedded `--eval-manifest` quality run. Use `--ocr-command <executable>` as an alternate adapter seam for the same commands; Glyphrush invokes the command only for OCR-routed pages and treats stdout as OCR text. Use `--ocr-http-url <url>` when an OCR service should be invoked page-selectively; Glyphrush POSTs `{"pdf_path": "...", "page_index": N}` by default, accepts plain response bodies as OCR text, and accepts `application/json` responses with a string `text` field. The default `--ocr-command-input pdf-page` contract passes the PDF path and zero-based page index as the first two command arguments or HTTP `pdf_path`/`page_index` JSON. With `--backend pdfium --ocr-command-input rendered-image`, Glyphrush renders only OCR-routed pages to temporary PPM files, records `render_us`, and removes each temporary image after OCR returns; command OCR receives the rendered image path and zero-based page index as the first two arguments, while HTTP OCR receives `{"rendered_image_path": "...", "page_index": N}`. `tools/ocr/tesseract-rendered-image.sh` is a ready local Tesseract wrapper for this rendered-image command contract; set `TESSERACT_BIN`, `TESSERACT_LANG`, or `TESSERACT_PSM` when a benchmark needs a specific Tesseract install or language. OCR commands and HTTP requests are bounded by `--ocr-timeout-ms`, defaulting to `120000`; timeout failures exit nonzero instead of silently returning incomplete OCR output. `--ocr-sidecar`, `--ocr-command`, and `--ocr-http-url` are mutually exclusive. Run `glyphrush ocr-check <pdf> --page-index <N> --ocr-command <executable> --strict`, the equivalent `--ocr-sidecar` form, or `--ocr-http-url <url>` before scanned/hybrid benchmark runs to preflight the configured adapter; add `--backend pdfium --ocr-command-input rendered-image` to preflight the rendered-image command, Tesseract wrapper, or HTTP contract. The preflight emits `glyphrush-ocr-check-report-v1` with adapter mode, non-empty output status, stdout SHA-256, line/word counts, render timing when applicable, timeout state, stderr preview, and stable failure kinds such as `timeout`, `empty_output`, `missing_dependency`, `spawn_failed`, `sidecar_read_failed`, `http_request_failed`, `http_status_failed`, `http_response_decode_failed`, and `render_backend_required`, then exits nonzero after writing JSON when strict mode fails or when a non-rendering backend is asked to check the rendered-image contract.

Use `glyphrush debug-page <pdf> <page-index>` when a benchmark or eval report needs page-level explanation. The command extracts only the requested page and reports `document_page_count`, `extracted_page_count`, parser/backend/source `metadata`, the document fingerprint, selected-page `artifact_id`, `page_fingerprint`, dimensions, classifier signals, page quality, page warnings, per-stage timings, derived text-output metrics, bounded layout block/table row/cell counts, drawn image and image-backed form artifact metadata, and the route decision so fallback investigations are reproducible without paying full-document extraction cost. Add `--ocr-sidecar <dir>`, `--ocr-command <executable>`, or `--ocr-http-url <url>` to verify whether an OCR-required page receives OCR text instead of only a `requires_ocr_without_ocr_output` warning.

Use `glyphrush manifest <pdf-or-directory>` to bootstrap an eval manifest from the current parser output:

```sh
glyphrush manifest test/ > test/corpus.generated.json
glyphrush manifest test/ --category datasheet > test/corpus.generated.json
glyphrush manifest test/ --category clean_digital --coverage-preset glyphrush-v0 > test/corpus.generated.json
glyphrush manifest test/ --category datasheet --required-category datasheet > test/corpus.generated.json
glyphrush manifest test/ --category datasheet --min-category-count datasheet=5 > test/corpus.generated.json
glyphrush manifest test/ --jobs 4 > test/corpus.generated.json
glyphrush manifest test/ --cache-dir .glyphrush-cache > test/corpus.generated.json
glyphrush eval test/corpus.generated.json
glyphrush eval test/corpus.generated.json --category datasheet
glyphrush eval test/corpus.generated.json --cache-dir .glyphrush-cache
```

The Python and Node wrappers expose the same manifest generator as `glyphrush.manifest(...)` and `manifest(...)`. Use those APIs when a script receives newly dropped PDFs and needs to create the eval skeleton before running quality-backed `eval` and `bench` gates; the wrappers only delegate to the native CLI and do not maintain separate manifest logic.

The generated manifest is a starting point, not ground truth. `manifest --category <name>` stamps every generated document with a benchmark coverage class so eval and benchmark reports can aggregate category counts and summaries immediately after PDFs are dropped into `test/`. Use `manifest --coverage-preset glyphrush-v0` to emit the core v0 coverage gate: `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`, each with a minimum count of one. Use repeated `manifest --required-category <name>` to emit manifest-level `required_categories` when a quality gate should fail until the corpus covers specific benchmark classes such as `clean_digital`, `scanned`, `hybrid`, or `rotated`; eval and embedded bench-quality reports emit `category_coverage` and count missing categories as failed checks. Use repeated `manifest --min-category-count <name=count>` to emit manifest-level `min_category_counts` when a class needs a minimum fixture count before accepting speed-plus-quality claims; manual minimums are merged with preset minimums by taking the larger count for each category. `eval --category <name>` filters a mixed manifest to one normalized coverage class, with missing or blank manifest categories selectable as `uncategorized`; category coverage gates are evaluated against that filtered category set. `manifest --jobs <N>` parallelizes directory skeleton generation per PDF, or single-PDF generation per page, then merges output back into deterministic filename/page order. `manifest --cache-dir <dir>` reuses parsed artifacts across repeated manifest generation, including warm artifacts produced by `parse`, `bench`, or `inspect --pages`. The generated JSON intentionally does not record worker count or cache status, so the same corpus produces stable fingerprints and expectations regardless of worker count or cold/warm cache state. It records deterministic provenance fields (`manifest_version`, `generator`, `corpus_fingerprint`, optional `required_categories`, optional `min_category_counts`, optional per-document `category`, per-document `document_fingerprint`, `source_size_bytes`, and `source_modified_unix_ms`) and pins structural and diagnostic expectations that can be measured automatically: page counts, fallback/OCR counts, OCR-needed classification, non-empty quality-flag classification gates, image artifact counts, warning counts, exact required warning strings, route counts, route-reason counts, quality-flag counts, recovered table-structure checks for pages with table blocks, bounded `span_bbox` checks for non-page-wide spans when `--span-geometry` is requested, `silent_failures: 0`, and page-level artifact ID, page fingerprint, route, empty-text, bounded page-local required-text anchor, image, layout-block, flag, and reason acknowledgements. Review generated table rows and bbox samples, then add labeled document-level `required_text`, `text_recall`, `reading_order`, `table_structure`, and additional `span_bbox` checks before using the manifest to make quality claims.

## External Baselines

`glyphrush bench` can run external parser wrappers with `--baseline NAME=EXECUTABLE`. The executable receives the PDF path as its only argument. Glyphrush records wall time, exit status, timeout status, stdout byte count, stderr byte count, stable failure `error_kind` values, and spawn errors without interpreting the baseline output. Each baseline invocation, including its optional `--describe` probe, is bounded by `--baseline-timeout-ms <ms>`; the default is 120000 ms. Timed-out baselines are reported as failed with `timed_out: true`, `error_kind: "timeout"`, an error string, and `comparison.speed_comparable: false` instead of blocking the benchmark.

Example:

```sh
scripts/setup-baselines.sh
scripts/bench-liteparse.sh --dry-run
GLYPHRUSH_BENCH_OUTPUT=.glyphrush-baselines/reports/liteparse-speed-gate.json scripts/bench-liteparse.sh
GLYPHRUSH_BENCH_CATEGORY=all GLYPHRUSH_BENCH_MANIFEST=test/corpus.full.json GLYPHRUSH_BENCH_COVERAGE_PRESET=glyphrush-v0 GLYPHRUSH_BENCH_OUTPUT=.glyphrush-baselines/reports/liteparse-full-gate.json scripts/bench-liteparse.sh
glyphrush bench test/ --baseline-preset glyphrush-v0
glyphrush --backend auto bench test/ --eval-manifest test/corpus.json --baseline-preset glyphrush-v0 --require-coverage-preset glyphrush-v0 --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5
glyphrush bench test/ --eval-manifest test/corpus.json --baseline-preset glyphrush-v0 --require-coverage-preset glyphrush-v0 --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5
glyphrush baseline-check --pdf test/ --baseline-preset glyphrush-v0
```

The checked-in baseline wrappers emit text to stdout and expose dependency-free metadata with `--describe`. Glyphrush records valid JSON object descriptions under each baseline result so reports show the comparison target, dependency requirements, command hint, and OCR mode when the wrapper exposes them. `--baseline-preset glyphrush-v0` expands to LiteParse, LiteParse no-OCR, PyMuPDF, and pdfplumber; Marker and Docling are intentionally left as manual `--baseline` additions for heavier quality-context runs. Run `scripts/setup-baselines.sh` to install the core baseline tools locally under `.glyphrush-baselines/`; wrappers auto-detect local `lit`, local PyMuPDF/pdfplumber, and local Tesseract `eng.traineddata` for LiteParse OCR.

Use `scripts/bench-liteparse.sh` for the repeatable local speed gate: by default it first runs a strict `baseline-check --pdf <target> --baseline-preset glyphrush-v0` preflight to catch missing wrappers or file-specific baseline failures before expensive benchmark work, then runs the optimized release-profile PDFium backend against `test/` with `test/corpus.datasheets.json`, requires the `glyphrush-v0` baselines, requires baseline quality checks, enforces `--require-speedup-claim liteparse=2.0`, and also enforces `--require-speedup-claim liteparse-no-ocr=1.5` so native-text wins are not hidden behind OCR cost. Set `GLYPHRUSH_BENCH_OUTPUT=<path>` to save the benchmark report and automatically follow it with `feature-parity --bench-report <path> --require-speed-evidence`, which verifies the saved report contains passing, quality-backed LiteParse and LiteParse-no-OCR speed claims. The default `GLYPHRUSH_BENCH_CATEGORY` is `datasheet`; set `GLYPHRUSH_BENCH_CATEGORY=all` to omit `--eval-category` and evaluate every manifest category in a mixed benchmark corpus. Set `GLYPHRUSH_BENCH_COVERAGE_PRESET=glyphrush-v0` when the benchmark manifest covers the full v0 corpus classes and the run should fail unless that coverage is present; leave it unset for the current datasheet-only seed corpus. When both `GLYPHRUSH_BENCH_OUTPUT` and `GLYPHRUSH_BENCH_COVERAGE_PRESET` are set, the follow-up parity check also receives `--require-coverage-preset <preset>` so the saved speed report is checked against the same full claim-readiness surface. PDFium benchmark reports may show `worker_count: 1` even when a higher `--jobs` value is requested, because the adapter reuses loaded document handles and currently serializes corpus-level document extraction to avoid concurrent live PDFium handles in one process. Override `GLYPHRUSH_BENCH_PDF_DIR`, `GLYPHRUSH_BENCH_MANIFEST`, `GLYPHRUSH_BENCH_CATEGORY`, `GLYPHRUSH_BENCH_JOBS`, `GLYPHRUSH_BENCH_BACKEND`, `GLYPHRUSH_BENCH_FEATURES`, `GLYPHRUSH_BENCH_LITEPARSE_SPEEDUP`, `GLYPHRUSH_BENCH_LITEPARSE_NO_OCR_SPEEDUP`, `GLYPHRUSH_BENCH_COVERAGE_PRESET`, `GLYPHRUSH_BENCH_BASELINE_TIMEOUT_MS`, or `GLYPHRUSH_BENCH_OUTPUT` when running a different corpus or saving a report. `baseline-check --pdf <directory>` smokes every top-level `.pdf`/`.PDF` in stable filename order and reports `smoke_document_count`, aggregate per-baseline document pass/fail counts, bounded failure samples, and per-document smoke results before a long corpus benchmark. `tools/baselines/liteparse-text.sh` uses LiteParse's `lit parse --format text --quiet` path with OCR enabled by default, while `tools/baselines/liteparse-no-ocr-text.sh` runs the same LiteParse text path with `--no-ocr` for native-text-only timing. You can also set `LITEPARSE_NO_OCR=1` on `liteparse-text.sh`, but the explicit no-OCR wrapper is preferred when reports need distinct baseline names.

`scripts/verify.sh --dry-run` prints the project verification commands, and `GLYPHRUSH_VERIFY_PDFIUM=1 scripts/verify.sh` adds focused PDFium-feature checks for the same path `scripts/bench-liteparse.sh` recommends: selected-backend feature parity plus rendered-image OCR command handoff. CI enables this flag so the faster PDFium backend and LiteParse-style rendered OCR seam are compiled and exercised on every pushed branch, even though the default local verifier stays lightweight.

The benchmark report keeps Glyphrush image, quality, and fallback counters next to baseline timing and success-rate metrics. This is intentional: LiteParse/PyMuPDF/pdfplumber comparisons should never be latency-only. Use `bench --eval-manifest <manifest.json>` for one-command speed-plus-quality reports, and add `--eval-category <name>` when a mixed corpus needs category-specific speed-plus-quality evidence. Use `bench --baseline-preset glyphrush-v0` for the core LiteParse, LiteParse no-OCR, PyMuPDF, and pdfplumber comparison set; benchmark and baseline-check JSON both include `requested_baseline_presets` so saved reports show which preset was expanded. Add `--require-baselines` to fail a benchmark job after JSON output when a requested wrapper is missing, exits nonzero, or times out. Add `--require-baseline-quality` to additionally fail when baseline text quality was not checked or did not pass the manifest expectations. Add `--require-coverage-preset glyphrush-v0` to fail the benchmark itself after writing JSON unless the embedded eval report covers `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`; the failure is recorded in `quality.category_coverage` and `requirements.require_coverage_preset`. Use `--require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5` with an eval manifest when a CI or release gate should reject a faster-than-LiteParse claim unless both the default OCR-capable LiteParse path and the native-text no-OCR path meet their speed thresholds with passing quality reports. Use `glyphrush eval` separately when you want only quality gates such as page counts, image artifact counts, OCR-required pages, route flags, layout block counts, text recall, reading order, flag precision/recall, and table structure. Add `eval --cache-dir <dir>` for repeated warm quality gates; top-level `cache_hits` and `cache_misses` show whether the report reused cached artifacts or reparsed documents.

## Eval Manifests

`glyphrush eval <manifest.json>` is the first quality harness. It parses each listed PDF and checks expected counts and required text snippets. Paths are resolved relative to the manifest file, and the report includes top-level `report_version` (`glyphrush-eval-report-v1`), `run_metadata`, `run_configuration`, `manifest_path`, `manifest_sha256`, and `corpus_fingerprint` over the evaluated document paths, document fingerprints, and page counts. A manifest or category filter that selects zero documents is a failed quality report, not a pass; it emits a `document_count` failure sample with `expected: {"min": 1}`. Add optional root-level document `category` values to track benchmark coverage classes such as `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, or `large`; eval reports expose aggregate `category_counts`, `category_summaries`, and per-document `category`, counting missing or blank categories as `uncategorized`. Add optional manifest-level `required_categories` to turn corpus coverage into a presence gate, and `min_category_counts` to require minimum document counts per class; the report emits `category_coverage` with sorted required, present, missing, minimum-count, and under-minimum category names. Missing categories increment `failed_checks` with a `required_categories` failure sample, while under-counted categories use `min_category_counts`. `category_summaries` records document/page counts, passed/failed document counts, failed checks, and category-level quality pass/fail state, making it clear whether a regression is isolated to a corpus class instead of only visible in top-level totals. `run_configuration` records output-affecting switches: `span_geometry`, `ocr_sidecar`, `ocr_command`, `ocr_http_url`, `ocr_command_input`, and `ocr_timeout_ms`. Use `--jobs <N>` to evaluate manifest documents concurrently; worker results are merged back in manifest order and the effective count is emitted as `worker_count`. Use `--cache-dir <dir>` to reuse structured artifacts across repeated eval runs; top-level `cache_hits` and `cache_misses` make cold/warm quality gates scriptable without scanning every document. Eval reports include aggregate and per-document `page_count`, `fallback_pages`, `ocr_required_pages`, `ocr_applied_pages`, `image_artifact_count`, `image_artifact_pages`, `empty_text_output_pages`, `route_counts`, `route_reason_counts`, `quality_flag_counts`, `fallback_action_counts`, and `warnings_count` even when the manifest does not explicitly assert those checks. Use per-document `expect_by_backend` overrides when a backend legitimately differs on diagnostic counters, routes, OCR-needed pages, image artifacts, or page flags; eval and embedded bench-quality reports shallow-merge `expect_by_backend.<backend>` over the shared `expect` object for the artifact backend being scored. If a document entry includes root-level `document_fingerprint`, `source_size_bytes`, or `source_modified_unix_ms`, eval checks those before content gates so stale generated manifests fail explicitly when the underlying PDF changes. Top-level `passed`, `quality_passed`, `quality_failed`, and `failed_checks` fields expose the aggregate gate state, and `failure_samples` expose up to ten failed checks for quick triage while the full per-document `checks` map remains available for complete debugging. Text-based checks use layout-block order when layout blocks are available, including opt-in span-geometry ordering, and append distinct OCR text when OCR spans are present. Each eval document includes artifact `metadata` with parser/backend/source provenance, plus `artifact_cache_status`, so standalone quality reports can be compared against benchmark reports without losing version context.

Supported v0 expectations:

- `page_count`.
- manifest-level `required_categories`: optional corpus coverage gate; missing categories are reported in top-level `category_coverage` and fail the eval or embedded bench-quality report.
- manifest-level `min_category_counts`: optional corpus coverage count gate such as `{"datasheet": 2, "scanned": 1}`; under-counted categories are reported in `category_coverage.under_minimum`.
- root-level `category`: optional benchmark coverage class copied into eval reports and summarized in top-level `category_counts` and `category_summaries`; missing or blank values are reported as `uncategorized`.
- root-level `document_fingerprint`, `source_size_bytes`, and `source_modified_unix_ms`: optional source provenance checks generated by `glyphrush manifest`.
- root-level `expect_by_backend`: optional map keyed by backend name, such as `lopdf` or `pdfium`; each value is shallow-merged over `expect` before scoring that backend, so only top-level fields that differ need to be repeated.
- `fallback_pages`.
- `ocr_required_pages`.
- `ocr_applied_pages`.
- `image_artifact_count`.
- `warnings_count`.
- `route_counts`: exact document-level page counts by classifier route.
- `route_reason_counts`: exact document-level classifier reason counts.
- `quality_flag_counts`: exact document-level page counts by quality flag.
- `required_warnings`: exact warning strings that must appear in artifact diagnostics.
- `required_text`: substrings that must appear in derived eval quality text, using layout-block order when available, serializing structured table grids as pipe-delimited rows with blank cells preserved, and appending distinct OCR text.
- `text_recall`: expected text plus optional minimum normalized word and character recall thresholds.
- `reading_order`: expected text snippet sequence plus optional minimum pairwise order score.
- `ocr_required_classification`: expected OCR-required page indices plus optional minimum precision and recall thresholds.
- `silent_failures`: maximum pages with unacknowledged quality flags.
- `quality_flag_classification`: expected page indices for any quality flag plus optional minimum precision and recall thresholds.
- `table_structure`: expected table rows per page plus optional row/cell precision, recall, and F1 thresholds.
- `span_bbox`: sample span text/provenance plus optional x0/y0/x1/y1 bounding-box ranges.
- `pages`: page-level expectations with zero-based `index`, optional `artifact_id`, `page_fingerprint`, `route`, `empty_text_output`, `required_text`, `image_artifact_count`, `layout_block_counts`, `required_flags`, and `required_reasons`.

Table checks score rows recovered from table layout blocks. Eval uses a table block's structured `table.rows[].cells[]` payload when present, then falls back to parsing the block text for compatibility with older artifacts. In v0 those blocks can come from pipe/tab text, table-routed whitespace rows including fixed-width rows with blank cells or header-guided rows with same-line or wrapped leading multi-word descriptor cells, lowercase trailing descriptor continuations, inferred trailing blank cells, and leading captions kept outside the structured grid, or table-routed positioned spans that form aligned multi-column rows, preserve omitted blank cells, merge same-line fragmented cells, same-column wrapped header rows, plus near-adjacent single- or multi-cell wrapped continuations, keep cross-column, first-column, or fragmented first-column interior section rows in-grid, and leave positioned captions outside the structured grid when `--span-geometry` is enabled. Markdown separator rows are ignored so `| --- | --- |` does not appear as an extracted data row.

Example:

```json
{
  "documents": [
    {
      "path": "invoice.pdf",
      "expect": {
        "page_count": 2,
        "fallback_pages": 0,
        "ocr_required_pages": 0,
        "ocr_applied_pages": 0,
        "image_artifact_count": 0,
        "warnings_count": 0,
        "route_counts": {
          "native_fast_path": 2,
          "needs_fallback": 0,
          "ocr_fallback": 0,
          "unsupported": 0
        },
        "route_reason_counts": {},
        "quality_flag_counts": {
          "requires_ocr": 0,
          "low_confidence_text": 0,
          "broken_encoding": 0,
          "layout_uncertain": 0,
          "table_uncertain": 0,
          "unsupported_feature": 0
        },
        "required_warnings": [],
        "required_text": ["Invoice"],
        "text_recall": {
          "expected": "Invoice total due",
          "min_word_recall": 0.95,
          "min_char_recall": 0.98
        },
        "reading_order": {
          "expected_sequence": ["Invoice", "Subtotal", "Total due"],
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
            "text": "Invoice",
            "provenance": "native",
            "min_x0": 40.0,
            "max_x0": 90.0,
            "min_y0": 20.0,
            "max_y0": 80.0
          }
        ],
        "pages": [
          {
            "index": 0,
            "artifact_id": "example-document-fingerprint:p000000:pagehashprefix",
            "page_fingerprint": "64-character-page-fingerprint",
            "route": "native_fast_path",
            "empty_text_output": false,
            "required_text": ["Invoice"],
            "image_artifact_count": 0,
            "layout_block_counts": {
              "block_count": 1,
              "paragraph_blocks": 1,
              "header_blocks": 0,
              "footer_blocks": 0
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

Supported page routes are `native_fast_path`, `needs_fallback`, `ocr_fallback`, and `unsupported`. Supported quality flags include `requires_ocr`, `low_confidence_text`, `broken_encoding`, `layout_uncertain`, `table_uncertain`, and `unsupported_feature`.

`route_counts` checks the exact document-level page count for each classifier route. Use it when a corpus fixture should lock the fast-path/fallback/OCR/unsupported split.

`route_reason_counts` checks the exact document-level count of classifier reason strings, such as `high_image_coverage_without_native_text`, `bbox_overlap`, `table_line_density`, or `duplicate_char_ratio`. Use it in corpus manifests when a page-classifier heuristic should remain stable across parser changes.

`quality_flag_counts` checks the exact document-level page count for each quality flag. Use it when a corpus fixture should lock the visible fallback/uncertainty surface, such as OCR-required, low-confidence, layout-uncertain, table-uncertain, broken-encoding, or unsupported-feature pages.

`image_artifact_count` checks the exact document-level count of drawn image and image-backed form artifacts surfaced in page artifacts. Use it when a corpus fixture should prove image-backed content remains observable instead of disappearing from the structured artifact.

Page-level `image_artifact_count` checks the exact drawn-image count on a specific page. Use it when a fixture should catch regressions that drop images from one page while the document-level total could still be masked by images elsewhere.

`required_warnings` checks exact document-level warning strings such as `p000000: requires_ocr_without_ocr_output`, `p000000: unsupported_feature: span_geometry_capped`, or `p000000: unsupported_feature: annotation_or_form`. Use it with `warnings_count` when a corpus fixture should prove an OCR-required or unsupported-feature diagnostic remains visible.

`text_recall` compares normalized expected text against derived eval quality text. Word recall uses a lowercase alphanumeric-token multiset; character recall uses a lowercase alphanumeric-character multiset. This catches missing text without requiring exact whitespace or line-break preservation.

`reading_order` finds the first occurrence of each expected snippet in derived eval quality text and scores all expected snippet pairs. Missing snippets and inverted pairs lower the score; the report includes matched positions, missing snippets, inversion count, and inversion pairs.

`ocr_required_classification` compares zero-based expected page indices against pages where Glyphrush emitted `requires_ocr`. The report includes precision, recall, true positives, false positives, and false negatives so scanned/hybrid page detection can be tracked as an explicit quality metric.

`silent_failures` counts pages with quality flags that the manifest did not explicitly acknowledge, plus pages whose derived text output is empty unless the page is explicitly marked with `"empty_text_output": true` or has an acknowledged `requires_ocr` flag. A flag is acknowledged by page-level `required_flags`, `ocr_required_classification` for `requires_ocr`, `quality_flag_classification` for the named flag, or `table_structure` for `table_uncertain`. Use `{"max_count": 0}` in strict corpus manifests to enforce that difficult or empty-output pages are flagged, expected, or evaluated, not ignored.

`quality_flag_classification` generalizes the same page-set scoring to any supported quality flag. Use it for `low_confidence_text`, `broken_encoding`, `layout_uncertain`, `table_uncertain`, `unsupported_feature`, or `requires_ocr` when a corpus label should track false positives and false negatives for that flag.

`table_structure` compares expected rows against rows parsed from detected `table` layout blocks on the target page. The v0 parser supports pipe- or tab-delimited table text, preserving empty delimited cells so blank columns are not silently collapsed, plus simple, fixed-width, or header-guided whitespace rows when the page was already routed for table recovery. Fixed-width rows align segments to header column starts so blank middle or trailing cells remain explicit in `table.rows[].cells[]`, lowercase wrapped descriptor fragments merge into the following row, and section rows stay in-grid as first-cell text with blank remaining cells; header-guided rows merge leading overflow tokens or a short lowercase wrapped descriptor line into a descriptor cell when short header columns explain multi-word row labels, merge lowercase trailing descriptor continuations into the preceding row descriptor, keep inferred trailing blank cells explicit, keep title-case section rows in-grid as first-cell text with blank remaining cells, and keep leading captions as surrounding layout text instead of structured table rows; positioned rows use the same column-grid idea when span geometry is enabled, leaving boxes absent only for omitted blank cells, merging same-line fragmented cells, same-column wrapped header rows, plus near-adjacent single- or multi-cell wrapped continuations into the same row, preserving cross-column, first-column, or fragmented first-column interior section rows as first-cell text with blank cells for the remaining columns, and keeping top/bottom captions as surrounding layout text instead of structured table rows. Positioned bullet-list rows are rejected before table acceptance, including the common PDF pattern where a standalone bullet marker row is followed by indented text rows. Markdown separator rows are filtered from Glyphrush artifacts and baseline stdout before row/cell scoring. Reports include extracted rows, missing/extra rows, missing/extra cells, row precision/recall/F1, and cell precision/recall/F1. Generated manifests seed this check when the current parser recovers at least two table rows, which gives newly dropped PDFs an immediate regression gate but should still be reviewed against labels. Baseline quality scoring applies the same row/cell metrics to pipe-, tab-, or whitespace-separated rows in baseline stdout. This is a table-quality gate for simple labeled cases, not full table reconstruction.

`span_bbox` checks that a sample native or OCR span on a page contains the requested text and falls within any supplied coordinate ranges. Use `provenance` when you need to distinguish `native` from `ocr` spans. Tight native span checks should be run with `eval --span-geometry` or `bench --span-geometry --eval-manifest`; otherwise the default hot path may only emit a page-wide native span. The `lopdf` span-geometry path applies simple text-matrix/content-matrix transforms, preserves text-state parameters across text objects, and applies text spacing, text rise, line leading, and `'`/`"` shortcut text showing before emitting native boxes. The `pdfium` path uses PDFium text-segment rectangles behind the same opt-in flag. Geometry-aware layout assembles same-baseline positioned fragments into text rows before reflow, keeps full-width bands, fragmented full-width heading rows, leading, middle, and trailing cross-column bands, conservative short section separators, and clearly separated 2-4 column reading order, and suffix-dedupes overlapping fragments when PDFium emits short repeated prefixes such as `Vo` before `Voltage`. If the requested geometry pass exceeds the bounded extraction cap or unsupported geometry such as rotation is encountered, the page keeps page-wide spans but reports `unsupported_feature` with reason `span_geometry_capped` and a document warning such as `p000000: unsupported_feature: span_geometry_capped`, so bbox-quality gaps are explicit in both quality flags and warning summaries.

Page-level `empty_text_output` checks assert whether the derived text view for a page is empty. Set it to `true` only for known blank or intentionally textless pages; otherwise strict `silent_failures` will treat an empty page as a quality failure unless the page is already acknowledged as `requires_ocr`.

The command emits machine-readable JSON with per-document checks. It exits non-zero after writing the report if any check fails, so it can be used in CI or local regression runs.

This is a measurement harness seed, not a release-grade benchmark. The next benchmark milestone should add:

- smoke-verified local installs for optional quality-context baselines such as Marker and Docling when those tools are part of a benchmark run.
- a larger labeled corpus for OCR-required classification, reading order, bbox samples, and table structure.
