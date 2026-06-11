#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "opendataloader-pdf",
  "target": "opendataloader-pdf",
  "kind": "text-baseline-wrapper",
  "command_hint": "python3 -c 'from opendataloader_pdf import convert; convert(format=\"text\")'",
  "requires": ["python3", "opendataloader-pdf", "java"],
  "ocr": "none"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: opendataloader-pdf-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "opendataloader-pdf baseline input does not exist: $pdf" >&2
  exit 66
fi

python_bin="$(baseline_resolve_python)"
if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "opendataloader-pdf baseline requires python3" >&2
  exit 127
fi

if ! command -v java >/dev/null 2>&1; then
  echo "opendataloader-pdf baseline requires java" >&2
  exit 127
fi

"$python_bin" - "$pdf" <<'PY'
import os
import sys
import tempfile

try:
    from opendataloader_pdf import convert
except ImportError:
    sys.stderr.write("opendataloader-pdf baseline requires opendataloader-pdf. Install globally, set GLYPHRUSH_BASELINE_PYTHON, or run scripts/setup-baselines.sh for a project-local venv.\n")
    sys.exit(127)

path = sys.argv[1]
stem = os.path.splitext(os.path.basename(path))[0]
with tempfile.TemporaryDirectory() as tmpdir:
    convert(input_path=path, output_dir=tmpdir, format="text", quiet=True)
    candidates = [
        os.path.join(tmpdir, f"{stem}.txt"),
        os.path.join(tmpdir, f"{stem}.text"),
    ]
    for candidate in candidates:
        if os.path.isfile(candidate):
            with open(candidate, encoding="utf-8", errors="replace") as handle:
                text = handle.read()
            if text:
                sys.stdout.write(text)
                if not text.endswith("\n"):
                    sys.stdout.write("\n")
            sys.exit(0)
    for name in sorted(os.listdir(tmpdir)):
        if name.endswith((".txt", ".text", ".md")):
            with open(os.path.join(tmpdir, name), encoding="utf-8", errors="replace") as handle:
                text = handle.read()
            if text:
                sys.stdout.write(text)
                if not text.endswith("\n"):
                    sys.stdout.write("\n")
            sys.exit(0)

sys.stderr.write("opendataloader-pdf baseline produced no text output\n")
sys.exit(127)
PY
