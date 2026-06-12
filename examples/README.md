# Committed output samples

Real Glyphrush output from the committed `test/v0/` corpus, so you can see what the parser produces without installing anything. Regenerate with `bash examples/regenerate.sh`; CI fails if these files drift from what the current parser emits.

| File | Source PDF | Command | What to look at |
|---|---|---|---|
| [`bert-two-column-reading-order.md`](bert-two-column-reading-order.md) | `test/v0/academic_columns/acl-bert-naacl-2019.pdf` | `parse --format markdown` | Two-column academic reading order: banner → title → authors → abstract → left column → right column, across all 16 pages. No column interleave. |
| [`gsa-voucher-ruled-grid-table.json`](gsa-voucher-ruled-grid-table.json) | `test/v0/forms/gsa-sf1035-filled-voucher.pdf` | `parse --format json --span-geometry` | Ruled-grid table recovery on a filled government form: `layout_blocks[].table.rows[].cells[]` puts quantities, unit prices, and amounts in the right columns, with blank cells preserved as explicit empties. The flat text view of this page is a scramble — the structured grid is the product. |
| [`uspto-scanned-requires-ocr.json`](uspto-scanned-requires-ocr.json) | `test/v0/scanned/uspto-us4399515-scanned.pdf` | `parse --format json` | The honesty contract: a scanned patent comes back with `route: ocr_fallback`, `requires_ocr` flags, and `requires_ocr_without_ocr_output` warnings on every page — never fake-clean empty text. |

Notes:

- JSON samples are normalized by `regenerate.sh`: per-page stage timings, `total_stage_time_us`, and `source_modified_unix_ms` are zeroed so regeneration is byte-identical across machines. Every other byte is verbatim parser output.
- The voucher sample needs `--span-geometry` (and the PDFium backend) because positioned-row and ruled-grid table recovery work from span geometry.
- These samples are illustrations; the behavior itself is pinned by the eval gates in `test/corpus.v0*.json`.
