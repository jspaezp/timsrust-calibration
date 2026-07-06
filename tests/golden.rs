use timsrust_calibration::RunCalibration;
use timsrust_core::{Converter, Im, ScanIndex, TofIndex};

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

#[test]
fn flat_file_im_converter_in_sane_range() {
    // flat_t1.d now has a valid TimsCalibration FK (Frames.TimsCalibration = 1)
    // and good coefficients, so this is the only place IM works end-to-end
    // (real DDA files have an empty TimsCalibration table, real DIA files
    // observed so far have null TimsCalibration coefficients).
    let run = RunCalibration::from_path("tests/fixtures/flat_t1.d/analysis.tdf").unwrap();

    let per_frame = run.im_converter(1).unwrap();
    let scan = ScanIndex::try_from(300u32).unwrap();
    let im_pf = f64::from(per_frame.convert(scan));
    assert!((0.5..1.6).contains(&im_pf), "im_pf={im_pf}");

    let median = run.im_converter_median().unwrap();
    let im_med = f64::from(median.convert(scan));
    assert!((0.5..1.6).contains(&im_med), "im_med={im_med}");

    // flat file: identical calibration + T1 doesn't even factor into the IM
    // model, so per-frame and median converters must agree exactly.
    assert!((im_pf - im_med).abs() < 1e-12);

    // round trip stays close to the original scan index.
    let back: ScanIndex = per_frame.convert(Im::from(im_pf));
    assert!((u32::from(back) as i64 - 300).abs() <= 1);
}
