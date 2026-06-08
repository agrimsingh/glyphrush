# Glyphrush Python Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate Python parser.

```python
import glyphrush

artifact = glyphrush.parse("test/example.pdf", binary="target/debug/glyphrush")
text = glyphrush.parse_text("test/example.pdf", binary="target/debug/glyphrush")
triage = glyphrush.inspect_pages("test/example.pdf", binary="target/debug/glyphrush")
quality = glyphrush.eval_manifest("test/corpus.json", binary="target/debug/glyphrush")
speed = glyphrush.bench("test/example.pdf", binary="target/debug/glyphrush")
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

`inspect_pages()` delegates to `glyphrush inspect <pdf> --pages` and returns the native page-triage JSON, including routes, quality flags, OCR/layout/table diagnostics, cache status, and timing counters.

`eval_manifest()` delegates to `glyphrush eval <manifest>` and returns the native quality report, including silent-failure, text-recall, reading-order, table, category, and cache diagnostics when the manifest asks for them.

`bench()` delegates to `glyphrush bench <pdf-or-directory>` and returns the native speed report, including quality-backed baseline and speedup-claim fields when an eval manifest and baselines are requested.

Run wrapper tests with:

```sh
python3 -m unittest discover -s bindings/python/tests
```
