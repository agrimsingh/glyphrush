#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
python3 -m unittest discover -s bindings/python/tests
node --test bindings/node/test/client.test.mjs
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -q -p glyphrush-cli -- baseline-check --strict --baseline-preset glyphrush-v0

shopt -s nullglob nocaseglob
local_corpus=(test/*.pdf)
shopt -u nullglob nocaseglob

if ((${#local_corpus[@]} > 0)); then
  cargo run -q -p glyphrush-cli -- eval test/corpus.datasheets.json --category datasheet --jobs 2
else
  echo "Skipping datasheet eval: no local PDFs found under test/."
fi
