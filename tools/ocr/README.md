# OCR Wrappers

These wrappers are optional OCR adapters for Glyphrush. They are not used unless passed explicitly through `--ocr-command`.

## Tesseract Rendered Image

`tesseract-rendered-image.sh` expects Glyphrush to render OCR-routed pages first:

```sh
cargo run -p glyphrush-cli --features pdfium -- \
  --backend pdfium \
  parse test/scan.pdf \
  --format json \
  --ocr-command tools/ocr/tesseract-rendered-image.sh \
  --ocr-command-input rendered-image
```

The wrapper receives the temporary page image path as argument 1 and the zero-based page index as argument 2. It invokes `tesseract <image> stdout -l <lang> --psm <psm>` and writes OCR text to stdout.

Environment overrides:

- `TESSERACT_BIN`: Tesseract executable path, default `tesseract`.
- `TESSERACT_LANG`: OCR language, default `eng`.
- `TESSERACT_PSM`: page segmentation mode, default `6`.

Preflight before a scanned or hybrid benchmark:

```sh
cargo run -p glyphrush-cli --features pdfium -- \
  --backend pdfium \
  ocr-check test/scan.pdf \
  --page-index 0 \
  --ocr-command tools/ocr/tesseract-rendered-image.sh \
  --ocr-command-input rendered-image \
  --strict
```
