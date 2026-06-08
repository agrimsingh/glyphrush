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
