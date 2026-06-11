#!/usr/bin/env node
/**
 * Warm-mode LiteParse in-process benchmark driver.
 *
 * Usage:
 *   node tools/bench/warm_bench.mjs --pdf test/extended/gsa-1page.pdf --runs 5 --warmup 1
 */

import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const require = createRequire(import.meta.url);
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const liteparseRoot = path.join(
  repoRoot,
  ".glyphrush-baselines/node_modules/@llamaindex/liteparse",
);

function parseArgs(argv) {
  const args = { runs: 5, warmup: 1, pdf: null };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === "--pdf") {
      args.pdf = argv[++index];
    } else if (token === "--runs") {
      args.runs = Number(argv[++index]);
    } else if (token === "--warmup") {
      args.warmup = Number(argv[++index]);
    } else if (token === "--help" || token === "-h") {
      console.error(
        "usage: warm_bench.mjs --pdf <path> [--runs N] [--warmup K]",
      );
      process.exit(64);
    }
  }
  if (!args.pdf) {
    console.error("missing required --pdf");
    process.exit(64);
  }
  return args;
}

function median(samples) {
  const sorted = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 0) {
    return (sorted[middle - 1] + sorted[middle]) / 2;
  }
  return sorted[middle];
}

async function loadLiteParse() {
  const modulePath = path.join(liteparseRoot, "dist/lib.js");
  return import(modulePath);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const pdfPath = path.resolve(args.pdf);

  const { LiteParse } = await loadLiteParse();
  const parser = new LiteParse({
    ocrEnabled: false,
    outputFormat: "text",
    quiet: true,
  });

  async function runOnce() {
    const result = await parser.parse(pdfPath);
    if (!result.text || !result.text.trim()) {
      throw new Error("liteparse produced no text");
    }
  }

  for (let index = 0; index < args.warmup; index += 1) {
    await runOnce();
  }

  const samples = [];
  for (let index = 0; index < args.runs; index += 1) {
    const start = performance.now();
    await runOnce();
    samples.push((performance.now() - start) / 1000);
  }

  const report = {
    parser: "liteparse",
    mode: "in_process",
    min_s: Math.min(...samples),
    median_s: median(samples),
    runs: samples.length,
  };

  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
