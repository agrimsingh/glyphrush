#![allow(unused_imports)]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use super::harness::*;
use glyphrush_core::sha256_hex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn liteparse_benchmark_gate_script_dry_run_uses_quality_backed_pdfium_command() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_JOBS", "3")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-gate.json",
        )
        .output()
        .expect("run bench-liteparse dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[0].contains("--pdf test/"));
    assert!(lines[0].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[0].contains("--strict"));
    assert!(lines[1].contains("cargo run -q --release -p glyphrush-cli"));
    assert!(lines[1].contains("--features pdfium"));
    assert!(lines[1].contains("--backend pdfium"));
    assert!(lines[1].contains("bench test/"));
    assert!(lines[1].contains("--eval-manifest test/corpus.datasheets.json"));
    assert!(lines[1].contains("--eval-category datasheet"));
    assert!(lines[1].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[1].contains("--require-baselines"));
    assert!(lines[1].contains("--require-baseline-quality"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
    assert!(lines[1].contains("--require-speedup-claim liteparse=2.0"));
    assert!(lines[1].contains("--require-speedup-claim liteparse-no-ocr=1.5"));
    assert!(lines[1].contains("--jobs 3"));
    assert!(lines[1].contains("> /tmp/glyphrush-liteparse-gate.json"));
    assert!(lines[2].contains("feature-parity"));
    assert!(lines[2].contains("--bench-report /tmp/glyphrush-liteparse-gate.json"));
    assert!(lines[2].contains("--require-speed-evidence"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_checks_saved_speed_report_without_coverage_gate() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-datasheet.json",
        )
        .output()
        .expect("run bench-liteparse dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[1].contains("bench test/"));
    assert!(lines[1].contains("> /tmp/glyphrush-liteparse-datasheet.json"));
    assert!(lines[2].contains("feature-parity"));
    assert!(lines[2].contains("--bench-report /tmp/glyphrush-liteparse-datasheet.json"));
    assert!(lines[2].contains("--require-speed-evidence"));
    assert!(!lines[2].contains("--require-coverage-preset"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_use_all_manifest_categories() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.full.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-full-coverage.json",
        )
        .output()
        .expect("run bench-liteparse dry run for all categories");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[1].contains("bench test/"));
    assert!(lines[1].contains("--eval-manifest test/corpus.full.json"));
    assert!(!lines[1].contains("--eval-category"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_use_native_text_v0_categories() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "native-text")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-v0-native-text.json",
        )
        .output()
        .expect("run bench-liteparse dry run for v0 native-text categories");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[1].contains("bench test/v0"));
    assert!(lines[1].contains("--eval-manifest test/corpus.v0.json"));
    assert!(lines[1].contains("--eval-category-preset glyphrush-v0-native-text"));
    assert!(!lines[1].contains("--eval-category academic_columns"));
    assert!(!lines[1].contains("--eval-category scanned"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0-native-text"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0-native-text"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_defaults_v0_manifest_to_v0_pdf_root() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-v0-coverage.json",
        )
        .output()
        .expect("run bench-liteparse dry run for v0 manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(
        lines[0].contains("baseline-check"),
        "v0 gate should still preflight baseline wrapper availability:\n{stdout}"
    );
    assert!(
        !lines[0].contains("--pdf"),
        "v0 gate should not duplicate full baseline parsing before the benchmark:\n{stdout}"
    );
    assert!(
        !lines[0].contains("--baseline-timeout-ms"),
        "describe-only v0 preflight should not need the large corpus baseline timeout:\n{stdout}"
    );
    assert!(
        lines[1].contains("bench test/v0"),
        "benchmark should run only the v0 corpus root:\n{stdout}"
    );
    assert!(lines[1].contains("--eval-manifest test/corpus.v0.json"));
    assert!(!lines[1].contains("--eval-category"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
    assert!(lines[1].contains("--baseline-timeout-ms 900000"));
    assert!(lines[1].contains("2> >(tee"));
    assert!(lines[1].contains("/tmp/glyphrush-liteparse-v0-coverage.progress.log"));
    assert!(lines[2].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_override_progress_log() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env("GLYPHRUSH_BENCH_OUTPUT", "/tmp/glyphrush-liteparse-v0.json")
        .env(
            "GLYPHRUSH_BENCH_PROGRESS_LOG",
            "/tmp/custom-glyphrush-progress.log",
        )
        .output()
        .expect("run bench-liteparse dry run for custom progress log");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "dry-run output:\n{stdout}");
    assert!(lines[1].contains("> /tmp/glyphrush-liteparse-v0.json"));
    assert!(lines[1].contains("2> >(tee"));
    assert!(lines[1].contains("/tmp/custom-glyphrush-progress.log"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_skip_preflight_for_long_v0_runs() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_PREFLIGHT", "none")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env("GLYPHRUSH_BENCH_COVERAGE_PRESET", "glyphrush-v0")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-liteparse-v0-coverage.json",
        )
        .output()
        .expect("run bench-liteparse dry run with preflight disabled");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "dry-run output:\n{stdout}");
    assert!(
        !lines[0].contains("baseline-check"),
        "preflight=none should start with the real benchmark command:\n{stdout}"
    );
    assert!(lines[0].contains("bench test/v0"));
    assert!(lines[0].contains("--baseline-timeout-ms 900000"));
    assert!(lines[1].contains("feature-parity"));
    assert!(lines[1].contains("--require-coverage-preset glyphrush-v0"));
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_probe_one_stalled_v0_pdf() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_CATEGORY", "all")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env(
            "GLYPHRUSH_BENCH_PROBE_PDF",
            "test/v0/academic_columns/acl-bert-naacl-2019.pdf",
        )
        .env("GLYPHRUSH_BENCH_OUTPUT", "/tmp/glyphrush-v0-probe.json")
        .output()
        .expect("run bench-liteparse probe dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "probe dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[0].contains("--pdf test/v0/academic_columns/acl-bert-naacl-2019.pdf"));
    assert!(lines[0].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[0].contains("--baseline-timeout-ms 60000"));
    assert!(lines[0].contains("--strict"));
    assert!(lines[0].contains("> /tmp/glyphrush-v0-probe.json"));
    assert!(
        !lines[0].contains("feature-parity"),
        "probe output should not ask feature-parity to read a baseline-check report:\n{stdout}"
    );
    assert!(
        !lines[0].contains("bench test/v0"),
        "probe output should avoid the full v0 benchmark command:\n{stdout}"
    );
}

#[test]
fn liteparse_benchmark_gate_script_dry_run_can_probe_one_baseline() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let output = Command::new(repo_root.join("scripts/bench-liteparse.sh"))
        .arg("--dry-run")
        .env("GLYPHRUSH_BENCH_MANIFEST", "test/corpus.v0.json")
        .env(
            "GLYPHRUSH_BENCH_PROBE_PDF",
            "test/v0/academic_columns/acl-bert-naacl-2019.pdf",
        )
        .env("GLYPHRUSH_BENCH_PROBE_BASELINE", "liteparse")
        .env(
            "GLYPHRUSH_BENCH_OUTPUT",
            "/tmp/glyphrush-v0-liteparse-probe.json",
        )
        .output()
        .expect("run bench-liteparse single-baseline probe dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "probe dry-run output:\n{stdout}");
    assert!(lines[0].contains("baseline-check"));
    assert!(lines[0].contains("--pdf test/v0/academic_columns/acl-bert-naacl-2019.pdf"));
    assert!(lines[0].contains("--baseline liteparse=tools/baselines/liteparse-text.sh"));
    assert!(!lines[0].contains("--baseline-preset glyphrush-v0"));
    assert!(lines[0].contains("--baseline-timeout-ms 60000"));
    assert!(lines[0].contains("> /tmp/glyphrush-v0-liteparse-probe.json"));
}

#[test]
fn verify_script_dry_run_exposes_opt_in_pdfium_speed_path_gate() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root");
    let workspace = temp_dir("verify-dry-run-local-corpora");
    fs::create_dir_all(workspace.join("test/v0/scanned")).unwrap();
    fs::write(workspace.join("test/root.pdf"), b"%PDF root fixture").unwrap();
    fs::write(
        workspace.join("test/v0/scanned/sample.pdf"),
        b"%PDF v0 fixture",
    )
    .unwrap();

    let output = Command::new(repo_root.join("scripts/verify.sh"))
        .arg("--dry-run")
        .current_dir(&workspace)
        .env("GLYPHRUSH_VERIFY_PDFIUM", "1")
        .output()
        .expect("run verify dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo fmt --all -- --check"));
    assert!(stdout.contains("cargo test --workspace"));
    assert!(stdout.contains("cargo clippy --workspace --all-targets -- -D warnings"));
    assert!(stdout.contains("cargo test -p glyphrush-cli --features pdfium"));
    assert!(
        stdout.contains("feature_parity_counts_pdfium_ocr_runtime_caps_and_cache_as_implemented")
    );
    assert!(
        stdout
            .contains("parse_pdfium_ocr_command_rendered_image_invokes_adapter_only_for_ocr_pages")
    );
    assert!(stdout.contains("eval test/corpus.datasheets.json --category datasheet --jobs 2"));
    assert!(
        stdout.contains("--backend pdfium eval test/corpus.v0.json --jobs 2"),
        "local v0 corpus should be part of the shared verify gate:\n{stdout}"
    );
}
