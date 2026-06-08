# Glyphrush Node Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate JavaScript parser.

```js
import { parse, parseText } from "glyphrush";

const artifact = parse("test/example.pdf", { binary: "target/debug/glyphrush" });
const text = parseText("test/example.pdf", { binary: "target/debug/glyphrush" });
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

Run wrapper tests with:

```sh
node --test bindings/node/test/client.test.mjs
```
