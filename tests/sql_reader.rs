use timsrust_calibration::sql::read_all;

fn test_tdf() -> Option<String> { std::env::var("TIMSRUST_CAL_TEST_TDF").ok() }

#[test]
fn reads_calibration_tables_and_frames() {
    let Some(tdf) = test_tdf() else {
        eprintln!("skip: set TIMSRUST_CAL_TEST_TDF");
        return;
    };
    let (mz, tims, frames) = read_all(&tdf).unwrap();
    assert!(!mz.is_empty(), "no MzCalibration");
    assert_eq!(mz[0].model_type, 1);
    assert!(mz[0].c0.is_some() && mz[0].c1.is_some());
    assert!(!frames.is_empty(), "no frames");
    assert!(frames.iter().all(|f| f.t1 > 0.0));
    // tims cal may be empty on some files; if present it must be model 2
    assert!(tims.iter().all(|t| t.model_type == 2));
}
