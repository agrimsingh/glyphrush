#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

dry_run=false
while (($# > 0)); do
  case "$1" in
    --dry-run)
      dry_run=true
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

pdf_dir="${GLYPHRUSH_BENCH_PDF_DIR:-test/}"
manifest="${GLYPHRUSH_BENCH_MANIFEST:-test/corpus.datasheets.json}"
category="${GLYPHRUSH_BENCH_CATEGORY:-datasheet}"
jobs="${GLYPHRUSH_BENCH_JOBS:-4}"
backend="${GLYPHRUSH_BENCH_BACKEND:-pdfium}"
features="${GLYPHRUSH_BENCH_FEATURES:-pdfium}"
speedup="${GLYPHRUSH_BENCH_LITEPARSE_SPEEDUP:-2.0}"
no_ocr_speedup="${GLYPHRUSH_BENCH_LITEPARSE_NO_OCR_SPEEDUP:-1.5}"
baseline_timeout_ms="${GLYPHRUSH_BENCH_BASELINE_TIMEOUT_MS:-120000}"
output="${GLYPHRUSH_BENCH_OUTPUT:-}"

cmd=(
  cargo run -q --release -p glyphrush-cli
  --features "$features"
  --
  --backend "$backend"
  bench "$pdf_dir"
  --eval-manifest "$manifest"
  --eval-category "$category"
  --baseline-preset glyphrush-v0
  --require-baselines
  --require-baseline-quality
  --require-speedup-claim "liteparse=$speedup"
  --require-speedup-claim "liteparse-no-ocr=$no_ocr_speedup"
  --baseline-timeout-ms "$baseline_timeout_ms"
  --jobs "$jobs"
)

print_command() {
  printf '%q ' "${cmd[@]}"
  if [[ -n "$output" ]]; then
    printf '> %q' "$output"
  fi
  printf '\n'
}

if [[ "$dry_run" == true ]]; then
  print_command
  exit 0
fi

cd "$repo_root"
if [[ -n "$output" ]]; then
  mkdir -p "$(dirname "$output")"
  "${cmd[@]}" > "$output"
else
  "${cmd[@]}"
fi
