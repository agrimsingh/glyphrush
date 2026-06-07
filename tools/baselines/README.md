# Baseline Wrappers

These wrappers normalize parser baselines into the `glyphrush bench --baseline NAME=EXECUTABLE` contract: one PDF path in, extracted text on stdout.

Available wrappers:

- `liteparse-text.sh`: runs run-llama/liteparse through `lit parse --format text --quiet`. OCR is enabled by LiteParse unless `LITEPARSE_NO_OCR=1` is set.
- `liteparse-no-ocr-text.sh`: runs run-llama/liteparse through `lit parse --format text --quiet --no-ocr` for native-text-only timing.
- `pymupdf-text.sh`: runs PyMuPDF and emits `page.get_text("text")` for each page.
- `pdfplumber-text.sh`: runs pdfplumber and emits `page.extract_text()` for each page.
- `marker-text.sh`: optional quality-context wrapper for Marker. It runs `marker_single` and emits the first generated Markdown/text file.
- `docling-text.sh`: optional quality-context wrapper for Docling. It uses Docling's Python `DocumentConverter` and emits `export_to_text()`.

Each wrapper supports `--describe` without requiring its parser dependency:

```sh
tools/baselines/liteparse-text.sh --describe
tools/baselines/liteparse-no-ocr-text.sh --describe
tools/baselines/pymupdf-text.sh --describe
tools/baselines/pdfplumber-text.sh --describe
tools/baselines/marker-text.sh --describe
tools/baselines/docling-text.sh --describe
```

Glyphrush can preflight the core comparison set without parsing PDFs:

```sh
glyphrush baseline-check --baseline-preset glyphrush-v0
```

`glyphrush-v0` expands to LiteParse, LiteParse no-OCR, PyMuPDF, and pdfplumber. Marker and Docling are excluded from the preset by design because they are heavier quality-context baselines; add them manually when a benchmark needs that comparison. Benchmark and baseline-check JSON reports include `requested_baseline_presets` so saved results show when this preset was expanded.

Use explicit baselines when you want to include optional tools:

```sh
glyphrush baseline-check \
  --baseline liteparse=tools/baselines/liteparse-text.sh \
  --baseline liteparse-no-ocr=tools/baselines/liteparse-no-ocr-text.sh \
  --baseline pymupdf=tools/baselines/pymupdf-text.sh \
  --baseline pdfplumber=tools/baselines/pdfplumber-text.sh \
  --baseline marker=tools/baselines/marker-text.sh \
  --baseline docling=tools/baselines/docling-text.sh
```

Add `--pdf test/example.pdf` to smoke-test the installed parser dependencies against one PDF before a long corpus benchmark. Use a directory such as `--pdf test/` to smoke every top-level PDF in stable filename order before a corpus run:

```sh
glyphrush baseline-check --pdf test/example.pdf --baseline-preset glyphrush-v0
glyphrush baseline-check --pdf test/ --baseline-preset glyphrush-v0
```

The command reports valid `--describe` JSON, missing wrapper paths, stderr previews, timeout details, smoke output digests/counts when requested, directory `smoke_document_count` and per-document smoke entries when a directory is supplied, and aggregate `describe_success_count`/`all_described` plus `smoke_success_count`/`all_smoke_passed` fields.

Example:

```sh
glyphrush bench test/ --baseline-preset glyphrush-v0 --eval-manifest test/corpus.json
```

When the eval manifest includes document `category` values, corpus baseline summaries include `quality_category_summaries` so LiteParse/PyMuPDF/pdfplumber and optional Marker/Docling quality failures can be read by corpus class instead of only as one aggregate pass rate.
