import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PACKAGE_ROOT))

import glyphrush  # noqa: E402


def write_fake_glyphrush(directory: Path) -> Path:
    script = directory / "glyphrush"
    script.write_text(
        "\n".join(
            [
                f"#!{sys.executable}",
                "import json",
                "import os",
                "import sys",
                "",
                "if os.environ.get('GLYPHRUSH_FAKE_FAIL') == '1':",
                "    print('fake failure from glyphrush', file=sys.stderr)",
                "    sys.exit(7)",
                "",
                "if '--format' in sys.argv and sys.argv[sys.argv.index('--format') + 1] == 'text':",
                "    print('fake text output')",
                "elif '--format' in sys.argv and sys.argv[sys.argv.index('--format') + 1] == 'markdown':",
                "    print('# fake markdown output')",
                "else:",
                "    print(json.dumps({'argv': sys.argv[1:]}))",
            ]
        )
    )
    mode = script.stat().st_mode
    script.chmod(mode | stat.S_IXUSR)
    return script


class GlyphrushClientTests(unittest.TestCase):
    def test_parse_json_delegates_to_native_cli_and_decodes_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")

            artifact = glyphrush.parse(
                pdf,
                binary=fake,
                backend="lopdf",
                span_geometry=True,
                cache_dir=root / "cache",
                jobs=2,
            )

        self.assertEqual(
            artifact["argv"],
            [
                "--backend",
                "lopdf",
                "parse",
                str(pdf),
                "--format",
                "json",
                "--span-geometry",
                "--cache-dir",
                str(root / "cache"),
                "--jobs",
                "2",
            ],
        )

    def test_parse_text_returns_stdout_without_json_decoding(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")

            text = glyphrush.parse_text(pdf, binary=fake)

        self.assertEqual(text, "fake text output\n")

    def test_parse_markdown_returns_stdout_without_json_decoding(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")

            markdown = glyphrush.parse_markdown(pdf, binary=fake)

        self.assertEqual(markdown, "# fake markdown output\n")

    def test_inspect_pages_delegates_to_native_page_triage_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")

            report = glyphrush.inspect_pages(
                pdf,
                binary=fake,
                backend="lopdf",
                cache_dir=root / "cache",
                jobs=3,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "inspect",
                str(pdf),
                "--pages",
                "--cache-dir",
                str(root / "cache"),
                "--jobs",
                "3",
            ],
        )

    def test_eval_manifest_delegates_to_native_quality_gate_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            manifest = root / "corpus.json"
            manifest.write_text('{"documents":[]}')

            report = glyphrush.eval_manifest(
                manifest,
                binary=fake,
                backend="lopdf",
                category="datasheet",
                span_geometry=True,
                cache_dir=root / "cache",
                jobs=4,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "eval",
                str(manifest),
                "--category",
                "datasheet",
                "--span-geometry",
                "--cache-dir",
                str(root / "cache"),
                "--jobs",
                "4",
            ],
        )

    def test_manifest_delegates_to_native_corpus_generator_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf_dir = root / "pdfs"
            pdf_dir.mkdir()

            report = glyphrush.manifest(
                pdf_dir,
                binary=fake,
                backend="lopdf",
                category="datasheet",
                category_from_path=True,
                coverage_preset="glyphrush-v0",
                required_category=["datasheet", "scanned"],
                min_category_count=["datasheet=5"],
                span_geometry=True,
                cache_dir=root / "cache",
                jobs=4,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "manifest",
                str(pdf_dir),
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
                str(root / "cache"),
                "--jobs",
                "4",
            ],
        )

    def test_bench_delegates_to_native_quality_backed_speed_gate_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")
            manifest = root / "corpus.json"
            manifest.write_text('{"documents":[]}')

            report = glyphrush.bench(
                pdf,
                binary=fake,
                backend="lopdf",
                eval_manifest=manifest,
                eval_category="datasheet",
                baseline_preset="glyphrush-v0",
                require_quality=True,
                require_baselines=True,
                require_baseline_quality=True,
                require_coverage_preset="glyphrush-v0",
                require_speedup_claim=["liteparse=2.0", "liteparse-no-ocr=1.5"],
                cache_probe=True,
                baseline_timeout_ms=1234,
                cache_dir=root / "cache",
                jobs=2,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "bench",
                str(pdf),
                "--eval-manifest",
                str(manifest),
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
                str(root / "cache"),
                "--jobs",
                "2",
            ],
        )

    def test_backend_check_delegates_to_native_preflight_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf_dir = root / "pdfs"
            pdf_dir.mkdir()

            report = glyphrush.backend_check(
                pdf=pdf_dir,
                binary=fake,
                backend="lopdf",
                jobs=4,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "backend-check",
                "--pdf",
                str(pdf_dir),
                "--jobs",
                "4",
            ],
        )

    def test_debug_page_delegates_to_native_page_debugger_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")
            ocr = root / "ocr"
            ocr.mkdir()

            report = glyphrush.debug_page(
                pdf,
                3,
                binary=fake,
                backend="lopdf",
                span_geometry=True,
                ocr_sidecar=ocr,
                ocr_timeout_ms=2500,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "debug-page",
                str(pdf),
                "3",
                "--span-geometry",
                "--ocr-sidecar",
                str(ocr),
                "--ocr-timeout-ms",
                "2500",
            ],
        )

    def test_ocr_check_delegates_to_native_adapter_preflight_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")
            command = root / "ocr.sh"
            command.write_text("#!/bin/sh\nprintf OCR\n")

            report = glyphrush.ocr_check(
                pdf,
                page_index=2,
                binary=fake,
                backend="pdfium",
                ocr_command=command,
                ocr_command_input="rendered-image",
                ocr_timeout_ms=1500,
                strict=True,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "pdfium",
                "ocr-check",
                str(pdf),
                "--page-index",
                "2",
                "--ocr-command",
                str(command),
                "--ocr-command-input",
                "rendered-image",
                "--ocr-timeout-ms",
                "1500",
                "--strict",
            ],
        )

    def test_feature_parity_delegates_to_native_report_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)

            report = glyphrush.feature_parity(
                binary=fake,
                backend="lopdf",
                bench_report=root / "bench.json",
                require_speed_evidence=True,
                require_coverage_preset="glyphrush-v0",
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "feature-parity",
                "--bench-report",
                str(root / "bench.json"),
                "--require-speed-evidence",
                "--require-coverage-preset",
                "glyphrush-v0",
            ],
        )

    def test_baseline_check_delegates_to_native_baseline_preflight_and_decodes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf_dir = root / "pdfs"
            pdf_dir.mkdir()

            report = glyphrush.baseline_check(
                binary=fake,
                backend="lopdf",
                pdf=pdf_dir,
                baseline_preset="glyphrush-v0",
                baseline=["custom=/bin/echo"],
                baseline_timeout_ms=4321,
                strict=True,
            )

        self.assertEqual(
            report["argv"],
            [
                "--backend",
                "lopdf",
                "baseline-check",
                "--baseline-preset",
                "glyphrush-v0",
                "--baseline",
                "custom=/bin/echo",
                "--pdf",
                str(pdf_dir),
                "--baseline-timeout-ms",
                "4321",
                "--strict",
            ],
        )

    def test_cli_failure_raises_with_exit_status_and_stderr(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = write_fake_glyphrush(root)
            pdf = root / "sample.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")
            env = os.environ.copy()
            env["GLYPHRUSH_FAKE_FAIL"] = "1"

            with self.assertRaises(glyphrush.GlyphrushError) as error:
                glyphrush.parse(pdf, binary=fake, env=env)

        self.assertEqual(error.exception.returncode, 7)
        self.assertIn("fake failure", str(error.exception))


if __name__ == "__main__":
    unittest.main()
