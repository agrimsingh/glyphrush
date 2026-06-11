#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def resolve_lit() -> str | None:
    explicit = os.environ.get("LITEPARSE_BIN")
    if explicit:
        return explicit
    candidate = repo_root() / ".glyphrush-baselines/node_modules/.bin/lit"
    if candidate.is_file() and os.access(candidate, os.X_OK):
        return str(candidate)
    return shutil.which("lit")


def resolve_glyphrush_bin() -> str | None:
    explicit = os.environ.get("GLYPHRUSH_BIN")
    if explicit:
        return explicit
    candidate = repo_root() / "target/pdfium/debug/glyphrush"
    if candidate.is_file() and os.access(candidate, os.X_OK):
        return str(candidate)
    return shutil.which("glyphrush")


def page_count(pdf: Path) -> int | None:
    try:
        from pypdf import PdfReader

        return len(PdfReader(str(pdf)).pages)
    except Exception:
        try:
            import fitz

            with fitz.open(str(pdf)) as document:
                return document.page_count
        except Exception:
            return None


def median_seconds(samples: list[float]) -> float:
    return statistics.median(samples)


def summarize(parser: str, mode: str, samples: list[float]) -> dict:
    return {
        "mode": mode,
        "min_s": min(samples),
        "median_s": median_seconds(samples),
        "runs": len(samples),
    }


def bench_in_process(
    parser: str,
    warmup: int,
    runs: int,
    fn: Callable[[], None],
) -> dict:
    for _ in range(warmup):
        fn()
    samples: list[float] = []
    for _ in range(runs):
        start = time.perf_counter()
        fn()
        samples.append(time.perf_counter() - start)
    return summarize(parser, "in_process", samples)


def bench_subprocess(
    parser: str,
    warmup: int,
    runs: int,
    command: list[str],
) -> dict:
    for _ in range(warmup):
        subprocess.run(
            command,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
        )
    samples: list[float] = []
    for _ in range(runs):
        start = time.perf_counter()
        subprocess.run(
            command,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
        )
        samples.append(time.perf_counter() - start)
    return summarize(parser, "subprocess", samples)


def run_pymupdf(pdf: Path) -> None:
    import fitz

    with fitz.open(str(pdf)) as document:
        chunks = [page.get_text("text") or "" for page in document]
    if not any(chunks):
        raise RuntimeError("pymupdf produced no text")


def run_pypdf(pdf: Path) -> None:
    from pypdf import PdfReader

    reader = PdfReader(str(pdf))
    chunks = [page.extract_text() or "" for page in reader.pages]
    if not any(chunks):
        raise RuntimeError("pypdf produced no text")


def run_pymupdf4llm(pdf: Path) -> None:
    import pymupdf4llm

    text = pymupdf4llm.to_markdown(str(pdf)) or ""
    if not text.strip():
        raise RuntimeError("pymupdf4llm produced no text")


def run_markitdown(pdf: Path) -> None:
    from markitdown import MarkItDown

    text = MarkItDown().convert(str(pdf)).text_content or ""
    if not text.strip():
        raise RuntimeError("markitdown produced no text")


def opendataloader_command(pdf: Path) -> list[str]:
    wrapper = repo_root() / "tools/baselines/opendataloader-pdf-text.sh"
    return [str(wrapper), str(pdf)]


def available_parsers() -> list[str]:
    return [
        "pymupdf",
        "pypdf",
        "pymupdf4llm",
        "markitdown",
        "pdftotext",
        "glyphrush-cold",
        "liteparse-cold",
        "opendataloader",
    ]


def run_selected(
    parser: str,
    pdf: Path,
    warmup: int,
    runs: int,
) -> dict:
    if parser == "pymupdf":
        return bench_in_process(parser, warmup, runs, lambda: run_pymupdf(pdf))
    if parser == "pypdf":
        return bench_in_process(parser, warmup, runs, lambda: run_pypdf(pdf))
    if parser == "pymupdf4llm":
        return bench_in_process(parser, warmup, runs, lambda: run_pymupdf4llm(pdf))
    if parser == "markitdown":
        return bench_in_process(parser, warmup, runs, lambda: run_markitdown(pdf))
    if parser == "pdftotext":
        if not shutil.which("pdftotext"):
            raise RuntimeError("pdftotext not found on PATH")
        return bench_subprocess(
            parser,
            warmup,
            runs,
            ["pdftotext", "-q", str(pdf), "-"],
        )
    if parser == "glyphrush-cold":
        glyphrush = resolve_glyphrush_bin()
        if not glyphrush:
            raise RuntimeError("glyphrush binary not found; set GLYPHRUSH_BIN")
        return bench_subprocess(
            parser,
            warmup,
            runs,
            [glyphrush, "--backend", "pdfium", "parse", str(pdf), "--format", "text"],
        )
    if parser == "liteparse-cold":
        lit = resolve_lit()
        if not lit:
            raise RuntimeError("lit CLI not found; set LITEPARSE_BIN")
        return bench_subprocess(
            parser,
            warmup,
            runs,
            [lit, "parse", "--format", "text", "--quiet", "--no-ocr", str(pdf)],
        )
    if parser == "opendataloader":
        return bench_subprocess(parser, warmup, runs, opendataloader_command(pdf))
    raise KeyError(parser)


def main() -> int:
    parser = argparse.ArgumentParser(description="Warm-mode parser benchmark harness")
    parser.add_argument("--pdf", required=True, type=Path)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument(
        "--parsers",
        default=",".join(available_parsers()),
        help="Comma-separated parser names (default: all available)",
    )
    args = parser.parse_args()

    pdf = args.pdf.resolve()
    if not pdf.is_file():
        print(f"pdf does not exist: {pdf}", file=sys.stderr)
        return 66

    selected = [name.strip() for name in args.parsers.split(",") if name.strip()]
    results: dict[str, dict] = {}
    errors: dict[str, str] = {}

    for name in selected:
        try:
            results[name] = run_selected(name, pdf, args.warmup, args.runs)
        except Exception as exc:
            errors[name] = str(exc)

    report = {
        "pdf": str(pdf),
        "page_count": page_count(pdf),
        "results": results,
    }
    if errors:
        report["errors"] = errors

    json.dump(report, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
