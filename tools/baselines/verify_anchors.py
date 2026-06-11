#!/usr/bin/env python3
"""Verify and repair eval-manifest page anchors against baseline outputs.

Generated required-text anchors are picked from Glyphrush's own extraction and
can encode backend-specific quirks that external text baselines never
reproduce. This lever checks every page anchor in a manifest against captured
baseline stdout (squashed comparison: whitespace and control characters
removed) and, with --repair, replaces failing anchors with a span-level line
verified against every captured baseline. Pages with no universally
reproducible line keep their original anchor; those represent genuine
extraction differences, not labeling artifacts.

Usage:
  1. Capture baseline outputs:
       for f in test/v0/*/*.pdf; do n=$(basename "$f" .pdf); \
         for w in liteparse-text pymupdf-text pdfplumber-text; do \
           tools/baselines/$w.sh "$f" > "$OUT/$n.$w.txt"; done; done
  2. Report:  tools/baselines/verify_anchors.py test/corpus.v0.json "$OUT"
  3. Repair:  tools/baselines/verify_anchors.py test/corpus.v0.json "$OUT" \
                --repair --glyphrush target/debug/glyphrush --pdf-root test
"""

import argparse
import glob
import json
import os
import subprocess
import sys


def squash(text):
    return "".join(
        c
        for c in text
        if c.isalnum() or (c.isascii() and c.isprintable() and not c.isspace())
    )


def load_baseline_outputs(directory):
    outputs = {}
    for path in glob.glob(os.path.join(directory, "*.txt")):
        stem, wrapper = os.path.basename(path).split(".", 1)
        wrapper = wrapper[: -len(".txt")]
        with open(path, errors="replace") as handle:
            outputs.setdefault(stem, {})[wrapper] = squash(handle.read())
    return outputs


def anchor_misses(manifest, outputs):
    misses = []
    for document in manifest["documents"]:
        stem = os.path.basename(document["path"])[: -len(".pdf")]
        document_outputs = outputs.get(stem, {})
        for page in document["expect"].get("pages", []):
            for anchor in page.get("required_text", []):
                squashed = squash(anchor)
                missed_by = [
                    wrapper
                    for wrapper, text in document_outputs.items()
                    if squashed not in text
                ]
                if missed_by:
                    misses.append((document, page, anchor, missed_by))
    return misses


def replacement_anchor(spans, document_outputs, document_line_counts):
    fallback = None
    for span in spans:
        line = span["text"].strip()
        if not 12 <= len(line) <= 80:
            continue
        if any(ch == "|" or ord(ch) < 32 for ch in line):
            continue
        squashed = squash(line)
        if len(squashed) < 12:
            continue
        if not all(squashed in text for text in document_outputs.values()):
            continue
        words = line.split()
        letters = sum(c.isalpha() for c in line)
        alnum = sum(c.isalnum() for c in line) or 1
        if len(words) >= 3 and letters * 10 >= alnum * 7:
            # Prefer lines unique to this page; a running header or footer
            # repeated across pages pins nothing about page content.
            if document_line_counts.get(squashed, 0) <= 2:
                return line
            if fallback is None:
                fallback = line
    return fallback


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest")
    parser.add_argument("baseline_output_dir")
    parser.add_argument("--repair", action="store_true")
    parser.add_argument("--glyphrush", default="target/debug/glyphrush")
    parser.add_argument("--pdf-root", default="test")
    args = parser.parse_args()

    manifest = json.load(open(args.manifest))
    outputs = load_baseline_outputs(args.baseline_output_dir)
    misses = anchor_misses(manifest, outputs)

    for document, page, anchor, missed_by in misses:
        print(
            f"MISS doc={document['path']} page={page['index']} "
            f"anchor={anchor[:60]!r} missed_by={sorted(missed_by)}"
        )
    print(f"total anchor misses: {len(misses)}")
    if not args.repair or not misses:
        return 1 if misses else 0

    artifacts = {}
    repaired = 0
    for document, page, anchor, _ in misses:
        path = document["path"]
        if path not in artifacts:
            result = subprocess.run(
                [
                    args.glyphrush,
                    "--backend",
                    "pdfium",
                    "parse",
                    os.path.join(args.pdf_root, path),
                    "--format",
                    "json",
                    "--span-geometry",
                ],
                capture_output=True,
                text=True,
            )
            artifacts[path] = json.loads(result.stdout)
        stem = os.path.basename(path)[: -len(".pdf")]
        artifact = artifacts[path]
        if "line_counts" not in artifact:
            counts = {}
            for artifact_page in artifact["pages"]:
                for span in artifact_page["native_spans"]:
                    squashed = squash(span["text"].strip())
                    counts[squashed] = counts.get(squashed, 0) + 1
            artifact["line_counts"] = counts
        spans = artifact["pages"][page["index"]]["native_spans"]
        candidate = replacement_anchor(
            spans, outputs.get(stem, {}), artifact["line_counts"]
        )
        if candidate:
            page["required_text"] = [candidate]
            repaired += 1
            print(f"REPAIRED doc={path} page={page['index']} -> {candidate!r}")
        else:
            print(
                f"KEPT doc={path} page={page['index']}: no universally "
                "reproducible line; genuine extraction difference"
            )

    with open(args.manifest, "w") as handle:
        json.dump(manifest, handle, indent=2)
        handle.write("\n")
    print(f"repaired {repaired} of {len(misses)} misses")
    return 0


if __name__ == "__main__":
    sys.exit(main())
