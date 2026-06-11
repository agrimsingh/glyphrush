use crate::*;

use std::io::{Read, Write};

use std::net::{TcpStream, ToSocketAddrs};

use std::{
    fs,
    path::Path,
    process::{Command as ProcessCommand, Output as ProcessOutput},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use glyphrush_core::{PageSignals, classify_page, sha256_hex};
use serde::Serialize;
use serde_json::{Value, json};

#[cfg(test)]
use glyphrush_core::BBox;
#[cfg(any(test, feature = "pdfium"))]
use glyphrush_core::{ExtractedTextSpan, normalize_text_for_span_check};

pub(crate) const OCR_CHECK_REPORT_VERSION: &str = "glyphrush-ocr-check-report-v1";

pub(crate) const DEFAULT_OCR_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Serialize)]
pub(crate) struct OcrCheckOutput {
    pub(crate) report_version: &'static str,
    pub(crate) parser_name: &'static str,
    pub(crate) parser_version: &'static str,
    pub(crate) strict: bool,
    pub(crate) pdf: String,
    pub(crate) page_index: u32,
    pub(crate) adapter: &'static str,
    pub(crate) passed: bool,
    pub(crate) success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) http_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sidecar_path: Option<String>,
    pub(crate) exit_status: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) timeout_ms: u64,
    pub(crate) wall_us: u128,
    pub(crate) render_us: u64,
    pub(crate) output_bytes: u64,
    pub(crate) stdout_sha256: Option<String>,
    pub(crate) stdout_line_count: usize,
    pub(crate) stdout_word_count: usize,
    pub(crate) stderr_bytes: u64,
    pub(crate) empty_output: bool,
    pub(crate) stderr_preview: Option<String>,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_kind: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OcrOptions<'a> {
    pub(crate) sidecar: Option<&'a Path>,
    pub(crate) command: Option<&'a Path>,
    pub(crate) http_url: Option<&'a str>,
    pub(crate) command_input: OcrCommandInput,
    pub(crate) timeout: Duration,
}

impl<'a> OcrOptions<'a> {
    pub(crate) fn new(
        sidecar: Option<&'a Path>,
        command: Option<&'a Path>,
        http_url: Option<&'a str>,
        command_input: OcrCommandInput,
        timeout_ms: u64,
    ) -> Result<Self> {
        let adapter_count =
            sidecar.is_some() as u8 + command.is_some() as u8 + http_url.is_some() as u8;
        if adapter_count > 1 {
            bail!("choose only one of --ocr-sidecar, --ocr-command, or --ocr-http-url");
        }
        if command_input != OcrCommandInput::PdfPage && command.is_none() && http_url.is_none() {
            bail!("--ocr-command-input requires --ocr-command or --ocr-http-url");
        }

        Ok(Self {
            sidecar,
            command,
            http_url,
            command_input,
            timeout: Duration::from_millis(timeout_ms),
        })
    }
}

pub(crate) fn ocr_check_output<B: PdfBackend + ?Sized>(
    backend: &B,
    pdf: &Path,
    page_index: u32,
    ocr: OcrOptions<'_>,
    strict: bool,
) -> OcrCheckOutput {
    if ocr.command_input == OcrCommandInput::RenderedImage {
        return backend.ocr_check_rendered_image(pdf, page_index, ocr, strict);
    }

    if let Some(command) = ocr.command {
        return ocr_command_check_output(pdf, page_index, command, ocr.timeout, strict);
    }

    if let Some(http_url) = ocr.http_url {
        return ocr_http_check_output(pdf, page_index, http_url, ocr.timeout, strict);
    }

    if let Some(sidecar) = ocr.sidecar {
        return ocr_sidecar_check_output(pdf, page_index, sidecar, ocr.timeout, strict);
    }

    OcrCheckOutput {
        report_version: OCR_CHECK_REPORT_VERSION,
        parser_name: PARSER_NAME,
        parser_version: PARSER_VERSION,
        strict,
        pdf: pdf.display().to_string(),
        page_index,
        adapter: "none",
        passed: false,
        success: false,
        command: None,
        http_url: None,
        sidecar_path: None,
        exit_status: None,
        timed_out: false,
        timeout_ms: duration_millis(ocr.timeout),
        wall_us: 0,
        render_us: 0,
        output_bytes: 0,
        stdout_sha256: None,
        stdout_line_count: 0,
        stdout_word_count: 0,
        stderr_bytes: 0,
        empty_output: true,
        stderr_preview: None,
        error: Some(
            "ocr-check requires --ocr-sidecar, --ocr-command, or --ocr-http-url".to_string(),
        ),
        error_kind: Some("missing_adapter"),
    }
}

pub(crate) fn ocr_render_backend_required_check_output(
    pdf: &Path,
    page_index: u32,
    ocr: OcrOptions<'_>,
    strict: bool,
) -> OcrCheckOutput {
    OcrCheckOutput {
        report_version: OCR_CHECK_REPORT_VERSION,
        parser_name: PARSER_NAME,
        parser_version: PARSER_VERSION,
        strict,
        pdf: pdf.display().to_string(),
        page_index,
        adapter: "ocr_command_rendered_image",
        passed: false,
        success: false,
        command: ocr.command.map(|command| command.display().to_string()),
        http_url: None,
        sidecar_path: None,
        exit_status: None,
        timed_out: false,
        timeout_ms: duration_millis(ocr.timeout),
        wall_us: 0,
        render_us: 0,
        output_bytes: 0,
        stdout_sha256: None,
        stdout_line_count: 0,
        stdout_word_count: 0,
        stderr_bytes: 0,
        empty_output: true,
        stderr_preview: None,
        error: Some("rendered-image OCR command input requires a rendering backend".to_string()),
        error_kind: Some("render_backend_required"),
    }
}

pub(crate) fn ocr_http_check_output(
    pdf: &Path,
    page_index: u32,
    http_url: &str,
    timeout: Duration,
    strict: bool,
) -> OcrCheckOutput {
    let timeout_ms = duration_millis(timeout);
    match run_ocr_http_request(http_url, OcrHttpInput::PdfPage(pdf), page_index, timeout) {
        Ok(response) => {
            let success = response
                .status_code
                .is_some_and(|status| (200..300).contains(&status));
            let decoded_body = success.then(|| decode_ocr_http_response_body(&response));
            let output_body = match decoded_body.as_ref() {
                Some(Ok(text)) => text.as_bytes(),
                _ => response.body.as_slice(),
            };
            let empty_output =
                success && matches!(decoded_body.as_ref(), Some(Ok(text)) if text.is_empty());
            let decode_failed = matches!(decoded_body, Some(Err(_)));
            let error_kind = if decode_failed {
                Some("http_response_decode_failed")
            } else if empty_output {
                Some("empty_output")
            } else if success {
                None
            } else {
                Some("http_status_failed")
            };
            let error = if let Some(Err(error)) = decoded_body.as_ref() {
                Some(format!("{error:#}"))
            } else if empty_output {
                Some("OCR HTTP output was empty".to_string())
            } else if success {
                None
            } else {
                Some(format!(
                    "OCR HTTP endpoint returned status {}",
                    response.status_code.unwrap_or_default()
                ))
            };

            OcrCheckOutput {
                report_version: OCR_CHECK_REPORT_VERSION,
                parser_name: PARSER_NAME,
                parser_version: PARSER_VERSION,
                strict,
                pdf: pdf.display().to_string(),
                page_index,
                adapter: "ocr_http",
                passed: success && !empty_output && !decode_failed,
                success,
                command: None,
                http_url: Some(http_url.to_string()),
                sidecar_path: None,
                exit_status: response.status_code.map(i32::from),
                timed_out: false,
                timeout_ms,
                wall_us: response.wall_us,
                render_us: 0,
                output_bytes: output_body.len() as u64,
                stdout_sha256: Some(sha256_hex(output_body)),
                stdout_line_count: stdout_line_count(output_body),
                stdout_word_count: stdout_word_count(output_body),
                stderr_bytes: 0,
                empty_output,
                stderr_preview: None,
                error,
                error_kind,
            }
        }
        Err(error) => OcrCheckOutput {
            report_version: OCR_CHECK_REPORT_VERSION,
            parser_name: PARSER_NAME,
            parser_version: PARSER_VERSION,
            strict,
            pdf: pdf.display().to_string(),
            page_index,
            adapter: "ocr_http",
            passed: false,
            success: false,
            command: None,
            http_url: Some(http_url.to_string()),
            sidecar_path: None,
            exit_status: None,
            timed_out: ocr_http_error_kind(&error) == "timeout",
            timeout_ms,
            wall_us: 0,
            render_us: 0,
            output_bytes: 0,
            stdout_sha256: None,
            stdout_line_count: 0,
            stdout_word_count: 0,
            stderr_bytes: 0,
            empty_output: true,
            stderr_preview: None,
            error: Some(format!("{error:#}")),
            error_kind: Some(ocr_http_error_kind(&error)),
        },
    }
}

pub(crate) fn ocr_command_check_output(
    pdf: &Path,
    page_index: u32,
    command: &Path,
    timeout: Duration,
    strict: bool,
) -> OcrCheckOutput {
    let timeout_ms = duration_millis(timeout);
    let mut process = ProcessCommand::new(command);
    process.arg(pdf).arg(page_index.to_string());

    match command_output_with_timeout(process, timeout) {
        Ok(timed_output) => {
            let output = timed_output.output;
            let success = output.status.success() && !timed_output.timed_out;
            let empty_output = success && output.stdout.is_empty();
            let error_kind = if empty_output {
                Some("empty_output")
            } else {
                baseline_process_error_kind(&output, timed_output.timed_out)
            };
            let error = ocr_command_check_error(&output, timed_output.timed_out, empty_output);

            OcrCheckOutput {
                report_version: OCR_CHECK_REPORT_VERSION,
                parser_name: PARSER_NAME,
                parser_version: PARSER_VERSION,
                strict,
                pdf: pdf.display().to_string(),
                page_index,
                adapter: "ocr_command",
                passed: success && !empty_output,
                success,
                command: Some(command.display().to_string()),
                http_url: None,
                sidecar_path: None,
                exit_status: output.status.code(),
                timed_out: timed_output.timed_out,
                timeout_ms,
                wall_us: timed_output.wall_us,
                render_us: 0,
                output_bytes: output.stdout.len() as u64,
                stdout_sha256: Some(sha256_hex(&output.stdout)),
                stdout_line_count: stdout_line_count(&output.stdout),
                stdout_word_count: stdout_word_count(&output.stdout),
                stderr_bytes: output.stderr.len() as u64,
                empty_output,
                stderr_preview: stderr_preview(&output.stderr),
                error,
                error_kind,
            }
        }
        Err(error) => OcrCheckOutput {
            report_version: OCR_CHECK_REPORT_VERSION,
            parser_name: PARSER_NAME,
            parser_version: PARSER_VERSION,
            strict,
            pdf: pdf.display().to_string(),
            page_index,
            adapter: "ocr_command",
            passed: false,
            success: false,
            command: Some(command.display().to_string()),
            http_url: None,
            sidecar_path: None,
            exit_status: None,
            timed_out: false,
            timeout_ms,
            wall_us: 0,
            render_us: 0,
            output_bytes: 0,
            stdout_sha256: None,
            stdout_line_count: 0,
            stdout_word_count: 0,
            stderr_bytes: 0,
            empty_output: true,
            stderr_preview: None,
            error: Some(format!("{}: {error}", command.display())),
            error_kind: Some("spawn_failed"),
        },
    }
}

pub(crate) fn ocr_sidecar_check_output(
    pdf: &Path,
    page_index: u32,
    sidecar: &Path,
    timeout: Duration,
    strict: bool,
) -> OcrCheckOutput {
    let sidecar_path = sidecar.join(sidecar_file_name(pdf, page_index));
    let started = Instant::now();
    let read = fs::read(&sidecar_path);
    let wall_us = started.elapsed().as_micros().max(1);
    let timeout_ms = duration_millis(timeout);

    match read {
        Ok(output) => {
            let empty_output = output.is_empty();
            OcrCheckOutput {
                report_version: OCR_CHECK_REPORT_VERSION,
                parser_name: PARSER_NAME,
                parser_version: PARSER_VERSION,
                strict,
                pdf: pdf.display().to_string(),
                page_index,
                adapter: "ocr_sidecar",
                passed: !empty_output,
                success: true,
                command: None,
                http_url: None,
                sidecar_path: Some(sidecar_path.display().to_string()),
                exit_status: None,
                timed_out: false,
                timeout_ms,
                wall_us,
                render_us: 0,
                output_bytes: output.len() as u64,
                stdout_sha256: Some(sha256_hex(&output)),
                stdout_line_count: stdout_line_count(&output),
                stdout_word_count: stdout_word_count(&output),
                stderr_bytes: 0,
                empty_output,
                stderr_preview: None,
                error: empty_output.then(|| "OCR sidecar output was empty".to_string()),
                error_kind: empty_output.then_some("empty_output"),
            }
        }
        Err(error) => OcrCheckOutput {
            report_version: OCR_CHECK_REPORT_VERSION,
            parser_name: PARSER_NAME,
            parser_version: PARSER_VERSION,
            strict,
            pdf: pdf.display().to_string(),
            page_index,
            adapter: "ocr_sidecar",
            passed: false,
            success: false,
            command: None,
            http_url: None,
            sidecar_path: Some(sidecar_path.display().to_string()),
            exit_status: None,
            timed_out: false,
            timeout_ms,
            wall_us,
            render_us: 0,
            output_bytes: 0,
            stdout_sha256: None,
            stdout_line_count: 0,
            stdout_word_count: 0,
            stderr_bytes: 0,
            empty_output: true,
            stderr_preview: None,
            error: Some(format!(
                "read OCR sidecar {}: {error}",
                sidecar_path.display()
            )),
            error_kind: Some("sidecar_read_failed"),
        },
    }
}

pub(crate) fn ocr_command_check_error(
    output: &ProcessOutput,
    timed_out: bool,
    empty_output: bool,
) -> Option<String> {
    if timed_out {
        Some("OCR command timed out".to_string())
    } else if !output.status.success() {
        Some(format!(
            "OCR command exited with status {:?}",
            output.status.code()
        ))
    } else if empty_output {
        Some("OCR command output was empty".to_string())
    } else {
        None
    }
}

pub(crate) fn ocr_check_error(output: &OcrCheckOutput) -> Option<String> {
    if output.adapter == "none" {
        return Some(
            "ocr-check requires --ocr-sidecar, --ocr-command, or --ocr-http-url".to_string(),
        );
    }

    if output.adapter == "ocr_command_rendered_image"
        && output.error_kind == Some("render_backend_required")
    {
        return output.error.clone();
    }

    if output.strict && !output.passed {
        return Some(format!(
            "ocr-check strict failed: {}",
            output.error_kind.unwrap_or("unknown")
        ));
    }

    None
}

pub(crate) fn ocr_fingerprint(ocr: OcrOptions<'_>, source_path: &Path) -> Result<String> {
    if let Some(path) = ocr.sidecar {
        return sidecar_fingerprint(path, source_path);
    }

    if let Some(command) = ocr.command {
        return ocr_command_fingerprint(command, ocr.command_input, ocr.timeout);
    }

    if let Some(http_url) = ocr.http_url {
        return Ok(ocr_http_fingerprint(
            http_url,
            ocr.command_input,
            ocr.timeout,
        ));
    }

    Ok("no-sidecar".to_string())
}

pub(crate) fn sidecar_fingerprint(path: &Path, source_path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok("missing-sidecar".to_string());
    }
    let mut files = fs::read_dir(path)
        .with_context(|| format!("read OCR sidecar directory {}", path.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ty| ty.is_file()).unwrap_or(false))
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|file_name| is_document_sidecar_file(source_path, file_name))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    files.sort();

    let mut payload = Vec::new();
    for file in files {
        payload.extend_from_slice(
            file.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .as_bytes(),
        );
        payload.push(0);
        payload.extend_from_slice(
            &fs::read(&file).with_context(|| format!("read OCR sidecar {}", file.display()))?,
        );
        payload.push(0);
    }

    Ok(sha256_hex(payload))
}

pub(crate) fn ocr_command_fingerprint(
    command: &Path,
    command_input: OcrCommandInput,
    timeout: Duration,
) -> Result<String> {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"ocr-command");
    payload.push(0);
    payload.extend_from_slice(format!("{command_input:?}").as_bytes());
    payload.push(0);
    payload.extend_from_slice(command.to_string_lossy().as_bytes());
    payload.push(0);
    payload.extend_from_slice(duration_millis(timeout).to_string().as_bytes());
    payload.push(0);
    if command.is_file() {
        payload.extend_from_slice(
            &fs::read(command)
                .with_context(|| format!("read OCR command {}", command.display()))?,
        );
    } else {
        payload.extend_from_slice(b"unresolved-command");
    }

    Ok(sha256_hex(payload))
}

pub(crate) fn ocr_http_fingerprint(
    http_url: &str,
    command_input: OcrCommandInput,
    timeout: Duration,
) -> String {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"ocr-http");
    payload.push(0);
    payload.extend_from_slice(format!("{command_input:?}").as_bytes());
    payload.push(0);
    payload.extend_from_slice(http_url.as_bytes());
    payload.push(0);
    payload.extend_from_slice(duration_millis(timeout).to_string().as_bytes());
    sha256_hex(payload)
}

pub(crate) fn is_document_sidecar_file(source_path: &Path, file_name: &str) -> bool {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document");
    let prefix = format!("{stem}.p");
    let Some(page_suffix) = file_name
        .strip_prefix(&prefix)
        .and_then(|suffix| suffix.strip_suffix(".txt"))
    else {
        return false;
    };

    page_suffix.len() == 6 && page_suffix.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(any(test, feature = "pdfium"))]
pub(crate) fn partially_compatible_positioned_text_spans(
    native_text: &str,
    spans: Vec<ExtractedTextSpan>,
) -> Vec<ExtractedTextSpan> {
    let native = normalize_text_for_span_check(native_text);
    spans
        .into_iter()
        .filter(|span| {
            let text = normalize_text_for_span_check(&span.text);
            !text.is_empty() && native.contains(&text)
        })
        .collect()
}

pub(crate) fn load_ocr_if_needed(
    source_path: &Path,
    ocr: OcrOptions<'_>,
    signals: &PageSignals,
) -> Result<Option<String>> {
    if !classify_page(signals).run_ocr {
        return Ok(None);
    }

    if ocr.command_input == OcrCommandInput::RenderedImage {
        bail!("rendered-image OCR command input requires a rendering backend");
    }

    let text = if let Some(ocr_sidecar) = ocr.sidecar {
        let sidecar_path = ocr_sidecar.join(sidecar_file_name(source_path, signals.page_index));
        if !sidecar_path.exists() {
            return Ok(None);
        }
        fs::read_to_string(&sidecar_path)
            .with_context(|| format!("read OCR sidecar {}", sidecar_path.display()))?
    } else if let Some(command) = ocr.command {
        run_ocr_command(command, source_path, signals.page_index, ocr.timeout)?
    } else if let Some(http_url) = ocr.http_url {
        run_ocr_http(http_url, source_path, signals.page_index, ocr.timeout)?
    } else {
        return Ok(None);
    };

    if text.is_empty() {
        return Ok(None);
    }

    Ok(Some(text))
}

pub(crate) fn run_ocr_command(
    command: &Path,
    source_path: &Path,
    page_index: u32,
    timeout: Duration,
) -> Result<String> {
    let mut process = ProcessCommand::new(command);
    process.arg(source_path).arg(page_index.to_string());
    let timed_output = command_output_with_timeout(process, timeout)
        .with_context(|| format!("run OCR command {}", command.display()))?;
    let output = timed_output.output;

    if timed_output.timed_out {
        bail!(
            "OCR command timed out after {} ms for page {page_index}: {}",
            duration_millis(timeout),
            command.display()
        );
    }

    if !output.status.success() {
        bail!(
            "OCR command {} failed for page {page_index}: {}",
            command.display(),
            stderr_preview(&output.stderr).unwrap_or_else(|| "no stderr".to_string())
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug)]
pub(crate) struct OcrHttpResponse {
    pub(crate) status_code: Option<u16>,
    pub(crate) content_type: Option<String>,
    pub(crate) body: Vec<u8>,
    pub(crate) wall_us: u128,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OcrHttpInput<'a> {
    PdfPage(&'a Path),
    #[cfg(feature = "pdfium")]
    RenderedImage(&'a Path),
}

impl<'a> OcrHttpInput<'a> {
    pub(crate) fn request_body(self, page_index: u32) -> Result<Vec<u8>> {
        match self {
            Self::PdfPage(source_path) => serde_json::to_vec(&json!({
                "pdf_path": source_path.display().to_string(),
                "page_index": page_index,
            })),
            #[cfg(feature = "pdfium")]
            Self::RenderedImage(rendered_path) => serde_json::to_vec(&json!({
                "rendered_image_path": rendered_path.display().to_string(),
                "page_index": page_index,
            })),
        }
        .context("encode OCR HTTP request")
    }
}

#[derive(Debug)]
pub(crate) struct ParsedHttpUrl {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) path: String,
}

pub(crate) fn run_ocr_http_request(
    http_url: &str,
    input: OcrHttpInput<'_>,
    page_index: u32,
    timeout: Duration,
) -> Result<OcrHttpResponse> {
    let url = parse_http_url(http_url)?;
    let body = input.request_body(page_index)?;
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        url.path,
        url.host,
        body.len()
    );
    let started = Instant::now();
    let address = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .with_context(|| format!("resolve OCR HTTP endpoint {http_url}"))?
        .next()
        .with_context(|| format!("resolve OCR HTTP endpoint {http_url}"))?;
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .with_context(|| format!("connect OCR HTTP endpoint {http_url}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("set OCR HTTP read timeout")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("set OCR HTTP write timeout")?;
    stream
        .write_all(request.as_bytes())
        .context("write OCR HTTP request headers")?;
    stream
        .write_all(&body)
        .context("write OCR HTTP request body")?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("read OCR HTTP response")?;
    let wall_us = started.elapsed().as_micros().max(1);
    let (status_code, content_type, body) = parse_http_response(&response)?;

    Ok(OcrHttpResponse {
        status_code: Some(status_code),
        content_type,
        body,
        wall_us,
    })
}

pub(crate) fn parse_http_url(http_url: &str) -> Result<ParsedHttpUrl> {
    let Some(rest) = http_url.strip_prefix("http://") else {
        bail!("OCR HTTP URL must start with http://");
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty() {
        bail!("OCR HTTP URL missing host");
    }
    if authority.contains('@') {
        bail!("OCR HTTP URL userinfo is not supported");
    }
    let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
        let port = port
            .parse::<u16>()
            .with_context(|| format!("parse OCR HTTP URL port {port}"))?;
        (host, port)
    } else {
        (authority, 80)
    };
    if host.is_empty() {
        bail!("OCR HTTP URL missing host");
    }

    Ok(ParsedHttpUrl {
        host: host.to_string(),
        port,
        path: format!("/{path}"),
    })
}

pub(crate) fn parse_http_response(response: &[u8]) -> Result<(u16, Option<String>, Vec<u8>)> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .context("OCR HTTP response missing header terminator")?;
    let headers = String::from_utf8_lossy(&response[..header_end]);
    let status_line = headers
        .lines()
        .next()
        .context("OCR HTTP response missing status line")?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .context("OCR HTTP response missing status code")?
        .parse::<u16>()
        .context("parse OCR HTTP response status code")?;
    let content_type = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-type")
            .then(|| value.trim().to_string())
    });

    Ok((status_code, content_type, response[header_end..].to_vec()))
}

pub(crate) fn decode_ocr_http_response_body(response: &OcrHttpResponse) -> Result<String> {
    if response
        .content_type
        .as_deref()
        .map(|content_type| {
            content_type
                .to_ascii_lowercase()
                .contains("application/json")
        })
        .unwrap_or(false)
    {
        let value: Value =
            serde_json::from_slice(&response.body).context("decode OCR HTTP JSON response")?;
        let text = value
            .get("text")
            .and_then(Value::as_str)
            .context("OCR HTTP JSON response missing text field")?;
        return Ok(text.to_string());
    }

    Ok(String::from_utf8_lossy(&response.body).into_owned())
}

pub(crate) fn ocr_http_error_kind(error: &anyhow::Error) -> &'static str {
    let error = format!("{error:#}");
    if error.contains("timed out") || error.contains("would block") {
        "timeout"
    } else if error.contains("returned status") {
        "http_status_failed"
    } else {
        "http_request_failed"
    }
}

pub(crate) fn run_ocr_http(
    http_url: &str,
    source_path: &Path,
    page_index: u32,
    timeout: Duration,
) -> Result<String> {
    run_ocr_http_with_input(
        http_url,
        OcrHttpInput::PdfPage(source_path),
        page_index,
        timeout,
    )
}

pub(crate) fn run_ocr_http_with_input(
    http_url: &str,
    input: OcrHttpInput<'_>,
    page_index: u32,
    timeout: Duration,
) -> Result<String> {
    let response = run_ocr_http_request(http_url, input, page_index, timeout)?;
    let status_code = response.status_code.unwrap_or_default();
    if !(200..300).contains(&status_code) {
        bail!("OCR HTTP endpoint returned status {status_code}");
    }

    decode_ocr_http_response_body(&response)
}

pub(crate) fn sidecar_file_name(source_path: &Path, page_index: u32) -> String {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document");
    format!("{stem}.p{page_index:06}.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_span_compatibility_keeps_matching_spans_without_dropping_all_segments() {
        let spans = vec![
            ExtractedTextSpan {
                text: "First line".to_string(),
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 10.0,
                    y1: 10.0,
                },
            },
            ExtractedTextSpan {
                text: "Not in native text".to_string(),
                bbox: BBox {
                    x0: 0.0,
                    y0: 12.0,
                    x1: 10.0,
                    y1: 22.0,
                },
            },
            ExtractedTextSpan {
                text: "Second line".to_string(),
                bbox: BBox {
                    x0: 0.0,
                    y0: 24.0,
                    x1: 10.0,
                    y1: 34.0,
                },
            },
        ];

        let filtered = partially_compatible_positioned_text_spans("First line\nSecond line", spans);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].text, "First line");
        assert_eq!(filtered[1].text, "Second line");
    }
}
