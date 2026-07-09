//! Physical (M2) scan-index <-> ion-mobility (1/K0) conversion; see
//! [`CalibratedScan2ImConverter`].

use timsrust_core::{Converter, Im, ScanIndex};

use crate::{sql::TimsCalibration, CalibrationError};

/// Physical (M2) scan-index <-> ion-mobility (1/K0) converter, built from
/// one Bruker `TimsCalibration` row.
///
/// Implements [`timsrust_core::Converter`] in both directions
/// ([`ScanIndex`] -> [`Im`] and [`Im`] -> [`ScanIndex`]).
///
/// # Limitation: no pressure compensation
///
/// This applies the static `TimsCalibration` polynomial only. Bruker's
/// schema also has a per-frame `Frames.Pressure` column (nullable),
/// documented as "required to perform a pressure compensated tims
/// calibration" — i.e. Bruker's own calibration can additionally correct
/// for pressure drift over a run. We don't have Bruker's pressure-comp
/// formula, so it is intentionally not implemented here. Results may diverge on
/// runs with significant pressure drift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibratedScan2ImConverter {
    c6: f64,
    c7: f64,
    offset: f64,
    slope: f64,
}

impl CalibratedScan2ImConverter {
    /// Build a converter from a `TimsCalibration` row.
    ///
    /// Only `model_type == 2` is supported; other model types and
    /// calibrations missing any of `c0`..`c4`/`c6`/`c7` return an error.
    pub fn try_from_calibration(cal: &TimsCalibration) -> Result<Self, CalibrationError> {
        if cal.model_type != 2 {
            return Err(CalibrationError::UnsupportedImModel(cal.model_type));
        }
        let (c0, c1, c2, c3, c4, c6, c7) =
            match (cal.c0, cal.c1, cal.c2, cal.c3, cal.c4, cal.c6, cal.c7) {
                (Some(c0), Some(c1), Some(c2), Some(c3), Some(c4), Some(c6), Some(c7)) => {
                    (c0, c1, c2, c3, c4, c6, c7)
                }
                _ => return Err(CalibrationError::MissingImCoefficients(cal.id)),
            };
        // `c1 == 0` would otherwise divide by zero and propagate NaN through
        // every converted value; there's no physically meaningful slope in
        // that case, so pin it to 0.0 (a flat, degenerate but finite model)
        // instead of poisoning downstream math with NaN.
        let slope = if c1 == 0.0 { 0.0 } else { (c3 - c2) / c1 };
        let offset = c2 - slope * (c4 + c0);
        Ok(Self {
            c6,
            c7,
            offset,
            slope,
        })
    }

    fn convert_f64(&self, scan_no: f64) -> f64 {
        1.0 / (self.c6 + self.c7 / (self.offset + self.slope * scan_no))
    }

    fn invert_f64(&self, im: f64) -> f64 {
        // im = 1/(c6 + c7/(offset + slope*scan))
        // => scan = (c7/(1/im - c6) - offset) / slope
        let denom = (1.0 / im) - self.c6;
        ((self.c7 / denom) - self.offset) / self.slope
    }
}

impl Converter<ScanIndex, Im> for CalibratedScan2ImConverter {
    fn convert(&self, value: ScanIndex) -> Im {
        Im::from(self.convert_f64(u32::from(value) as f64))
    }
}

impl Converter<Im, ScanIndex> for CalibratedScan2ImConverter {
    fn convert(&self, value: Im) -> ScanIndex {
        let result = self.invert_f64(f64::from(value));
        // See the analogous comment on `Converter<Mz, TofIndex>` in
        // `mz.rs`: `denom == 0` (when `1/im == c6`) or `slope == 0` makes
        // `invert_f64` return `+Inf`/`NaN`, which would otherwise panic in
        // `ScanIndex::try_from`. Clamp instead.
        ScanIndex::try_from(crate::clamp_f64_to_index(result))
            .expect("clamp_f64_to_index always returns a value ScanIndex can represent")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sql::TimsCalibration;
    use timsrust_core::{Converter, Im, ScanIndex};

    #[test]
    fn scan2im_matches_fork_reference() {
        let cal = TimsCalibration {
            id: 1,
            model_type: 2,
            c0: Some(1.0),
            c1: Some(708.0),
            c2: Some(241.751905250524),
            c3: Some(99.2437539638487),
            c4: Some(33.9622641509434),
            c6: Some(0.0071422641733084),
            c7: Some(164.998795925213),
        };
        let conv = CalibratedScan2ImConverter::try_from_calibration(&cal).unwrap();
        const TOL: f64 = 5e-2;
        let im1 = f64::from(conv.convert(ScanIndex::try_from(1u32).unwrap()));
        assert!((im1 - 1.45).abs() < TOL, "im1={im1}");
        let im708 = f64::from(conv.convert(ScanIndex::try_from(708u32).unwrap()));
        assert!((im708 - 0.64).abs() < TOL, "im708={im708}");

        // round trip
        let back: ScanIndex = conv.convert(Im::from(im708));
        assert!((u32::from(back) as i64 - 708).abs() <= 1);
    }

    fn base_cal() -> TimsCalibration {
        TimsCalibration {
            id: 1,
            model_type: 2,
            c0: Some(1.0),
            c1: Some(708.0),
            c2: Some(241.751905250524),
            c3: Some(99.2437539638487),
            c4: Some(33.9622641509434),
            c6: Some(0.0071422641733084),
            c7: Some(164.998795925213),
        }
    }

    /// `c1 == 0` must not propagate NaN through `slope`/`offset`; the
    /// forward conversion guards it to a deterministic `slope = 0.0`.
    #[test]
    fn forward_with_zero_c1_does_not_produce_nan() {
        let mut cal = base_cal();
        cal.c1 = Some(0.0);
        let conv = CalibratedScan2ImConverter::try_from_calibration(&cal).unwrap();
        let im = f64::from(conv.convert(ScanIndex::try_from(300u32).unwrap()));
        assert!(im.is_finite(), "im={im}");
    }

    /// `denom == 0` (`1/im == c6`) makes `invert_f64` return `+Inf`; this
    /// must clamp instead of panicking (see the `Converter<Im, ScanIndex>`
    /// impl).
    #[test]
    fn invert_with_denom_zero_does_not_panic() {
        let cal = base_cal();
        let conv = CalibratedScan2ImConverter::try_from_calibration(&cal).unwrap();
        // im such that 1/im == c6 exactly.
        let im = 1.0 / conv.c6;
        let idx: ScanIndex = conv.convert(Im::from(im));
        assert!(u32::from(idx) < u32::MAX);
    }

    /// NaN and huge finite inputs must also clamp deterministically instead
    /// of panicking.
    #[test]
    fn invert_with_non_finite_or_huge_input_does_not_panic() {
        let cal = base_cal();
        let conv = CalibratedScan2ImConverter::try_from_calibration(&cal).unwrap();

        // `1/im` for `im == NaN` is `NaN`, which propagates through
        // `invert_f64` to a `NaN` scan value; the clamp maps that to 0.
        let nan_idx: ScanIndex = conv.convert(Im::from(f64::NAN));
        assert_eq!(u32::from(nan_idx), 0);

        // Unlike the m/z model, this model's `1/im` term means `+Inf`/huge
        // `im` inputs collapse `1/im` toward `0` rather than diverging, so
        // these don't hit the clamp's saturation branch the way the
        // `denom == 0` singularity above does. The contract under test is
        // simply that conversion never panics and always yields a valid,
        // in-range `ScanIndex`.
        let inf_idx: ScanIndex = conv.convert(Im::from(f64::INFINITY));
        assert!(u32::from(inf_idx) < u32::MAX);

        let huge_idx: ScanIndex = conv.convert(Im::from(f64::MAX));
        assert!(u32::from(huge_idx) < u32::MAX);
    }
}
