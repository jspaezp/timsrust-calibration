use timsrust_calibration::RunCalibration;
use timsrust_core::{Converter, TofIndex};

#[test]
fn flat_file_median_equals_per_frame() {
    let run = RunCalibration::from_path("tests/fixtures/flat_t1.d/analysis.tdf").unwrap();
    assert!(run.t1_spread() < 1e-6, "flat file should have ~0 spread");
    let m = run.mz_converter_median().unwrap();
    let f = run.mz_converter(1).unwrap();
    let t = TofIndex::try_from(1000u32).unwrap();
    assert!((f64::from(m.convert(t)) - f64::from(f.convert(t))).abs() < 1e-9);
}

#[test]
fn spread_file_median_differs_from_per_frame() {
    let run = RunCalibration::from_path("tests/fixtures/spread_t1.d/analysis.tdf").unwrap();
    assert!(
        run.t1_spread() > 0.01,
        "spread file should have real spread"
    );
    let m = run.mz_converter_median().unwrap();
    let f = run.mz_converter(1).unwrap();
    let t = TofIndex::try_from(500000u32).unwrap();
    // meaningfully different at high m/z where the c1 correction matters
    assert!((f64::from(m.convert(t)) - f64::from(f.convert(t))).abs() > 0.0);
}
