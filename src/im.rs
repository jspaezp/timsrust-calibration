use timsrust_core::{Converter, Im, ScanIndex};

use crate::{sql::TimsCalibration, CalibrationError};

/// Physical (M2) scan-index <-> ion-mobility (1/K0) converter, built from
/// one Bruker `TimsCalibration` row.
///
/// Implements [`timsrust_core::Converter`] in both directions
/// ([`ScanIndex`] -> [`Im`] and [`Im`] -> [`ScanIndex`]).
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
        let slope = (c3 - c2) / c1;
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
        ScanIndex::try_from(result.round().max(0.0) as u32)
            .expect("ScanIndex conversion out of bounds")
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
            c5: Some(1.0),
            c6: Some(0.0071422641733084),
            c7: Some(164.998795925213),
            c8: Some(16.3705403907576),
            c9: Some(2553.11607142569),
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
}
