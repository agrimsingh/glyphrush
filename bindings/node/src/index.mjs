import { spawnSync } from "node:child_process";
import process from "node:process";

export class GlyphrushError extends Error {
  constructor(message, { command, status, stdout, stderr }) {
    super(message);
    this.name = "GlyphrushError";
    this.command = [...command];
    this.status = status;
    this.stdout = stdout;
    this.stderr = stderr;
  }
}

export function parse(pdf, options = {}) {
  const outputFormat = options.outputFormat ?? "json";
  const command = baseCommand(options);
  command.push("parse", pathString(pdf), "--format", outputFormat);
  appendCommonOptions(command, options);
  const output = run(command, options.env);
  if (outputFormat === "json") {
    return JSON.parse(output);
  }
  return output;
}

export function parseText(pdf, options = {}) {
  return parse(pdf, { ...options, outputFormat: "text" });
}

export function inspectPages(pdf, options = {}) {
  const command = baseCommand(options);
  command.push("inspect", pathString(pdf), "--pages");
  appendCommonOptions(command, options);
  return JSON.parse(run(command, options.env));
}

export function evalManifest(manifest, options = {}) {
  const command = baseCommand(options);
  command.push("eval", pathString(manifest));
  if (options.category !== undefined) {
    command.push("--category", options.category);
  }
  appendCommonOptions(command, options);
  return JSON.parse(run(command, options.env));
}

export function manifest(pdf, options = {}) {
  const command = baseCommand(options);
  command.push("manifest", pathString(pdf));
  if (options.category !== undefined) {
    command.push("--category", options.category);
  }
  if (options.coveragePreset !== undefined) {
    command.push("--coverage-preset", options.coveragePreset);
  }
  appendRepeated(command, "--required-category", options.requiredCategory);
  appendRepeated(command, "--min-category-count", options.minCategoryCount);
  appendCommonOptions(command, options);
  return JSON.parse(run(command, options.env));
}

export function bench(pdf, options = {}) {
  const command = baseCommand(options);
  command.push("bench", pathString(pdf));
  if (options.evalManifest !== undefined) {
    command.push("--eval-manifest", pathString(options.evalManifest));
  }
  if (options.evalCategory !== undefined) {
    command.push("--eval-category", options.evalCategory);
  }
  if (options.baselinePreset !== undefined) {
    command.push("--baseline-preset", options.baselinePreset);
  }
  if (options.requireQuality) {
    command.push("--require-quality");
  }
  if (options.requireBaselines) {
    command.push("--require-baselines");
  }
  if (options.requireBaselineQuality) {
    command.push("--require-baseline-quality");
  }
  appendRepeated(command, "--require-speedup", options.requireSpeedup);
  appendRepeated(command, "--require-speedup-claim", options.requireSpeedupClaim);
  appendRepeated(command, "--baseline", options.baseline);
  if (options.cacheProbe) {
    command.push("--cache-probe");
  }
  if (options.baselineTimeoutMs !== undefined) {
    command.push("--baseline-timeout-ms", String(options.baselineTimeoutMs));
  }
  appendCommonOptions(command, options);
  return JSON.parse(run(command, options.env));
}

export function backendCheck(options = {}) {
  const command = baseCommand(options);
  command.push("backend-check");
  if (options.pdf !== undefined) {
    command.push("--pdf", pathString(options.pdf));
  }
  if (options.jobs !== undefined) {
    command.push("--jobs", String(options.jobs));
  }
  return JSON.parse(run(command, options.env));
}

export function debugPage(pdf, pageIndex, options = {}) {
  const command = baseCommand(options);
  command.push("debug-page", pathString(pdf), String(pageIndex));
  appendCommonOptions(command, {
    ...options,
    cacheDir: undefined,
    jobs: undefined,
  });
  return JSON.parse(run(command, options.env));
}

export function ocrCheck(pdf, options = {}) {
  const command = baseCommand(options);
  command.push("ocr-check", pathString(pdf), "--page-index", String(options.pageIndex));
  appendCommonOptions(command, {
    ...options,
    spanGeometry: false,
    cacheDir: undefined,
    jobs: undefined,
  });
  if (options.strict) {
    command.push("--strict");
  }
  return JSON.parse(run(command, options.env));
}

export function featureParity(options = {}) {
  const command = baseCommand(options);
  command.push("feature-parity");
  if (options.benchReport !== undefined) {
    command.push("--bench-report", pathString(options.benchReport));
  }
  if (options.requireSpeedEvidence) {
    command.push("--require-speed-evidence");
  }
  return JSON.parse(run(command, options.env));
}

export function baselineCheck(options = {}) {
  const command = baseCommand(options);
  command.push("baseline-check");
  if (options.baselinePreset !== undefined) {
    command.push("--baseline-preset", options.baselinePreset);
  }
  appendRepeated(command, "--baseline", options.baseline);
  if (options.pdf !== undefined) {
    command.push("--pdf", pathString(options.pdf));
  }
  if (options.baselineTimeoutMs !== undefined) {
    command.push("--baseline-timeout-ms", String(options.baselineTimeoutMs));
  }
  if (options.strict) {
    command.push("--strict");
  }
  return JSON.parse(run(command, options.env));
}

function baseCommand(options) {
  const command = [pathString(options.binary ?? process.env.GLYPHRUSH_BIN ?? "glyphrush")];
  if (options.backend !== undefined) {
    command.push("--backend", options.backend);
  }
  return command;
}

function appendCommonOptions(command, options) {
  if (options.spanGeometry) {
    command.push("--span-geometry");
  }
  if (options.ocrSidecar !== undefined) {
    command.push("--ocr-sidecar", pathString(options.ocrSidecar));
  }
  if (options.ocrCommand !== undefined) {
    command.push("--ocr-command", pathString(options.ocrCommand));
  }
  if (options.ocrHttpUrl !== undefined) {
    command.push("--ocr-http-url", options.ocrHttpUrl);
  }
  if (options.ocrCommandInput !== undefined) {
    command.push("--ocr-command-input", options.ocrCommandInput);
  }
  if (options.ocrTimeoutMs !== undefined) {
    command.push("--ocr-timeout-ms", String(options.ocrTimeoutMs));
  }
  if (options.cacheDir !== undefined) {
    command.push("--cache-dir", pathString(options.cacheDir));
  }
  if (options.jobs !== undefined) {
    command.push("--jobs", String(options.jobs));
  }
}

function appendRepeated(command, flag, values) {
  if (values === undefined) {
    return;
  }
  for (const value of values) {
    command.push(flag, String(value));
  }
}

function run(command, env) {
  const completed = spawnSync(command[0], command.slice(1), {
    encoding: "utf8",
    env,
  });
  if (completed.error !== undefined) {
    throw new GlyphrushError(completed.error.message, {
      command,
      status: null,
      stdout: completed.stdout ?? "",
      stderr: completed.stderr ?? "",
    });
  }
  if (completed.status !== 0) {
    const detail = completed.stderr.trim() || completed.stdout.trim();
    throw new GlyphrushError(detail || `glyphrush exited with status ${completed.status}`, {
      command,
      status: completed.status,
      stdout: completed.stdout,
      stderr: completed.stderr,
    });
  }
  return completed.stdout;
}

function pathString(value) {
  return String(value);
}
