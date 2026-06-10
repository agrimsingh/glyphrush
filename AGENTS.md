# AGENTS.md

## Cursor Cloud specific instructions

### Product overview

Glyphrush is a **Rust CLI** (no web server, no database). The binary is `glyphrush` from `glyphrush-cli`; core logic lives in `glyphrush-core`. PDF parsing uses the `lopdf` backend.

### Toolchain

- **Rust edition 2024** is required (`Cargo.toml` sets `edition = "2024"`).
- The VM image may ship with Rust 1.83; run `rustup default stable` (or newer) before building. The update script handles this.
- No `npm`, Docker, or database services are needed for core development.

### Common commands

| Task | Command |
|------|---------|
| Build | `cargo build --workspace` |
| Test | `cargo test --workspace` |
| Format check | `cargo fmt --all -- --check` |
| Backend preflight | `cargo run -p glyphrush-cli -- backend-check` |
| Inspect PDF | `cargo run -p glyphrush-cli -- inspect <pdf>` |
| Parse to text/JSON | `cargo run -p glyphrush-cli -- parse <pdf> --format text` |

See `README.md` for `bench`, `eval`, `manifest`, `debug-page`, and baseline commands.

### Test corpus

- `cargo test --workspace` generates tiny PDFs at runtime; **no local PDFs required** for the test suite.
- The v0 benchmark corpus PDFs are **committed** under `test/v0/` (public/US-government documents listed in `test/corpus.v0.sources.json`), so the `eval test/corpus.v0.json` quality gate runs without re-downloading.
- Other PDFs dropped at the top level of `test/` remain gitignored for manual benchmarks.
- `test/corpus.datasheets.json` exists but referenced datasheet PDFs are not committed (unclear redistribution terms).

### Optional dependencies (not required for core dev)

- **Python 3** + PyMuPDF/pdfplumber for baseline comparisons (`tools/baselines/*.sh`)
- **Node** + `@llamaindex/liteparse` (`lit`) for LiteParse baselines
- OCR via `--ocr-sidecar` or `--ocr-command` (external engine not bundled)

### Known environment caveats

- Two CLI integration tests assert subprocess timeout behavior (`bench_reports_timed_out_external_baseline_without_hanging`, `parse_with_ocr_command_times_out_slow_adapter`). They expect ~50ms kills; in some cloud VMs the child may run ~2s before termination, causing flaky failures. Core build and 173+ other tests still pass.
