# Glyphrush

[![CI](https://github.com/agrimsingh/glyphrush/actions/workflows/ci.yml/badge.svg)](https://github.com/agrimsingh/glyphrush/actions/workflows/ci.yml)

A fast, honest PDF parser in Rust. Glyphrush extracts native text, layout, and tables at hundreds of pages per second, and it **tells you when it can't**: pages that need OCR, uncertain layout, or ambiguous tables are flagged instead of silently returned as clean text.

```sh
glyphrush parse paper.pdf --format markdown
glyphrush parse paper.pdf --format json     # structured artifact: spans, layout blocks, tables, quality flags
glyphrush inspect report.pdf --pages        # per-page triage: routes, flags, timings
```

## Benchmarks

Measured on the committed 9-document, 925-page `glyphrush-v0` corpus (academic papers, government reports, forms, budget tables, an invoice voucher, a broken-encoding fixture). Every run is **quality-gated**: speed only counts when the labeled content checks pass on the same run.

| Parser | Corpus wall time | Glyphrush speedup | Quality checks |
|---|---|---|---|
| **Glyphrush** (PDFium backend) | **2.15 s** (430 pages/sec) | — | pass |
| LiteParse (default, OCR on) | 145.3 s | **67.5×** | pass |
| LiteParse (`--no-ocr`) | 3.5 s | **1.65×** | pass |
| PyMuPDF | 6.3 s | 2.9× | 2 genuine failures¹ |
| pdfplumber | 80.0 s | 37.2× | 1 genuine failure¹ |

The headline claim is the strict one: **Glyphrush and LiteParse both pass the labeled quality gates, and Glyphrush is faster.** The claim is machine-checked: `feature-parity --require-speed-evidence` exits nonzero unless the saved benchmark report contains passing, quality-backed speedup claims. Methodology, caveats (process-startup floors, worker symmetry), and the exact reproduction command live in [docs/benchmarking.md](docs/benchmarking.md).

¹ PyMuPDF reorders budget-table cell text; both Python parsers produce divergent output on a deliberately broken-CMap fixture. Those failures are kept failing on purpose; quality labels are verified against every baseline's actual output by [`tools/baselines/verify_anchors.py`](tools/baselines/verify_anchors.py) so no parser is penalized for formatting quirks.

## Install

Glyphrush builds from source (Rust edition 2024):

```sh
git clone https://github.com/agrimsingh/glyphrush && cd glyphrush

# Fast path: PDFium backend (auto-downloads a PDFium runtime on first use)
cargo build --release -p glyphrush-cli --features pdfium

# Dependency-light: pure-Rust lopdf backend only
cargo build --release -p glyphrush-cli

./target/release/glyphrush parse your.pdf --format markdown
```

Language bindings are thin wrappers over the same native core, so they can never drift from the CLI:

```python
# Python (bindings/python): shells out to the native binary
import glyphrush
artifact = glyphrush.parse("your.pdf", binary="target/release/glyphrush")
markdown = glyphrush.parse_markdown("your.pdf", binary="target/release/glyphrush")
```

```js
// Node (bindings/node)
import { parse, parseMarkdown } from "./bindings/node/src/index.mjs";
const artifact = parse("your.pdf", { binary: "target/release/glyphrush" });
```

```js
// WASM (bindings/wasm): bytes in, identical JSON artifact out
import { parse_pdf_bytes } from "./bindings/wasm/pkg/glyphrush_wasm.js";
const artifact = JSON.parse(parse_pdf_bytes(pdfBytes, false));
```

Build the wasm package with `bash bindings/wasm/build.sh`. Set `GLYPHRUSH_BIN=/path/to/glyphrush` to avoid passing `binary` on every Python/Node call.

## What you get

`parse --format json` emits a deterministic **document artifact**: same input and options always produce the same page order, span order, flags, and artifact IDs.

- **Per-page text spans** with bounding boxes when `--span-geometry` is requested.
- **Layout blocks**: paragraphs, headings, lists, headers/footers, figures, and tables with structured `rows[].cells[]` (blank cells preserved).
- **Quality flags per page**: `requires_ocr`, `layout_uncertain`, `table_uncertain`, `broken_encoding`, `unsupported_feature`. A scanned page comes back flagged, never as silently empty "success".
- **Routing diagnostics**: why each page stayed on the fast path or escalated (`debug-page` explains any single page).
- `--format text` and `--format markdown` are derived views of the same artifact; markdown renders recovered tables as pipe tables.

## How it works

**Route cheap, escalate honestly.** A per-page classifier computes cheap signals (image coverage, encoding health, vector ruling density, text duplication) and routes each page: native fast path, heavier layout work, table recovery, or OCR. Fallbacks are explicit decisions recorded in the artifact, not hidden retries.

**Native-text hot path.** The default path does no rendering, no OCR, no per-character geometry. That is where the 67× over OCR-enabled LiteParse comes from: most digital PDFs never need the heavy machinery, and Glyphrush proves it per page instead of assuming it.

**Geometry-aware reading order.** With `--span-geometry`, positioned spans are grouped by a column-row band model: rows that straddle the page gutter (titles, centered page numbers, footers) become bands, runs of column-fitting rows split into columns. Two-column academic papers read title → abstract → left column → right column instead of interleaving line by line. Pages with unresolved multi-column evidence are flagged `layout_uncertain` rather than silently mangled.

**Three table-recovery families, all conservative.** Text-pattern recovery for delimited/whitespace grids (driven by a declarative pattern table), positioned-row recovery for aligned span grids (with parallel-prose rejection so body text never becomes a fake table), and ruled-grid recovery that uses extracted vector ruling lines as column boundaries — that is how a filled GSA voucher's line items come out as structured cells. Ambiguous structures keep `table_uncertain` flags instead of overclaiming.

**OCR is page-selective and external.** No bundled OCR engine, no hidden network calls. Sidecar files, an external command (a Tesseract wrapper ships in `tools/ocr/`), or an HTTP endpoint are invoked only for the pages the classifier routes to OCR. Without an adapter, those pages are flagged `requires_ocr` so downstream consumers can detect incomplete extraction.

**Quality is enforced, not asserted.** The corpus, its labeled expectations (text anchors, reading-order sequences, table structure, OCR precision/recall), and the benchmark gates are committed to this repo. `scripts/verify.sh` runs them in CI. Benchmark reports embed the quality results of the exact artifacts that were timed, and release claims are gated on `feature-parity` verdicts, not on prose.

## Commands

| Command | Purpose |
|---|---|
| `parse <pdf> --format json\|text\|markdown` | Extract one document |
| `inspect <pdf-or-dir> [--pages]` | Fast triage: routes, flags, timings |
| `debug-page <pdf> <n>` | Explain one page's routing and layout |
| `eval <manifest>` | Run labeled quality gates over a corpus |
| `bench <pdf-or-dir> [--baseline-preset glyphrush-v0]` | Speed + quality report vs external parsers |
| `manifest <pdf-or-dir>` | Bootstrap an eval manifest from current output |
| `feature-parity [--bench-report <json>]` | LiteParse capability matrix + claim readiness |
| `backend-check` / `baseline-check` / `ocr-check` | Preflights for backends, baseline wrappers, OCR adapters |

Every command emits machine-readable JSON. The full flag-by-flag reference is in [docs/cli-reference.md](docs/cli-reference.md).

## Project layout

| Path | What it is |
|---|---|
| `crates/glyphrush-core` | Artifact model, classifier, layout, table recovery (pure Rust) |
| `crates/glyphrush-lopdf` | Dependency-light lopdf extraction backend |
| `crates/glyphrush-cli` | The `glyphrush` binary; optional PDFium backend behind `--features pdfium` |
| `bindings/python`, `bindings/node`, `bindings/wasm` | Thin wrappers over the native core |
| `test/v0/` + `test/corpus.v0*.json` | Committed benchmark corpus and labeled quality gates |
| `docs/` | [CLI reference](docs/cli-reference.md), [benchmarking](docs/benchmarking.md), [pipeline](docs/pipeline.md), [artifact schema](docs/artifact-schema.md), [remaining work](docs/remaining-work.md) |

## Development

```sh
cargo test --workspace                                  # no local PDFs needed
GLYPHRUSH_VERIFY_PDFIUM=1 bash scripts/verify.sh        # full CI gate: fmt, clippy, tests, corpus + wasm parity gates
```

The feature-parity matrix currently reports 11 of 13 LiteParse capabilities implemented, 0 partial, 0 planned, and 2 intentionally rejected with recorded rationale (a MuPDF backend on AGPL grounds; bundled OCR by hot-path policy). See [docs/remaining-work.md](docs/remaining-work.md) for the full history.

## License

MIT
