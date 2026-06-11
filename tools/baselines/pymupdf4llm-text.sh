#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "pymupdf4llm",
  "target": "pymupdf4llm",
  "kind": "text-baseline-wrapper",
  "command_hint": "python3 -c 'import pymupdf4llm; pymupdf4llm.to_markdown(path)'",
  "requires": ["python3", "pymupdf4llm"],
  "ocr": "none"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: pymupdf4llm-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "pymupdf4llm baseline input does not exist: $pdf" >&2
  exit 66
fi

python_bin="$(baseline_resolve_python)"
if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "pymupdf4llm baseline requires python3" >&2
  exit 127
fi

"$python_bin" - "$pdf" <<'PY'
import sys

try:
    import pymupdf4llm
except ImportError:
    sys.stderr.write("pymupdf4llm baseline requires pymupdf4llm. Install globally, set GLYPHRUSH_BASELINE_PYTHON, or run scripts/setup-baselines.sh for a project-local venv.\n")
    sys.exit(127)

path = sys.argv[1]
text = pymupdf4llm.to_markdown(path) or ""
if text:
    sys.stdout.write(text)
    if not text.endswith("\n"):
        sys.stdout.write("\n")
PY
