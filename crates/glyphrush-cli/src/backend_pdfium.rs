#![cfg(feature = "pdfium")]

use crate::*;

use std::io::Write;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::{Instant, UNIX_EPOCH},
};

#[cfg(all(test, feature = "pdfium"))]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use glyphrush_core::{
    BBox, ExtractedPage, PageDimensions, PageSignals, PageTimings, classify_page, sha256_hex,
};

#[cfg(feature = "pdfium")]
use std::cell::OnceCell;
#[cfg(feature = "pdfium")]
use std::time::SystemTime;

#[cfg(feature = "pdfium")]
use glyphrush_core::{
    ExtractedImage, ExtractedRulingLine, ExtractedTextSpan, MAX_EXTRACTED_RULING_LINES,
    RULED_TABLE_SATURATION_SEGMENTS, broken_encoding_ratio, combined_table_line_density,
    duplicate_char_ratio, image_artifact_coverage_ratio, is_ruling_segment,
    positioned_bbox_overlap_ratio, ruling_density, ruling_line_from_segment,
};
#[cfg(feature = "pdfium")]
use pdfium_render::prelude::{
    PdfBitmapFormat, PdfDocument, PdfMatrix as PdfiumMatrix, PdfPage, PdfPageObject,
    PdfPageObjectCommon, PdfPageObjectsCommon, PdfPageText, PdfPageXObjectFormObject,
    PdfPathSegmentType, PdfPathSegments, PdfQuadPoints, PdfRect, PdfRenderConfig, Pdfium,
    PdfiumError,
};

#[cfg(feature = "pdfium")]
pub(crate) const PDFIUM_BACKEND_NAME: &str = "pdfium";

#[cfg(feature = "pdfium")]
pub(crate) const PDFIUM_BACKEND_VERSION: &str = "pdfium-adapter-v1";

#[cfg(feature = "pdfium")]
pub(crate) const MAX_PDFIUM_TEXT_SEGMENT_NATIVE_TEXT_BYTES: u32 = 256 * 1024;

#[cfg(feature = "pdfium")]
pub(crate) const DEFAULT_OCR_RENDER_WIDTH: i32 = 1600;

#[cfg(feature = "pdfium")]
pub(crate) const MAX_OCR_RENDER_HEIGHT: i32 = 2400;

#[cfg(all(test, feature = "pdfium"))]
pub(crate) static PDFIUM_TEST_FILE_LOAD_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "pdfium")]
thread_local! {
    pub(crate) static PDFIUM_RUNTIME: OnceCell<&'static Pdfium> = const { OnceCell::new() };
}

#[cfg(all(test, feature = "pdfium"))]
pub(crate) fn reset_pdfium_test_file_load_count() {
    PDFIUM_TEST_FILE_LOAD_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(all(test, feature = "pdfium"))]
pub(crate) fn pdfium_test_file_load_count() -> u64 {
    PDFIUM_TEST_FILE_LOAD_COUNT.load(Ordering::Relaxed)
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_ocr_check_rendered_image_output(
    pdf: &Path,
    page_index: u32,
    ocr: OcrOptions<'_>,
    strict: bool,
) -> OcrCheckOutput {
    if let Some(http_url) = ocr.http_url {
        return pdfium_ocr_check_rendered_image_http_output(pdf, page_index, http_url, ocr, strict);
    }

    let timeout_ms = duration_millis(ocr.timeout);
    let started = Instant::now();
    let Some(command) = ocr.command else {
        return OcrCheckOutput {
            report_version: OCR_CHECK_REPORT_VERSION,
            parser_name: PARSER_NAME,
            parser_version: PARSER_VERSION,
            strict,
            pdf: pdf.display().to_string(),
            page_index,
            adapter: "ocr_command_rendered_image",
            passed: false,
            success: false,
            command: None,
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
            error: Some("ocr-check requires --ocr-command".to_string()),
            error_kind: Some("missing_adapter"),
        };
    };

    let (rendered_path, render_us) = match render_pdfium_pdf_page_to_temp_ppm(pdf, page_index) {
        Ok(rendered) => rendered,
        Err(error) => {
            return OcrCheckOutput {
                report_version: OCR_CHECK_REPORT_VERSION,
                parser_name: PARSER_NAME,
                parser_version: PARSER_VERSION,
                strict,
                pdf: pdf.display().to_string(),
                page_index,
                adapter: "ocr_command_rendered_image",
                passed: false,
                success: false,
                command: Some(command.display().to_string()),
                http_url: None,
                sidecar_path: None,
                exit_status: None,
                timed_out: false,
                timeout_ms,
                wall_us: started.elapsed().as_micros(),
                render_us: 0,
                output_bytes: 0,
                stdout_sha256: None,
                stdout_line_count: 0,
                stdout_word_count: 0,
                stderr_bytes: 0,
                empty_output: true,
                stderr_preview: None,
                error: Some(format!("{error:#}")),
                error_kind: Some(pdfium_rendered_ocr_check_error_kind(&error)),
            };
        }
    };

    let mut process = ProcessCommand::new(command);
    process.arg(&rendered_path).arg(page_index.to_string());
    let command_result = command_output_with_timeout(process, ocr.timeout);
    let cleanup_error = fs::remove_file(&rendered_path)
        .with_context(|| format!("remove temporary OCR image {}", rendered_path.display()))
        .err();
    let wall_us = started.elapsed().as_micros();

    match command_result {
        Ok(timed_output) => {
            let output = timed_output.output;
            let command_success = output.status.success() && !timed_output.timed_out;
            let empty_output = command_success && output.stdout.is_empty();
            let cleanup_failed_after_success = command_success && cleanup_error.is_some();
            let success = command_success && !cleanup_failed_after_success;
            let error_kind = if cleanup_failed_after_success {
                Some("cleanup_failed")
            } else if empty_output {
                Some("empty_output")
            } else {
                baseline_process_error_kind(&output, timed_output.timed_out)
            };
            let error = if let Some(error) = cleanup_error.filter(|_| cleanup_failed_after_success)
            {
                Some(format!("{error:#}"))
            } else {
                ocr_command_check_error(&output, timed_output.timed_out, empty_output)
            };

            OcrCheckOutput {
                report_version: OCR_CHECK_REPORT_VERSION,
                parser_name: PARSER_NAME,
                parser_version: PARSER_VERSION,
                strict,
                pdf: pdf.display().to_string(),
                page_index,
                adapter: "ocr_command_rendered_image",
                passed: success && !empty_output,
                success,
                command: Some(command.display().to_string()),
                http_url: None,
                sidecar_path: None,
                exit_status: output.status.code(),
                timed_out: timed_output.timed_out,
                timeout_ms,
                wall_us,
                render_us,
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
            adapter: "ocr_command_rendered_image",
            passed: false,
            success: false,
            command: Some(command.display().to_string()),
            http_url: None,
            sidecar_path: None,
            exit_status: None,
            timed_out: false,
            timeout_ms,
            wall_us,
            render_us,
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

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_ocr_check_rendered_image_http_output(
    pdf: &Path,
    page_index: u32,
    http_url: &str,
    ocr: OcrOptions<'_>,
    strict: bool,
) -> OcrCheckOutput {
    let timeout_ms = duration_millis(ocr.timeout);
    let started = Instant::now();
    let adapter = "ocr_http_rendered_image";
    let (rendered_path, render_us) = match render_pdfium_pdf_page_to_temp_ppm(pdf, page_index) {
        Ok(rendered) => rendered,
        Err(error) => {
            return OcrCheckOutput {
                report_version: OCR_CHECK_REPORT_VERSION,
                parser_name: PARSER_NAME,
                parser_version: PARSER_VERSION,
                strict,
                pdf: pdf.display().to_string(),
                page_index,
                adapter,
                passed: false,
                success: false,
                command: None,
                http_url: Some(http_url.to_string()),
                sidecar_path: None,
                exit_status: None,
                timed_out: false,
                timeout_ms,
                wall_us: started.elapsed().as_micros(),
                render_us: 0,
                output_bytes: 0,
                stdout_sha256: None,
                stdout_line_count: 0,
                stdout_word_count: 0,
                stderr_bytes: 0,
                empty_output: true,
                stderr_preview: None,
                error: Some(format!("{error:#}")),
                error_kind: Some(pdfium_rendered_ocr_check_error_kind(&error)),
            };
        }
    };

    let response_result = run_ocr_http_request(
        http_url,
        OcrHttpInput::RenderedImage(&rendered_path),
        page_index,
        ocr.timeout,
    );
    let cleanup_error = fs::remove_file(&rendered_path)
        .with_context(|| format!("remove temporary OCR image {}", rendered_path.display()))
        .err();
    let wall_us = started.elapsed().as_micros();

    match response_result {
        Ok(response) => {
            let status_success = response
                .status_code
                .is_some_and(|status| (200..300).contains(&status));
            let decoded_body = status_success.then(|| decode_ocr_http_response_body(&response));
            let output_body = match decoded_body.as_ref() {
                Some(Ok(text)) => text.as_bytes(),
                _ => response.body.as_slice(),
            };
            let empty_output = status_success
                && matches!(decoded_body.as_ref(), Some(Ok(text)) if text.is_empty());
            let decode_failed = matches!(decoded_body, Some(Err(_)));
            let cleanup_failed_after_success = status_success && cleanup_error.is_some();
            let success = status_success && !cleanup_failed_after_success;
            let error_kind = if cleanup_failed_after_success {
                Some("cleanup_failed")
            } else if decode_failed {
                Some("http_response_decode_failed")
            } else if empty_output {
                Some("empty_output")
            } else if status_success {
                None
            } else {
                Some("http_status_failed")
            };
            let error = if let Some(error) = cleanup_error.filter(|_| cleanup_failed_after_success)
            {
                Some(format!("{error:#}"))
            } else if let Some(Err(error)) = decoded_body.as_ref() {
                Some(format!("{error:#}"))
            } else if empty_output {
                Some("OCR HTTP output was empty".to_string())
            } else if status_success {
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
                adapter,
                passed: success && !empty_output && !decode_failed,
                success,
                command: None,
                http_url: Some(http_url.to_string()),
                sidecar_path: None,
                exit_status: response.status_code.map(i32::from),
                timed_out: false,
                timeout_ms,
                wall_us,
                render_us,
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
            adapter,
            passed: false,
            success: false,
            command: None,
            http_url: Some(http_url.to_string()),
            sidecar_path: None,
            exit_status: None,
            timed_out: ocr_http_error_kind(&error) == "timeout",
            timeout_ms,
            wall_us,
            render_us,
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

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_rendered_ocr_check_error_kind(error: &anyhow::Error) -> &'static str {
    let error = format!("{error:#}");
    if error.contains("page index") && error.contains("not found") {
        "page_not_found"
    } else {
        "render_failed"
    }
}

#[cfg(feature = "pdfium")]
pub(crate) struct PdfiumBackend;

#[cfg(feature = "pdfium")]
pub(crate) struct PdfiumDocument {
    pub(crate) pdf_document: PdfDocument<'static>,
    pub(crate) page_count: usize,
}

#[cfg(feature = "pdfium")]
pub(crate) fn bind_pdfium_runtime() -> Result<Pdfium> {
    pdfium_auto::bind_pdfium_silent().context("bind PDFium runtime")
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_runtime() -> Result<&'static Pdfium> {
    PDFIUM_RUNTIME.with(|runtime| {
        if let Some(pdfium) = runtime.get() {
            return Ok(*pdfium);
        }

        let pdfium = Box::leak(Box::new(bind_pdfium_runtime()?));
        runtime
            .set(pdfium)
            .map_err(|_| anyhow!("PDFium runtime already initialized"))?;
        Ok(pdfium)
    })
}

#[cfg(feature = "pdfium")]
pub(crate) fn load_pdfium_document_from_file(path: &Path) -> Result<PdfDocument<'static>> {
    #[cfg(all(test, feature = "pdfium"))]
    PDFIUM_TEST_FILE_LOAD_COUNT.fetch_add(1, Ordering::Relaxed);

    pdfium_runtime()?
        .load_pdf_from_file(path, None)
        .with_context(|| format!("load PDF with PDFium {}", path.display()))
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_page_count(document: &PdfDocument<'_>) -> Result<usize> {
    Ok(usize::from(document.pages().len()))
}

#[cfg(feature = "pdfium")]
pub(crate) fn extract_pdfium_pages(
    document: &PdfiumDocument,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
) -> Result<Vec<ExtractedPage>> {
    (0..document.page_count)
        .map(|page_index| {
            extract_pdfium_loaded_page(
                &document.pdf_document,
                source_path,
                ocr,
                options,
                page_index as u32,
            )
        })
        .collect()
}

#[cfg(feature = "pdfium")]
pub(crate) fn extract_pdfium_page_by_index(
    document: &PdfiumDocument,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
    page_index: u32,
) -> Result<ExtractedPage> {
    if page_index as usize >= document.page_count {
        bail!("page index {page_index} not found");
    }

    extract_pdfium_loaded_page(
        &document.pdf_document,
        source_path,
        ocr,
        options,
        page_index,
    )
}

#[cfg(feature = "pdfium")]
pub(crate) fn extract_pdfium_loaded_page(
    document: &PdfDocument<'_>,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
    page_index: u32,
) -> Result<ExtractedPage> {
    let open_start = Instant::now();
    let page = document
        .pages()
        .get(u16::try_from(page_index).context("page index exceeds PDFium range")?)
        .with_context(|| format!("load PDFium page {page_index}"))?;
    let open_us = open_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;

    let dimensions = PageDimensions::new(page.width().value, page.height().value);
    let rotation_degrees = page
        .rotation()
        .map(|rotation| rotation.as_degrees() as i16)
        .unwrap_or_default();

    let native_extract_start = Instant::now();
    let text_page = page
        .text()
        .map_err(map_pdfium_text_error)
        .with_context(|| format!("extract PDFium native text from page {page_index}"))?;
    let native_text = text_page.all();
    let native_extract_us = native_extract_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;

    let native_text_bytes = native_text.trim().len() as u32;
    let can_extract_positioned_spans =
        should_extract_pdfium_text_segments(native_text_bytes, rotation_degrees);
    let span_geometry_capped = options.span_geometry && !can_extract_positioned_spans;
    let native_spans = if options.span_geometry && can_extract_positioned_spans {
        partially_compatible_positioned_text_spans(
            &native_text,
            extract_pdfium_text_segments(&text_page, &dimensions),
        )
    } else {
        Vec::new()
    };
    let bbox_overlap_ratio = positioned_bbox_overlap_ratio(&native_spans);
    let native_span_count = native_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        .max(native_spans.len())
        .max((native_text_bytes > 0) as usize) as u32;
    let glyph_count = native_text.chars().filter(|ch| !ch.is_whitespace()).count() as u32;
    let image_artifacts = pdfium_image_artifacts(&page, &dimensions);
    let image_area_ratio = image_artifact_coverage_ratio(&image_artifacts, &dimensions);
    let table_start = Instant::now();
    let table_line_density =
        combined_table_line_density(&native_text, || pdfium_ruled_table_line_density(&page));
    let table_us = table_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;

    let signals = PageSignals {
        page_index,
        dimensions: dimensions.clone(),
        native_span_count,
        native_text_bytes,
        glyph_count,
        image_area_ratio,
        duplicate_char_ratio: duplicate_char_ratio(&native_text),
        bbox_overlap_ratio,
        broken_encoding_ratio: broken_encoding_ratio(&native_text),
        rotation_degrees,
        table_line_density,
        annotation_count: page.annotations().len() as u32,
        form_field_count: 0,
        huge_object_count: 0,
        span_geometry_capped,
    };
    let (ocr_text, render_us, ocr_us) =
        load_pdfium_ocr_if_needed(source_path, ocr, &signals, &page)?;
    let ruling_lines = if signals.table_line_density > 0.0 {
        pdfium_ruling_lines(&page, &dimensions)
    } else {
        Vec::new()
    };
    if std::env::var_os("GLYPHRUSH_DEBUG_RULINGS").is_some() {
        eprintln!(
            "debug-rulings p{page_index}: density={table_line_density} lines={}",
            ruling_lines.len()
        );
        for line in &ruling_lines {
            eprintln!(
                "debug-rulings p{page_index}:   {:?} pos={:.1} extent={:.1}..{:.1}",
                line.orientation, line.position, line.start, line.end
            );
        }
    }

    Ok(ExtractedPage {
        page_index,
        dimensions,
        native_text,
        native_spans,
        ruling_lines,
        image_artifacts,
        signals,
        ocr_text,
        timings: PageTimings {
            open_us,
            native_extract_us,
            table_us,
            render_us,
            ocr_us,
            ..PageTimings::default()
        },
    })
}

#[cfg(feature = "pdfium")]
pub(crate) fn map_pdfium_text_error(error: PdfiumError) -> anyhow::Error {
    anyhow!(error)
}

#[cfg(feature = "pdfium")]
pub(crate) fn should_extract_pdfium_text_segments(
    native_text_bytes: u32,
    rotation_degrees: i16,
) -> bool {
    native_text_bytes <= MAX_PDFIUM_TEXT_SEGMENT_NATIVE_TEXT_BYTES
        && rotation_degrees.rem_euclid(360) == 0
}

#[cfg(feature = "pdfium")]
pub(crate) fn extract_pdfium_text_segments(
    text_page: &PdfPageText<'_>,
    dimensions: &PageDimensions,
) -> Vec<ExtractedTextSpan> {
    text_page
        .segments()
        .iter()
        .filter_map(|segment| {
            let text = segment.text();
            if text.trim().is_empty() {
                return None;
            }

            pdfium_text_segment_bbox(segment.bounds(), dimensions)
                .map(|bbox| ExtractedTextSpan { text, bbox })
        })
        .collect()
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_text_segment_bbox(
    bounds: PdfRect,
    dimensions: &PageDimensions,
) -> Option<BBox> {
    let x0 = bounds.left().value.clamp(0.0, dimensions.width);
    let x1 = bounds.right().value.clamp(0.0, dimensions.width);
    let y0 = (dimensions.height - bounds.top().value).clamp(0.0, dimensions.height);
    let y1 = (dimensions.height - bounds.bottom().value).clamp(0.0, dimensions.height);

    if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) {
        return None;
    }

    let left = x0.min(x1);
    let right = x0.max(x1);
    let top = y0.min(y1);
    let bottom = y0.max(y1);
    (right - left > f32::EPSILON && bottom - top > f32::EPSILON).then_some(BBox {
        x0: left,
        y0: top,
        x1: right,
        y1: bottom,
    })
}

#[cfg(feature = "pdfium")]
pub(crate) fn load_pdfium_ocr_if_needed(
    source_path: &Path,
    ocr: OcrOptions<'_>,
    signals: &PageSignals,
    page: &PdfPage<'_>,
) -> Result<(Option<String>, u64, u64)> {
    if ocr.command_input != OcrCommandInput::RenderedImage {
        let ocr_start = Instant::now();
        let text = load_ocr_if_needed(source_path, ocr, signals)?;
        let ocr_us = ocr_start.elapsed().as_micros().max(1).min(u64::MAX as u128) as u64;
        return Ok((text, 0, ocr_us));
    }

    if !classify_page(signals).run_ocr {
        return Ok((None, 0, 0));
    }

    if ocr.command.is_none() && ocr.http_url.is_none() {
        return Ok((None, 0, 0));
    }

    let (rendered_path, render_us) =
        render_pdfium_page_to_temp_ppm(page, source_path, signals.page_index)?;
    let ocr_start = Instant::now();
    let text_result = if let Some(command) = ocr.command {
        run_ocr_command(command, &rendered_path, signals.page_index, ocr.timeout)
    } else if let Some(http_url) = ocr.http_url {
        run_ocr_http_with_input(
            http_url,
            OcrHttpInput::RenderedImage(&rendered_path),
            signals.page_index,
            ocr.timeout,
        )
    } else {
        unreachable!("rendered-image OCR checked for an adapter before rendering")
    };
    let adapter_succeeded = text_result.is_ok();
    let cleanup_result = fs::remove_file(&rendered_path)
        .with_context(|| format!("remove temporary OCR image {}", rendered_path.display()));
    if adapter_succeeded {
        cleanup_result?;
    }
    let text = text_result?;
    let ocr_us = ocr_start.elapsed().as_micros().max(1).min(u64::MAX as u128) as u64;

    if text.is_empty() {
        return Ok((None, render_us, ocr_us));
    }

    Ok((Some(text), render_us, ocr_us))
}

#[cfg(feature = "pdfium")]
pub(crate) fn render_pdfium_pdf_page_to_temp_ppm(
    pdf: &Path,
    page_index: u32,
) -> Result<(PathBuf, u64)> {
    let document = load_pdfium_document_from_file(pdf)?;
    let page_count = pdfium_page_count(&document)?;
    if page_index as usize >= page_count {
        bail!("page index {page_index} not found");
    }
    let page = document
        .pages()
        .get(u16::try_from(page_index).context("page index exceeds PDFium range")?)
        .with_context(|| format!("load PDFium page {page_index}"))?;

    render_pdfium_page_to_temp_ppm(&page, pdf, page_index)
}

#[cfg(feature = "pdfium")]
pub(crate) fn render_pdfium_page_to_temp_ppm(
    page: &PdfPage<'_>,
    source_path: &Path,
    page_index: u32,
) -> Result<(PathBuf, u64)> {
    let render_start = Instant::now();
    let rendered_path = rendered_ocr_temp_path(source_path, page_index);
    let config = PdfRenderConfig::new()
        .set_target_width(DEFAULT_OCR_RENDER_WIDTH)
        .set_maximum_height(MAX_OCR_RENDER_HEIGHT)
        .set_format(PdfBitmapFormat::BGRA)
        .set_reverse_byte_order(true)
        .render_annotations(true)
        .render_form_data(true);
    let bitmap = page
        .render_with_config(&config)
        .with_context(|| format!("render PDFium page {page_index} for OCR"))?;
    let rgba = bitmap.as_rgba_bytes();
    write_rgba_ppm(
        &rendered_path,
        bitmap.width() as usize,
        bitmap.height() as usize,
        &rgba,
    )?;
    let render_us = render_start
        .elapsed()
        .as_micros()
        .max(1)
        .min(u64::MAX as u128) as u64;

    Ok((rendered_path, render_us))
}

#[cfg(feature = "pdfium")]
pub(crate) fn rendered_ocr_temp_path(source_path: &Path, page_index: u32) -> PathBuf {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_temp_stem)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "document".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    std::env::temp_dir().join(format!(
        "glyphrush-ocr-{stem}-p{page_index:06}-{}-{nanos}.ppm",
        std::process::id()
    ))
}

#[cfg(feature = "pdfium")]
pub(crate) fn sanitize_temp_stem(stem: &str) -> String {
    stem.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(feature = "pdfium")]
pub(crate) fn write_rgba_ppm(path: &Path, width: usize, height: usize, rgba: &[u8]) -> Result<()> {
    if width == 0 || height == 0 {
        bail!("rendered OCR image has empty dimensions");
    }
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .context("rendered OCR image dimensions overflow")?;
    if rgba.len() != expected_len {
        bail!(
            "rendered OCR image buffer size mismatch: expected {expected_len} bytes, got {}",
            rgba.len()
        );
    }

    let mut file =
        fs::File::create(path).with_context(|| format!("create OCR image {}", path.display()))?;
    write!(file, "P6\n{width} {height}\n255\n")
        .with_context(|| format!("write OCR image header {}", path.display()))?;
    for pixel in rgba.chunks_exact(4) {
        file.write_all(&pixel[..3])
            .with_context(|| format!("write OCR image pixels {}", path.display()))?;
    }

    Ok(())
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_image_artifacts(
    page: &PdfPage<'_>,
    dimensions: &PageDimensions,
) -> Vec<ExtractedImage> {
    let mut images = Vec::new();
    let mut image_index = 0;

    for object in page.objects().iter() {
        pdfium_collect_image_artifact(&object, dimensions, &mut image_index, &mut images);
    }

    images
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_ruled_table_line_density(page: &PdfPage<'_>) -> f32 {
    let mut stroked_ruling_segments = 0u32;

    for object in page.objects().iter() {
        stroked_ruling_segments += pdfium_object_ruling_segments(&object);
        if stroked_ruling_segments >= RULED_TABLE_SATURATION_SEGMENTS {
            return 1.0;
        }
    }

    ruling_density(stroked_ruling_segments)
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_object_ruling_segments(object: &PdfPageObject<'_>) -> u32 {
    let mut stroked_ruling_segments = 0u32;

    if let Some(path) = object.as_path_object()
        && path.is_stroked().unwrap_or(true)
    {
        let mut current = None;
        for segment in path.segments().iter() {
            match segment.segment_type() {
                PdfPathSegmentType::MoveTo => {
                    current = Some((segment.x().value, segment.y().value));
                }
                PdfPathSegmentType::LineTo => {
                    let end = (segment.x().value, segment.y().value);
                    if let Some(start) = current
                        && is_ruling_segment(start, end)
                    {
                        stroked_ruling_segments += 1;
                    }
                    current = Some(end);
                }
                PdfPathSegmentType::BezierTo | PdfPathSegmentType::Unknown => {
                    current = None;
                }
            }
        }

        if stroked_ruling_segments == 0
            && let Ok(bounds) = object.bounds()
            && pdfium_bounds_look_like_ruling(bounds)
        {
            stroked_ruling_segments += 1;
        }
    }

    if let Some(form) = object.as_x_object_form_object() {
        for child in form.iter() {
            stroked_ruling_segments += pdfium_object_ruling_segments(&child);
            if stroked_ruling_segments >= RULED_TABLE_SATURATION_SEGMENTS {
                break;
            }
        }
    }

    stroked_ruling_segments
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_bounds_look_like_ruling(bounds: PdfQuadPoints) -> bool {
    is_ruling_segment(
        (bounds.left().value, bounds.bottom().value),
        (bounds.right().value, bounds.top().value),
    )
}

#[cfg(feature = "pdfium")]
pub(crate) type PdfiumRulingTransform = [f32; 6];

#[cfg(feature = "pdfium")]
pub(crate) const PDFIUM_RULING_IDENTITY: PdfiumRulingTransform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Collects positioned horizontal/vertical stroked ruling segments in
/// page-local top-left coordinates, bounded by `MAX_EXTRACTED_RULING_LINES`.
/// Cheap metadata for ruled-grid table recovery; never rendered.
#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_ruling_lines(
    page: &PdfPage<'_>,
    dimensions: &PageDimensions,
) -> Vec<ExtractedRulingLine> {
    let mut ruling_lines = Vec::new();

    for object in page.objects().iter() {
        pdfium_collect_object_ruling_lines(
            &object,
            &PDFIUM_RULING_IDENTITY,
            dimensions,
            &mut ruling_lines,
        );
        if ruling_lines.len() >= MAX_EXTRACTED_RULING_LINES {
            ruling_lines.truncate(MAX_EXTRACTED_RULING_LINES);
            break;
        }
    }

    ruling_lines
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_compose_ruling_transform(
    parent: &PdfiumRulingTransform,
    matrix: &PdfiumMatrix,
) -> PdfiumRulingTransform {
    let [pa, pb, pc, pd, pe, pf] = *parent;
    let (ma, mb, mc, md, me, mf) = (
        matrix.a(),
        matrix.b(),
        matrix.c(),
        matrix.d(),
        matrix.e(),
        matrix.f(),
    );

    [
        ma * pa + mb * pc,
        ma * pb + mb * pd,
        mc * pa + md * pc,
        mc * pb + md * pd,
        me * pa + mf * pc + pe,
        me * pb + mf * pd + pf,
    ]
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_apply_ruling_transform(
    transform: &PdfiumRulingTransform,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let [a, b, c, d, e, f] = *transform;
    (a * x + c * y + e, b * x + d * y + f)
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_collect_object_ruling_lines(
    object: &PdfPageObject<'_>,
    parent_transform: &PdfiumRulingTransform,
    dimensions: &PageDimensions,
    ruling_lines: &mut Vec<ExtractedRulingLine>,
) {
    let transform = match object.matrix() {
        Ok(matrix) => pdfium_compose_ruling_transform(parent_transform, &matrix),
        Err(_) => *parent_transform,
    };

    if let Some(path) = object.as_path_object()
        && path.is_stroked().unwrap_or(true)
    {
        let collected_before = ruling_lines.len();
        let mut current = None;
        for segment in path.segments().iter() {
            match segment.segment_type() {
                PdfPathSegmentType::MoveTo => {
                    current = Some(pdfium_apply_ruling_transform(
                        &transform,
                        segment.x().value,
                        segment.y().value,
                    ));
                }
                PdfPathSegmentType::LineTo => {
                    let end = pdfium_apply_ruling_transform(
                        &transform,
                        segment.x().value,
                        segment.y().value,
                    );
                    if let Some(start) = current
                        && is_ruling_segment(start, end)
                        && let Some(line) = ruling_line_from_segment(start, end, dimensions)
                    {
                        ruling_lines.push(line);
                    }
                    current = Some(end);
                }
                PdfPathSegmentType::BezierTo | PdfPathSegmentType::Unknown => {
                    current = None;
                }
            }
        }

        // Object bounds are already in page space; no transform needed.
        if ruling_lines.len() == collected_before
            && let Ok(bounds) = object.bounds()
            && pdfium_bounds_look_like_ruling(bounds)
            && let Some(line) = ruling_line_from_segment(
                (bounds.left().value, bounds.bottom().value),
                (bounds.right().value, bounds.top().value),
                dimensions,
            )
        {
            ruling_lines.push(line);
        }
    }

    if let Some(form) = object.as_x_object_form_object() {
        for child in form.iter() {
            pdfium_collect_object_ruling_lines(&child, &transform, dimensions, ruling_lines);
            if ruling_lines.len() >= MAX_EXTRACTED_RULING_LINES {
                break;
            }
        }
    }
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_collect_image_artifact(
    object: &PdfPageObject<'_>,
    dimensions: &PageDimensions,
    image_index: &mut usize,
    images: &mut Vec<ExtractedImage>,
) {
    if (object.as_image_object().is_some() || pdfium_object_form_contains_image(object))
        && let Ok(bounds) = object.bounds()
        && let Some(image) = pdfium_image_from_bounds(bounds, dimensions, *image_index)
    {
        images.push(image);
        *image_index += 1;
    }
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_object_form_contains_image(object: &PdfPageObject<'_>) -> bool {
    let Some(form) = object.as_x_object_form_object() else {
        return false;
    };

    pdfium_form_contains_image(form)
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_form_contains_image(form: &PdfPageXObjectFormObject<'_>) -> bool {
    form.iter().any(|child| {
        child.as_image_object().is_some()
            || child
                .as_x_object_form_object()
                .is_some_and(pdfium_form_contains_image)
    })
}

#[cfg(feature = "pdfium")]
pub(crate) fn pdfium_image_from_bounds(
    bounds: PdfQuadPoints,
    dimensions: &PageDimensions,
    image_index: usize,
) -> Option<ExtractedImage> {
    let rect = bounds.to_rect();
    let x0 = rect.left().value.min(rect.right().value).max(0.0);
    let x1 = rect
        .left()
        .value
        .max(rect.right().value)
        .min(dimensions.width);
    let y0 = rect.bottom().value.min(rect.top().value).max(0.0);
    let y1 = rect
        .bottom()
        .value
        .max(rect.top().value)
        .min(dimensions.height);

    if ![x0, x1, y0, y1].into_iter().all(f32::is_finite) || x1 <= x0 || y1 <= y0 {
        return None;
    }

    let page_area = dimensions.width.max(0.0) * dimensions.height.max(0.0);
    let image_area = (x1 - x0) * (y1 - y0);
    let area_ratio = if page_area > f32::EPSILON {
        (image_area / page_area).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Some(ExtractedImage {
        bbox: BBox { x0, y0, x1, y1 },
        area_ratio,
        source_name: Some(format!("pdfium-image-{image_index:06}")),
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[cfg(feature = "pdfium")]
    #[test]
    fn pdfium_document_reuses_loaded_handle_for_full_document_extraction() -> Result<()> {
        reset_pdfium_test_file_load_count();
        let pdf_path = temp_pdf_path("pdfium-single-load");
        fs::write(
            &pdf_path,
            minimal_unit_pdf_with_streams(&[
                "BT /F1 12 Tf 72 720 Td (First page) Tj ET",
                "BT /F1 12 Tf 72 720 Td (Second page) Tj ET",
            ]),
        )
        .with_context(|| format!("write test PDF {}", pdf_path.display()))?;

        let backend = PdfiumBackend;
        let document = backend.load_document(&pdf_path)?;
        assert_eq!(backend.page_count(&document), 2);

        let pages = backend.extract_pages(
            &document,
            &pdf_path,
            OcrOptions::new(
                None,
                None,
                None,
                OcrCommandInput::PdfPage,
                DEFAULT_OCR_TIMEOUT_MS,
            )?,
            ExtractionOptions {
                span_geometry: false,
                page_jobs: 1,
            },
        )?;
        assert_eq!(pages.len(), 2);
        assert_eq!(
            pdfium_test_file_load_count(),
            1,
            "full-document PDFium extraction should use the document opened during load_document"
        );

        let _ = fs::remove_file(&pdf_path);
        Ok(())
    }

    #[cfg(feature = "pdfium")]
    fn temp_pdf_path(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{label}-{}-{suffix}.pdf", std::process::id()))
    }

    #[cfg(feature = "pdfium")]
    fn minimal_unit_pdf_with_streams(streams: &[&str]) -> Vec<u8> {
        let page_count = streams.len();
        assert!(page_count > 0);
        let font_object_id = 3 + page_count;
        let kids = (0..page_count)
            .map(|index| format!("{} 0 R", index + 3))
            .collect::<Vec<_>>()
            .join(" ");
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"),
        ];

        for (index, stream) in streams.iter().enumerate() {
            let content_object_id = 4 + page_count + index;
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_object_id} 0 R >> >> /Contents {content_object_id} 0 R >>"
            ));
            assert!(!stream.is_empty());
        }

        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
        for stream in streams {
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ));
        }

        let mut pdf = Vec::new();
        writeln!(&mut pdf, "%PDF-1.4").unwrap();

        let mut offsets = vec![0usize];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            writeln!(&mut pdf, "{} 0 obj", index + 1).unwrap();
            writeln!(&mut pdf, "{object}").unwrap();
            writeln!(&mut pdf, "endobj").unwrap();
        }

        let xref_start = pdf.len();
        writeln!(&mut pdf, "xref").unwrap();
        writeln!(&mut pdf, "0 {}", objects.len() + 1).unwrap();
        writeln!(&mut pdf, "0000000000 65535 f ").unwrap();
        for offset in offsets.iter().skip(1) {
            writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
        }
        writeln!(&mut pdf, "trailer").unwrap();
        writeln!(&mut pdf, "<< /Size {} /Root 1 0 R >>", objects.len() + 1).unwrap();
        writeln!(&mut pdf, "startxref").unwrap();
        writeln!(&mut pdf, "{xref_start}").unwrap();
        writeln!(&mut pdf, "%%EOF").unwrap();

        pdf
    }
}
