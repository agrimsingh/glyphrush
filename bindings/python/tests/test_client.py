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
