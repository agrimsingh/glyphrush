#![allow(unused_imports)]

use super::harness::*;

#[test]
fn warm_bench_emits_valid_timing_json() {
    let pdf_path = write_test_pdf("warm-bench", "Hello Warm Bench");

    let json = glyphrush(&[
        "--backend",
        "lopdf",
        "warm-bench",
        pdf_path.to_str().unwrap(),
        "--runs",
        "3",
        "--warmup",
        "1",
    ]);

    assert_eq!(json["parser"], "glyphrush");
    assert_eq!(json["mode"], "in_process");
    assert_eq!(json["runs"], 3);
    assert!(json["min_s"].as_f64().unwrap_or(0.0) >= 0.0);
    assert!(json["median_s"].as_f64().unwrap_or(0.0) >= 0.0);
    assert!(json["median_s"].as_f64().unwrap() >= json["min_s"].as_f64().unwrap());
}
