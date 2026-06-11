# Glyphrush WASM binding

Thin wrapper over `glyphrush-core` and `glyphrush-lopdf`. It does not implement a separate PDF parser; the JSON artifact matches the CLI's lopdf backend output.

## Build

```bash
bash bindings/wasm/build.sh
```

Requires `rustup target add wasm32-unknown-unknown` and a `wasm-bindgen-cli` version matching the crate's `wasm-bindgen` dependency.

## Test

From the repo root, after building the CLI and wasm package:

```bash
cargo build -q -p glyphrush-cli
bash bindings/wasm/build.sh
node bindings/wasm/test/parity.mjs
node bindings/wasm/test/parity.mjs test/v0/forms/irs-f1040-2025.pdf --span-geometry
```

The parity script compares wasm output to `glyphrush --backend lopdf parse … --format json`, ignoring timings, `global_diagnostics.total_stage_time_us`, `metadata.source_modified_unix_ms`, and `metadata.parser_version`.

## API

Node.js (via `wasm-bindgen --target nodejs`):

```javascript
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";

const require = createRequire(import.meta.url);
const { parse_pdf_bytes } = require("./pkg/glyphrush_wasm.js");

const pdfBytes = readFileSync("sample.pdf");
const artifactJson = parse_pdf_bytes(new Uint8Array(pdfBytes), false);
const artifact = JSON.parse(artifactJson);
```

`parse_pdf_bytes(bytes, span_geometry)` returns the same `DocumentArtifact` JSON as the CLI. Encrypted PDFs are rejected with the same error message as the CLI.

OCR adapters (sidecar, command, HTTP) are not available in wasm. Pages that need OCR keep their `requires_ocr` quality flags and warnings, matching a CLI run without OCR configured.
