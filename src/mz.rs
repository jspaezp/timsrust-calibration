//! Physical (M2) TOF-index <-> m/z conversion; see
//! [`CalibratedTof2MzConverter`].

use timsrust_core::{Converter, Mz, TofIndex};

use crate::{sql::MzCalibration, CalibrationError};

/// Physical (M2) TOF-index <-> m/z converter, built from one Bruker
/// `MzCalibration` row plus the actual digitizer T1 observed for a given
/// frame (or a run-median T1; see [`crate::RunCalibration`]).
///
/// Implements [`timsrust_core::Converter`] in both directions
/// ([`TofIndex`] -> [`Mz`] and [`Mz`] -> [`TofIndex`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibratedTof2MzConverter {
    c0: f64,
    c1: f64,
    digitizer_timebase: f64,
    delay: f64,
}

impl CalibratedTof2MzConverter {
    /// Build a converter from an `MzCalibration` row and the real
    /// (per-frame or median) T1 used for the digitizer-drift correction.
    ///
    /// Only `model_type == 1` is supported; other model types and
    /// calibrations missing `c0`/`c1` return an error.
    pub fn try_from_calibration(
        cal: &MzCalibration,
        real_t1: f64,
    ) -> Result<Self, CalibrationError> {
        if cal.model_type != 1 {
            return Err(CalibrationError::UnsupportedMzModel(cal.model_type));
        }
        let (c0, c1) = match (cal.c0, cal.c1) {
            (Some(c0), Some(c1)) => (c0, c1),
            _ => return Err(CalibrationError::MissingMzCoefficients(cal.id)),
        };
        // cf = dc1 * (T1_reference - real_t1); (dc2 term dropped, assumed 0)
        let cf = cal.dc1 * (cal.t1 - real_t1);
        let cf = 1.0 + (cf / 1.0e6);
        Ok(Self {
            c0,
            c1: c1 * cf,
            digitizer_timebase: cal.digitizer_timebase,
            delay: cal.digitizer_delay,
        })
    }

    fn convert_f64(&self, idx: f64) -> f64 {
        let tof = (idx * self.digitizer_timebase) + self.delay;
        let inner = tof - self.c0;
        (self.c1 * inner.powi(2)) / 1e12
    }

    fn invert_f64(&self, mz: f64) -> f64 {
        let tof = ((mz * 1e12) / self.c1).sqrt() + self.c0;
        (tof - self.delay) / self.digitizer_timebase
    }
}

impl Converter<TofIndex, Mz> for CalibratedTof2MzConverter {
    fn convert(&self, value: TofIndex) -> Mz {
        Mz::from(self.convert_f64(u32::from(value) as f64))
    }
}

impl Converter<Mz, TofIndex> for CalibratedTof2MzConverter {
    fn convert(&self, value: Mz) -> TofIndex {
        let result = self.invert_f64(f64::from(value));
        // `Converter::convert` is infallible, but `TofIndex::try_from(u32)`
        // panics on `u32::MAX` (it computes `x + 1` into a `NonZeroU32`).
        // That's reachable here: e.g. `c1 == 0` makes `invert_f64` return
        // `+Inf`, which saturates to `u32::MAX` via `as u32`. Clamp
        // non-finite/out-of-range results to a deterministic, always-valid
        // index instead of ever panicking.
        TofIndex::try_from(crate::clamp_f64_to_index(result))
            .expect("clamp_f64_to_index always returns a value TofIndex can represent")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sql::MzCalibration;
    use timsrust_core::{Converter, Mz, TofIndex};

    #[test]
    fn tof2mz_matches_fork_reference() {
        let cal = MzCalibration {
            id: 1,
            model_type: 1,
            digitizer_timebase: 0.125,
            digitizer_delay: 25741.0,
            t1: 20.9410989491122,
            dc1: 20.0,
            c0: Some(286.065160463331),
            c1: Some(154317.348188993),
        };
        let real_t1 = 20.9455139021767;
        let conv = CalibratedTof2MzConverter::try_from_calibration(&cal, real_t1).unwrap();

        let mz0 = f64::from(conv.convert(TofIndex::try_from(0u32).unwrap()));
        let mz_max = f64::from(conv.convert(TofIndex::try_from(636029u32).unwrap()));
        const TOL: f64 = 1e-3;
        assert!((mz0 - 99.990834).abs() < TOL, "mz0={mz0}");
        assert!((mz_max - 1700.005).abs() < TOL, "mz_max={mz_max}");

        // round trip
        let back: TofIndex = conv.convert(Mz::from(mz_max));
        assert!((u32::from(back) as i64 - 636029).abs() <= 1);
    }

    /// `c1 == 0` makes `invert_f64` compute `sqrt(mz*1e12/0) == +Inf`; this
    /// must clamp to a valid index rather than panicking (see the
    /// `Converter<Mz, TofIndex>` impl).
    #[test]
    fn invert_with_zero_c1_does_not_panic() {
        let cal = MzCalibration {
            id: 1,
            model_type: 1,
            digitizer_timebase: 0.125,
            digitizer_delay: 25741.0,
            t1: 20.0,
            dc1: 0.0,
            c0: Some(286.0),
            c1: Some(0.0),
        };
        let conv = CalibratedTof2MzConverter::try_from_calibration(&cal, 20.0).unwrap();
        let idx: TofIndex = conv.convert(Mz::from(500.0));
        assert!(u32::from(idx) < u32::MAX);
    }

    /// NaN and huge finite inputs must also clamp deterministically instead
    /// of panicking.
    #[test]
    fn invert_with_non_finite_or_huge_input_does_not_panic() {
        let cal = MzCalibration {
            id: 1,
            model_type: 1,
            digitizer_timebase: 0.125,
            digitizer_delay: 25741.0,
            t1: 20.0,
            dc1: 0.0,
            c0: Some(286.0),
            c1: Some(154317.348188993),
        };
        let conv = CalibratedTof2MzConverter::try_from_calibration(&cal, 20.0).unwrap();

        let nan_idx: TofIndex = conv.convert(Mz::from(f64::NAN));
        assert_eq!(u32::from(nan_idx), 0);

        let inf_idx: TofIndex = conv.convert(Mz::from(f64::INFINITY));
        assert_eq!(u32::from(inf_idx), u32::MAX - 1);

        let huge_idx: TofIndex = conv.convert(Mz::from(f64::MAX));
        assert_eq!(u32::from(huge_idx), u32::MAX - 1);
    }
}
