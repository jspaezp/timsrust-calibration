use timsrust_calibration::sql::read_all;

fn test_tdf() -> Option<String> {
    std::env::var("TIMSRUST_CAL_TEST_TDF").ok()
}

/// Requires a real Bruker `.tdf` file: run with
/// `TIMSRUST_CAL_TEST_TDF=/path/to/analysis.tdf cargo test -- --ignored`.
/// `#[ignore]`d (rather than a runtime early-return) so a plain `cargo test`
/// shows this as skipped instead of a false-green pass-while-testing-nothing.
#[test]
#[ignore]
fn reads_calibration_tables_and_frames() {
    let tdf = test_tdf().expect("set TIMSRUST_CAL_TEST_TDF to run this ignored test");
    let tables = read_all(&tdf).unwrap();
    let mz = &tables.mz;
    let tims = &tables.tims;
    let frames = &tables.frames;
    assert!(!mz.is_empty(), "no MzCalibration");
    assert_eq!(mz[0].model_type, 1);
    assert!(mz[0].c0.is_some() && mz[0].c1.is_some());
    assert!(!frames.is_empty(), "no frames");
    assert!(frames.iter().all(|f| f.t1 > 0.0));
    // tims cal may be empty on some files; if present it must be model 2
    assert!(tims.iter().all(|t| t.model_type == 2));
}
