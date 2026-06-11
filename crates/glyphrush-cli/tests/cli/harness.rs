//! Shared CLI integration-test harness.
#![allow(dead_code, unused_imports)]

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::mpsc::{self, Receiver},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use sha2::Sha256;

use glyphrush_core::sha256_hex;

#[cfg(feature = "pdfium")]
#[derive(Debug)]
pub struct RenderedOcrHttpObservation {
    pub request: String,
    pub rendered_image_path: Option<String>,
    pub image_existed: bool,
    pub header: Option<String>,
    pub bytes: Option<usize>,
}

/// Run glyphrush, assert success, parse stdout as JSON.
pub fn glyphrush(args: &[&str]) -> Value {
    let output = glyphrush_raw(args);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("command output is json")
}

/// Run glyphrush without assertions (for failure-path and stderr checks).
pub fn glyphrush_raw(args: &[&str]) -> Output {
    build_command(args).output().expect("run glyphrush command")
}

/// Run glyphrush, assert success, return parsed JSON and raw output.
pub fn glyphrush_with_output(args: &[&str]) -> (Value, Output) {
    let output = glyphrush_raw(args);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("command output is json");
    (json, output)
}

/// Run glyphrush with extra environment variables.
pub fn glyphrush_raw_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = build_command(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run glyphrush command")
}

/// Run glyphrush from a working directory.
pub fn glyphrush_raw_in(dir: &Path, args: &[&str]) -> Output {
    build_command(args)
        .current_dir(dir)
        .output()
        .expect("run glyphrush command")
}

/// Run glyphrush from a working directory with extra environment variables.
pub fn glyphrush_raw_in_with_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = build_command(args);
    command.current_dir(dir);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run glyphrush command")
}

/// Parse JSON from stdout after a (typically failing) run; returns `(json, output)`.
pub fn glyphrush_expect_failure(args: &[&str]) -> (Value, Output) {
    let output = glyphrush_raw(args);
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("command failure output is json");
    (json, output)
}

fn build_command(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_glyphrush"));
    if !args.iter().copied().any(|arg| arg == "--backend") {
        command.args(["--backend", "lopdf"]);
    }
    command.args(args);
    command
}

/// Run glyphrush with exact args (no default `--backend lopdf` injection).
pub fn glyphrush_raw_exact(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_glyphrush"))
        .args(args)
        .output()
        .expect("run glyphrush command")
}

/// Run glyphrush with exact args, assert success, parse JSON.
pub fn glyphrush_exact(args: &[&str]) -> Value {
    let output = glyphrush_raw_exact(args);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("command output is json")
}

/// Builder for minimal synthetic PDF fixtures used across CLI tests.
pub struct TestPdf {
    catalog_suffix: String,
    pages_suffix: String,
    page_media_box: Option<String>,
    page_crop_box: Option<String>,
    page_resources: String,
    page_annots: Option<String>,
    content_streams: Vec<String>,
    trailing_objects: Vec<String>,
    trailer_suffix: String,
    text_position: (f32, f32),
}

impl TestPdf {
    pub fn new() -> Self {
        Self {
            catalog_suffix: String::new(),
            pages_suffix: String::new(),
            page_media_box: Some("[0 0 612 792]".to_string()),
            page_crop_box: None,
            page_resources: "<< /Font << /F1 4 0 R >> >>".to_string(),
            page_annots: None,
            content_streams: Vec::new(),
            trailing_objects: Vec::new(),
            trailer_suffix: String::new(),
            text_position: (72.0, 720.0),
        }
    }

    pub fn page_stream(mut self, stream: impl Into<String>) -> Self {
        self.content_streams = vec![stream.into()];
        self
    }

    pub fn pages(mut self, streams: &[&str]) -> Self {
        self.content_streams = streams.iter().map(|s| (*s).to_string()).collect();
        self
    }

    pub fn text_at(mut self, x: f32, y: f32) -> Self {
        self.text_position = (x, y);
        self
    }

    pub fn rotation(mut self, degrees: i16) -> Self {
        self.pages_suffix = format!(" /Rotate {degrees}");
        self
    }

    pub fn inherited_media_box(mut self, width: f32, height: f32) -> Self {
        self.pages_suffix = format!(" /MediaBox [0 0 {width} {height}]");
        self.page_media_box = None;
        self.text_position = (36.0, 360.0);
        self
    }

    pub fn page_media_box(mut self, width: f32, height: f32) -> Self {
        self.page_media_box = Some(format!("[0 0 {width} {height}]"));
        self.text_position = (36.0, 360.0);
        self
    }

    pub fn crop_box(mut self, width: f32, height: f32) -> Self {
        self.page_crop_box = Some(format!("[0 0 {width} {height}]"));
        self.text_position = (36.0, 360.0);
        self
    }

    pub fn nonzero_crop_box(mut self) -> Self {
        self.page_crop_box = Some("[100 100 406 496]".to_string());
        self
    }

    pub fn encrypted(mut self) -> Self {
        self.trailing_objects
            .push("<< /Filter /Standard /V 1 /R 2 /O <> /U <> /P -4 >>".to_string());
        self.trailer_suffix = format!(" /Encrypt {} 0 R", 6);
        self
    }

    pub fn image_xobject(mut self, name: &str, page_stream: impl Into<String>) -> Self {
        let image_data = "0";
        let image_object = format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        );
        self.page_resources = format!("<< /Font << /F1 4 0 R >> /XObject << /{name} 6 0 R >> >>");
        self.content_streams = vec![page_stream.into()];
        self.trailing_objects = vec![image_object];
        self
    }

    pub fn form_xobject(
        mut self,
        __form_stream: &str,
        page_stream: impl Into<String>,
        small: bool,
    ) -> Self {
        let image_data = "0";
        let form_scale = if small {
            "q 0.1 0 0 0.1 0 0 cm /Im1 Do Q"
        } else {
            "q 1 0 0 1 0 0 cm /Im1 Do Q"
        };
        let image_object = format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        );
        let form_object = format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Im1 7 0 R >> >> /Length {} >>\nstream\n{form_scale}\nendstream",
            form_scale.len()
        );
        self.page_resources = "<< /Font << /F1 4 0 R >> /XObject << /Fm1 6 0 R >> >>".to_string();
        self.content_streams = vec![page_stream.into()];
        self.trailing_objects = vec![form_object, image_object];
        self
    }

    pub fn widget_annotation(mut self, stream: impl Into<String>) -> Self {
        self.catalog_suffix = " /AcroForm << /Fields [6 0 R] >>".to_string();
        self.page_annots = Some(" /Annots [6 0 R]".to_string());
        self.content_streams = vec![stream.into()];
        self.trailing_objects = vec![
            "<< /Type /Annot /Subtype /Widget /Rect [72 690 180 720] /T (Name) >>".to_string(),
        ];
        self
    }

    pub fn text_annotation(mut self, stream: impl Into<String>) -> Self {
        self.page_annots = Some(" /Annots [6 0 R]".to_string());
        self.content_streams = vec![stream.into()];
        self.trailing_objects = vec![
            "<< /Type /Annot /Subtype /Text /Rect [72 690 180 720] /Contents (Unextracted annotation note) >>"
                .to_string(),
        ];
        self
    }

    pub fn catalog_acroform(mut self, stream: impl Into<String>) -> Self {
        self.catalog_suffix = " /AcroForm << /Fields [6 0 R] >>".to_string();
        self.content_streams = vec![stream.into()];
        self.trailing_objects =
            vec!["<< /FT /Tx /T (HiddenName) /V (Unextracted value) >>".to_string()];
        self
    }

    pub fn object(mut self, raw: impl Into<String>) -> Self {
        self.trailing_objects.push(raw.into());
        self
    }

    pub fn build(self) -> Vec<u8> {
        if self.content_streams.is_empty() {
            panic!("TestPdf requires at least one content stream");
        }

        if self.content_streams.len() == 1 && self.trailing_objects.is_empty() {
            return self.build_single_page_standard();
        }
        if self.content_streams.len() > 1 && self.trailing_objects.is_empty() {
            return self.build_multi_page();
        }
        self.build_single_page_with_trailing()
    }

    fn build_single_page_standard(&self) -> Vec<u8> {
        let stream = &self.content_streams[0];
        let page_media = self
            .page_media_box
            .as_ref()
            .map(|box_| format!(" /MediaBox {box_}"))
            .unwrap_or_default();
        let page_crop = self
            .page_crop_box
            .as_ref()
            .map(|box_| format!(" /CropBox {box_}"))
            .unwrap_or_default();
        let page_annots = self.page_annots.clone().unwrap_or_default();
        let objects = [
            format!(
                "<< /Type /Catalog /Pages 2 0 R{catalog} >>",
                catalog = self.catalog_suffix
            ),
            format!(
                "<< /Type /Pages /Kids [3 0 R] /Count 1{pages} >>",
                pages = self.pages_suffix
            ),
            format!(
                "<< /Type /Page /Parent 2 0 R{media}{crop} /Resources {resources}{annots} /Contents 5 0 R >>",
                media = page_media,
                crop = page_crop,
                resources = self.page_resources,
                annots = page_annots,
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ),
        ];
        assemble_pdf(&objects, &self.trailer_suffix)
    }

    fn build_single_page_with_trailing(&self) -> Vec<u8> {
        let stream = &self.content_streams[0];
        let page_media = self
            .page_media_box
            .as_ref()
            .map(|box_| format!(" /MediaBox {box_}"))
            .unwrap_or_default();
        let page_crop = self
            .page_crop_box
            .as_ref()
            .map(|box_| format!(" /CropBox {box_}"))
            .unwrap_or_default();
        let page_annots = self.page_annots.clone().unwrap_or_default();
        let mut objects = vec![
            format!(
                "<< /Type /Catalog /Pages 2 0 R{catalog} >>",
                catalog = self.catalog_suffix
            ),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!(
                "<< /Type /Page /Parent 2 0 R{media}{crop} /Resources {resources}{annots} /Contents 5 0 R >>",
                media = page_media,
                crop = page_crop,
                resources = self.page_resources,
                annots = page_annots,
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ),
        ];
        objects.extend(self.trailing_objects.clone());
        assemble_pdf(&objects, &self.trailer_suffix)
    }

    fn build_multi_page(&self) -> Vec<u8> {
        let page_count = self.content_streams.len();
        let font_object_id = 3 + page_count;
        let kids = (0..page_count)
            .map(|index| format!("{} 0 R", index + 3))
            .collect::<Vec<_>>()
            .join(" ");
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"),
        ];
        for (index, _stream) in self.content_streams.iter().enumerate() {
            let content_object_id = 4 + page_count + index;
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_object_id} 0 R >> >> /Contents {content_object_id} 0 R >>"
            ));
        }
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
        for stream in &self.content_streams {
            assert!(!stream.is_empty());
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ));
        }
        assemble_pdf(&objects, "")
    }
}

fn assemble_pdf(objects: &[String], trailer_suffix: &str) -> Vec<u8> {
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
    writeln!(
        &mut pdf,
        "<< /Size {} /Root 1 0 R{trailer_suffix} >>",
        objects.len() + 1,
        trailer_suffix = trailer_suffix
    )
    .unwrap();
    writeln!(&mut pdf, "startxref").unwrap();
    writeln!(&mut pdf, "{xref_start}").unwrap();
    writeln!(&mut pdf, "%%EOF").unwrap();
    pdf
}

fn escape_pdf_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

pub fn minimal_pdf(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().page_stream(stream).build()
}

pub fn minimal_pdf_with_stream(stream: &str) -> Vec<u8> {
    TestPdf::new().page_stream(stream).build()
}

pub fn minimal_encrypted_pdf(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().page_stream(stream).encrypted().build()
}

pub fn minimal_pdf_with_inherited_rotation(text: &str, rotation_degrees: i16) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new()
        .page_stream(stream)
        .rotation(rotation_degrees)
        .build()
}

pub fn minimal_pdf_with_inherited_media_box(text: &str, width: f32, height: f32) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 36 360 Td ({escaped}) Tj ET");
    TestPdf::new()
        .page_stream(stream)
        .inherited_media_box(width, height)
        .build()
}

pub fn minimal_pdf_with_page_crop_box(
    text: &str,
    media_width: f32,
    media_height: f32,
    crop_width: f32,
    crop_height: f32,
) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 36 360 Td ({escaped}) Tj ET");
    TestPdf::new()
        .page_stream(stream)
        .page_media_box(media_width, media_height)
        .crop_box(crop_width, crop_height)
        .build()
}

pub fn minimal_pdf_with_streams(streams: &[&str]) -> Vec<u8> {
    TestPdf::new().pages(streams).build()
}

pub fn minimal_pdf_with_nonzero_crop_box_stream(stream: &str) -> Vec<u8> {
    TestPdf::new()
        .page_stream(stream)
        .nonzero_crop_box()
        .build()
}

pub fn minimal_pdf_with_full_page_image_and_text(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("q 612 0 0 792 0 0 cm /Im1 Do Q BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().image_xobject("Im1", stream).build()
}

pub fn minimal_pdf_with_overlapping_half_page_images_and_text(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!(
        "q 306 0 0 792 0 0 cm /Im1 Do Q q 306 0 0 792 0 0 cm /Im1 Do Q BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET"
    );
    TestPdf::new().image_xobject("Im1", stream).build()
}

pub fn minimal_pdf_with_nonzero_crop_box_full_page_image_and_sparse_text(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream =
        format!("q 306 0 0 396 100 100 cm /Im1 Do Q BT /F1 24 Tf 120 450 Td ({escaped}) Tj ET");
    TestPdf::new()
        .nonzero_crop_box()
        .image_xobject("Im1", stream)
        .build()
}

pub fn minimal_pdf_with_nonzero_crop_box_off_crop_image_and_text(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("q 50 0 0 50 0 0 cm /Im1 Do Q BT /F1 24 Tf 120 450 Td ({escaped}) Tj ET");
    TestPdf::new()
        .nonzero_crop_box()
        .image_xobject("Im1", stream)
        .build()
}

pub fn minimal_pdf_with_form_wrapped_full_page_image_and_text(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let page_stream =
        format!("q 612 0 0 792 0 0 cm /Fm1 Do Q BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().form_xobject("", page_stream, false).build()
}

pub fn minimal_pdf_with_small_form_wrapped_image_and_text(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let page_stream =
        format!("q 612 0 0 792 0 0 cm /Fm1 Do Q BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().form_xobject("", page_stream, true).build()
}

pub fn minimal_pdf_with_widget_annotation(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().widget_annotation(stream).build()
}

pub fn minimal_pdf_with_text_annotation(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().text_annotation(stream).build()
}

pub fn minimal_pdf_with_catalog_acroform(text: &str) -> Vec<u8> {
    let escaped = escape_pdf_text(text);
    let stream = format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET");
    TestPdf::new().catalog_acroform(stream).build()
}

pub fn minimal_pdf_with_ruled_table() -> Vec<u8> {
    let stream = [
        "72 600 m 360 600 l S",
        "72 560 m 360 560 l S",
        "72 520 m 360 520 l S",
        "72 480 m 360 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "BT /F1 12 Tf 84 574 Td (Part Value) Tj ET",
        "BT /F1 12 Tf 84 534 Td (A 1) Tj ET",
        "BT /F1 12 Tf 84 494 Td (B 2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

pub fn minimal_pdf_with_aligned_whitespace_ruled_table() -> Vec<u8> {
    let stream = [
        "72 600 m 504 600 l S",
        "72 560 m 504 560 l S",
        "72 520 m 504 520 l S",
        "72 480 m 504 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "504 480 m 504 600 l S",
        "BT /F1 12 Tf 18 TL 84 574 Td (Part          Value        Note) Tj T* (A                          missing value) Tj T* (B             2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

pub fn minimal_pdf_with_positioned_ruled_table() -> Vec<u8> {
    let stream = [
        "72 600 m 360 600 l S",
        "72 560 m 360 560 l S",
        "72 520 m 360 520 l S",
        "72 480 m 360 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "BT /F1 12 Tf 84 574 Td (Part) Tj ET",
        "BT /F1 12 Tf 228 574 Td (Value) Tj ET",
        "BT /F1 12 Tf 84 534 Td (A) Tj ET",
        "BT /F1 12 Tf 228 534 Td (1) Tj ET",
        "BT /F1 12 Tf 84 494 Td (B) Tj ET",
        "BT /F1 12 Tf 228 494 Td (2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

pub fn minimal_pdf_with_positioned_ruled_table_empty_cells() -> Vec<u8> {
    let stream = [
        "72 600 m 504 600 l S",
        "72 560 m 504 560 l S",
        "72 520 m 504 520 l S",
        "72 480 m 504 480 l S",
        "72 480 m 72 600 l S",
        "216 480 m 216 600 l S",
        "360 480 m 360 600 l S",
        "504 480 m 504 600 l S",
        "BT /F1 12 Tf 84 574 Td (Part) Tj ET",
        "BT /F1 12 Tf 228 574 Td (Value) Tj ET",
        "BT /F1 12 Tf 372 574 Td (Note) Tj ET",
        "BT /F1 12 Tf 84 534 Td (A) Tj ET",
        "BT /F1 12 Tf 372 534 Td (missing value) Tj ET",
        "BT /F1 12 Tf 84 494 Td (B) Tj ET",
        "BT /F1 12 Tf 228 494 Td (2) Tj ET",
    ]
    .join("\n");
    minimal_pdf_with_stream(&stream)
}

pub fn write_test_pdf(label: &str, text: &str) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("sample.pdf");
    fs::write(&path, minimal_pdf(text)).unwrap();
    path
}

pub fn write_baseline_script(label: &str, body: &str) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("baseline.sh");
    write_executable(&path, &format!("#!/bin/sh\n{body}\n"));
    path
}

pub fn write_executable(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

pub fn write_ocr_command_script(label: &str, log_path: &std::path::Path) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("ocr-command.sh");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s:%s\\n' \"$1\" \"$2\" >> '{}'\nprintf 'Command OCR text page %s' \"$2\"\n",
            log_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

pub fn write_rendered_ocr_command_script(label: &str, log_path: &std::path::Path) -> PathBuf {
    let dir = temp_dir(label);
    let path = dir.join("rendered-ocr-command.sh");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ ! -f \"$1\" ]; then echo 'missing rendered image' >&2; exit 2; fi\nheader=$(dd if=\"$1\" bs=2 count=1 2>/dev/null)\nbytes=$(wc -c < \"$1\" | tr -d ' ')\nprintf '%s\\t%s\\t%s\\t%s\\n' \"$1\" \"$2\" \"$header\" \"$bytes\" >> '{}'\nprintf 'Rendered OCR text page %s' \"$2\"\n",
            log_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

pub fn start_ocr_http_server(
    response_text: &'static str,
) -> (String, Receiver<String>, JoinHandle<()>) {
    start_ocr_http_server_with_response("text/plain", response_text)
}

pub fn start_ocr_http_server_with_response(
    content_type: &'static str,
    response_body: &'static str,
) -> (String, Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind OCR HTTP test server");
    let url = format!(
        "http://{}/ocr",
        listener.local_addr().expect("read OCR HTTP server addr")
    );
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept OCR HTTP request");
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 512];
        loop {
            let read = stream.read(&mut chunk).expect("read OCR HTTP request");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if complete_http_request(&buffer) {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buffer).into_owned();
        request_tx.send(request).expect("send OCR HTTP request");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write OCR HTTP response");
    });

    (url, request_rx, server)
}

#[cfg(feature = "pdfium")]
pub fn start_rendered_ocr_http_server(
    response_text: &'static str,
) -> (String, Receiver<RenderedOcrHttpObservation>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind rendered OCR HTTP test server");
    listener
        .set_nonblocking(true)
        .expect("set rendered OCR HTTP server nonblocking");
    let url = format!(
        "http://{}/ocr",
        listener.local_addr().expect("read OCR HTTP server addr")
    );
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        request_tx
                            .send(RenderedOcrHttpObservation {
                                request: String::new(),
                                rendered_image_path: None,
                                image_existed: false,
                                header: None,
                                bytes: None,
                            })
                            .expect("send missing rendered OCR HTTP request observation");
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept rendered OCR HTTP request: {error}"),
            }
        };

        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 512];
        loop {
            let read = stream
                .read(&mut chunk)
                .expect("read rendered OCR HTTP request");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if complete_http_request(&buffer) {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buffer).into_owned();
        let body = http_request_body(&buffer);
        let rendered_image_path = serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("rendered_image_path")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        let (image_existed, header, bytes) = rendered_image_path
            .as_deref()
            .map(|path| {
                let path = Path::new(path);
                let image_existed = path.exists();
                let header = fs::read(path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.into_iter().take(2).collect()).ok());
                let bytes = fs::metadata(path)
                    .ok()
                    .map(|metadata| metadata.len() as usize);
                (image_existed, header, bytes)
            })
            .unwrap_or((false, None, None));
        request_tx
            .send(RenderedOcrHttpObservation {
                request,
                rendered_image_path,
                image_existed,
                header,
                bytes,
            })
            .expect("send rendered OCR HTTP request observation");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_text.len(),
            response_text
        );
        stream
            .write_all(response.as_bytes())
            .expect("write rendered OCR HTTP response");
    });

    (url, request_rx, server)
}

pub fn complete_http_request(buffer: &[u8]) -> bool {
    let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_end = header_end + 4;
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    buffer.len() >= header_end + content_length
}

#[cfg(feature = "pdfium")]
pub fn http_request_body(buffer: &[u8]) -> &[u8] {
    let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
        return &[];
    };
    &buffer[(header_end + 4)..]
}

pub fn source_modified_unix_ms(path: &std::path::Path) -> u64 {
    fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub fn rewrite_until_modified_ms_changes(path: &std::path::Path, bytes: &[u8]) -> u64 {
    let initial = source_modified_unix_ms(path);
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(20));
        fs::write(path, bytes).unwrap();
        let modified = source_modified_unix_ms(path);
        if modified != initial {
            return modified;
        }
    }
    panic!("failed to change modified timestamp for {}", path.display());
}

pub fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "glyphrush-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn expected_corpus_fingerprint(json: &Value) -> Value {
    let mut payload = String::from("glyphrush-corpus-v1\n");
    for document in json["documents"].as_array().unwrap() {
        payload.push_str(document["path"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(document["document_fingerprint"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(&document["page_count"].as_u64().unwrap().to_string());
        payload.push('\n');
    }
    Value::String(sha256_hex(payload))
}

pub fn expected_generated_manifest_corpus_fingerprint(json: &Value) -> Value {
    let mut payload = String::from("glyphrush-corpus-v1\n");
    for document in json["documents"].as_array().unwrap() {
        payload.push_str(document["path"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(document["document_fingerprint"].as_str().unwrap());
        payload.push('\t');
        payload.push_str(
            &document["expect"]["page_count"]
                .as_u64()
                .unwrap()
                .to_string(),
        );
        payload.push('\n');
    }
    Value::String(sha256_hex(payload))
}

pub fn capability<'a>(capabilities: &'a [Value], id: &str) -> &'a Value {
    capabilities
        .iter()
        .find(|capability| capability["id"] == id)
        .unwrap_or_else(|| panic!("missing capability {id}"))
}
