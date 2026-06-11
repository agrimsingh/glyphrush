use crate::*;

use std::{path::Path, time::Instant};

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct WarmBenchOutput {
    pub(crate) parser: &'static str,
    pub(crate) mode: &'static str,
    pub(crate) min_s: f64,
    pub(crate) median_s: f64,
    pub(crate) runs: usize,
}

pub(crate) fn run_warm_bench<B: PdfBackend>(
    backend: &B,
    pdf: &Path,
    runs: usize,
    warmup: usize,
) -> Result<()> {
    let ocr = OcrOptions::new(
        None,
        None,
        None,
        OcrCommandInput::PdfPage,
        DEFAULT_OCR_TIMEOUT_MS,
    )?;
    let options = ExtractionOptions {
        span_geometry: false,
        page_jobs: 1,
    };

    for _ in 0..warmup {
        warm_bench_once(backend, pdf, ocr, options)?;
    }

    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        warm_bench_once(backend, pdf, ocr, options)?;
        samples.push(start.elapsed().as_secs_f64());
    }

    write_json(&WarmBenchOutput {
        parser: "glyphrush",
        mode: "in_process",
        min_s: samples.iter().copied().fold(f64::INFINITY, f64::min),
        median_s: median_seconds(&samples),
        runs: samples.len(),
    })?;

    Ok(())
}

fn warm_bench_once<B: PdfBackend>(
    backend: &B,
    pdf: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
) -> Result<()> {
    let artifact = parse_pdf(backend, pdf, ocr, None, options)?;
    let text = plain_text_from_artifact(&artifact);
    if text.trim().is_empty() {
        anyhow::bail!("warm-bench parse produced no text");
    }
    Ok(())
}

fn median_seconds(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_seconds_handles_even_and_odd_lengths() {
        assert_eq!(median_seconds(&[1.0, 3.0]), 2.0);
        assert_eq!(median_seconds(&[1.0, 2.0, 3.0]), 2.0);
    }
}
