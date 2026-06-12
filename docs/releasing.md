# Releasing

## Crates.io (`glyphrush-core`, `glyphrush-lopdf`, `glyphrush-cli`)

Publish in dependency order; each later crate requires the earlier one to be live on crates.io (first-publish dry-runs of dependents fail on the index lookup, which is expected):

```sh
cargo publish -p glyphrush-core
cargo publish -p glyphrush-lopdf
cargo publish -p glyphrush-cli
```

All three crates are published (v0.1.0, 2026-06-12); `cargo install glyphrush-cli --features pdfium` is verified working from the live registry. Future releases bump versions in dependency order with the same three commands.

The `glyphrush-wasm` crate is `publish = false`; it ships through npm instead.

## npm (`glyphrush-wasm`)

```sh
bash bindings/wasm/build.sh     # builds wasm + generates pkg/package.json
cd bindings/wasm/pkg && npm publish --access public
```

`npm pack --dry-run` is verified: 5 files, ~822 KB packed. Requires an npm token with publish rights.

## GitHub release binaries

`.github/workflows/release.yml` triggers on `v*` tags: builds the PDFium-feature CLI for Linux x86_64/aarch64 and macOS arm64/x86_64, packages tarballs with SHA-256 checksums, and attaches them to a draft release. Tag, push, review the draft, publish:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

## PyPI (decision: deferred)

The Python binding is intentionally a thin shim over the native binary; a useful PyPI package must ship or obtain that binary. Options considered:

1. **Binary-in-wheel** (platform wheels bundling the CLI): best UX; requires per-platform build infrastructure (the release workflow's artifacts can feed this later).
2. **Download-on-first-use**: smaller wheels, but a hidden network call at import time conflicts with the project's no-hidden-network policy.
3. **Shim-only package** requiring a separately installed binary: honest but a poor `pip install` experience.

Deferred until the GitHub release pipeline is live, then option 1 reuses its artifacts. Until then the documented Python path is: install the binary (cargo or release tarball), `GLYPHRUSH_BIN=...`, and use `bindings/python` from the repo.

## Re-verify claims before any public release

LiteParse ships point releases frequently. Before tagging:

```sh
npm install --prefix .glyphrush-baselines @llamaindex/liteparse@latest
GLYPHRUSH_BENCH_CATEGORY=native-text GLYPHRUSH_BENCH_MANIFEST=test/corpus.v0.json \
  GLYPHRUSH_BENCH_OUTPUT=.glyphrush-baselines/reports/liteparse-v0-native-text-strict.json \
  scripts/bench-liteparse.sh
cargo run -q --release -p glyphrush-cli --features pdfium -- --backend pdfium feature-parity \
  --bench-report .glyphrush-baselines/reports/liteparse-v0-native-text-strict.json \
  --require-speed-evidence --require-coverage-preset glyphrush-v0-native-text
```

The parity command must exit 0, and the README numbers must match the fresh report. Last verified: LiteParse 2.0.8 (2026-06-12), 110.19x default / 2.26x no-OCR, strict gate green.
