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

### 1. Close `span_geometry_layout` from partial to implemented

Why it matters: this is one of the two remaining partial LiteParse-parity capabilities, and it directly affects reading order, bbox quality, multi-column documents, and confidence flags.

Concrete tasks:

- Add labeled real-PDF fixtures for multi-column papers, footnotes, sidebars, page headers, page numbers, figures, rotated/cropped pages, and wide cross-column headings.
- Add reading-order expectations that fail when columns interleave, headers land in body text, captions are misplaced, or page numbers pollute content.
- Add bbox sanity expectations for representative spans so geometry regressions are caught without requiring full visual overlays.
- Improve the layout confidence model so bad span geometry, overlapping bands, implausible coordinates, and unsupported writing modes produce `layout_uncertain` or a heavier route instead of quiet bad output.
- Reduce cases where caps or defensive heuristics throw away useful geometry; caps should route work or flag uncertainty, not hide content.
- Add debug output that explains why a page stayed on the fast path or escalated to heavier layout work.

Exit criteria:

- The feature-parity status can move from `partial` only when the labeled layout fixtures pass and unsupported cases are explicitly flagged.
- Multi-column native-text PDFs preserve stable, human-readable order without a broad latency regression.

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

### 3. Refresh LiteParse benchmark evidence

Why it matters: the project goal is "faster LiteParse," but the claim must be phrased correctly. Today the narrow Glyphrush native-text speed advantage is ready, while the stricter quality-backed LiteParse comparison is not ready.

Concrete tasks:

- Re-run the native-text v0 benchmark against LiteParse and LiteParse-no-OCR using the current parser.
- Preserve the full saved report under `.glyphrush-baselines/reports/` and keep the progress log for stalled or slow baselines.
- Investigate saved LiteParse quality failures by category before using the result for public claims.
- Decide whether a release note should say "Glyphrush passes quality and is faster on this corpus" or the stricter "Glyphrush and LiteParse both pass quality, and Glyphrush is faster."
- Document the exact command, corpus manifest, PDF root, coverage preset, timing, and quality status with every claim.

Recommended command:

```sh
GLYPHRUSH_BENCH_CATEGORY=native-text \
GLYPHRUSH_BENCH_MANIFEST=test/corpus.v0.json \
GLYPHRUSH_BENCH_OUTPUT=.glyphrush-baselines/reports/liteparse-v0-native-text-gate.json \
scripts/bench-liteparse.sh
```

Exit criteria:

- `feature-parity --bench-report <saved-report> --require-speed-advantage --require-coverage-preset glyphrush-v0-native-text` passes.
- If using the stricter claim, `--require-speed-evidence` also passes and the report contains quality-backed LiteParse claims.

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
