#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "liteparse",
  "target": "run-llama/liteparse",
  "kind": "text-baseline-wrapper",
  "command_hint": "lit parse --format text --quiet <pdf>",
  "requires": ["lit"],
  "ocr": "enabled by LiteParse unless LITEPARSE_NO_OCR=1"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: liteparse-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "liteparse baseline input does not exist: $pdf" >&2
  exit 66
fi

bin="${LITEPARSE_BIN:-lit}"
if ! command -v "$bin" >/dev/null 2>&1; then
  echo "liteparse baseline requires the 'lit' CLI. Install run-llama/liteparse, for example: npm install -g @llamaindex/liteparse" >&2
  exit 127
fi

args=(parse --format text --quiet)
if [[ "${LITEPARSE_NO_OCR:-}" == "1" ]]; then
  args+=(--no-ocr)
fi
args+=("$pdf")

exec "$bin" "${args[@]}"
