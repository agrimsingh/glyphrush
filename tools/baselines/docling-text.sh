#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "docling",
  "target": "Docling",
  "kind": "quality-context-baseline-wrapper",
  "command_hint": "python3 -c 'from docling.document_converter import DocumentConverter; result.document.export_to_text()'",
  "requires": ["python3", "docling"],
  "ocr": "provider-dependent"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: docling-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "docling baseline input does not exist: $pdf" >&2
  exit 66
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "docling baseline requires python3" >&2
  exit 127
fi

python3 - "$pdf" <<'PY'
import sys

try:
    from docling.document_converter import DocumentConverter
except ImportError:
    sys.stderr.write("docling baseline requires docling. Install with: python3 -m pip install docling\n")
    sys.exit(127)

path = sys.argv[1]
result = DocumentConverter().convert(path)
text = result.document.export_to_text() or ""
if text:
    sys.stdout.write(text)
    if not text.endswith("\n"):
        sys.stdout.write("\n")
PY
