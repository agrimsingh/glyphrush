#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tool_root="${GLYPHRUSH_BASELINE_ROOT:-$repo_root/.glyphrush-baselines}"
venv_dir="$tool_root/venv"
tessdata_dir="$tool_root/tessdata"

mkdir -p "$tool_root"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to install PyMuPDF/pdfplumber baselines" >&2
  exit 127
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "npm is required to install the LiteParse baseline" >&2
  exit 127
fi

python3 -m venv "$venv_dir"
"$venv_dir/bin/python" -m pip install --upgrade pip
"$venv_dir/bin/python" -m pip install pymupdf pdfplumber

mkdir -p "$tessdata_dir"
if [[ ! -s "$tessdata_dir/eng.traineddata" ]]; then
  "$venv_dir/bin/python" - "$tessdata_dir/eng.traineddata" <<'PY'
import sys
from pathlib import Path
from urllib.request import urlretrieve

target = Path(sys.argv[1])
url = "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/eng.traineddata"
urlretrieve(url, target)
PY
fi

npm install --prefix "$tool_root" @llamaindex/liteparse

cat <<EOF
Installed Glyphrush baseline tools under:
  $tool_root

Wrappers will auto-detect:
  $tool_root/node_modules/.bin/lit
  $venv_dir/bin/python3
  $tessdata_dir/eng.traineddata

Smoke-test the install with:
  cargo run -q -p glyphrush-cli -- baseline-check --pdf test/ --baseline-preset glyphrush-v0 --strict
EOF
