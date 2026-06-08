# Glyphrush Node Wrapper

This package is a thin wrapper over the native `glyphrush` CLI. It delegates parsing to the shared core and decodes the CLI JSON artifact instead of implementing a separate JavaScript parser.

```js
import {
  backendCheck,
  baselineCheck,
  bench,
  debugPage,
  evalManifest,
  featureParity,
  inspectPages,
  manifest,
  ocrCheck,
  parse,
  parseMarkdown,
  parseText,
} from "glyphrush";

const artifact = parse("test/example.pdf", { binary: "target/debug/glyphrush" });
const text = parseText("test/example.pdf", { binary: "target/debug/glyphrush" });
const markdown = parseMarkdown("test/example.pdf", { binary: "target/debug/glyphrush" });
const triage = inspectPages("test/example.pdf", { binary: "target/debug/glyphrush" });
const page = debugPage("test/example.pdf", 0, { binary: "target/debug/glyphrush" });
const ocr = ocrCheck("test/example.pdf", { pageIndex: 0, binary: "target/debug/glyphrush" });
const backend = backendCheck({ pdf: "test/", binary: "target/debug/glyphrush" });
const baselines = baselineCheck({ binary: "target/debug/glyphrush", baselinePreset: "glyphrush-v0" });
const parity = featureParity({
  binary: "target/debug/glyphrush",
  benchReport: ".glyphrush-baselines/reports/liteparse-speed-gate.json",
  requireSpeedEvidence: true,
  requireCoveragePreset: "glyphrush-v0",
});
const quality = evalManifest("test/corpus.json", { binary: "target/debug/glyphrush" });
const speed = bench("test/", {
  binary: "target/debug/glyphrush",
  evalManifest: "test/corpus.json",
  baselinePreset: "glyphrush-v0",
  requireCoveragePreset: "glyphrush-v0",
  requireSpeedupClaim: ["liteparse=2.0", "liteparse-no-ocr=1.5"],
});
const generated = manifest("test/", { binary: "target/debug/glyphrush", category: "datasheet" });
const full = manifest("test/", {
  binary: "target/debug/glyphrush",
  categoryFromPath: true,
  coveragePreset: "glyphrush-v0",
});
```

If `binary` is omitted, the wrapper uses `GLYPHRUSH_BIN` and then falls back to `glyphrush` on `PATH`.

`parseText()` and `parseMarkdown()` return the native CLI derived text views without JSON decoding.

`inspectPages()` delegates to `glyphrush inspect <pdf> --pages` and returns the native page-triage JSON, including routes, quality flags, OCR/layout/table diagnostics, cache status, and timing counters.

`debugPage()` delegates to `glyphrush debug-page <pdf> <page-index>` and returns the native single-page diagnostic JSON.

`ocrCheck()`, `backendCheck()`, and `baselineCheck()` delegate to the native preflight surfaces for OCR adapters, parser backends, and external comparison wrappers.

`featureParity()` delegates to `glyphrush feature-parity` and returns the conservative LiteParse capability matrix. Pass `benchReport` with `requireSpeedEvidence: true` to require the saved benchmark report to contain passing, quality-backed `liteparse` and `liteparse-no-ocr` speedup claims. Add `requireCoveragePreset: "glyphrush-v0"` to also fail unless the saved benchmark covers the core v0 PDF categories.

`evalManifest()` delegates to `glyphrush eval <manifest>` and returns the native quality report, including silent-failure, text-recall, reading-order, table, category, and cache diagnostics when the manifest asks for them.

`bench()` delegates to `glyphrush bench <pdf-or-directory>` and returns the native speed report, including quality-backed baseline, corpus coverage, and speedup-claim fields when an eval manifest and baselines are requested. Pass `requireCoveragePreset: "glyphrush-v0"` to require the same v0 corpus coverage gate as the CLI.

`manifest()` delegates to `glyphrush manifest <pdf-or-directory>` and returns the native eval-manifest skeleton, including category coverage gates and deterministic document/page expectations for dropped PDFs. Use `categoryFromPath: true` for mixed corpora organized as `test/<category>/<file>.pdf`.

Run wrapper tests with:

```sh
node --test bindings/node/test/client.test.mjs
```
