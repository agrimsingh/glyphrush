# Glyphrush Python Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate Python parser.

```python
import glyphrush

artifact = glyphrush.parse("test/example.pdf", binary="target/debug/glyphrush")
text = glyphrush.parse_text("test/example.pdf", binary="target/debug/glyphrush")
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

Run wrapper tests with:

```sh
python3 -m unittest discover -s bindings/python/tests
```
