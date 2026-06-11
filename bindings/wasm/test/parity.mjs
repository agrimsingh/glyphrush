import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const defaultPdf = path.join(
  repoRoot,
  "test/v0/weird_encoding/verapdf-cmap-fail-a.pdf",
);
const cliBinary = path.join(repoRoot, "target/debug/glyphrush");
const pkgDir = path.join(repoRoot, "bindings/wasm/pkg");

function parseArgs(argv) {
  let pdf = defaultPdf;
  let spanGeometry = false;

  for (const arg of argv) {
    if (arg === "--span-geometry") {
      spanGeometry = true;
      continue;
    }
    if (arg.startsWith("-")) {
      throw new Error(`unknown argument: ${arg}`);
    }
    pdf = path.isAbsolute(arg) ? arg : path.join(repoRoot, arg);
  }

  return { pdf, spanGeometry };
}

function stripForParity(value) {
  if (Array.isArray(value)) {
    return value.map(stripForParity);
  }
  if (value && typeof value === "object") {
    const out = {};
    for (const [key, child] of Object.entries(value)) {
      if (key === "timings") {
        continue;
      }
      out[key] = stripForParity(child);
    }
    if (out.global_diagnostics && typeof out.global_diagnostics === "object") {
      delete out.global_diagnostics.total_stage_time_us;
    }
    if (out.metadata && typeof out.metadata === "object") {
      delete out.metadata.source_modified_unix_ms;
      delete out.metadata.parser_version;
    }
    return out;
  }
  return value;
}

function findFirstDiff(left, right, currentPath = "$") {
  if (Object.is(left, right)) {
    return null;
  }
  if (typeof left !== typeof right) {
    return `${currentPath} (type ${typeof left} vs ${typeof right})`;
  }
  if (left === null || right === null || typeof left !== "object") {
    return `${currentPath} (${JSON.stringify(left)} vs ${JSON.stringify(right)})`;
  }
  if (Array.isArray(left) !== Array.isArray(right)) {
    return `${currentPath} (array mismatch)`;
  }
  if (Array.isArray(left)) {
    const length = Math.max(left.length, right.length);
    for (let index = 0; index < length; index += 1) {
      const diff = findFirstDiff(left[index], right[index], `${currentPath}[${index}]`);
      if (diff) {
        return diff;
      }
    }
    return null;
  }
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of [...keys].sort()) {
    const diff = findFirstDiff(left[key], right[key], `${currentPath}.${key}`);
    if (diff) {
      return diff;
    }
  }
  return null;
}

function runCli(pdf, spanGeometry) {
  const args = ["--backend", "lopdf", "parse", pdf, "--format", "json"];
  if (spanGeometry) {
    args.push("--span-geometry");
  }
  const completed = spawnSync(cliBinary, args, {
    encoding: "utf8",
    cwd: repoRoot,
  });
  if (completed.error) {
    throw completed.error;
  }
  if (completed.status !== 0) {
    throw new Error(
      completed.stderr.trim() || completed.stdout.trim() || `glyphrush exited ${completed.status}`,
    );
  }
  return JSON.parse(completed.stdout);
}

async function runWasm(pdf, spanGeometry) {
  const { parse_pdf_bytes } = require(path.join(pkgDir, "glyphrush_wasm.js"));
  const bytes = readFileSync(pdf);
  const json = parse_pdf_bytes(new Uint8Array(bytes), spanGeometry);
  return JSON.parse(json);
}

async function comparePdf(pdf, spanGeometry) {
  const label = `${path.relative(repoRoot, pdf)}${spanGeometry ? " (span-geometry)" : ""}`;
  const wasmArtifact = await runWasm(pdf, spanGeometry);
  const cliArtifact = runCli(pdf, spanGeometry);
  const diff = findFirstDiff(stripForParity(wasmArtifact), stripForParity(cliArtifact));
  if (diff) {
    console.error(`FAIL ${label}: first difference at ${diff}`);
    process.exitCode = 1;
    return false;
  }
  console.log(`PASS ${label}`);
  return true;
}

async function main() {
  const { pdf, spanGeometry } = parseArgs(process.argv.slice(2));
  const ok = await comparePdf(pdf, spanGeometry);
  if (!ok) {
    process.exit(1);
  }
}

await main();
