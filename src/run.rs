use std::collections::HashMap;

use crate::{
    im::CalibratedScan2ImConverter,
    mz::CalibratedTof2MzConverter,
    sql::{read_all, FrameCal, MzCalibration, TimsCalibration},
    CalibrationError,
};

pub struct RunCalibration {
    mz_cals: Vec<MzCalibration>,
    tims_cals: Vec<TimsCalibration>,
    frames: Vec<FrameCal>,
    frame_by_id: HashMap<usize, usize>, // frame_id -> index into frames
    median_t1: f64,
}

impl RunCalibration {
    pub fn from_path(path: impl AsRef<str>) -> Result<Self, CalibrationError> {
        let (mz_cals, tims_cals, frames) = read_all(path.as_ref())?;
        let frame_by_id = frames
            .iter()
            .enumerate()
            .map(|(i, f)| (f.frame_id, i))
            .collect();
        let mut t1s: Vec<f64> = frames.iter().map(|f| f.t1).collect();
        t1s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_t1 = t1s[t1s.len() / 2];
        Ok(Self {
            mz_cals,
            tims_cals,
            frames,
            frame_by_id,
            median_t1,
        })
    }

    fn mz_cal_by_id(&self, id: u8) -> Result<&MzCalibration, CalibrationError> {
        self.mz_cals
            .iter()
            .find(|c| c.id == id)
            .ok_or(CalibrationError::CalIdNotFound(id))
    }

    fn tims_cal_by_id(&self, id: u8) -> Result<&TimsCalibration, CalibrationError> {
        self.tims_cals
            .iter()
            .find(|c| c.id == id)
            .ok_or(CalibrationError::CalIdNotFound(id))
    }

    fn frame(&self, frame_id: usize) -> Result<&FrameCal, CalibrationError> {
        self.frame_by_id
            .get(&frame_id)
            .map(|&i| &self.frames[i])
            .ok_or(CalibrationError::CalIdNotFound(frame_id as u8))
    }

    pub fn mz_converter(
        &self,
        frame_id: usize,
    ) -> Result<CalibratedTof2MzConverter, CalibrationError> {
        let frame = self.frame(frame_id)?;
        let cal = self.mz_cal_by_id(frame.cal_id)?;
        CalibratedTof2MzConverter::try_from_calibration(cal, frame.t1)
    }

    pub fn mz_converter_median(&self) -> Result<CalibratedTof2MzConverter, CalibrationError> {
        // use the calibration referenced by the first frame
        let cal_id = self.frames[0].cal_id;
        let cal = self.mz_cal_by_id(cal_id)?;
        CalibratedTof2MzConverter::try_from_calibration(cal, self.median_t1)
    }

    pub fn im_converter(
        &self,
        frame_id: usize,
    ) -> Result<CalibratedScan2ImConverter, CalibrationError> {
        let frame = self.frame(frame_id)?;
        let cal = self.tims_cal_by_id(frame.cal_id)?;
        CalibratedScan2ImConverter::try_from_calibration(cal)
    }

    pub fn im_converter_median(&self) -> Result<CalibratedScan2ImConverter, CalibrationError> {
        let cal_id = self.frames[0].cal_id;
        let cal = self.tims_cal_by_id(cal_id)?;
        CalibratedScan2ImConverter::try_from_calibration(cal)
    }

    pub fn t1_spread(&self) -> f64 {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for f in &self.frames {
            lo = lo.min(f.t1);
            hi = hi.max(f.t1);
        }
        hi - lo
    }
}
