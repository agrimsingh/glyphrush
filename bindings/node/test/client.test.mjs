import assert from "node:assert/strict";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { GlyphrushError, parse, parseText } from "../src/index.mjs";

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
