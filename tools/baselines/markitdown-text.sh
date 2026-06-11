#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "markitdown",
  "target": "markitdown",
  "kind": "text-baseline-wrapper",
  "command_hint": "markitdown <pdf>",
  "requires": ["python3", "markitdown"],
  "ocr": "none"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: markitdown-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "markitdown baseline input does not exist: $pdf" >&2
  exit 66
fi

python_bin="$(baseline_resolve_python)"
if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "markitdown baseline requires python3" >&2
  exit 127
fi

root="$(baseline_repo_root)"
markitdown_cli="$root/.glyphrush-baselines/venv/bin/markitdown"
if [[ -x "$markitdown_cli" ]]; then
  exec "$markitdown_cli" "$pdf"
fi

"$python_bin" - "$pdf" <<'PY'
import sys

try:
    from markitdown import MarkItDown
except ImportError:
    sys.stderr.write("markitdown baseline requires markitdown. Install globally, set GLYPHRUSH_BASELINE_PYTHON, or run scripts/setup-baselines.sh for a project-local venv.\n")
    sys.exit(127)

path = sys.argv[1]
result = MarkItDown().convert(path)
text = result.text_content or ""
if text:
    sys.stdout.write(text)
    if not text.endswith("\n"):
        sys.stdout.write("\n")
PY
