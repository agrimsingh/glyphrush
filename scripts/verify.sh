#!/usr/bin/env bash
set -euo pipefail

dry_run=0
if [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=1
  shift
fi

run() {
  if ((dry_run)); then
    printf '%q' "$1"
    shift
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

run cargo fmt --all -- --check
run python3 -m unittest discover -s bindings/python/tests
run node --test bindings/node/test/client.test.mjs
run cargo test --workspace
run cargo clippy --workspace --all-targets -- -D warnings
run cargo run -q -p glyphrush-cli -- baseline-check --strict --baseline-preset glyphrush-v0

if [[ "${GLYPHRUSH_VERIFY_PDFIUM:-0}" == "1" ]]; then
  run cargo test -p glyphrush-cli --features pdfium \
    feature_parity_counts_pdfium_ocr_runtime_caps_and_cache_as_implemented -- --nocapture
  run cargo test -p glyphrush-cli --features pdfium \
    parse_pdfium_ocr_command_rendered_image_invokes_adapter_only_for_ocr_pages -- --nocapture
fi

shopt -s nullglob nocaseglob
local_corpus=(test/*.pdf)
shopt -u nullglob nocaseglob

if ((${#local_corpus[@]} > 0)); then
  run cargo run -q -p glyphrush-cli -- eval test/corpus.datasheets.json --category datasheet --jobs 2
else
  echo "Skipping datasheet eval: no local PDFs found under test/."
fi

v0_corpus=()
if [[ -d test/v0 ]]; then
  while IFS= read -r -d '' pdf; do
    v0_corpus+=("$pdf")
  done < <(find test/v0 -type f -iname '*.pdf' -print0)
fi

if ((${#v0_corpus[@]} > 0)); then
  if [[ "${GLYPHRUSH_VERIFY_PDFIUM:-0}" == "1" ]]; then
    run cargo run -q -p glyphrush-cli --features pdfium -- --backend pdfium eval test/corpus.v0.json --jobs 2
  else
    echo "Skipping v0 eval: set GLYPHRUSH_VERIFY_PDFIUM=1 to evaluate PDFium-generated test/corpus.v0.json."
  fi
else
  echo "Skipping v0 eval: no local PDFs found under test/v0/."
fi
