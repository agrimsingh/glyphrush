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

manifest="${GLYPHRUSH_BENCH_MANIFEST:-test/corpus.datasheets.json}"
category="${GLYPHRUSH_BENCH_CATEGORY:-datasheet}"
jobs="${GLYPHRUSH_BENCH_JOBS:-4}"
backend="${GLYPHRUSH_BENCH_BACKEND:-pdfium}"
features="${GLYPHRUSH_BENCH_FEATURES:-pdfium}"
speedup="${GLYPHRUSH_BENCH_LITEPARSE_SPEEDUP:-2.0}"
no_ocr_speedup="${GLYPHRUSH_BENCH_LITEPARSE_NO_OCR_SPEEDUP:-1.5}"
baseline_timeout_ms="${GLYPHRUSH_BENCH_BASELINE_TIMEOUT_MS:-120000}"
coverage_preset="${GLYPHRUSH_BENCH_COVERAGE_PRESET:-}"
output="${GLYPHRUSH_BENCH_OUTPUT:-}"
pdf_dir="${GLYPHRUSH_BENCH_PDF_DIR:-}"
if [[ -z "$pdf_dir" ]]; then
  case "$manifest" in
    test/corpus.v0.json | */test/corpus.v0.json)
      pdf_dir="test/v0"
      ;;
    *)
      pdf_dir="test/"
      ;;
  esac
fi

preflight_cmd=(
  cargo run -q --release -p glyphrush-cli
  --features "$features"
  --
  --backend "$backend"
  baseline-check
  --pdf "$pdf_dir"
  --baseline-preset glyphrush-v0
  --baseline-timeout-ms "$baseline_timeout_ms"
  --strict
)

cmd=(
  cargo run -q --release -p glyphrush-cli
  --features "$features"
  --
  --backend "$backend"
  bench "$pdf_dir"
  --eval-manifest "$manifest"
  --baseline-preset glyphrush-v0
  --require-baselines
  --require-baseline-quality
  --require-speedup-claim "liteparse=$speedup"
  --require-speedup-claim "liteparse-no-ocr=$no_ocr_speedup"
  --baseline-timeout-ms "$baseline_timeout_ms"
  --jobs "$jobs"
)

if [[ "$category" != "all" ]]; then
  cmd+=(--eval-category "$category")
fi

if [[ -n "$coverage_preset" ]]; then
  cmd+=(--require-coverage-preset "$coverage_preset")
fi

feature_parity_cmd=()
if [[ -n "$output" ]]; then
  feature_parity_cmd=(
    cargo run -q --release -p glyphrush-cli
    --features "$features"
    --
    --backend "$backend"
    feature-parity
    --bench-report "$output"
    --require-speed-evidence
  )
  if [[ -n "$coverage_preset" ]]; then
    feature_parity_cmd+=(--require-coverage-preset "$coverage_preset")
  fi
fi

print_command() {
  printf '%q ' "${preflight_cmd[@]}"
  printf '\n'
  printf '%q ' "${cmd[@]}"
  if [[ -n "$output" ]]; then
    printf '> %q' "$output"
  fi
  printf '\n'
  if ((${#feature_parity_cmd[@]} > 0)); then
    printf '%q ' "${feature_parity_cmd[@]}"
    printf '\n'
  fi
}

if [[ "$dry_run" == true ]]; then
  print_command
  exit 0
fi

cd "$repo_root"
"${preflight_cmd[@]}"

if [[ -n "$output" ]]; then
  mkdir -p "$(dirname "$output")"
  "${cmd[@]}" > "$output"
else
  "${cmd[@]}"
fi

if ((${#feature_parity_cmd[@]} > 0)); then
  "${feature_parity_cmd[@]}"
fi
