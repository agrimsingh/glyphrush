# Glyphrush Remaining Work

This is the parity-driven TODO list for the current v0 branch. It is grounded in the `feature-parity` gate, the local v0 corpus seed, and the original LiteParse goal: build a faster parser for the native-text hot path without pretending that OCR, layout, or table failures are solved when they are only flagged.

Current checkpoint:

- Branch state before this guide: `codex/manifest-quality-gates` was clean, pushed, and aligned with `origin/codex/manifest-quality-gates`.
- Current feature-parity summary: 13 target capabilities, 8 implemented, 2 partial, 2 planned, and 1 intentionally not planned.
- Native-text speed-race readiness: ready.
- Narrow native-text speed-advantage signal: ready.
- Strict quality-backed "faster than LiteParse" claim: not ready because the saved LiteParse report is blocked by `missing_quality_backed_liteparse_claims`.
- Full LiteParse drop-in parity: not ready.
- Product parity: not ready.

## Non-Negotiable Rules

- Do not optimize speed by silently dropping text, pages, images, OCR needs, layout uncertainty, or table uncertainty.
- Do not globally skip OCR. Keep OCR out of the default hot path, but route or flag pages that need it.
- Do not merge a speed claim without quality evidence from the same corpus/report.
- Keep artifacts deterministic: same input and options should produce stable page order, span order, flags, timing fields, and artifact IDs.
- Keep Python, Node, and future WASM bindings thin over the native core.

## P0: Next Engineering Work

### 1. Close `span_geometry_layout` from partial to implemented — DONE

Status: implemented. The column-row band strategy keeps centered banners, gutter-straddling rows, and trailing page numbers out of column splits, so multi-column academic pages (BERT fixture) read title → abstract → left column → right column → page number instead of interleaving columns line-by-line. Pages with unresolved multi-column evidence are flagged `layout_uncertain` with a `column_layout_unresolved` reason, each page artifact reports its reading-order `layout_strategy`, and `test/corpus.v0.layout.json` is the labeled span-geometry gate (reading-order sequences, span-bbox samples, per-page block counts) wired into `scripts/verify.sh`.

Remaining follow-ups (not parity blockers):

- Add a strict nonzero-`/Rotate` fixture; the current `rotated` fixture is landscape-orientation only, and rotated pages are flagged via `rotated_page` rather than re-ordered.
- Add dedicated sidebar/footnote-heavy fixtures beyond the BERT and Watson coverage.
- Consider estimating asymmetric gutters instead of assuming the column gutter brackets the page center.

### 2. Close `table_recovery` from partial to implemented

Why it matters: current table support is useful but conservative. LiteParse-style parity requires stronger behavior across invoices, forms, budget tables, datasheets, and academic tables, not just synthetic or narrow table patterns.

Concrete tasks:

- Add labeled table fixtures for invoices, receipts, forms, academic result tables, budget tables, datasheets, and simple ruled tables.
- Add table structure checks for rows, columns, blank cells, wrapped cells, section rows, captions, footers, repeated headers, and merged-looking header groups.
- Use vector line/ruling metadata where available instead of relying only on text geometry.
- Improve multi-page table continuation handling, repeated headers, and caption/prose separation.
- Keep false positives low by requiring table-header or geometry evidence before routing prose into table recovery.
- Preserve uncertainty flags for partial or ambiguous table structures instead of overclaiming clean recovery.

Exit criteria:

- Labeled table fixtures pass across at least datasheet, invoice/form, budget, and academic categories.
- Table recovery improves structured output without harming non-table pages or plain text order.

### 3. Refresh LiteParse benchmark evidence — DONE (narrow claim)

Status: the native-text v0 benchmark was re-run against the committed corpus and the narrow claim gate passes: `feature-parity --bench-report .glyphrush-baselines/reports/liteparse-v0-native-text-gate.json --require-speed-advantage --require-coverage-preset glyphrush-v0-native-text` exits zero with `native_text_speed_advantage_ready: true`. Headline numbers and the exact command, manifest SHA-256, PDF root, coverage preset, timing, and quality status are recorded in `docs/benchmarking.md` under "Saved v0 native-text evidence": Glyphrush 1.88 s / 491 pages/sec over 924 pages with passing quality, 77.4x vs LiteParse default and 1.90x vs LiteParse no-OCR.

The stricter both-pass claim remains intentionally blocked by `missing_quality_backed_liteparse_claims`. Triage shows LiteParse's `required_text` failures are largely caused by backend-flavored generated anchors (PDFium spacing quirks such as `Helloworld`, `\u0002` hyphenation markers) and stdout-format table expectations, not proven LiteParse content loss.

Remaining follow-up to unlock the strict claim:

- Make generated required-text anchors backend-neutral (skip or normalize spacing/control-character artifacts) or move backend-flavored anchors into `expect_by_backend.pdfium`, then re-run the gate with `--require-speed-evidence`.

### 4. Harden OCR fallback on scanned and hybrid documents

Why it matters: being faster than LiteParse on native PDFs is not enough if scanned or hybrid PDFs are misreported as successful native extraction.

Concrete tasks:

- Add OCR-needed precision/recall labels for scanned and hybrid v0 documents.
- Validate sidecar, command, HTTP, and rendered-image OCR handoff paths against at least one scanned and one hybrid fixture.
- Add cold-start, render, OCR, and merge timing counters to benchmark summaries.
- Add memory and queue-bound checks for large scanned PDFs.
- Verify no-OCR runs clearly flag `requires_ocr` instead of returning incomplete text as complete output.

Exit criteria:

- OCR-backed output is produced when an adapter is configured.
- Without OCR, required pages are flagged and downstream consumers can detect incomplete extraction.
- Clean native PDFs still avoid OCR entirely.

## P1: Product Readiness

- Cache and snapshot robustness: move beyond the current JSON-friendly cache toward a compact or mmap-friendly artifact format if warm-run performance becomes a bottleneck.
- Parallelism and memory: profile PDFium constraints, keep deterministic page merges, and bound image/render/span caches on large PDFs.
- Packaging: define macOS and Linux CLI builds, PDFium runtime behavior, version metadata, and install-size limits.
- Python package: keep it thin over the native CLI or stable core ABI, with artifact parity tests against the CLI.
- Node package: keep it thin over the same artifact model after the CLI/core API stabilizes.
- MuPDF spike: compare text span quality, license implications, packaging, rendering, and thread safety against PDFium before wiring it as a real backend.
- Debug overlays: add HTML or image overlays for bbox, reading-order, table-grid, and OCR-merge diagnostics.

## P2: Later Work

- WASM wrapper over the same core artifact model.
- HTTP/server wrapper for batch or agent workflows.
- Richer forms and annotations extraction.
- Figure/image extraction with captions and provenance.
- Advanced table semantics for merged cells, header groups, row groups, and multi-page continuation.
- Additional backend adapters only when they improve measured quality, speed, packaging, or reliability.

## Explicitly Not Planned

- Bundled built-in OCR in the default parser path.
- Hidden network OCR calls.
- Independent Python, Node, or WASM parser implementations that can diverge from the native core.
- Speed benchmarks that do not also record quality, coverage, and fallback behavior.

## Operating Checklist For Each Future Slice

1. Pick one real parity gap or one real PDF failure.
2. Add or tighten the smallest failing fixture, manifest expectation, or unit test.
3. Implement the smallest core change that fixes that failure.
4. Update `feature-parity`, docs, or benchmark metadata if the exposed behavior changes.
5. Run the focused test first.
6. Run the v0 corpus eval when the change affects extraction, layout, OCR routing, or tables.
7. Run the feature-parity gate with the relevant saved LiteParse report.
8. Run `GLYPHRUSH_VERIFY_PDFIUM=1 CARGO_INCREMENTAL=0 bash scripts/verify.sh` before pushing implementation work.
9. Commit, push, and verify GitHub CI for the branch.

Suggested next slice: tackle `span_geometry_layout` before new bindings. It is a current partial parity blocker and affects the core promise more directly than packaging or WASM.
