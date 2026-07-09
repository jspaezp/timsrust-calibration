//! [`RunCalibration`]: the run-level entry point that reads a `.tdf`'s
//! calibration tables and hands out per-frame/run-median converters.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    im::CalibratedScan2ImConverter,
    mz::CalibratedTof2MzConverter,
    sql::{read_all, CalibrationTables, FrameCal, MzCalibration, TimsCalibration},
    CalibrationError,
};

/// Physical (M2) calibration for one Bruker `.d` run, built from the
/// `MzCalibration`, `TimsCalibration`, and `Frames` tables of its
/// `analysis.tdf`.
///
/// Construct with [`RunCalibration::from_path`], then obtain per-frame or
/// run-median converters via `mz_converter`/`im_converter` and their
/// `_median` counterparts.
#[derive(Debug)]
pub struct RunCalibration {
    mz_cals: Vec<MzCalibration>,
    tims_cals: Vec<TimsCalibration>,
    frames: Vec<FrameCal>,
    frame_by_id: HashMap<usize, usize>, // frame_id -> index into frames
    median_t1: f64,
}

impl RunCalibration {
    /// Read the calibration tables from a Bruker run and index them by
    /// frame.
    ///
    /// `path` accepts any of:
    /// - the `analysis.tdf` file itself (e.g.
    ///   `"/data/run.d/analysis.tdf"`),
    /// - the enclosing `.d` run directory (e.g. `"/data/run.d"`), or
    /// - any other `impl AsRef<str>`, in particular a
    ///   `timsrust::TimsTofPath` — that type implements `AsRef<str>`,
    ///   yielding the `.d` run directory, so `from_path(&timstof_path)`
    ///   works without this crate depending on `timsrust` at all.
    ///
    /// Resolution: a `file://` prefix is stripped if present; the remaining
    /// path is used as-is if it already names an existing file or ends in
    /// `analysis.tdf` (no double-appending), otherwise `analysis.tdf` is
    /// joined onto it as a `.d` directory.
    ///
    /// The file is opened strictly read-only; see [`crate::sql`] module
    /// docs for why this never creates a `-wal`/`-shm` sidecar next to it.
    ///
    /// # Limitation: no acquisition-format auto-detection
    ///
    /// This does not detect the acquisition format the way `timsrust` does.
    /// A path to a non-TDF acquisition (TSF, miniTDF, Parquet, ...) simply
    /// fails to find an `analysis.tdf` and returns
    /// [`CalibrationError::Open`]. Reusing timsrust's format detection
    /// would need its `pub(crate)` `file_type()` made public upstream.
    pub fn from_path(path: impl AsRef<str>) -> Result<Self, CalibrationError> {
        let tdf = resolve_tdf_path(path.as_ref());
        let CalibrationTables {
            mz: mz_cals,
            tims: tims_cals,
            frames,
        } = read_all(tdf)?;
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
        let cal_id = self
            .frames
            .first()
            .expect("RunCalibration is only constructed via from_path, which errors (NoFrames) on empty frames")
            .mz_cal_id;
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
        let cal_id = self
            .frames
            .first()
            .expect("RunCalibration is only constructed via from_path, which errors (NoFrames) on empty frames")
            .tims_cal_id;
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

/// Resolve a user-supplied path/URI-ish string into the concrete
/// `analysis.tdf` file path.
///
/// Strips a `file://` prefix if present, then uses the remainder as-is if
/// it already names an existing file or ends in `analysis.tdf` (avoiding a
/// double-append when already given the file), otherwise treats it as a
/// `.d` run directory and joins `analysis.tdf` onto it.
fn resolve_tdf_path(s: &str) -> PathBuf {
    let s = s.strip_prefix("file://").unwrap_or(s);
    let p = Path::new(s);
    if p.is_file() || p.ends_with("analysis.tdf") {
        p.to_path_buf()
    } else {
        p.join("analysis.tdf")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// `from_path` must accept the `.d` run directory (no `analysis.tdf`
    /// suffix) and resolve to the same data as the explicit file path,
    /// mirroring how a `timsrust::TimsTofPath` (via its `AsRef<str>`,
    /// yielding the `.d` dir) would be passed in.
    #[test]
    fn from_path_accepts_d_dir_and_matches_file_path() {
        let from_dir = RunCalibration::from_path("tests/fixtures/flat_t1.d")
            .expect("should resolve tests/fixtures/flat_t1.d/analysis.tdf");
        let from_file = RunCalibration::from_path("tests/fixtures/flat_t1.d/analysis.tdf")
            .expect("explicit file path should still work");

        assert_eq!(from_dir.mz_cals, from_file.mz_cals);
        assert_eq!(from_dir.tims_cals, from_file.tims_cals);
        assert_eq!(from_dir.frames, from_file.frames);
        assert_eq!(from_dir.median_t1, from_file.median_t1);
    }

    /// A path already ending in `analysis.tdf` must not get `analysis.tdf`
    /// appended again.
    #[test]
    fn resolve_tdf_path_does_not_double_append() {
        let resolved = resolve_tdf_path("tests/fixtures/flat_t1.d/analysis.tdf");
        assert_eq!(resolved, Path::new("tests/fixtures/flat_t1.d/analysis.tdf"));
    }
}
