#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "liteparse-no-ocr",
  "target": "run-llama/liteparse",
  "kind": "text-baseline-wrapper",
  "command_hint": "lit parse --format text --quiet --no-ocr <pdf>",
  "requires": ["lit"],
  "ocr": "disabled with --no-ocr for native-text-only timing"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: liteparse-no-ocr-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "liteparse no-ocr baseline input does not exist: $pdf" >&2
  exit 66
fi

bin="$(baseline_resolve_tool "${LITEPARSE_BIN:-}" lit)"
if ! command -v "$bin" >/dev/null 2>&1; then
  echo "liteparse no-ocr baseline requires the 'lit' CLI. Install run-llama/liteparse globally, set LITEPARSE_BIN, or run scripts/setup-baselines.sh for a project-local install." >&2
  exit 127
fi

exec "$bin" parse --format text --quiet --no-ocr "$pdf"
