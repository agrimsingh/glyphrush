#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

if [[ "${1:-}" == "--describe" ]]; then
  cat <<'JSON'
{
  "name": "marker",
  "target": "Marker",
  "kind": "quality-context-baseline-wrapper",
  "command_hint": "marker_single <pdf> --output_dir <tmp> --output_format markdown",
  "requires": ["marker_single"],
  "ocr": "provider-dependent"
}
JSON
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  echo "usage: marker-text.sh <pdf>" >&2
  exit 64
fi

pdf="$1"
if [[ ! -f "$pdf" ]]; then
  echo "marker baseline input does not exist: $pdf" >&2
  exit 66
fi

bin="$(baseline_resolve_tool "${MARKER_BIN:-}" marker_single)"
if ! command -v "$bin" >/dev/null 2>&1; then
  echo "marker baseline requires the 'marker_single' CLI. Install Marker globally, set MARKER_BIN, or run scripts/setup-baselines.sh for a project-local install." >&2
  exit 127
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/glyphrush-marker.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

"$bin" "$pdf" --output_dir "$tmp_dir" --output_format markdown >/dev/null

output_file="$(
  find "$tmp_dir" -type f \( -name '*.md' -o -name '*.markdown' -o -name '*.txt' \) \
    | sort \
    | head -n 1
)"

if [[ -z "$output_file" ]]; then
  echo "marker baseline produced no markdown or text output in $tmp_dir" >&2
  exit 70
fi

cat "$output_file"
