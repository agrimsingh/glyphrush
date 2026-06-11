use crate::*;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use glyphrush_core::{CacheStatus, DocumentArtifact, PageTimings, sha256_hex};
use serde::{Deserialize, Serialize};

pub(crate) const CACHE_SCHEMA_VERSION: &str = "glyphrush-cache-v43";

pub(crate) const CACHE_SNAPSHOT_VERSION: &str = "glyphrush-cache-snapshot-v1";

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CachedArtifactSnapshot {
    pub(crate) snapshot_version: String,
    pub(crate) cache_schema: String,
    pub(crate) cache_key: String,
    pub(crate) parser_name: String,
    pub(crate) parser_version: String,
    pub(crate) backend: String,
    pub(crate) backend_version: String,
    pub(crate) document_fingerprint: String,
    pub(crate) artifact: DocumentArtifact,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum CachedArtifactFile {
    Snapshot(CachedArtifactSnapshot),
    LegacyArtifact(DocumentArtifact),
}

pub(crate) struct CachedArtifactLoad {
    pub(crate) artifact: Option<DocumentArtifact>,
    pub(crate) ignored_warning: Option<String>,
}

impl CachedArtifactLoad {
    pub(crate) fn miss() -> Self {
        Self {
            artifact: None,
            ignored_warning: None,
        }
    }

    pub(crate) fn hit(artifact: DocumentArtifact) -> Self {
        Self {
            artifact: Some(artifact),
            ignored_warning: None,
        }
    }

    pub(crate) fn ignored(path: &Path, error: anyhow::Error) -> Self {
        let reason = error.to_string().replace('\n', " ");
        Self {
            artifact: None,
            ignored_warning: Some(format!(
                "cache_snapshot_ignored: {}: {reason}",
                path.display()
            )),
        }
    }
}

impl CachedArtifactSnapshot {
    pub(crate) fn from_artifact(cache_key: &str, artifact: &DocumentArtifact) -> Self {
        Self {
            snapshot_version: CACHE_SNAPSHOT_VERSION.to_string(),
            cache_schema: CACHE_SCHEMA_VERSION.to_string(),
            cache_key: cache_key.to_string(),
            parser_name: artifact.metadata.parser_name.clone(),
            parser_version: artifact.metadata.parser_version.clone(),
            backend: artifact.metadata.backend.clone(),
            backend_version: artifact.metadata.backend_version.clone(),
            document_fingerprint: artifact.document_fingerprint.clone(),
            artifact: artifact.clone(),
        }
    }

    pub(crate) fn into_artifact(
        self,
        expected_cache_key: &str,
        path: &Path,
    ) -> Result<DocumentArtifact> {
        if self.snapshot_version != CACHE_SNAPSHOT_VERSION {
            bail!(
                "cache snapshot {} has unsupported version {}",
                path.display(),
                self.snapshot_version
            );
        }
        if self.cache_schema != CACHE_SCHEMA_VERSION {
            bail!(
                "cache snapshot {} has unsupported schema {}",
                path.display(),
                self.cache_schema
            );
        }
        if self.cache_key != expected_cache_key {
            bail!(
                "cache snapshot {} key mismatch: expected {}, found {}",
                path.display(),
                expected_cache_key,
                self.cache_key
            );
        }
        if self.parser_name != PARSER_NAME || self.parser_version != PARSER_VERSION {
            bail!(
                "cache snapshot {} parser mismatch: expected {} {}, found {} {}",
                path.display(),
                PARSER_NAME,
                PARSER_VERSION,
                self.parser_name,
                self.parser_version
            );
        }
        if self.document_fingerprint != self.artifact.document_fingerprint {
            bail!(
                "cache snapshot {} document fingerprint mismatch",
                path.display()
            );
        }
        if self.backend != self.artifact.metadata.backend
            || self.backend_version != self.artifact.metadata.backend_version
        {
            bail!(
                "cache snapshot {} backend metadata mismatch",
                path.display()
            );
        }

        Ok(self.artifact)
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CacheProbeOutput {
    pub(crate) cold: CacheProbeRunOutput,
    pub(crate) warm: CacheProbeRunOutput,
    pub(crate) cache_key_match: bool,
    pub(crate) warm_speedup: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct CacheProbeRunOutput {
    pub(crate) cache_status: CacheStatus,
    pub(crate) wall_us: u128,
    pub(crate) pages_per_sec: f64,
    pub(crate) artifact_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) allocated_bytes_per_page: f64,
    pub(crate) text_output_bytes: u64,
    pub(crate) text_output_line_count: usize,
    pub(crate) text_output_word_count: usize,
    pub(crate) empty_text_output: bool,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) stage_timings_us: BenchStageTimings,
    pub(crate) page_latency_us: PageLatencySummary,
    pub(crate) route_counts: RouteCounts,
    pub(crate) route_latency_us: RouteLatencySummary,
    pub(crate) route_reason_counts: BTreeMap<String, u32>,
    pub(crate) fallback_pages: u32,
    pub(crate) ocr_required_pages: u32,
    pub(crate) ocr_applied_pages: u32,
    pub(crate) image_artifact_count: u32,
    pub(crate) image_artifact_pages: u32,
    pub(crate) quality_flag_counts: QualityFlagCounts,
    pub(crate) fallback_action_counts: FallbackActionCounts,
    pub(crate) warnings_count: usize,
    pub(crate) warnings: Vec<String>,
    pub(crate) cache_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusCacheProbeOutput {
    pub(crate) document_count: usize,
    pub(crate) cold_wall_us: u128,
    pub(crate) warm_wall_us: u128,
    pub(crate) cold_pages_per_sec: f64,
    pub(crate) warm_pages_per_sec: f64,
    pub(crate) cold_allocated_bytes: u64,
    pub(crate) warm_allocated_bytes: u64,
    pub(crate) cold_allocated_bytes_per_page: f64,
    pub(crate) warm_allocated_bytes_per_page: f64,
    pub(crate) cold_fallback_action_counts: FallbackActionCounts,
    pub(crate) warm_fallback_action_counts: FallbackActionCounts,
    pub(crate) cold_stage_timings_us: BenchStageTimings,
    pub(crate) warm_stage_timings_us: BenchStageTimings,
    pub(crate) warm_speedup: f64,
    pub(crate) cold_cache_misses: u32,
    pub(crate) warm_cache_hits: u32,
}

pub(crate) fn run_cache_probe<B: PdfBackend>(
    backend: &B,
    path: &Path,
    ocr: OcrOptions<'_>,
    cache_dir: &Path,
    options: ExtractionOptions,
    cold: CacheProbeRunOutput,
) -> Result<CacheProbeOutput> {
    let warm_config = BenchRunConfig {
        ocr,
        cache_dir: Some(cache_dir),
        cache_probe: false,
        jobs: 1,
        extraction: options,
        baselines: &[],
        requested_baseline_presets: &[],
        baseline_timeout: Duration::from_millis(DEFAULT_BASELINE_TIMEOUT_MS),
        require_quality: false,
        require_baselines: false,
        require_baseline_quality: false,
        require_coverage_preset: None,
        required_speedups: &[],
        required_speedup_claims: &[],
    };
    let warm_bench = bench_pdf(backend, path, warm_config, None)?;
    let warm = cache_probe_run_from_bench(&warm_bench);

    Ok(CacheProbeOutput {
        cache_key_match: cold.cache_key == warm.cache_key,
        warm_speedup: speedup(cold.wall_us, warm.wall_us),
        cold,
        warm,
    })
}

pub(crate) fn cache_probe_run_from_artifact(
    artifact: &DocumentArtifact,
    wall_us: u128,
    artifact_bytes: u64,
    allocated_bytes: u64,
    peak_rss_bytes: u64,
) -> CacheProbeRunOutput {
    let page_count = artifact.pages.len();
    let text_output_metrics = text_output_metrics_from_artifact(artifact);

    CacheProbeRunOutput {
        cache_status: artifact.global_diagnostics.cache_status.clone(),
        wall_us,
        pages_per_sec: pages_per_sec(page_count, wall_us),
        artifact_bytes,
        allocated_bytes,
        allocated_bytes_per_page: bytes_per_page(allocated_bytes, page_count),
        text_output_bytes: text_output_metrics.bytes,
        text_output_line_count: text_output_metrics.line_count,
        text_output_word_count: text_output_metrics.word_count,
        empty_text_output: text_output_metrics.empty,
        peak_rss_bytes,
        stage_timings_us: stage_timings_from_artifact(artifact),
        page_latency_us: page_latency_from_artifact(artifact),
        route_counts: route_counts_from_artifact(artifact),
        route_latency_us: route_latency_from_artifact(artifact),
        route_reason_counts: route_reason_counts_from_artifact(artifact),
        fallback_pages: artifact.global_diagnostics.fallback_pages,
        ocr_required_pages: artifact.global_diagnostics.ocr_required_pages,
        ocr_applied_pages: artifact.global_diagnostics.ocr_applied_pages,
        image_artifact_count: image_artifact_count_from_artifact(artifact),
        image_artifact_pages: image_artifact_pages_from_artifact(artifact),
        quality_flag_counts: quality_flag_counts_from_artifact(artifact),
        fallback_action_counts: fallback_action_counts_from_artifact(artifact),
        warnings_count: artifact.global_diagnostics.warnings.len(),
        warnings: artifact.global_diagnostics.warnings.clone(),
        cache_key: artifact.global_diagnostics.cache_key.clone(),
    }
}

pub(crate) fn aggregate_corpus_cache_probe(
    documents: &[CorpusBenchDocument],
    page_count: usize,
) -> Option<CorpusCacheProbeOutput> {
    let probes = documents
        .iter()
        .filter_map(|document| document.cache_probe.as_ref())
        .collect::<Vec<_>>();

    if probes.is_empty() {
        return None;
    }

    let cold_wall_us = probes.iter().map(|probe| probe.cold.wall_us).sum();
    let warm_wall_us = probes.iter().map(|probe| probe.warm.wall_us).sum();
    let cold_allocated_bytes = probes.iter().map(|probe| probe.cold.allocated_bytes).sum();
    let warm_allocated_bytes = probes.iter().map(|probe| probe.warm.allocated_bytes).sum();
    let cold_fallback_action_counts =
        probes
            .iter()
            .fold(FallbackActionCounts::default(), |mut counts, probe| {
                counts.add(probe.cold.fallback_action_counts);
                counts
            });
    let warm_fallback_action_counts =
        probes
            .iter()
            .fold(FallbackActionCounts::default(), |mut counts, probe| {
                counts.add(probe.warm.fallback_action_counts);
                counts
            });
    let cold_stage_timings_us =
        probes
            .iter()
            .fold(BenchStageTimings::default(), |mut timings, probe| {
                timings.add(probe.cold.stage_timings_us);
                timings
            });
    let warm_stage_timings_us =
        probes
            .iter()
            .fold(BenchStageTimings::default(), |mut timings, probe| {
                timings.add(probe.warm.stage_timings_us);
                timings
            });
    let cold_cache_misses = probes
        .iter()
        .filter(|probe| probe.cold.cache_status == CacheStatus::Miss)
        .count() as u32;
    let warm_cache_hits = probes
        .iter()
        .filter(|probe| probe.warm.cache_status == CacheStatus::Hit)
        .count() as u32;

    Some(CorpusCacheProbeOutput {
        document_count: probes.len(),
        cold_wall_us,
        warm_wall_us,
        cold_pages_per_sec: pages_per_sec(page_count, cold_wall_us),
        warm_pages_per_sec: pages_per_sec(page_count, warm_wall_us),
        cold_allocated_bytes,
        warm_allocated_bytes,
        cold_allocated_bytes_per_page: bytes_per_page(cold_allocated_bytes, page_count),
        warm_allocated_bytes_per_page: bytes_per_page(warm_allocated_bytes, page_count),
        cold_fallback_action_counts,
        warm_fallback_action_counts,
        cold_stage_timings_us,
        warm_stage_timings_us,
        warm_speedup: speedup(cold_wall_us, warm_wall_us),
        cold_cache_misses,
        warm_cache_hits,
    })
}

pub(crate) fn cache_key(
    backend_name: &str,
    backend_version: &str,
    document_fingerprint: &str,
    source_path: &Path,
    ocr: OcrOptions<'_>,
    options: ExtractionOptions,
) -> Result<String> {
    let ocr_fingerprint = ocr_fingerprint(ocr, source_path)?;
    Ok(sha256_hex(format!(
        "{CACHE_SCHEMA_VERSION}:{PARSER_NAME}:{PARSER_VERSION}:{backend_name}:{backend_version}:{document_fingerprint}:{ocr_fingerprint}:span-geometry={}",
        options.span_geometry
    )))
}

pub(crate) fn remove_cached_artifact_for_document(
    backend_name: &str,
    backend_version: &str,
    path: &Path,
    ocr: OcrOptions<'_>,
    cache_dir: &Path,
    options: ExtractionOptions,
) -> Result<()> {
    let fingerprint = document_fingerprint(path)?;
    let cache_key = cache_key(
        backend_name,
        backend_version,
        &fingerprint,
        path,
        ocr,
        options,
    )?;
    let path = cache_path(cache_dir, &cache_key);

    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("remove cache artifact {}", path.display()))?;
    }

    Ok(())
}

pub(crate) fn load_cached_artifact(
    cache_dir: &Path,
    cache_key: &str,
) -> Result<CachedArtifactLoad> {
    let path = cache_path(cache_dir, cache_key);
    if !path.exists() {
        return Ok(CachedArtifactLoad::miss());
    }

    let bytes =
        match fs::read(&path).with_context(|| format!("read cache artifact {}", path.display())) {
            Ok(bytes) => bytes,
            Err(error) => return Ok(CachedArtifactLoad::ignored(&path, error)),
        };
    let cache_file: CachedArtifactFile = match serde_json::from_slice(&bytes)
        .with_context(|| format!("decode cache artifact {}", path.display()))
    {
        Ok(cache_file) => cache_file,
        Err(error) => return Ok(CachedArtifactLoad::ignored(&path, error)),
    };
    let artifact = match cache_file {
        CachedArtifactFile::Snapshot(snapshot) => match snapshot.into_artifact(cache_key, &path) {
            Ok(artifact) => artifact,
            Err(error) => return Ok(CachedArtifactLoad::ignored(&path, error)),
        },
        CachedArtifactFile::LegacyArtifact(artifact) => artifact,
    };
    Ok(CachedArtifactLoad::hit(artifact))
}

pub(crate) fn clear_page_stage_timings(artifact: &mut DocumentArtifact) {
    for page in &mut artifact.pages {
        page.timings = PageTimings::default();
    }
    artifact.global_diagnostics.total_stage_time_us = 0;
}

pub(crate) fn store_cached_artifact(
    cache_dir: &Path,
    cache_key: &str,
    artifact: &DocumentArtifact,
) -> Result<()> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("create cache directory {}", cache_dir.display()))?;
    let path = cache_path(cache_dir, cache_key);
    let snapshot = CachedArtifactSnapshot::from_artifact(cache_key, artifact);
    let bytes = serde_json::to_vec_pretty(&snapshot)?;
    fs::write(&path, bytes).with_context(|| format!("write cache artifact {}", path.display()))?;
    Ok(())
}

pub(crate) fn cache_path(cache_dir: &Path, cache_key: &str) -> PathBuf {
    cache_dir.join(format!("{cache_key}.json"))
}
