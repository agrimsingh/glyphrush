#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "pdftotext",
  "target": "poppler-utils",
  "kind": "text-baseline-wrapper",
  "command_hint": "pdftotext -q <pdf> -",
  "requires": ["pdftotext"],
  "ocr": "none"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: pdftotext-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "pdftotext baseline input does not exist: $pdf" >&2
  exit 66
fi

if ! command -v pdftotext >/dev/null 2>&1; then
  echo "pdftotext baseline requires pdftotext (poppler-utils)" >&2
  exit 127
fi

exec pdftotext -q "$pdf" -
