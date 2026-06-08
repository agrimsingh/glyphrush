#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{"name":"tesseract-rendered-image","target":"Tesseract OCR","kind":"ocr-command-wrapper","input":"rendered-image","command_hint":"glyphrush --backend pdfium parse <pdf> --ocr-command tools/ocr/tesseract-rendered-image.sh --ocr-command-input rendered-image","requires":["tesseract"],"ocr":"local Tesseract invoked only for Glyphrush OCR-routed pages"}
JSON
  exit 0
fi

if (($# < 1)); then
  echo "usage: tesseract-rendered-image.sh <rendered-image> [page-index]" >&2
  exit 2
fi

image="$1"
tesseract_bin="${TESSERACT_BIN:-tesseract}"
lang="${TESSERACT_LANG:-eng}"
psm="${TESSERACT_PSM:-6}"

if [[ ! -f "$image" ]]; then
  echo "rendered image not found: $image" >&2
  exit 2
fi

exec "$tesseract_bin" "$image" stdout -l "$lang" --psm "$psm"
