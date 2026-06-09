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
baseline_timeout_ms="${GLYPHRUSH_BENCH_BASELINE_TIMEOUT_MS:-}"
coverage_preset="${GLYPHRUSH_BENCH_COVERAGE_PRESET:-}"
output="${GLYPHRUSH_BENCH_OUTPUT:-}"
progress_log="${GLYPHRUSH_BENCH_PROGRESS_LOG:-}"
pdf_dir="${GLYPHRUSH_BENCH_PDF_DIR:-}"
preflight_mode="${GLYPHRUSH_BENCH_PREFLIGHT:-}"
probe_pdf="${GLYPHRUSH_BENCH_PROBE_PDF:-}"
probe_baseline="${GLYPHRUSH_BENCH_PROBE_BASELINE:-}"
probe_timeout_ms="${GLYPHRUSH_BENCH_PROBE_TIMEOUT_MS:-60000}"
native_text_categories="academic_columns,clean_digital,forms,hybrid,large,rotated,tables,weird_encoding"
is_v0_manifest=false
case "$manifest" in
  test/corpus.v0.json | */test/corpus.v0.json)
    is_v0_manifest=true
    ;;
esac
case "$category" in
  native-text | native_text)
    category="$native_text_categories"
    if [[ -z "$coverage_preset" ]]; then
      coverage_preset="glyphrush-v0-native-text"
    fi
    ;;
esac
if [[ -z "$pdf_dir" ]]; then
  if [[ "$is_v0_manifest" == true ]]; then
    pdf_dir="test/v0"
  else
    pdf_dir="test/"
  fi
fi
if [[ -z "$baseline_timeout_ms" ]]; then
  if [[ -n "$probe_pdf" ]]; then
    baseline_timeout_ms="$probe_timeout_ms"
  elif [[ "$is_v0_manifest" == true ]]; then
    baseline_timeout_ms="900000"
  else
    baseline_timeout_ms="120000"
  fi
fi
if [[ -n "$output" && -z "$progress_log" ]]; then
  progress_log="${output%.json}.progress.log"
fi
if [[ -z "$preflight_mode" ]]; then
  if [[ "$is_v0_manifest" == true ]]; then
    preflight_mode="describe"
  else
    preflight_mode="smoke"
  fi
fi

preflight_cmd=()
case "$preflight_mode" in
  smoke)
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
    ;;
  describe)
    preflight_cmd=(
      cargo run -q --release -p glyphrush-cli
      --features "$features"
      --
      --backend "$backend"
      baseline-check
      --baseline-preset glyphrush-v0
      --strict
    )
    ;;
  none)
    ;;
  *)
    echo "invalid GLYPHRUSH_BENCH_PREFLIGHT: $preflight_mode (expected smoke, describe, or none)" >&2
    exit 2
    ;;
esac

probe_cmd=()
if [[ -n "$probe_pdf" ]]; then
  probe_baseline_args=(--baseline-preset glyphrush-v0)
  if [[ -n "$probe_baseline" ]]; then
    case "$probe_baseline" in
      liteparse)
        probe_baseline_args=(--baseline "liteparse=tools/baselines/liteparse-text.sh")
        ;;
      liteparse-no-ocr)
        probe_baseline_args=(--baseline "liteparse-no-ocr=tools/baselines/liteparse-no-ocr-text.sh")
        ;;
      pymupdf)
        probe_baseline_args=(--baseline "pymupdf=tools/baselines/pymupdf-text.sh")
        ;;
      pdfplumber)
        probe_baseline_args=(--baseline "pdfplumber=tools/baselines/pdfplumber-text.sh")
        ;;
      *)
        echo "invalid GLYPHRUSH_BENCH_PROBE_BASELINE: $probe_baseline (expected liteparse, liteparse-no-ocr, pymupdf, or pdfplumber)" >&2
        exit 2
        ;;
    esac
  fi
  probe_cmd=(
    cargo run -q --release -p glyphrush-cli
    --features "$features"
    --
    --backend "$backend"
    baseline-check
    --pdf "$probe_pdf"
    "${probe_baseline_args[@]}"
    --baseline-timeout-ms "$baseline_timeout_ms"
    --strict
  )
fi

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
  if ((${#probe_cmd[@]} > 0)); then
    printf '%q ' "${probe_cmd[@]}"
    if [[ -n "$output" ]]; then
      printf '> %q' "$output"
      if [[ -n "$progress_log" ]]; then
        printf ' 2> >(tee %q >&2)' "$progress_log"
      fi
    fi
    printf '\n'
    return
  fi

  if ((${#preflight_cmd[@]} > 0)); then
    printf '%q ' "${preflight_cmd[@]}"
    printf '\n'
  fi
  printf '%q ' "${cmd[@]}"
  if [[ -n "$output" ]]; then
    printf '> %q' "$output"
    if [[ -n "$progress_log" ]]; then
      printf ' 2> >(tee %q >&2)' "$progress_log"
    fi
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
if ((${#probe_cmd[@]} > 0)); then
  if [[ -n "$output" ]]; then
    mkdir -p "$(dirname "$output")"
    if [[ -n "$progress_log" ]]; then
      mkdir -p "$(dirname "$progress_log")"
      "${probe_cmd[@]}" > "$output" 2> >(tee "$progress_log" >&2)
    else
      "${probe_cmd[@]}" > "$output"
    fi
  else
    "${probe_cmd[@]}"
  fi
  exit $?
fi

if ((${#preflight_cmd[@]} > 0)); then
  "${preflight_cmd[@]}"
fi

if [[ -n "$output" ]]; then
  mkdir -p "$(dirname "$output")"
  if [[ -n "$progress_log" ]]; then
    mkdir -p "$(dirname "$progress_log")"
    "${cmd[@]}" > "$output" 2> >(tee "$progress_log" >&2)
  else
    "${cmd[@]}" > "$output"
  fi
else
  "${cmd[@]}"
fi

if ((${#feature_parity_cmd[@]} > 0)); then
  "${feature_parity_cmd[@]}"
fi
