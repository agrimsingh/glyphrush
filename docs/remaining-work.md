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

### 2. Close `table_recovery` from partial to implemented — DONE

Status: implemented. Side-by-side per-column tables on two-column academic pages now split into one table per body column instead of mashing into one grid, and the prose-window filter exempts rows ending in standalone numeric fragments so in-column result tables survive. All four exit-criteria categories pass with hand-checked labels: datasheet (synthetic positioned/text fixtures), invoice/form (SF-1035 voucher, 18 rows), budget (OMB), and academic (BERT SWAG result table, values verified against the paper). Two-level header groups, merged cells, and cross-page continuation stitching remain conservative under `table_uncertain` flags and are tracked in P2 advanced table semantics.

Earlier progress notes for this item follow.

#### Earlier pass notes

Progress in this pass (false-positive hardening):

- Positioned table recovery now rejects candidate windows that are really the page's own two-column prose lines (`positioned_window_is_page_column_prose` plus a parallel-prose row guard), so figure-ruling-routed academic pages no longer mangle body prose into fake tables. On the BERT fixture this removed most fake parallel-prose tables while real academic result tables (the SQuAD leaderboard grids) are now recovered.
- Leftover non-table rows on table-routed two-column pages are split by the page's inferred body columns (`split_spans_by_known_columns`), so short prose segments between recovered tables keep column reading order.
- Body-column inference requires prose-line medians per column so clusters of short table cells are never mistaken for text columns (protects datasheet grids from the new rejection).
- Regression-tested by `table_routed_two_column_prose_is_not_recovered_as_fake_tables` and the existing positioned-table fixtures.

Progress in this pass (ruled-grid recovery from vector metadata):

- PDFium extraction now exposes positioned ruling lines (`ExtractedRulingLine`, page-local top-left coordinates, composed through nested form XObject transforms; the untransformed-coordinate bug was caught against the real SF-1035 voucher and fixed).
- New column-ruled grid recovery: vertical ruling clusters define column boundaries, text rows define row structure, blank cells are preserved, wrapped descriptor lines merge, and diagram lattices (architecture figures with token-cloud cells) are rejected by cell-density checks.
- New invoice-class fixture: `test/v0/forms/gsa-sf1035-filled-voucher.pdf`, generated reproducibly by `tools/baselines/make_invoice_fixture.py` from the public-domain GSA SF-1035 (real invoices are rarely redistributable). Hand-labeled 18-row `table_structure` (quantities, unit prices, units, amounts in distinct ruled columns) passes in `test/corpus.v0.layout.json`.
- The IRS f1095-C "Covered Individuals" ruled month-grid now produces structured cells (Jan..Dec as columns).

Labeled-fixture scorecard against the exit criteria (datasheet, invoice/form, budget, academic):

- datasheet: passing (extensive synthetic positioned/text fixtures).
- invoice/form: passing (SF-1035 voucher hand label; f1095-C month grid).
- budget: passing (OMB hand label).
- academic: NOT passing. BERT's side-by-side SQuAD leaderboard tables are recovered as one mashed grid instead of two adjacent tables, so they cannot honestly be labeled. This is the remaining blocker for flipping `table_recovery` to implemented.

Remaining work before flipping parity to implemented:

- Split side-by-side adjacent tables (BERT leaderboards) instead of mashing them into one grid, then hand-label them.
- Residual small fake tables remain on figure-diagram and label-margin appendix pages (BERT p2/p4/p14), still flagged `table_uncertain`.
- Multi-page table continuation handling and repeated headers (OMB fixture is the natural gate).

### 3. Refresh LiteParse benchmark evidence — DONE (strict claim)

Status: the strict both-pass claim is now evidence-backed. `feature-parity --bench-report .glyphrush-baselines/reports/liteparse-v0-native-text-strict.json --require-speed-evidence --require-coverage-preset glyphrush-v0-native-text` exits zero with `native_text_speed_claim_ready: true`: Glyphrush and LiteParse both pass the labeled v0 native-text quality gates, and Glyphrush is 73.98x faster than LiteParse default and 1.76x faster than LiteParse no-OCR on the 924-page corpus.

What made the labels fair without weakening them (details and methodology caveats in `docs/benchmarking.md`):

- Generated anchors prefer backend-neutral, single-span, prose-like lines; `tools/baselines/verify_anchors.py` verifies every page anchor against captured LiteParse/PyMuPDF/pdfplumber stdout and repairs unfair ones to page-unique content lines. Anchor distinctiveness went up versus the previous manifest (the OMB doc went from 3 distinct anchors to 384).
- Required-text matching gained a squashed tier (whitespace/control-character-free, minimum eight characters) for extractor spacing quirks.
- Baseline stdout is scored on `table_structure` only when an expectation opts in with `"baseline": true`.
- PyMuPDF/pdfplumber retain three genuine failures (OMB cell reordering, broken-CMap fixture); they are kept failing deliberately.

### 4. Harden OCR fallback on scanned and hybrid documents — DONE

Status: complete against the v0 corpus.

- OCR-needed precision/recall labels exist for every v0 document via `ocr_required_classification` in `test/corpus.v0.json` (scanned patent expects all 6 pages, hybrid Watson expects none).
- All four adapter paths were validated against the scanned fixture: rendered-image Tesseract command (`ocr-check --ocr-command tools/ocr/tesseract-rendered-image.sh --ocr-command-input rendered-image`), sidecar, PDF-path command, and HTTP JSON adapter.
- `test/corpus.v0.ocr.json` plus the committed Tesseract sidecar text under `test/ocr-v0/` form the repeatable OCR-applied gate: with `--ocr-sidecar test/ocr-v0` the scanned doc must report `ocr_applied_pages: 6`, zero warnings, OCR text-recall anchors, and OCR reading order, while the hybrid doc must keep `ocr_required_pages: 0` so clean native text never invokes OCR. Without an adapter the same manifest fails, proving `requires_ocr_without_ocr_output` stays visible. Both are wired into `scripts/verify.sh` (the live-Tesseract `ocr-check` runs when `tesseract` is installed).
- Render/OCR/merge timing counters already flow into page timings and benchmark `stage_timings_us`; the rendered-image run records nonzero `render_us` and `ocr_us` per OCR page.

Remaining follow-up (not blocking): memory/queue-bound checks for very large scanned PDFs need a large scanned fixture; the current v0 scanned fixture is intentionally small.

## P1: Product Readiness

- Cache and snapshot robustness: move beyond the current JSON-friendly cache toward a compact or mmap-friendly artifact format if warm-run performance becomes a bottleneck.
- Parallelism and memory: profile PDFium constraints, keep deterministic page merges, and bound image/render/span caches on large PDFs.
- Packaging: define macOS and Linux CLI builds, PDFium runtime behavior, version metadata, and install-size limits.
- Python package: keep it thin over the native CLI or stable core ABI, with artifact parity tests against the CLI.
- Node package: keep it thin over the same artifact model after the CLI/core API stabilizes.
- MuPDF spike — RESOLVED (rejected): MuPDF is AGPL-3.0 while Glyphrush is MIT; shipping it as a backend would constrain every downstream distribution, and the BSD-licensed PDFium adapter already provides the measured native-text fast path plus rendered-image OCR handoff. Converted to `not_planned` in the parity matrix with the rationale recorded; `backend-check` keeps the adapter slot visible as rejected.
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
