#!/usr/bin/env bash
# Regenerates every committed sample under examples/ from the corpus PDFs.
# Run from the repository root. Requires the PDFium feature (the voucher's
# ruled-grid table recovery needs span geometry + PDFium).
#
#   bash examples/regenerate.sh
#   GLYPHRUSH_BIN=target/release/glyphrush bash examples/regenerate.sh
#
# JSON samples are normalized: per-page stage timings, total stage time, and
# the source file's mtime are zeroed so regeneration is byte-stable across
# machines and clones. Everything else is the parser's verbatim output.
set -euo pipefail

cd "$(dirname "$0")/.."

glyphrush() {
  if [[ -n "${GLYPHRUSH_BIN:-}" ]]; then
    "$GLYPHRUSH_BIN" "$@"
  else
    cargo run -q -p glyphrush-cli --features pdfium -- "$@"
  fi
}

normalize_json() {
  python3 -c '
import json, sys

artifact = json.load(sys.stdin)
artifact.setdefault("metadata", {})["source_modified_unix_ms"] = 0
for page in artifact.get("pages", []):
    timings = page.get("timings", {})
    for key in timings:
        timings[key] = 0
diagnostics = artifact.get("global_diagnostics", {})
if "total_stage_time_us" in diagnostics:
    diagnostics["total_stage_time_us"] = 0
json.dump(artifact, sys.stdout, indent=1, ensure_ascii=False)
sys.stdout.write("\n")
'
}

glyphrush --backend pdfium parse test/v0/academic_columns/acl-bert-naacl-2019.pdf \
  --format markdown > examples/bert-two-column-reading-order.md

glyphrush --backend pdfium parse test/v0/forms/gsa-sf1035-filled-voucher.pdf \
  --format json --span-geometry | normalize_json > examples/gsa-voucher-ruled-grid-table.json

glyphrush --backend pdfium parse test/v0/scanned/uspto-us4399515-scanned.pdf \
  --format json | normalize_json > examples/uspto-scanned-requires-ocr.json

echo "examples regenerated"
