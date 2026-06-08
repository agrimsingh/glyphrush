# Glyphrush Python Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate Python parser.

```python
import glyphrush

artifact = glyphrush.parse("test/example.pdf", binary="target/debug/glyphrush")
text = glyphrush.parse_text("test/example.pdf", binary="target/debug/glyphrush")
markdown = glyphrush.parse_markdown("test/example.pdf", binary="target/debug/glyphrush")
triage = glyphrush.inspect_pages("test/example.pdf", binary="target/debug/glyphrush")
page = glyphrush.debug_page("test/example.pdf", 0, binary="target/debug/glyphrush")
ocr = glyphrush.ocr_check("test/example.pdf", page_index=0, binary="target/debug/glyphrush")
backend = glyphrush.backend_check(pdf="test/", binary="target/debug/glyphrush")
baselines = glyphrush.baseline_check(binary="target/debug/glyphrush", baseline_preset="glyphrush-v0")
parity = glyphrush.feature_parity(
    binary="target/debug/glyphrush",
    bench_report=".glyphrush-baselines/reports/liteparse-speed-gate.json",
    require_speed_evidence=True,
    require_coverage_preset="glyphrush-v0",
)
quality = glyphrush.eval_manifest("test/corpus.json", binary="target/debug/glyphrush")
speed = glyphrush.bench("test/example.pdf", binary="target/debug/glyphrush")
generated = glyphrush.manifest("test/", binary="target/debug/glyphrush", category="datasheet")
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

`parse_text()` and `parse_markdown()` return the native CLI derived text views without JSON decoding.

`inspect_pages()` delegates to `glyphrush inspect <pdf> --pages` and returns the native page-triage JSON, including routes, quality flags, OCR/layout/table diagnostics, cache status, and timing counters.

`debug_page()` delegates to `glyphrush debug-page <pdf> <page-index>` and returns the native single-page diagnostic JSON.

`ocr_check()`, `backend_check()`, and `baseline_check()` delegate to the native preflight surfaces for OCR adapters, parser backends, and external comparison wrappers.

`feature_parity()` delegates to `glyphrush feature-parity` and returns the conservative LiteParse capability matrix. Pass `bench_report` with `require_speed_evidence=True` to require the saved benchmark report to contain passing, quality-backed `liteparse` and `liteparse-no-ocr` speedup claims. Add `require_coverage_preset="glyphrush-v0"` to also fail unless the saved benchmark covers the core v0 PDF categories.

`eval_manifest()` delegates to `glyphrush eval <manifest>` and returns the native quality report, including silent-failure, text-recall, reading-order, table, category, and cache diagnostics when the manifest asks for them.

`bench()` delegates to `glyphrush bench <pdf-or-directory>` and returns the native speed report, including quality-backed baseline and speedup-claim fields when an eval manifest and baselines are requested.

`manifest()` delegates to `glyphrush manifest <pdf-or-directory>` and returns the native eval-manifest skeleton, including category coverage gates and deterministic document/page expectations for dropped PDFs.

Run wrapper tests with:

```sh
python3 -m unittest discover -s bindings/python/tests
```
