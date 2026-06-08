#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "pymupdf",
  "target": "PyMuPDF",
  "kind": "text-baseline-wrapper",
  "command_hint": "python3 -c 'import fitz; page.get_text(\"text\")'",
  "requires": ["python3", "pymupdf"],
  "ocr": "none"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: pymupdf-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "pymupdf baseline input does not exist: $pdf" >&2
  exit 66
fi

python_bin="$(baseline_resolve_python)"
if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "pymupdf baseline requires python3" >&2
  exit 127
fi

"$python_bin" - "$pdf" <<'PY'
import sys

try:
    import fitz
except ImportError:
    sys.stderr.write("pymupdf baseline requires PyMuPDF. Install globally, set GLYPHRUSH_BASELINE_PYTHON, or run scripts/setup-baselines.sh for a project-local venv.\n")
    sys.exit(127)

path = sys.argv[1]
with fitz.open(path) as document:
    for page in document:
        text = page.get_text("text") or ""
        if text:
            sys.stdout.write(text)
            if not text.endswith("\n"):
                sys.stdout.write("\n")
PY
