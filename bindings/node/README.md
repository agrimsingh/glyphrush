# Glyphrush Node Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate JavaScript parser.

```js
import { evalManifest, inspectPages, parse, parseText } from "glyphrush";

const artifact = parse("test/example.pdf", { binary: "target/debug/glyphrush" });
const text = parseText("test/example.pdf", { binary: "target/debug/glyphrush" });
const triage = inspectPages("test/example.pdf", { binary: "target/debug/glyphrush" });
const quality = evalManifest("test/corpus.json", { binary: "target/debug/glyphrush" });
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

`inspectPages()` delegates to `glyphrush inspect <pdf> --pages` and returns the native page-triage JSON, including routes, quality flags, OCR/layout/table diagnostics, cache status, and timing counters.

`evalManifest()` delegates to `glyphrush eval <manifest>` and returns the native quality report, including silent-failure, text-recall, reading-order, table, category, and cache diagnostics when the manifest asks for them.

Run wrapper tests with:

```sh
node --test bindings/node/test/client.test.mjs
```
