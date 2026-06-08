import assert from "node:assert/strict";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { GlyphrushError, parse, parseMarkdown, parseText } from "../src/index.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

async function withTempDir(fn) {
  const root = await mkdtemp(path.join(tmpdir(), "glyphrush-node-test-"));
  try {
    return await fn(root);
  } finally {
    await rm(root, { force: true, recursive: true });
  }
}

async function writeFakeGlyphrush(root) {
  const script = path.join(root, "glyphrush");
  await writeFile(
    script,
    [
      `#!${process.execPath}`,
      'import process from "node:process";',
      "",
      'if (process.env.GLYPHRUSH_FAKE_FAIL === "1") {',
      '  console.error("fake failure from glyphrush");',
      "  process.exit(7);",
      "}",
      "",
      'const formatIndex = process.argv.indexOf("--format");',
      'if (formatIndex !== -1 && process.argv[formatIndex + 1] === "text") {',
      '  console.log("fake text output");',
      '} else if (formatIndex !== -1 && process.argv[formatIndex + 1] === "markdown") {',
      '  console.log("# fake markdown output");',
      "} else {",
      "  console.log(JSON.stringify({ argv: process.argv.slice(2) }));",
      "}",
      "",
    ].join("\n"),
  );
  await chmod(script, 0o755);
  return script;
}

test("parse delegates to native CLI and decodes JSON artifacts", async () => {
  await withTempDir(async (root) => {
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    await writeFile(pdf, "%PDF-1.4 fake");

    const artifact = parse(pdf, {
      binary: fake,
      backend: "lopdf",
      spanGeometry: true,
      cacheDir: path.join(root, "cache"),
      jobs: 2,
    });

    assert.deepEqual(artifact.argv, [
      "--backend",
      "lopdf",
      "parse",
      pdf,
      "--format",
      "json",
      "--span-geometry",
      "--cache-dir",
      path.join(root, "cache"),
      "--jobs",
      "2",
    ]);
  });
});

test("parseText returns stdout without JSON decoding", async () => {
  await withTempDir(async (root) => {
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    await writeFile(pdf, "%PDF-1.4 fake");

    assert.equal(parseText(pdf, { binary: fake }), "fake text output\n");
  });
});

test("parseMarkdown returns stdout without JSON decoding", async () => {
  await withTempDir(async (root) => {
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    await writeFile(pdf, "%PDF-1.4 fake");

    assert.equal(parseMarkdown(pdf, { binary: fake }), "# fake markdown output\n");
  });
});

test("inspectPages delegates to native page triage and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { inspectPages } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    await writeFile(pdf, "%PDF-1.4 fake");

    const report = inspectPages(pdf, {
      binary: fake,
      backend: "lopdf",
      cacheDir: path.join(root, "cache"),
      jobs: 3,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "inspect",
      pdf,
      "--pages",
      "--cache-dir",
      path.join(root, "cache"),
      "--jobs",
      "3",
    ]);
  });
});

test("evalManifest delegates to native quality gate and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { evalManifest } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const manifest = path.join(root, "corpus.json");
    await writeFile(manifest, '{"documents":[]}');

    const report = evalManifest(manifest, {
      binary: fake,
      backend: "lopdf",
      category: "datasheet",
      spanGeometry: true,
      cacheDir: path.join(root, "cache"),
      jobs: 4,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "eval",
      manifest,
      "--category",
      "datasheet",
      "--span-geometry",
      "--cache-dir",
      path.join(root, "cache"),
      "--jobs",
      "4",
    ]);
  });
});

test("manifest delegates to native corpus generator and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { mkdir } = await import("node:fs/promises");
    const { manifest } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdfDir = path.join(root, "pdfs");
    await mkdir(pdfDir);

    const report = manifest(pdfDir, {
      binary: fake,
      backend: "lopdf",
      category: "datasheet",
      categoryFromPath: true,
      coveragePreset: "glyphrush-v0",
      requiredCategory: ["datasheet", "scanned"],
      minCategoryCount: ["datasheet=5"],
      spanGeometry: true,
      cacheDir: path.join(root, "cache"),
      jobs: 4,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "manifest",
      pdfDir,
      "--category",
      "datasheet",
      "--category-from-path",
      "--coverage-preset",
      "glyphrush-v0",
      "--required-category",
      "datasheet",
      "--required-category",
      "scanned",
      "--min-category-count",
      "datasheet=5",
      "--span-geometry",
      "--cache-dir",
      path.join(root, "cache"),
      "--jobs",
      "4",
    ]);
  });
});

test("bench delegates to native quality-backed speed gate and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { bench } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    const manifest = path.join(root, "corpus.json");
    await writeFile(pdf, "%PDF-1.4 fake");
    await writeFile(manifest, '{"documents":[]}');

    const report = bench(pdf, {
      binary: fake,
      backend: "lopdf",
      evalManifest: manifest,
      evalCategory: "datasheet",
      baselinePreset: "glyphrush-v0",
      requireQuality: true,
      requireBaselines: true,
      requireBaselineQuality: true,
      requireCoveragePreset: "glyphrush-v0",
      requireSpeedupClaim: ["liteparse=2.0", "liteparse-no-ocr=1.5"],
      cacheProbe: true,
      baselineTimeoutMs: 1234,
      cacheDir: path.join(root, "cache"),
      jobs: 2,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "bench",
      pdf,
      "--eval-manifest",
      manifest,
      "--eval-category",
      "datasheet",
      "--baseline-preset",
      "glyphrush-v0",
      "--require-quality",
      "--require-baselines",
      "--require-baseline-quality",
      "--require-coverage-preset",
      "glyphrush-v0",
      "--require-speedup-claim",
      "liteparse=2.0",
      "--require-speedup-claim",
      "liteparse-no-ocr=1.5",
      "--cache-probe",
      "--baseline-timeout-ms",
      "1234",
      "--cache-dir",
      path.join(root, "cache"),
      "--jobs",
      "2",
    ]);
  });
});

test("backendCheck delegates to native preflight and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { mkdir } = await import("node:fs/promises");
    const { backendCheck } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdfDir = path.join(root, "pdfs");
    await mkdir(pdfDir);

    const report = backendCheck({
      pdf: pdfDir,
      binary: fake,
      backend: "lopdf",
      jobs: 4,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "backend-check",
      "--pdf",
      pdfDir,
      "--jobs",
      "4",
    ]);
  });
});

test("debugPage delegates to native page debugger and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { mkdir } = await import("node:fs/promises");
    const { debugPage } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    const ocr = path.join(root, "ocr");
    await writeFile(pdf, "%PDF-1.4 fake");
    await mkdir(ocr);

    const report = debugPage(pdf, 3, {
      binary: fake,
      backend: "lopdf",
      spanGeometry: true,
      ocrSidecar: ocr,
      ocrTimeoutMs: 2500,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "debug-page",
      pdf,
      "3",
      "--span-geometry",
      "--ocr-sidecar",
      ocr,
      "--ocr-timeout-ms",
      "2500",
    ]);
  });
});

test("ocrCheck delegates to native adapter preflight and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { ocrCheck } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    const command = path.join(root, "ocr.sh");
    await writeFile(pdf, "%PDF-1.4 fake");
    await writeFile(command, "#!/bin/sh\nprintf OCR\n");

    const report = ocrCheck(pdf, {
      pageIndex: 2,
      binary: fake,
      backend: "pdfium",
      ocrCommand: command,
      ocrCommandInput: "rendered-image",
      ocrTimeoutMs: 1500,
      strict: true,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "pdfium",
      "ocr-check",
      pdf,
      "--page-index",
      "2",
      "--ocr-command",
      command,
      "--ocr-command-input",
      "rendered-image",
      "--ocr-timeout-ms",
      "1500",
      "--strict",
    ]);
  });
});

test("featureParity delegates to native report and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { featureParity } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);

    const report = featureParity({
      binary: fake,
      backend: "lopdf",
      benchReport: path.join(root, "bench.json"),
      requireSpeedEvidence: true,
      requireCoveragePreset: "glyphrush-v0",
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "feature-parity",
      "--bench-report",
      path.join(root, "bench.json"),
      "--require-speed-evidence",
      "--require-coverage-preset",
      "glyphrush-v0",
    ]);
  });
});

test("baselineCheck delegates to native baseline preflight and decodes JSON", async () => {
  await withTempDir(async (root) => {
    const { mkdir } = await import("node:fs/promises");
    const { baselineCheck } = await import("../src/index.mjs");
    const fake = await writeFakeGlyphrush(root);
    const pdfDir = path.join(root, "pdfs");
    await mkdir(pdfDir);

    const report = baselineCheck({
      binary: fake,
      backend: "lopdf",
      pdf: pdfDir,
      baselinePreset: "glyphrush-v0",
      baseline: ["custom=/bin/echo"],
      baselineTimeoutMs: 4321,
      strict: true,
    });

    assert.deepEqual(report.argv, [
      "--backend",
      "lopdf",
      "baseline-check",
      "--baseline-preset",
      "glyphrush-v0",
      "--baseline",
      "custom=/bin/echo",
      "--pdf",
      pdfDir,
      "--baseline-timeout-ms",
      "4321",
      "--strict",
    ]);
  });
});

test("CLI failures raise GlyphrushError with exit status and stderr", async () => {
  await withTempDir(async (root) => {
    const fake = await writeFakeGlyphrush(root);
    const pdf = path.join(root, "sample.pdf");
    await writeFile(pdf, "%PDF-1.4 fake");

    assert.throws(
      () =>
        parse(pdf, {
          binary: fake,
          env: { ...process.env, GLYPHRUSH_FAKE_FAIL: "1" },
        }),
      (error) => {
        assert.ok(error instanceof GlyphrushError);
        assert.equal(error.status, 7);
        assert.match(error.message, /fake failure/);
        assert.match(error.stderr, /fake failure/);
        assert.deepEqual(error.command.slice(0, 1), [fake]);
        return true;
      },
    );
  });
});

test("tests import package source relative to this test file", () => {
  assert.equal(path.basename(__dirname), "test");
});
