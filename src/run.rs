use std::collections::HashMap;
use std::path::Path;

use crate::{
    im::CalibratedScan2ImConverter,
    mz::CalibratedTof2MzConverter,
    sql::{read_all, FrameCal, MzCalibration, TimsCalibration},
    CalibrationError,
};

/// Physical (M2) calibration for one Bruker `.d` run, built from the
/// `MzCalibration`, `TimsCalibration`, and `Frames` tables of its
/// `analysis.tdf`.
///
/// Construct with [`RunCalibration::from_path`], then obtain per-frame or
/// run-median converters via `mz_converter`/`im_converter` and their
/// `_median` counterparts.
pub struct RunCalibration {
    mz_cals: Vec<MzCalibration>,
    tims_cals: Vec<TimsCalibration>,
    frames: Vec<FrameCal>,
    frame_by_id: HashMap<usize, usize>, // frame_id -> index into frames
    median_t1: f64,
}

impl RunCalibration {
    /// Read the calibration tables from the `.tdf` sqlite file at `path`
    /// and index them by frame.
    ///
    /// `path` is the path to the `analysis.tdf` file itself, not the
    /// enclosing `.d` directory. The file is opened strictly read-only; see
    /// [`crate::sql`] module docs for why this never creates a `-wal`/`-shm`
    /// sidecar next to `path`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, CalibrationError> {
        let (mz_cals, tims_cals, frames) = read_all(path)?;
        let frame_by_id = frames
            .iter()
            .enumerate()
            .map(|(i, f)| (f.frame_id, i))
            .collect();
        let mut t1s: Vec<f64> = frames.iter().map(|f| f.t1).collect();
        t1s.sort_by(|a, b| a.total_cmp(b));
        // NB: for an even-length `t1s` this is the upper-middle element
        // (`t1s[len/2]`), not the usual average-of-two-middles median.
        let median_t1 = t1s[t1s.len() / 2];
        Ok(Self {
            mz_cals,
            tims_cals,
            frames,
            frame_by_id,
            median_t1,
        })
    }

    fn mz_cal_by_id(&self, id: u32) -> Result<&MzCalibration, CalibrationError> {
        self.mz_cals
            .iter()
            .find(|c| c.id == id)
            .ok_or(CalibrationError::CalIdNotFound(id))
    }

    fn tims_cal_by_id(&self, id: u32) -> Result<&TimsCalibration, CalibrationError> {
        self.tims_cals
            .iter()
            .find(|c| c.id == id)
            .ok_or(CalibrationError::CalIdNotFound(id))
    }

    fn frame(&self, frame_id: usize) -> Result<&FrameCal, CalibrationError> {
        self.frame_by_id
            .get(&frame_id)
            .map(|&i| &self.frames[i])
            .ok_or(CalibrationError::FrameNotFound(frame_id))
    }

    /// Build a TOF->m/z converter for a single frame.
    ///
    /// `frame_id` is the Bruker `Frames.Id` (1-based instrument frame
    /// index), **not** a 0-based offset into any in-memory list.
    pub fn mz_converter(
        &self,
        frame_id: usize,
    ) -> Result<CalibratedTof2MzConverter, CalibrationError> {
        let frame = self.frame(frame_id)?;
        let cal = self.mz_cal_by_id(frame.mz_cal_id)?;
        CalibratedTof2MzConverter::try_from_calibration(cal, frame.t1)
    }

    /// Build a TOF->m/z converter using the `MzCalibration` row referenced
    /// by the *first* frame and the median T1 across all frames.
    ///
    /// This assumes a single calibration per run: it uses `frames[0]`'s
    /// `mz_cal_id` and mixes T1 values across *all* frames regardless of
    /// which calibration each frame actually references. If a run ever
    /// contains multiple distinct calibrations, this method will silently
    /// combine T1s from different calibrations into one median.
    ///
    /// "Median" here is `t1s[len/2]` on the sorted T1s, i.e. the
    /// upper-middle element for an even-length run, not the conventional
    /// average of the two middle elements.
    pub fn mz_converter_median(&self) -> Result<CalibratedTof2MzConverter, CalibrationError> {
        // use the calibration referenced by the first frame
        let cal_id = self.frames[0].mz_cal_id;
        let cal = self.mz_cal_by_id(cal_id)?;
        CalibratedTof2MzConverter::try_from_calibration(cal, self.median_t1)
    }

    /// Build a scan->ion-mobility (1/K0) converter for a single frame.
    ///
    /// `frame_id` is the Bruker `Frames.Id` (1-based instrument frame
    /// index), **not** a 0-based offset into any in-memory list.
    pub fn im_converter(
        &self,
        frame_id: usize,
    ) -> Result<CalibratedScan2ImConverter, CalibrationError> {
        let frame = self.frame(frame_id)?;
        let cal = self.tims_cal_by_id(frame.tims_cal_id)?;
        CalibratedScan2ImConverter::try_from_calibration(cal)
    }

    /// Build a scan->ion-mobility (1/K0) converter using the
    /// `TimsCalibration` row referenced by the *first* frame.
    ///
    /// Like [`Self::mz_converter_median`], this assumes a single
    /// calibration per run (it uses `frames[0]`'s `tims_cal_id`); it does
    /// not attempt to detect or reconcile multiple distinct calibrations
    /// within one run.
    pub fn im_converter_median(&self) -> Result<CalibratedScan2ImConverter, CalibrationError> {
        let cal_id = self.frames[0].tims_cal_id;
        let cal = self.tims_cal_by_id(cal_id)?;
        CalibratedScan2ImConverter::try_from_calibration(cal)
    }

    /// The T1 range (max - min) across all frames in the run.
    pub fn t1_spread(&self) -> f64 {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for f in &self.frames {
            lo = lo.min(f.t1);
            hi = hi.max(f.t1);
        }
        hi - lo
    }
}
