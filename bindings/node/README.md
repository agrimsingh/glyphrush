# Glyphrush Node Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate JavaScript parser.

```js
import { inspectPages, parse, parseText } from "glyphrush";

const artifact = parse("test/example.pdf", { binary: "target/debug/glyphrush" });
const text = parseText("test/example.pdf", { binary: "target/debug/glyphrush" });
const triage = inspectPages("test/example.pdf", { binary: "target/debug/glyphrush" });
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

`inspectPages()` delegates to `glyphrush inspect <pdf> --pages` and returns the native page-triage JSON, including routes, quality flags, OCR/layout/table diagnostics, cache status, and timing counters.

Run wrapper tests with:

```sh
node --test bindings/node/test/client.test.mjs
```
