# Contributing to Glyphrush

## Ground rules

Glyphrush makes public speed and quality claims, so the bar for changes is that the claims stay machine-verified:

1. **Never trade silent correctness for speed.** Pages that need OCR, uncertain layout, or ambiguous tables must be flagged, not returned as clean output.
2. **Quality gates are the contract.** If your change affects extraction, layout, OCR routing, or tables, the committed corpus gates must pass, and new behavior needs a labeled fixture or test that fails without your change.
3. **Benchmark claims need quality evidence from the same run.** Speed-only numbers are not accepted; see [docs/benchmarking.md](docs/benchmarking.md).

## Workflow

```sh
cargo test --workspace                              # fast; generates tiny PDFs at runtime
GLYPHRUSH_VERIFY_PDFIUM=1 bash scripts/verify.sh    # the full CI gate
```

`verify.sh` runs formatting, clippy with warnings denied, all workspace tests, the Python/Node wrapper tests, the committed corpus quality gates (`test/corpus.v0.json`, the span-geometry layout gate, the OCR sidecar gate), and the wasm artifact-parity test when wasm tooling is installed.

Pick one real parity gap or one real PDF failure per change, add the smallest failing fixture first, then the smallest core change that fixes it. The operating checklist in [docs/remaining-work.md](docs/remaining-work.md) describes the loop maintainers use.

## Code organization

Files stay under ~1,000 lines where practical; behavior lives in the crate that owns the concept (`glyphrush-core` for the artifact model and layout, `glyphrush-lopdf` for the dependency-light backend, the CLI for command surfaces and adapters). Bindings are thin wrappers over the native core and must never grow parser logic of their own.
