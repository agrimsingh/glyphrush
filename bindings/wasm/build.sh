#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

cargo build -p glyphrush-wasm --target wasm32-unknown-unknown --release --features wasm_js
wasm-bindgen --target nodejs --out-dir bindings/wasm/pkg \
  target/wasm32-unknown-unknown/release/glyphrush_wasm.wasm
