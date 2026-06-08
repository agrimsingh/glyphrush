from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any, Mapping, Sequence


class GlyphrushError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        command: Sequence[str],
        returncode: int,
        stdout: str,
        stderr: str,
    ) -> None:
        super().__init__(message)
        self.command = list(command)
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


def parse(
    pdf: str | os.PathLike[str],
    *,
    binary: str | os.PathLike[str] | None = None,
    backend: str | None = None,
    output_format: str = "json",
    span_geometry: bool = False,
    ocr_sidecar: str | os.PathLike[str] | None = None,
    ocr_command: str | os.PathLike[str] | None = None,
    ocr_http_url: str | None = None,
    ocr_command_input: str | None = None,
    ocr_timeout_ms: int | None = None,
    cache_dir: str | os.PathLike[str] | None = None,
    jobs: int | None = None,
    env: Mapping[str, str] | None = None,
) -> dict[str, Any] | str:
    command = _base_command(binary, backend)
    command.extend(["parse", _path(pdf), "--format", output_format])
    _append_common_options(
        command,
        span_geometry=span_geometry,
        ocr_sidecar=ocr_sidecar,
        ocr_command=ocr_command,
        ocr_http_url=ocr_http_url,
        ocr_command_input=ocr_command_input,
        ocr_timeout_ms=ocr_timeout_ms,
        cache_dir=cache_dir,
        jobs=jobs,
    )
    output = _run(command, env=env)
    if output_format == "json":
        return json.loads(output)
    return output


def parse_text(
    pdf: str | os.PathLike[str],
    *,
    binary: str | os.PathLike[str] | None = None,
    backend: str | None = None,
    env: Mapping[str, str] | None = None,
) -> str:
    return parse(
        pdf,
        binary=binary,
        backend=backend,
        output_format="text",
        env=env,
    )


def inspect_pages(
    pdf: str | os.PathLike[str],
    *,
    binary: str | os.PathLike[str] | None = None,
    backend: str | None = None,
    span_geometry: bool = False,
    ocr_sidecar: str | os.PathLike[str] | None = None,
    ocr_command: str | os.PathLike[str] | None = None,
    ocr_http_url: str | None = None,
    ocr_command_input: str | None = None,
    ocr_timeout_ms: int | None = None,
    cache_dir: str | os.PathLike[str] | None = None,
    jobs: int | None = None,
    env: Mapping[str, str] | None = None,
) -> dict[str, Any]:
    command = _base_command(binary, backend)
    command.extend(["inspect", _path(pdf), "--pages"])
    _append_common_options(
        command,
        span_geometry=span_geometry,
        ocr_sidecar=ocr_sidecar,
        ocr_command=ocr_command,
        ocr_http_url=ocr_http_url,
        ocr_command_input=ocr_command_input,
        ocr_timeout_ms=ocr_timeout_ms,
        cache_dir=cache_dir,
        jobs=jobs,
    )
    return json.loads(_run(command, env=env))


def eval_manifest(
    manifest: str | os.PathLike[str],
    *,
    binary: str | os.PathLike[str] | None = None,
    backend: str | None = None,
    category: str | None = None,
    span_geometry: bool = False,
    ocr_sidecar: str | os.PathLike[str] | None = None,
    ocr_command: str | os.PathLike[str] | None = None,
    ocr_http_url: str | None = None,
    ocr_command_input: str | None = None,
    ocr_timeout_ms: int | None = None,
    cache_dir: str | os.PathLike[str] | None = None,
    jobs: int | None = None,
    env: Mapping[str, str] | None = None,
) -> dict[str, Any]:
    command = _base_command(binary, backend)
    command.extend(["eval", _path(manifest)])
    if category is not None:
        command.extend(["--category", category])
    _append_common_options(
        command,
        span_geometry=span_geometry,
        ocr_sidecar=ocr_sidecar,
        ocr_command=ocr_command,
        ocr_http_url=ocr_http_url,
        ocr_command_input=ocr_command_input,
        ocr_timeout_ms=ocr_timeout_ms,
        cache_dir=cache_dir,
        jobs=jobs,
    )
    return json.loads(_run(command, env=env))


def _base_command(
    binary: str | os.PathLike[str] | None,
    backend: str | None,
) -> list[str]:
    command = [_path(binary) if binary is not None else os.environ.get("GLYPHRUSH_BIN", "glyphrush")]
    if backend is not None:
        command.extend(["--backend", backend])
    return command


def _append_common_options(
    command: list[str],
    *,
    span_geometry: bool,
    ocr_sidecar: str | os.PathLike[str] | None,
    ocr_command: str | os.PathLike[str] | None,
    ocr_http_url: str | None,
    ocr_command_input: str | None,
    ocr_timeout_ms: int | None,
    cache_dir: str | os.PathLike[str] | None,
    jobs: int | None,
) -> None:
    if span_geometry:
        command.append("--span-geometry")
    if ocr_sidecar is not None:
        command.extend(["--ocr-sidecar", _path(ocr_sidecar)])
    if ocr_command is not None:
        command.extend(["--ocr-command", _path(ocr_command)])
    if ocr_http_url is not None:
        command.extend(["--ocr-http-url", ocr_http_url])
    if ocr_command_input is not None:
        command.extend(["--ocr-command-input", ocr_command_input])
    if ocr_timeout_ms is not None:
        command.extend(["--ocr-timeout-ms", str(ocr_timeout_ms)])
    if cache_dir is not None:
        command.extend(["--cache-dir", _path(cache_dir)])
    if jobs is not None:
        command.extend(["--jobs", str(jobs)])


def _run(command: Sequence[str], *, env: Mapping[str, str] | None) -> str:
    completed = subprocess.run(
        list(command),
        capture_output=True,
        env=env,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        message = detail or f"glyphrush exited with status {completed.returncode}"
        raise GlyphrushError(
            message,
            command=command,
            returncode=completed.returncode,
            stdout=completed.stdout,
            stderr=completed.stderr,
        )
    return completed.stdout


def _path(path: str | os.PathLike[str]) -> str:
    return str(Path(path))


__all__ = ["GlyphrushError", "eval_manifest", "inspect_pages", "parse", "parse_text"]
