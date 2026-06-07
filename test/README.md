# Local PDF Drop Zone

Drop local PDFs here for manual parsing and benchmarking:

```sh
cargo run -p glyphrush-cli -- inspect test/your-file.pdf
cargo run -p glyphrush-cli -- backend-check --pdf test/your-file.pdf
cargo run -p glyphrush-cli -- parse test/your-file.pdf --format json
cargo run -p glyphrush-cli -- parse test/your-file.pdf --format json --span-geometry
cargo run -p glyphrush-cli -- parse test/your-file.pdf --format markdown
cargo run -p glyphrush-cli -- parse test/your-file.pdf --format json --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- parse test/your-file.pdf --format json --ocr-command test/ocr-command.sh
cargo run -p glyphrush-cli -- parse test/your-file.pdf --format json --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- bench test/your-file.pdf
cargo run -p glyphrush-cli -- bench test/your-file.pdf --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/your-file.pdf --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- bench test/your-file.pdf --ocr-command test/ocr-command.sh
cargo run -p glyphrush-cli -- bench test/your-file.pdf --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- manifest test/your-file.pdf > test/corpus.generated.json
cargo run -p glyphrush-cli -- debug-page test/your-file.pdf 0
cargo run -p glyphrush-cli -- eval test/corpus.datasheets.json
cargo run -p glyphrush-cli -- inspect test/
cargo run -p glyphrush-cli -- backend-check --pdf test/
cargo run -p glyphrush-cli -- backend-check --pdf test/ --jobs 4
cargo run -p glyphrush-cli -- bench test/
cargo run -p glyphrush-cli -- bench test/ --baseline-preset glyphrush-v0
cargo run -p glyphrush-cli -- bench test/ --ocr-sidecar test/ocr
cargo run -p glyphrush-cli -- bench test/ --ocr-command test/ocr-command.sh
cargo run -p glyphrush-cli -- bench test/ --eval-manifest test/corpus.datasheets.json
cargo run -p glyphrush-cli -- bench test/ --cache-dir .glyphrush-cache
cargo run -p glyphrush-cli -- manifest test/ > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --category clean_digital --coverage-preset glyphrush-v0 > test/corpus.generated.json
```

`*.pdf` and `*.PDF` files in this directory are ignored by git. Batch commands currently scan only top-level files in this directory.

Sidecar OCR files use zero-based page indexes. For `your-file.pdf`, OCR text for page `0` should be written to `test/ocr/your-file.p000000.txt`.

OCR command adapters are invoked only for pages routed to OCR fallback. The command receives the PDF path as `$1` and the zero-based page index as `$2`, and stdout becomes that page's OCR text. Use `--ocr-timeout-ms <ms>` to bound slow adapters; the default is `120000`.

Eval manifests are JSON files. Paths are relative to the manifest file, so `test/corpus.json` can refer to `"your-file.pdf"` directly:

Use `manifest` to bootstrap a passing structural manifest after adding PDFs. The generated file records generator provenance, a corpus fingerprint, and per-document source fingerprints that `eval` checks for source drift, then pins OCR-needed classification, non-empty quality-flag classification gates, page layout block counts, and exact warning pins for OCR-required or unsupported pages. Then add human/labeled text, reading-order, table, or bbox expectations before using it for quality claims. Text gates such as `required_text` use the derived layout-aware eval text, not the raw native span list:

```sh
cargo run -p glyphrush-cli -- manifest test/ > test/corpus.generated.json
cargo run -p glyphrush-cli -- manifest test/ --category clean_digital --coverage-preset glyphrush-v0 > test/corpus.generated.json
cargo run -p glyphrush-cli -- eval test/corpus.generated.json
```

Use `--coverage-preset glyphrush-v0` when building the broader benchmark corpus rather than a single-category manifest. It requires at least one PDF in each core v0 class: `clean_digital`, `scanned`, `hybrid`, `academic_columns`, `tables`, `forms`, `rotated`, `weird_encoding`, and `large`. If only one category has been labeled so far, `eval` should fail coverage until the missing classes are added.

`test/corpus.datasheets.json` is a local seed manifest for the datasheet PDFs currently used in this workspace. It requires the `datasheet` category with five documents, pins each PDF's SHA-256 fingerprint and byte size to catch source drift, then gates page counts, image artifact counts, OCR-required pages, selected text recall anchors, known fallback pages, and `silent_failures: 0`.

```json
{
  "documents": [
    {
      "path": "your-file.pdf",
      "expect": {
        "page_count": 1,
        "fallback_pages": 0,
        "ocr_required_pages": 0,
        "ocr_applied_pages": 0,
        "image_artifact_count": 0,
        "required_text": ["known text"],
        "pages": [
	          {
	            "index": 0,
	            "route": "native_fast_path",
	            "image_artifact_count": 0,
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

For layout-sensitive fixtures, pin page-level block counts. This is useful for repeated header/footer regressions or pages expected to expose a table block:

```json
{
  "index": 0,
  "layout_block_counts": {
    "block_count": 3,
    "paragraph_blocks": 1,
    "header_blocks": 1,
    "footer_blocks": 1
  }
}
```

For scanned or hybrid pages, use page-level gates to ensure regressions do not silently drop OCR requirements:

```json
{
  "index": 7,
  "route": "ocr_fallback",
  "required_flags": ["requires_ocr", "low_confidence_text"],
  "required_reasons": ["high_image_coverage_without_native_text"]
}
```

For geometry checks, add `span_bbox` expectations and run eval or bench with `--span-geometry`. Bounded native span geometry includes simple text-matrix/content-matrix transforms, text-state persistence across text objects, text spacing, text rise, line leading, and `'`/`"` shortcut text showing when boxes are emitted:

```json
{
  "page": 0,
  "text": "known text",
  "provenance": "native",
  "min_x0": 40.0,
  "max_x0": 120.0
}
```
