use timsrust_core::{Converter, Mz, TofIndex};

use crate::{sql::MzCalibration, CalibrationError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibratedTof2MzConverter {
    c0: f64,
    c1: f64,
    digitizer_timebase: f64,
    delay: f64,
}

impl CalibratedTof2MzConverter {
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
        TofIndex::try_from(result.round().max(0.0) as u32)
            .expect("TofIndex conversion out of bounds")
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
            t2: 24.8706161298104,
            dc1: 20.0,
            dc2: 0.0,
            c0: Some(286.065160463331),
            c1: Some(154317.348188993),
            c2: None,
            c3: None,
            c4: None,
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
}
