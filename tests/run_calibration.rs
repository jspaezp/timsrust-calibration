use timsrust_calibration::RunCalibration;
use timsrust_core::{Converter, TofIndex};

fn test_tdf() -> Option<String> {
    std::env::var("TIMSRUST_CAL_TEST_TDF").ok()
}

#[test]
fn builds_converters_from_real_file() {
    let Some(tdf) = test_tdf() else {
        eprintln!("skip: set TIMSRUST_CAL_TEST_TDF");
        return;
    };
    let run = RunCalibration::from_path(&tdf).unwrap();

    // median converter produces m/z within the acquisition range for tof 0.
    let median = run.mz_converter_median().unwrap();
    let mz0 = f64::from(median.convert(TofIndex::try_from(0u32).unwrap()));
    assert!(mz0 > 0.0 && mz0 < 2000.0, "mz0={mz0}");

    // per-frame converter for the first frame agrees closely with median
    // on a low-spread file.
    let per_frame = run.mz_converter(1).unwrap();
    let mz0_pf = f64::from(per_frame.convert(TofIndex::try_from(0u32).unwrap()));
    assert!((mz0 - mz0_pf).abs() < 1.0, "median {mz0} vs frame {mz0_pf}");

    // spread is a finite non-negative number
    assert!(run.t1_spread() >= 0.0 && run.t1_spread().is_finite());
}
