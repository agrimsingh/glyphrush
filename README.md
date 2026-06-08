# Glyphrush

[![CI](https://github.com/agrimsingh/glyphrush/actions/workflows/ci.yml/badge.svg)](https://github.com/agrimsingh/glyphrush/actions/workflows/ci.yml)

Glyphrush is a native PDF parsing experiment focused on fast native-text extraction with explicit quality and fallback signals. The v0 implementation is a Rust workspace with:

- `glyphrush-core`: deterministic artifacts, page signals, classifier decisions, and extracted-page parsing.
- `glyphrush-cli`: `inspect`, `parse`, `bench`, `debug-page`, and backend/baseline preflight commands backed by a thin backend interface. The default backend choice is `auto`: plain builds resolve to `lopdf`, while PDFium-feature builds resolve to the faster PDFium adapter.
- `bindings/python` and `bindings/node`: experimental dependency-free wrappers that shell out to the native CLI and return the same JSON artifact. WASM bindings remain planned.

The dependency-light backend extracts native text through `lopdf`, can preserve simple positioned text spans with approximate boxes when explicitly requested, records cheap drawn-image metadata for direct image XObjects, image-backed form XObjects, and detected inline images without copying pixels, follows nested form image transforms for image coverage, routes OCR-required pages to optional sidecar, command, or HTTP OCR adapters, and emits structured artifacts with parser/backend/source size and modified-time metadata. The experimental PDFium backend opens PDFs through PDFium, extracts native page text, can emit PDFium text-segment boxes when `--span-geometry` is requested, records cheap image-object metadata without rendering pixels on the native extraction path, detects ruled-table vector paths, and can render OCR-routed pages to temporary PPM images for command or HTTP adapters; it does not provide built-in OCR. Bundled OCR engines are intentionally outside the default path; full table reconstruction, richer geometry-aware layout recovery, and MuPDF comparison are later milestones. Use `backend-check` to inspect the selected backend and the pending MuPDF adapter candidate.

## Commands

```sh
cargo run -p glyphrush-cli -- eval test/corpus.datasheets.json --category datasheet --jobs 2
bash scripts/verify.sh
scripts/bench-liteparse.sh --dry-run
```

`scripts/verify.sh` is the shared local/GitHub CI gate. It runs formatting, Python and Node wrapper tests, the full Rust workspace test suite, clippy with warnings denied, strict `glyphrush-v0` baseline-preset metadata preflight, and the datasheet eval gate when ignored local PDFs exist under `test/`. In a fresh GitHub checkout those PDFs are absent by design, so CI skips only that local corpus gate rather than failing on non-committed benchmark files.

```sh
cargo run -p glyphrush-cli -- inspect test/example.pdf
cargo run -p glyphrush-cli -- inspect test/example.pdf --pages
cargo run -p glyphrush-cli -- inspect test/example.pdf --pages --jobs 4
cargo run -p glyphrush-cli -- inspect test/example.pdf --pages --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- backend-check
cargo run -p glyphrush-cli -- feature-parity
cargo run -p glyphrush-cli -- feature-parity --bench-report .glyphrush-baselines/reports/liteparse-speed-gate.json --require-speed-evidence
cargo run -p glyphrush-cli -- backend-check --pdf test/example.pdf
cargo run -p glyphrush-cli -- backend-check --pdf test/
cargo run -p glyphrush-cli -- backend-check --pdf test/ --jobs 4
cargo run -p glyphrush-cli -- --backend lopdf inspect test/example.pdf
cargo run -p glyphrush-cli -- --backend lopdf backend-check
cargo run -p glyphrush-cli --features pdfium -- --backend auto bench test/ --eval-manifest test/corpus.datasheets.json --baseline-preset glyphrush-v0 --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5
GLYPHRUSH_BENCH_OUTPUT=.glyphrush-baselines/reports/liteparse-speed-gate.json scripts/bench-liteparse.sh
cargo run -p glyphrush-cli --features pdfium -- --backend pdfium backend-check
cargo run -p glyphrush-cli --features pdfium -- --backend pdfium backend-check --pdf test/
cargo run -p glyphrush-cli --features pdfium -- --backend pdfium bench test/ --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- parse test/example.pdf --format json
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --jobs 4
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --span-geometry
cargo run -p glyphrush-cli -- parse test/example.pdf --format text
cargo run -p glyphrush-cli -- parse test/example.pdf --format markdown
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --ocr-command tools/ocr/my-ocr.sh --ocr-timeout-ms 120000
cargo run -p glyphrush-cli --features pdfium -- --backend pdfium parse test/example.pdf --format json --ocr-command tools/ocr/tesseract-rendered-image.sh --ocr-command-input rendered-image
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --ocr-http-url http://127.0.0.1:8080/ocr
cargo run -p glyphrush-cli -- ocr-check test/example.pdf --page-index 0 --ocr-command tools/ocr/my-ocr.sh --strict
cargo run -p glyphrush-cli --features pdfium -- --backend pdfium ocr-check test/example.pdf --page-index 0 --ocr-command tools/ocr/tesseract-rendered-image.sh --ocr-command-input rendered-image --strict
cargo run -p glyphrush-cli -- ocr-check test/example.pdf --page-index 0 --ocr-sidecar test/ocr --strict
cargo run -p glyphrush-cli -- ocr-check test/example.pdf --page-index 0 --ocr-http-url http://127.0.0.1:8080/ocr --strict
cargo run -p glyphrush-cli -- parse test/example.pdf --format json --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- bench test/example.pdf
cargo run -p glyphrush-cli -- bench test/example.pdf --jobs 4
cargo run -p glyphrush-cli -- bench test/example.pdf --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- bench test/example.pdf --ocr-command tools/ocr/my-ocr.sh
cargo run -p glyphrush-cli -- bench test/example.pdf --ocr-http-url http://127.0.0.1:8080/ocr
cargo run -p glyphrush-cli -- bench test/example.pdf --eval-manifest test/corpus.json
cargo run -p glyphrush-cli -- bench test/example.pdf --eval-manifest test/corpus.json --eval-category datasheet
cargo run -p glyphrush-cli -- bench test/example.pdf --require-quality --eval-manifest test/corpus.json
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/example.pdf --require-baselines --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/example.pdf --require-baseline-quality --eval-manifest test/corpus.json --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline-preset glyphrush-v0 --require-speedup liteparse=2.0
cargo run -p glyphrush-cli -- bench test/example.pdf --eval-manifest test/corpus.json --baseline-preset glyphrush-v0 --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline liteparse=tools/baselines/liteparse-text.sh
cargo run -p glyphrush-cli -- bench test/example.pdf --baseline liteparse-no-ocr=tools/baselines/liteparse-no-ocr-text.sh
cargo run -p glyphrush-cli -- baseline-check --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- baseline-check --pdf test/ --baseline-preset glyphrush-v0
scripts/setup-baselines.sh
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
cargo run -p glyphrush-cli -- bench test/ --baseline-preset glyphrush-v0 --require-speedup liteparse=2.0
cargo run -p glyphrush-cli -- bench test/ --eval-manifest test/corpus.json --baseline-preset glyphrush-v0 --require-speedup-claim liteparse=2.0 --require-speedup-claim liteparse-no-ocr=1.5
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

## Thin Wrappers

The Python and Node packages are intentionally thin: they do not parse PDFs themselves, and they should not grow behavior that diverges from the native core. Install or point them at a built `glyphrush` binary, then call the same CLI artifact path:

```python
import glyphrush

artifact = glyphrush.parse(
    "test/example.pdf",
    binary="target/debug/glyphrush",
    backend="lopdf",
)
text = glyphrush.parse_text("test/example.pdf", binary="target/debug/glyphrush")
markdown = glyphrush.parse_markdown("test/example.pdf", binary="target/debug/glyphrush")
triage = glyphrush.inspect_pages("test/example.pdf", binary="target/debug/glyphrush")
quality = glyphrush.eval_manifest("test/corpus.json", binary="target/debug/glyphrush")
speed = glyphrush.bench("test/example.pdf", binary="target/debug/glyphrush")
generated = glyphrush.manifest("test/", binary="target/debug/glyphrush", category="datasheet")
```

```js
import { bench, evalManifest, inspectPages, manifest, parse, parseMarkdown, parseText } from "./bindings/node/src/index.mjs";

const artifact = parse("test/example.pdf", { binary: "target/debug/glyphrush" });
const text = parseText("test/example.pdf", { binary: "target/debug/glyphrush" });
const markdown = parseMarkdown("test/example.pdf", { binary: "target/debug/glyphrush" });
const triage = inspectPages("test/example.pdf", { binary: "target/debug/glyphrush" });
const quality = evalManifest("test/corpus.json", { binary: "target/debug/glyphrush" });
const speed = bench("test/example.pdf", { binary: "target/debug/glyphrush" });
const generated = manifest("test/", { binary: "target/debug/glyphrush", category: "datasheet" });
```

Set `GLYPHRUSH_BIN=/path/to/glyphrush` to avoid passing a binary on each call. Use `python3 -m unittest discover -s bindings/python/tests` and `node --test bindings/node/test/client.test.mjs` to run the wrapper tests.

The global `--backend` option defaults to `auto`, which resolves to the fastest enabled backend for the current binary. Plain builds resolve `auto` to `lopdf`; builds compiled with `--features pdfium` resolve `auto` to the PDFium adapter, so the faster native backend is the default when it is available. You can still select `--backend lopdf` explicitly for dependency-light runs, and `--backend pdfium` explicitly in PDFium builds. The optional adapter uses `pdfium-auto`, which may download and cache a matching PDFium runtime the first time the PDFium backend is actually used. `backend-check` emits `glyphrush-backend-check-report-v1` with parser version, selected backend, enabled backend count, candidate backend count, and per-backend capability/limitation metadata. `feature-parity` emits `glyphrush-feature-parity-report-v1`, a conservative LiteParse comparison matrix that separates implemented, partial, planned, and intentionally-not-planned capabilities; its `readiness` block distinguishes native-text speed-race readiness from full LiteParse drop-in parity, and keeps built-in OCR, bindings, layout/table maturity, and the quality-backed speedup gate visible instead of relying on README prose. When `feature-parity --bench-report <saved-bench-json>` is used, `benchmark_evidence.quality_categories` also lists the eval categories, document counts, page counts, failed checks, and pass state behind the speed evidence so a faster-than-LiteParse claim is tied to visible corpus coverage. Add `--require-coverage-preset glyphrush-v0` to fail the parity command unless that saved benchmark covers the core v0 classes: `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`. Add `backend-check --pdf <file-or-directory>` to smoke the selected backend against one PDF or every top-level PDF in a directory and report open/extract success, page counts, native text bytes, image artifact count, OCR-required pages, source size/fingerprints for files, wall time, stable failure `error_kind` values such as `encrypted_pdf_requires_password`, sorted per-document results for directories, and bounded directory `failure_samples` so a mixed corpus failure is visible without scanning every document. Use `--jobs <N>` with directory smoke tests to run PDFs concurrently while merging results back into stable filename order and reporting the effective worker count. The PDFium adapter reuses the document opened for page counting during extraction, but currently caps corpus-level document workers to one so live PDFium document handles are not extracted concurrently in one process; reports expose this as `worker_count: 1`. Default builds report `lopdf` as enabled and PDFium/MuPDF as `not_wired`; PDFium-feature builds report `lopdf` and `pdfium` as enabled while MuPDF remains `not_wired`. PDFium-feature reports set `render_pages: true` because the adapter can render page images for OCR handoff, while `builtin_ocr` remains false.

`feature-parity` statuses are selected-backend aware. PDFium-feature runs report rendered-page OCR handoff as implemented because PDFium can render only OCR-routed pages to temporary images for command or HTTP adapters; non-rendering backends keep that LiteParse capability partial and explicit.

When comparing backends, prefer labeled text/table recall and silent-failure checks over route-count parity alone. Manifests generated from one backend can encode that backend's fallback behavior; for example, a page that `lopdf` marked `requires_ocr` because it extracted no native text may not require OCR under PDFium if PDFium extracts usable native text. Use per-document `expect_by_backend` overrides for backend-specific diagnostics such as route counts, OCR-required page counts, image artifact counts, or page flags while keeping shared labeled quality checks in `expect`. Eval and embedded bench-quality reports shallow-merge `expect_by_backend.<backend>` over `expect` for the artifact backend being scored; baseline text quality checks use only the shared `expect` fields.

Local PDFs can be dropped into `test/`. PDF files in that directory are ignored by git so benchmark corpora do not get committed accidentally. File and directory `inspect` outputs include parser/backend/source metadata. `inspect --pages <pdf-or-directory>` runs the normal extraction/classifier path and emits compact page triage summaries: page artifact IDs, page fingerprints, dimensions, per-stage timings, route, quality flags, route reasons, native/OCR span counts, native text bytes, image artifact counts, bounded layout summaries, recovered table row/cell counts, and page warnings. Use `inspect --pages --jobs <N>` to triage a single PDF with page workers or a directory with per-PDF workers; results are merged back into stable page or filename order and the effective worker count is reported as `worker_count`. Use `inspect --pages --cache-dir <dir>` for warm repeated triage; single-file output reports `cache_status`/`cache_key`, while directory output also reports aggregate `cache_hits` and `cache_misses` plus per-document cache status. Directory `inspect --pages`, `inspect`, and `bench` commands discover top-level `.pdf`/`.PDF` files in stable filename order and emit corpus-level aggregate JSON with a `corpus_fingerprint` over the ordered document labels, document fingerprints, and page counts.

`parse --jobs <N>` opts into page extraction workers for one PDF and reports the effective count as `global_diagnostics.worker_count`. Worker results are merged back by page number before building JSON, text, markdown, cache artifacts, or benchmark quality reports.

Bench output includes top-level `report_version` for the benchmark JSON envelope schema, `quality_status` for whether the run was eval-scored, `run_metadata` for parser/backend provenance, `run_configuration` for output-affecting options, `requirements` for strict gate switches requested by the command, `speedup_claims` for explicit faster-than-baseline verdicts requested with `--require-speedup` or `--require-speedup-claim`, `requested_baseline_presets` for any preset comparison set expanded into wrapper runs, `worker_count`, wall time, pages/sec, artifact size, parser-run allocation bytes, derived text output size/counts, peak RSS, parser stage timings, p50/p95 page latency, per-route latency, route counts, route-reason counts, image artifact counts, fallback counts, fallback action counts, OCR-required/OCR-applied counts, per-flag quality counts, and artifact warnings. Corpus reports include the same report version, quality status, run metadata/configuration, requirements, speedup claims, aggregate warning counts, bounded warning samples with document paths, per-document warning arrays, image artifact totals, and empty-text output counts. When a directory benchmark uses an eval manifest with document categories, top-level `category_summaries` reports Glyphrush benchmark timing, pages/sec, fallback/OCR counts, route counts, quality-flag counts, warnings, and eval failed checks by category. Single-PDF benchmarks accept `--jobs <N>` for opt-in parallel page extraction; directory benchmarks use `--jobs <N>` for opt-in parallel per-PDF runs. Both paths merge results back into stable page or filename order so artifacts, document arrays, and corpus fingerprints remain deterministic. `bench --eval-manifest <manifest.json>` embeds the full eval result under `quality`, sets `quality_status` to `checked`, includes the manifest path, SHA-256, and bounded failure samples, scores the exact artifacts that were benchmarked, and exits nonzero after writing JSON when quality gates fail. If the eval manifest includes `silent_failures` checks, bench reports also include top-level `silent_failure_count` and path-qualified `silent_failure_pages`; those fields are absent when the gate was not requested, which means "not checked", not "zero". Without `--eval-manifest`, `quality_status` is `not_checked_no_eval_manifest` so speed-only reports cannot be mistaken for quality-backed reports. Add `--require-quality` when benchmark jobs must fail instead of accepting a speed-only report; Glyphrush still writes the JSON report, then exits nonzero if no eval manifest quality report was checked. Use `bench --eval-category <name>` with an eval manifest to restrict the embedded quality report and benchmark category summaries to one normalized coverage class; if the category selects no documents, Glyphrush still writes the bench JSON with a `quality.document_count` failure before exiting nonzero. When baselines are present, manifest document-level `required_text`, page-level `required_text`, `text_recall`, `reading_order`, and `table_structure` expectations are also scored against baseline stdout under `baselines[].quality`, and each baseline result exposes optional top-level `target` plus `quality_status` so speed-only comparisons are explicit as `not_checked_no_expectations` instead of looking quality-backed. Page-level required-text anchors are checked as full-document stdout anchors for text-only baselines; page locality remains enforced by Glyphrush artifact eval. Corpus baseline summaries also expose optional top-level `target` from baseline description metadata, `quality_status` (`checked`, `partially_checked`, `not_checked_no_expectations`, or `not_checked_baseline_failures`), `quality_documents`, and `quality_unchecked_documents` alongside quality pass counts/rate, per-check failure counters, `quality_category_summaries` keyed by manifest category, and bounded `quality_failure_samples` naming the failed documents and check types. Category-filtered benchmark runs apply the same manifest category filter before scoring baseline quality. `bench --cache-probe --cache-dir <dir>` runs a forced cold cache miss followed by a warm cache hit in one command; corpus probe summaries include aggregate cold/warm stage timings, allocation counters, and fallback action counters so warm-cache reports can prove parser stages did not rerun. `bench --baseline NAME=EXECUTABLE` runs an external parser wrapper for each PDF and records optional `--describe` metadata, describe probe status under `description_status`, wall time, exit status, timeout status, stdout bytes, stdout SHA-256, stdout line/word counts, stderr bytes, empty-output status, bounded stderr previews, stable failure `error_kind` values, and a `comparison` object with Glyphrush-vs-baseline speed/output ratios. Use `bench --baseline-preset glyphrush-v0` for the core comparison set: LiteParse with its default OCR behavior, LiteParse with OCR disabled, PyMuPDF, and pdfplumber; saved reports then include `"requested_baseline_presets": ["glyphrush-v0"]`. Add `--require-baselines` when benchmark jobs must fail if no baseline was requested or any requested baseline execution failed or timed out; add `--require-baseline-quality` when they must also fail if baseline text quality was not checked or did not pass. Add `--require-speedup NAME=RATIO` to fail after writing JSON unless `baselines[].comparison.glyphrush_speedup` or the corpus baseline summary speedup for `NAME` is at least `RATIO`; for example, `--require-speedup liteparse=2.0` makes the faster-than-LiteParse speed gate executable. Add `--require-speedup-claim NAME=RATIO` when the job should fail unless the same faster-than-baseline threshold is met and both Glyphrush and the baseline have passing quality checks from the eval manifest. Each matching `speedup_claims[]` entry reports the required and actual speedup, speed comparability, whether the speed threshold passed, whether Glyphrush and the baseline were quality-checked and passed, and a conservative `claim_passed` verdict that is true only when speed and quality both support the claim. Glyphrush still writes the JSON report first so failure details remain inspectable. `--baseline-timeout-ms <ms>` bounds each baseline and its `--describe` probe; timed-out baselines stay in the report as failed, non-comparable runs with `error_kind: "timeout"` and `quality_status: "not_checked_timed_out"` when quality expectations existed. Corpus baseline summaries also report comparison metadata when available, description probe status, success rate, successful/failed/timed-out pages, empty-output pages, and failure samples with stable failure kinds, so a fast partial failure, timeout, empty extraction, metadata failure, or quality miss is visible. The wrapper receives the PDF path as its only argument. Marker/Docling remain manually addable as heavier quality-context baselines, while speed comparisons stay in the same report as Glyphrush image, fallback, and OCR-required counters.

`baseline-check --baseline NAME=EXECUTABLE` validates wrapper metadata without parsing a PDF. It emits top-level `report_version`, parser/backend `run_metadata`, `requested_baseline_presets`, runs each wrapper's `--describe` mode, reports valid JSON metadata, timeouts, missing executables, stderr previews, stable describe `error_kind` values such as `missing_dependency`, and aggregate `describe_success_count`/`all_described` fields. `baseline-check --baseline-preset glyphrush-v0` expands to the four core baselines: LiteParse, LiteParse no-OCR, PyMuPDF, and pdfplumber, and records `["glyphrush-v0"]` in `requested_baseline_presets`. At least one explicit baseline or preset baseline is required; an empty preflight still writes JSON but exits nonzero with `all_described: false` so setup scripts cannot treat a missing LiteParse/PyMuPDF/pdfplumber comparison as passing. Add `--pdf <file>` to run each wrapper against one smoke PDF and report output size, stdout digest, line/word counts, stderr previews, stable smoke `error_kind` values such as `missing_dependency`, and `smoke_success_count`/`all_smoke_passed`. Add `--pdf <directory>` to smoke every top-level `.pdf`/`.PDF` in stable filename order; the report includes `smoke_document_count`, aggregate per-baseline document pass/fail counts, bounded `smoke.failure_samples`, and per-document smoke entries so missing parser dependencies or file-specific failures are visible before a long corpus benchmark. Add `--strict` when this should behave like a setup gate: Glyphrush still writes the JSON report, then exits nonzero if any describe probe failed or if a smoke PDF/directory was supplied and any smoke probe failed. Use it before long `bench --baseline` corpus runs to catch missing wrappers, broken comparison metadata, or missing parser dependencies quickly.

OCR remains optional. For testable OCR plumbing without installing a heavy engine, `parse --ocr-sidecar <dir>`, `bench --ocr-sidecar <dir>`, `debug-page --ocr-sidecar <dir>`, and `eval --ocr-sidecar <dir>` read page text from files named `<pdf-stem>.p000000.txt`, using zero-based page indexes. `--ocr-command <executable>` is an alternate adapter for the same commands; Glyphrush invokes it only for pages the classifier routes to OCR fallback. The default `--ocr-command-input pdf-page` contract passes the PDF path as argument 1 and the zero-based page index as argument 2, then treats stdout as OCR text. `--ocr-http-url <url>` adds the LiteParse-style HTTP adapter seam: Glyphrush POSTs JSON containing `pdf_path` and `page_index` only for OCR-routed pages, accepts plain response bodies as OCR text, and accepts `application/json` responses with a string `text` field. With `--backend pdfium --ocr-command-input rendered-image`, Glyphrush renders only OCR-routed pages to temporary PPM files, records `render_us`, and removes the temporary file after OCR returns; command OCR receives the rendered image path as argument 1 and page index as argument 2, while HTTP OCR receives JSON containing `rendered_image_path` and `page_index` instead of `pdf_path`. `tools/ocr/tesseract-rendered-image.sh` is a ready local wrapper for that rendered-image command contract; it invokes Tesseract explicitly and is never used unless passed through `--ocr-command`. Non-rendering backends reject `rendered-image` when a routed page would need it instead of silently falling back to the PDF-path contract. Use `ocr-check <pdf> --page-index <N> --ocr-command <executable> --strict`, `ocr-check <pdf> --page-index <N> --ocr-sidecar <dir> --strict`, or `ocr-check <pdf> --page-index <N> --ocr-http-url <url> --strict` before scanned/hybrid benchmarks to prove the configured adapter produces non-empty text; add `--backend pdfium --ocr-command-input rendered-image` to preflight the rendered-image command, Tesseract wrapper, or HTTP contract. `ocr-check` emits `glyphrush-ocr-check-report-v1` with adapter mode, output digest, line/word counts, render timing when applicable, stderr preview, timeout state, and stable failure kinds such as `timeout`, `empty_output`, `missing_dependency`, `spawn_failed`, `sidecar_read_failed`, `http_request_failed`, `http_status_failed`, `http_response_decode_failed`, and `render_backend_required`, then exits nonzero after writing JSON when strict mode fails or a non-rendering backend is asked to check the rendered-image contract. OCR commands and HTTP requests are bounded by `--ocr-timeout-ms`, defaulting to `120000`, so a broken adapter cannot hang parse or benchmark runs indefinitely. `--ocr-sidecar`, `--ocr-command`, and `--ocr-http-url` are mutually exclusive. High image coverage with missing, very sparse, or broken-encoding native text is treated as OCR-required instead of a successful native fast path. When OCR text is applied for an OCR-routed page, derived layout/text views use the OCR text while native spans remain in the JSON artifact for provenance. If OCR is required but no OCR text is applied, `global_diagnostics.warnings` includes a stable page warning such as `p000000: requires_ocr_without_ocr_output`; if an explicit capability request hits a v0 cap or page annotations/form fields/widget annotations are present but not extracted, warnings include values such as `p000000: unsupported_feature: span_geometry_capped` or `p000000: unsupported_feature: annotation_or_form`. `parse --format text` and `parse --format markdown` also write these warnings to stderr while keeping stdout as the derived text view.

Classifier decisions include deterministic `reasons` strings and the cheap `signals` that produced them in JSON artifacts and `debug-page` output. `debug-page` extracts only the requested page, can apply page-selective OCR through `--ocr-sidecar`, `--ocr-command`, or `--ocr-http-url`, reports `document_page_count`, `extracted_page_count`, selected-page `artifact_id`, `page_fingerprint`, dimensions, page `quality`, page warnings, per-stage `timings`, derived `text_output` metrics, bounded `layout` block counts plus table row/cell counts when tables are recovered, and any drawn image artifact metadata, and includes parser/backend/source metadata so classifier investigations can be tied to the exact parser build and backend adapter. Reasons explain fallback routing with values such as `high_image_coverage_with_sparse_native_text`, `image_text_overlay`, `broken_encoding`, `broken_encoding_with_image_coverage`, `bbox_overlap`, `table_line_density`, `annotation_or_form`, and `rotated_page`; `annotation_or_form` is raised when page annotations, widget annotations, or catalog AcroForm fields are present but not extracted.

Structured page artifacts include `image_artifacts` for drawn image XObjects, image-backed form XObjects, and detected inline images. Each entry stores a deterministic page-local image ID, source XObject name when available, drawn bbox, and approximate per-artifact page-area ratio. The route-driving `signals.image_area_ratio` is computed from the union of image artifact boxes, clipped to the page, so overlapping or repeated images do not overstate scan coverage. For form-wrapped images, Glyphrush follows nested form content and transforms so small logos inside page-sized forms do not look like full-page scans. Skipped or unsupported inline image operators are still surfaced with `source_name: "inline"` so image-backed pages do not disappear from diagnostics. Glyphrush does not include image bytes in the artifact; the metadata exists so image-backed content stays visible without adding render/copy cost to the fast path.

Artifact caching is optional. `--cache-dir <dir>` keys cached JSON snapshots by parser version, backend name/version, cache schema, PDF bytes, OCR sidecar text state for files matching the current PDF stem, OCR command path/content/input-mode/timeout, OCR HTTP URL/input-mode/timeout, and span-geometry mode. Cache-schema bumps intentionally invalidate warm artifacts when output-relevant diagnostics such as warnings, quality flags, confidence scores, artifact metadata shape changes, or cache snapshot shape changes. Cache files include an explicit snapshot envelope with schema/parser/backend/source provenance plus the cached artifact. Cache diagnostics are emitted as `cache_status` and `cache_key`; cache-hit artifacts reset page-stage timings to zero because the page extraction pipeline did not rerun, while source metadata such as `source_size_bytes` and `source_modified_unix_ms` is refreshed from the current file. If a matching snapshot is unreadable, corrupt, or fails envelope validation, Glyphrush treats it as a miss, emits a `cache_snapshot_ignored` warning, reparses, and rewrites the snapshot instead of failing solely because the cache is bad.

Layout reconstruction v0 is text-derived. Glyphrush splits native or OCR text into deterministic page-local blocks, reflows common short extraction fragments, and classifies simple headings, lists, tables, and paragraphs. When opt-in span geometry produces usable boxes, the layout path can preserve full-width title/banner bands, leading, middle, and trailing cross-column bands, conservative short section-heading separators, and clearly separated 2-4 column reading order before falling back to vertical-gap grouping. Repeated positioned blocks in the top or bottom page margin are classified as `header` or `footer` when the same normalized text appears on multiple pages. The page classifier also treats dense stroked horizontal/vertical ruling lines as `table_uncertain` so ruled tables are escalated even when extracted text has no pipe/tab delimiters. When that table route is active, simple consistent whitespace rows are preserved and exposed as table blocks for quality gates; fixed-width whitespace rows can also preserve blank cells by aligning row segments to header column starts, merge lowercase wrapped descriptor fragments into the following row, and keep section rows inside the grid; header-guided whitespace rows can merge same-line or wrapped leading multi-word descriptor cells against short header columns, merge lowercase trailing descriptor continuations into the preceding row, keep inferred trailing blank cells explicit, and keep title-case section rows inside the grid; if positioned spans form aligned multi-column rows, Glyphrush groups them as a table before applying the geometry reading-order heuristic, keeps omitted cells explicit when rows skip blank columns, merges same-line fragmented positioned cells plus near-adjacent single- or multi-cell wrapped positioned continuations into the same structured row, keeps cross-column, first-column, or fragmented first-column interior section rows inside the table grid with blank cells for the remaining columns, and leaves top/bottom captions as surrounding text blocks. Table blocks include structured `table.rows[].cells[]` in JSON artifacts; pipe/tab, fixed-width whitespace, and positioned-row tables preserve empty cells, positioned cells with source spans include bounding boxes when span geometry is available, and text-only or omitted cells omit cell boxes. Plain text, markdown, and eval quality text prefer layout-block order when blocks are available while raw `native_spans` remain preserved in JSON artifacts. Markdown export normalizes structured, pipe-delimited, or whitespace-delimited table blocks into markdown tables with a generated separator row; other table blocks remain plain text.

Native span geometry is currently conservative and opt-in. Add `--span-geometry` to `parse`, `bench`, `debug-page`, or `eval` when you want bounded/simple `lopdf` text-positioning streams or PDFium text segments to produce multiple boxed spans, plus a measured `bbox_overlap_ratio` layout-risk signal. The `lopdf` path approximates boxes with simple text-matrix and content-matrix transforms plus text-state parameters preserved across text objects, `TJ`/`Tc`/`Tw`/`Tz` spacing, `Ts` text rise, `TL` leading, and `'`/`"` text-showing shortcut adjustments applied. The PDFium path uses PDFium's merged text-segment rectangles and converts them into Glyphrush page-local coordinates. The default hot path emits page-wide native spans. Large streams, large native text, or unsupported geometry such as rotation fall back to the page-wide native span and are flagged and warned with `unsupported_feature` plus `span_geometry_capped` instead of silently pretending requested bbox detail was produced. Decoded spans that do not match the backend's native text output still fall back to the page-wide native span instead of taxing the hot path or risking bad geometry.

The Python and Node wrappers expose `parse`, text output, `inspect --pages` triage, `debug-page` diagnostics, `ocr-check`, `backend-check`, `baseline-check`, `feature-parity`, `eval` quality reports, `bench` speed reports, and `manifest` corpus generation while remaining dependency-free shims over the native binary. They do not implement independent PDF parsing, OCR, layout, quality scoring, benchmarking, manifest generation, or cache logic; wrapper calls pass backend, cache, jobs, span-geometry, category/coverage, baseline, speed-claim, and OCR adapter options through to the CLI and decode the same JSON artifacts.

`eval <manifest.json>` turns local PDFs into repeatable gates. Manifest paths are resolved relative to the manifest file and can assert document-level counts, required text substrings against the derived layout-aware eval text, and page-level artifact ID, page fingerprint, route, required text, flag, reason, and layout-block-count expectations. Empty manifests and category filters that select no documents are not accepted as quality passes: `eval` emits a `document_count` failure sample and exits nonzero when no documents are selected. Add optional root-level document `category` values such as `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, or `large` to track benchmark corpus coverage; eval reports include top-level `category_counts`, `category_summaries`, and per-document categories, with missing or blank categories counted as `uncategorized`. Add manifest-level `required_categories` when a corpus must cover specific benchmark classes before its speed/quality results are accepted, and `min_category_counts` when a class needs more than one fixture; missing or under-counted categories produce a top-level `category_coverage` failure. Use `eval --category <name>` to run gates for one normalized category, including `uncategorized`, when a mixed manifest needs class-specific quality checks. `category_summaries` reports document/page counts, passed/failed document counts, failed checks, and category-level quality pass/fail state so benchmark regressions can be tied to a corpus class. Use `eval --jobs <N>` to evaluate manifest documents concurrently while preserving manifest document order in the report and emitting the effective `worker_count`. Use `eval --cache-dir <dir>` for repeated quality gates; top-level `report_version`, `run_metadata`, `run_configuration`, `corpus_fingerprint`, `cache_hits`, and `cache_misses` summarize the eval report schema, parser/backend provenance, output-affecting options, evaluated source set, and warm/cold artifact reuse while each document still reports `artifact_cache_status`. `run_configuration` records `span_geometry`, OCR adapter mode booleans, `ocr_command_input`, and `ocr_timeout_ms`. Eval reports also include aggregate and per-document diagnostics for page count, fallback/OCR counts, image artifact counts, empty-text pages, route counts, route-reason counts, quality-flag counts, layout block counts when asserted, fallback-action counts, and warning counts, even when the manifest does not explicitly assert those checks.

`manifest <pdf-or-directory>` emits an eval-compatible JSON skeleton for the current parser output. It is useful after dropping new PDFs into `test/`: save the output next to the PDFs, run `eval` to confirm the structural gates, then review any generated `table_structure` and, when `--span-geometry` is enabled, bounded `span_bbox` checks, and tighten the manifest with human/labeled document-level `required_text`, `text_recall`, `reading_order`, `table_structure`, or additional `span_bbox` expectations. Eval `required_text` uses layout-block order and serializes structured table grids as pipe-delimited rows, preserving blank cells such as `| A |  | missing value |` for table-quality anchors. Use `manifest --category <name>` to stamp every generated document with a benchmark coverage class such as `datasheet`, `clean_digital`, or `scanned`; downstream eval and benchmark reports then populate category counts and category summaries without hand-editing the generated JSON. Use `manifest --coverage-preset glyphrush-v0` to add the core v0 corpus coverage gate for `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`, with each category required at least once. Use repeated `manifest --required-category <name>` to generate the top-level presence gate when a corpus must include specific classes before speed/quality claims are accepted, and repeated `manifest --min-category-count <name=count>` to require enough documents in a class before accepting a corpus benchmark. Use `manifest --jobs <N>` to generate directory manifests with parallel per-PDF workers or single-PDF manifests with page workers; results are merged back into deterministic filename/page order and the generated JSON does not include worker-count provenance, so corpus fingerprints and expectations remain stable across worker counts. Use `manifest --cache-dir <dir>` to reuse parsed artifacts across repeated manifest generation; cache status is intentionally not serialized into the manifest so cold and warm generated JSON remain comparable. The generated skeleton includes deterministic generator provenance, a corpus fingerprint, optional `required_categories`, optional `min_category_counts`, optional per-document category, per-document source fingerprints, sizes, and modified times checked by `eval`, page counts, route counts, quality-flag counts, OCR-needed classification, non-empty quality-flag classification gates, warning counts, exact required warnings, image artifact counts, recovered table-structure checks for pages with table blocks, bounded span-bbox checks for non-page-wide spans when span geometry is requested, `silent_failures: 0`, and per-page artifact ID, page fingerprint, route, bounded page-local required-text anchor, layout-block, image, flag, and reason acknowledgements so current fallback decisions and diagnostics are explicit instead of implicit.

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
