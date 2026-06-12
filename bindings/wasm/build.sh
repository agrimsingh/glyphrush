#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

cargo build -p glyphrush-wasm --target wasm32-unknown-unknown --release --features wasm_js
wasm-bindgen --target nodejs --out-dir bindings/wasm/pkg \
  target/wasm32-unknown-unknown/release/glyphrush_wasm.wasm

version="$(grep -m1 '^version' bindings/wasm/Cargo.toml | cut -d'"' -f2)"
cat > bindings/wasm/pkg/package.json <<JSON
{
  "name": "glyphrush-wasm",
  "version": "${version}",
  "description": "Glyphrush PDF parser compiled to WebAssembly: PDF bytes in, structured JSON document artifact out, with per-page quality flags",
  "main": "glyphrush_wasm.js",
  "types": "glyphrush_wasm.d.ts",
  "files": ["glyphrush_wasm.js", "glyphrush_wasm.d.ts", "glyphrush_wasm_bg.wasm", "glyphrush_wasm_bg.wasm.d.ts"],
  "license": "MIT",
  "repository": { "type": "git", "url": "https://github.com/agrimsingh/glyphrush.git", "directory": "bindings/wasm" },
  "keywords": ["pdf", "parser", "wasm", "extraction", "tables"]
}
JSON
