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
