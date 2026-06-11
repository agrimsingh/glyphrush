#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "pypdf",
  "target": "pypdf",
  "kind": "text-baseline-wrapper",
  "command_hint": "python3 -c 'import pypdf; page.extract_text()'",
  "requires": ["python3", "pypdf"],
  "ocr": "none"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: pypdf-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "pypdf baseline input does not exist: $pdf" >&2
  exit 66
fi

python_bin="$(baseline_resolve_python)"
if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "pypdf baseline requires python3" >&2
  exit 127
fi

"$python_bin" - "$pdf" <<'PY'
import sys

try:
    from pypdf import PdfReader
except ImportError:
    sys.stderr.write("pypdf baseline requires pypdf. Install globally, set GLYPHRUSH_BASELINE_PYTHON, or run scripts/setup-baselines.sh for a project-local venv.\n")
    sys.exit(127)

path = sys.argv[1]
reader = PdfReader(path)
for page in reader.pages:
    text = page.extract_text() or ""
    if text:
        sys.stdout.write(text)
        if not text.endswith("\n"):
            sys.stdout.write("\n")
PY
