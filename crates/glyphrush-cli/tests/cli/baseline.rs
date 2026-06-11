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
fn baseline_check_rejects_empty_baseline_list_without_vacuous_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .arg("baseline-check")
        .output()
        .expect("run glyphrush baseline-check without baselines");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["report_version"], "glyphrush-baseline-check-report-v1");
    assert_eq!(json["baseline_count"], 0);
    assert_eq!(json["describe_success_count"], 0);
    assert_eq!(json["all_described"], false);
    assert!(json["baselines"].as_array().unwrap().is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("baseline-check requires at least one --baseline")
    );
}

#[test]
fn baseline_wrapper_describe_modes_identify_comparison_targets() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wrappers = [
        (
            "liteparse-text.sh",
            "liteparse",
            "run-llama/liteparse",
            "lit parse",
        ),
        (
            "liteparse-no-ocr-text.sh",
            "liteparse-no-ocr",
            "run-llama/liteparse",
            "--no-ocr",
        ),
        ("pymupdf-text.sh", "pymupdf", "PyMuPDF", "page.get_text"),
        (
            "pdfplumber-text.sh",
            "pdfplumber",
            "pdfplumber",
            "extract_text",
        ),
        ("marker-text.sh", "marker", "Marker", "marker_single"),
        ("docling-text.sh", "docling", "Docling", "docling"),
    ];

    for (script, name, target, command_hint) in wrappers {
        let output = Command::new(workspace_root.join("tools/baselines").join(script))
            .arg("--describe")
            .output()
            .unwrap_or_else(|error| panic!("run {script} --describe: {error}"));

        assert!(
            output.status.success(),
            "{script} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: Value =
            serde_json::from_slice(&output.stdout).expect("baseline describe output is json");
        assert_eq!(json["name"], name);
        assert_eq!(json["target"], target);
        assert!(
            json["command_hint"]
                .as_str()
                .unwrap()
                .contains(command_hint),
            "{script} command_hint: {}",
            json["command_hint"]
        );
    }
}

#[test]
fn python_baseline_wrappers_use_project_local_venv_python() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = temp_dir("baseline-local-python");
    let pdf_path = root.join("sample.pdf");
    fs::write(&pdf_path, minimal_pdf("Local Python baseline")).unwrap();
    let python = root
        .join(".glyphrush-baselines")
        .join("venv")
        .join("bin")
        .join("python3");
    write_executable(
        &python,
        "#!/bin/sh\nprintf 'local python %s\\n' \"${2:-missing-pdf}\"\n",
    );

    for script in ["pymupdf-text.sh", "pdfplumber-text.sh"] {
        let output = Command::new(workspace_root.join("tools/baselines").join(script))
            .env("GLYPHRUSH_BASELINE_ROOT", &root)
            .env_remove("GLYPHRUSH_BASELINE_PYTHON")
            .arg(&pdf_path)
            .output()
            .unwrap_or_else(|error| panic!("run {script} with project-local python: {error}"));

        assert!(
            output.status.success(),
            "{script} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("local python {}\n", pdf_path.display())
        );
    }
}

#[test]
fn tesseract_rendered_image_ocr_wrapper_describes_and_invokes_tesseract() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wrapper = workspace_root.join("tools/ocr/tesseract-rendered-image.sh");

    let describe = Command::new(&wrapper)
        .arg("--describe")
        .output()
        .expect("run tesseract OCR wrapper --describe");
    assert!(
        describe.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&describe.stderr)
    );
    let describe_json: Value =
        serde_json::from_slice(&describe.stdout).expect("OCR wrapper describe output is json");
    assert_eq!(describe_json["name"], "tesseract-rendered-image");
    assert_eq!(describe_json["target"], "Tesseract OCR");
    assert_eq!(describe_json["input"], "rendered-image");
    assert_eq!(describe_json["requires"], serde_json::json!(["tesseract"]));

    let dir = temp_dir("tesseract-rendered-image-wrapper");
    let image = dir.join("page.ppm");
    let log_path = dir.join("tesseract.log");
    fs::write(&image, b"P6\n1 1\n255\n\0\0\0").unwrap();
    let fake_tesseract = dir.join("tesseract");
    write_executable(
        &fake_tesseract,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf 'Fake OCR text from %s\\n' \"$1\"\n",
            log_path.display()
        ),
    );

    let output = Command::new(&wrapper)
        .env("TESSERACT_BIN", &fake_tesseract)
        .env("TESSERACT_LANG", "eng")
        .env("TESSERACT_PSM", "6")
        .arg(&image)
        .arg("3")
        .output()
        .expect("run tesseract OCR wrapper");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("Fake OCR text from {}\n", image.display())
    );
    assert_eq!(
        fs::read_to_string(log_path).unwrap(),
        format!("{} stdout -l eng --psm 6\n", image.display())
    );
}

#[test]
fn baseline_check_reports_wrapper_describe_health() {
    let healthy = write_baseline_script(
        "baseline-check-healthy",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"mock","target":"Mock Parser","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'mock baseline output'"#,
    );
    let missing = temp_dir("baseline-check-missing").join("missing-baseline.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline",
            &format!("mock={}", healthy.display()),
            "--baseline",
            &format!("missing={}", missing.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["report_version"], "glyphrush-baseline-check-report-v1");
    assert_eq!(json["baseline_count"], 2);
    assert_eq!(json["describe_success_count"], 1);
    assert_eq!(json["all_described"], false);
    assert_eq!(json["baselines"][0]["name"], "mock");
    assert_eq!(json["baselines"][0]["describe"]["success"], true);
    assert_eq!(json["baselines"][0]["describe"]["valid_json_object"], true);
    assert_eq!(json["baselines"][0]["description"]["target"], "Mock Parser");
    assert_eq!(json["baselines"][1]["name"], "missing");
    assert_eq!(json["baselines"][1]["describe"]["success"], false);
    assert_eq!(
        json["baselines"][1]["describe"]["error_kind"],
        "spawn_failed"
    );
    assert!(
        json["baselines"][1]["describe"]["error"]
            .as_str()
            .unwrap()
            .contains("missing-baseline.sh")
    );
}

#[test]
fn baseline_check_classifies_failed_describe_probe_kinds() {
    let execution_failed = write_baseline_script(
        "baseline-check-describe-execution-failed",
        r#"if [ "${1:-}" = "--describe" ]; then printf 'describe failed' >&2; exit 7; fi
printf 'unused'"#,
    );
    let missing_dependency = write_baseline_script(
        "baseline-check-describe-missing-dependency",
        r#"if [ "${1:-}" = "--describe" ]; then printf 'lit missing' >&2; exit 127; fi
printf 'unused'"#,
    );
    let invalid = write_baseline_script(
        "baseline-check-describe-invalid",
        r#"if [ "${1:-}" = "--describe" ]; then printf 'not json'; exit 0; fi
printf 'unused'"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline",
            &format!("execution={}", execution_failed.display()),
            "--baseline",
            &format!("dependency={}", missing_dependency.display()),
            "--baseline",
            &format!("invalid={}", invalid.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check with failed describe probes");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["describe_success_count"], 0);
    assert_eq!(json["all_described"], false);
    assert_eq!(
        json["baselines"][0]["describe"]["error_kind"],
        "execution_failed"
    );
    assert_eq!(
        json["baselines"][1]["describe"]["error_kind"],
        "missing_dependency"
    );
    assert_eq!(
        json["baselines"][2]["describe"]["error_kind"],
        "invalid_describe_output"
    );
}

#[test]
fn baseline_check_classifies_timed_out_describe_probe_kind() {
    let slow = write_baseline_script(
        "baseline-check-describe-timeout",
        r#"if [ "${1:-}" = "--describe" ]; then sleep 2; printf '{"name":"slow"}'; exit 0; fi
printf 'unused'"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline",
            &format!("slow={}", slow.display()),
            "--baseline-timeout-ms",
            "50",
        ])
        .output()
        .expect("run glyphrush baseline-check with timed out describe probe");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baselines"][0]["describe"]["timed_out"], true);
    assert_eq!(json["baselines"][0]["describe"]["error_kind"], "timeout");
}

#[test]
fn baseline_check_preset_describes_core_glyphrush_v0_baselines() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .current_dir(&workspace_root)
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--baseline-preset",
            "glyphrush-v0",
        ])
        .output()
        .expect("run glyphrush baseline-check with baseline preset");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baseline_count"], 4);
    assert_eq!(
        json["requested_baseline_presets"],
        serde_json::json!(["glyphrush-v0"])
    );
    assert_eq!(json["describe_success_count"], 4);
    assert_eq!(json["all_described"], true);
    assert_eq!(
        json["baselines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|baseline| baseline["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["liteparse", "liteparse-no-ocr", "pymupdf", "pdfplumber"]
    );
    assert_eq!(
        json["baselines"][0]["description"]["target"],
        "run-llama/liteparse"
    );
    assert_eq!(json["baselines"][2]["description"]["target"], "PyMuPDF");
    assert_eq!(json["baselines"][3]["description"]["target"], "pdfplumber");
}

#[test]
fn baseline_check_can_smoke_test_wrappers_against_pdf() {
    let pdf_path = write_test_pdf("baseline-check-smoke-pdf", "Smoke baseline PDF");
    let ok = write_baseline_script(
        "baseline-check-smoke-ok",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"ok","target":"Smoke OK","kind":"text-baseline-wrapper"}'; exit 0; fi
if [ -f "${1:-}" ]; then printf 'smoke output\n'; exit 0; fi
printf 'missing pdf' >&2
exit 66"#,
    );
    let failing = write_baseline_script(
        "baseline-check-smoke-failing",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"failing","target":"Smoke Failing","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'parser dependency missing' >&2
exit 127"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("ok={}", ok.display()),
            "--baseline",
            &format!("failing={}", failing.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check with smoke PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baseline_count"], 2);
    assert_eq!(json["describe_success_count"], 2);
    assert_eq!(json["all_described"], true);
    assert_eq!(
        json["smoke_pdf"].as_str().unwrap(),
        pdf_path.to_string_lossy()
    );
    assert_eq!(json["smoke_success_count"], 1);
    assert_eq!(json["all_smoke_passed"], false);

    assert_eq!(json["baselines"][0]["name"], "ok");
    assert_eq!(json["baselines"][0]["smoke"]["success"], true);
    assert_eq!(json["baselines"][0]["smoke"]["exit_status"], 0);
    assert_eq!(json["baselines"][0]["smoke"]["output_bytes"], 13);
    assert_eq!(
        json["baselines"][0]["smoke"]["stdout_sha256"],
        sha256_hex("smoke output\n")
    );
    assert_eq!(json["baselines"][0]["smoke"]["stdout_line_count"], 1);
    assert_eq!(json["baselines"][0]["smoke"]["stdout_word_count"], 2);
    assert_eq!(json["baselines"][0]["smoke"]["empty_output"], false);
    assert_eq!(json["baselines"][0]["smoke"]["error"], Value::Null);

    assert_eq!(json["baselines"][1]["name"], "failing");
    assert_eq!(json["baselines"][1]["smoke"]["success"], false);
    assert_eq!(json["baselines"][1]["smoke"]["exit_status"], 127);
    assert_eq!(
        json["baselines"][1]["smoke"]["error_kind"],
        "missing_dependency"
    );
    assert!(
        json["baselines"][1]["smoke"]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing")
    );
    assert!(
        json["baselines"][1]["smoke"]["error"]
            .as_str()
            .unwrap()
            .contains("status Some(127)")
    );
}

#[test]
fn baseline_check_drains_large_stdout_without_timing_out() {
    let pdf_path = write_test_pdf("baseline-check-large-stdout-pdf", "Large stdout baseline");
    let noisy = write_baseline_script(
        "baseline-check-large-stdout",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"noisy","target":"Noisy Baseline","kind":"text-baseline-wrapper"}'; exit 0; fi
python3 - <<'PY'
import sys
sys.stdout.write("word " * 40000)
PY"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("noisy={}", noisy.display()),
            "--baseline-timeout-ms",
            "5000",
            "--strict",
        ])
        .output()
        .expect("run glyphrush baseline-check with large stdout");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");
    let smoke = &json["baselines"][0]["smoke"];

    assert_eq!(smoke["success"], true);
    assert_eq!(smoke["timed_out"], false);
    assert!(smoke["output_bytes"].as_u64().unwrap() >= 200_000);
    assert_eq!(smoke["stdout_word_count"], 40000);
    assert_eq!(smoke["error"], Value::Null);
}

#[test]
fn baseline_check_can_smoke_test_wrappers_against_directory() {
    let dir = temp_dir("baseline-check-smoke-dir");
    fs::write(dir.join("b.pdf"), minimal_pdf("Second baseline smoke")).unwrap();
    fs::write(dir.join("a.PDF"), minimal_pdf("First baseline smoke")).unwrap();
    fs::write(dir.join("ignore.txt"), "not a pdf").unwrap();
    let ok = write_baseline_script(
        "baseline-check-smoke-dir-ok",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"ok","target":"Directory Smoke OK","kind":"text-baseline-wrapper"}'; exit 0; fi
if [ -f "${1:-}" ]; then printf 'dir smoke %s\n' "$(basename "$1")"; exit 0; fi
printf 'missing pdf' >&2
exit 66"#,
    );
    let failing = write_baseline_script(
        "baseline-check-smoke-dir-failing",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"failing","target":"Directory Smoke Failing","kind":"text-baseline-wrapper"}'; exit 0; fi
if [ "$(basename "${1:-}")" = "b.pdf" ]; then printf 'parser dependency missing for b.pdf' >&2; exit 127; fi
printf 'partial smoke %s\n' "$(basename "$1")""#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--pdf",
            dir.to_str().unwrap(),
            "--baseline",
            &format!("ok={}", ok.display()),
            "--baseline",
            &format!("failing={}", failing.display()),
        ])
        .output()
        .expect("run glyphrush baseline-check with smoke directory");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["baseline_count"], 2);
    assert_eq!(json["smoke_pdf"].as_str().unwrap(), dir.to_string_lossy());
    assert_eq!(json["smoke_document_count"], 2);
    assert_eq!(json["smoke_success_count"], 1);
    assert_eq!(json["all_smoke_passed"], false);

    assert_eq!(json["baselines"][0]["name"], "ok");
    assert_eq!(json["baselines"][0]["smoke"]["success"], true);
    assert_eq!(json["baselines"][0]["smoke"]["document_count"], 2);
    assert_eq!(json["baselines"][0]["smoke"]["successful_documents"], 2);
    assert_eq!(json["baselines"][0]["smoke"]["failed_documents"], 0);
    assert_eq!(
        json["baselines"][0]["smoke"]["documents"][0]["path"],
        "a.PDF"
    );
    assert_eq!(
        json["baselines"][0]["smoke"]["documents"][1]["path"],
        "b.pdf"
    );
    assert_eq!(
        json["baselines"][0]["smoke"]["documents"][0]["stdout_sha256"],
        sha256_hex("dir smoke a.PDF\n")
    );

    assert_eq!(json["baselines"][1]["name"], "failing");
    assert_eq!(json["baselines"][1]["smoke"]["success"], false);
    assert_eq!(json["baselines"][1]["smoke"]["document_count"], 2);
    assert_eq!(json["baselines"][1]["smoke"]["successful_documents"], 1);
    assert_eq!(json["baselines"][1]["smoke"]["failed_documents"], 1);
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][0]["success"],
        true
    );
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][1]["success"],
        false
    );
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][1]["exit_status"],
        127
    );
    assert!(
        json["baselines"][1]["smoke"]["documents"][1]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing for b.pdf")
    );
    assert_eq!(
        json["baselines"][1]["smoke"]["documents"][1]["error_kind"],
        "missing_dependency"
    );

    let failure_samples = json["baselines"][1]["smoke"]["failure_samples"]
        .as_array()
        .unwrap();
    assert_eq!(failure_samples.len(), 1);
    assert_eq!(failure_samples[0]["path"], "b.pdf");
    assert_eq!(failure_samples[0]["exit_status"], 127);
    assert_eq!(failure_samples[0]["error_kind"], "missing_dependency");
    assert!(
        failure_samples[0]["stderr_preview"]
            .as_str()
            .unwrap()
            .contains("parser dependency missing for b.pdf")
    );
    assert!(
        failure_samples[0]["error"]
            .as_str()
            .unwrap()
            .contains("status Some(127)")
    );
}

#[test]
fn baseline_check_strict_passes_when_describe_and_smoke_pass() {
    let pdf_path = write_test_pdf("baseline-check-strict-pass-pdf", "Strict baseline PDF");
    let ok = write_baseline_script(
        "baseline-check-strict-ok",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"ok","target":"Strict OK","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'strict smoke output\n'"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--strict",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("ok={}", ok.display()),
        ])
        .output()
        .expect("run strict glyphrush baseline-check with healthy smoke PDF");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["strict"], true);
    assert_eq!(json["all_described"], true);
    assert_eq!(json["all_smoke_passed"], true);
    assert_eq!(json["smoke_success_count"], 1);
}

#[test]
fn baseline_check_strict_exits_nonzero_when_smoke_fails_after_writing_json() {
    let pdf_path = write_test_pdf("baseline-check-strict-fail-pdf", "Strict failing PDF");
    let failing = write_baseline_script(
        "baseline-check-strict-failing",
        r#"if [ "${1:-}" = "--describe" ]; then printf '{"name":"failing","target":"Strict Failing","kind":"text-baseline-wrapper"}'; exit 0; fi
printf 'parser dependency missing' >&2
exit 127"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args([
            "--backend",
            "lopdf",
            "baseline-check",
            "--strict",
            "--pdf",
            pdf_path.to_str().unwrap(),
            "--baseline",
            &format!("failing={}", failing.display()),
        ])
        .output()
        .expect("run strict glyphrush baseline-check with failing smoke PDF");

    assert!(!output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("baseline-check output is json");

    assert_eq!(json["strict"], true);
    assert_eq!(json["all_described"], true);
    assert_eq!(json["all_smoke_passed"], false);
    assert_eq!(json["smoke_success_count"], 0);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("baseline-check strict failed"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
