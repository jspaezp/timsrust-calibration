//! Raw row types and the Bruker TDF sqlite reader for M2 (physical) calibration.
//!
//! # Read-only / WAL sidecar caveat
//!
//! [`read_all`] opens the TDF file through the [`turso`] crate (v0.1.5).
//! That version of turso has **no read-only or no-WAL open mode**: even a
//! purely read-only session unconditionally opens (and, if absent, creates)
//! a `<path>-wal` sidecar file next to the database, because
//! `turso_core::storage::wal::WalFileShared::open_shared_if_exists` always
//! opens the wal path with `OpenFlags::Create` regardless of any read-only
//! flag applied to the main database file. The public `turso::Builder` API
//! does not expose `OpenFlags`/`DatabaseOpts` at all, so this cannot be
//! worked around from this crate without vendoring/replacing turso's
//! high-level `Connection`/`Rows` wrappers against the lower-level
//! `turso_core` API.
//!
//! Practical consequences for callers:
//! - Opening a `.tdf` inside a real Bruker `.d` folder will leave a
//!   `analysis.tdf-wal` file (typically empty) next to `analysis.tdf`.
//! - If the `.d` folder's filesystem is genuinely read-only (immutable
//!   mount, read-only permissions), [`read_all`] will fail outright while
//!   trying to create that sidecar, not just leave a stray file.
//!
//! If a future turso release adds a read-only/no-WAL mode, `read_all`
//! should be updated to use it and this note removed.

use crate::CalibrationError;

/// One row of the `MzCalibration` table (physical TOF->m/z model).
#[derive(Clone, Debug, PartialEq)]
pub struct MzCalibration {
    pub id: u8,
    pub model_type: u8,
    pub digitizer_timebase: f64,
    pub digitizer_delay: f64,
    pub t1: f64,
    pub t2: f64,
    pub dc1: f64,
    pub dc2: f64,
    pub c0: Option<f64>,
    pub c1: Option<f64>,
    pub c2: Option<f64>,
    pub c3: Option<f64>,
    pub c4: Option<f64>,
}

/// One row of the `TimsCalibration` table (physical scan->1/K0 mobility model).
#[derive(Clone, Debug, PartialEq)]
pub struct TimsCalibration {
    pub id: u8,
    pub model_type: u8,
    pub c0: Option<f64>,
    pub c1: Option<f64>,
    pub c2: Option<f64>,
    pub c3: Option<f64>,
    pub c4: Option<f64>,
    pub c5: Option<f64>,
    pub c6: Option<f64>,
    pub c7: Option<f64>,
    pub c8: Option<f64>,
    pub c9: Option<f64>,
}

/// A single `Frames` row's calibration linkage: which calibration rows a
/// given frame refers to, plus the T1 value used for the digitizer-drift
/// correction.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameCal {
    /// The Bruker `Frames.Id` (1-based instrument frame index).
    pub frame_id: usize,
    pub t1: f64,
    /// FK into `MzCalibration.Id` (`Frames.MzCalibration`).
    pub mz_cal_id: u8,
    /// FK into `TimsCalibration.Id` (`Frames.TimsCalibration`).
    ///
    /// Bruker's `Frames` table has two *separate* calibration FK columns;
    /// they happen to share the same value on files observed so far, but
    /// must not be assumed equal.
    pub tims_cal_id: u8,
}

/// Return type of [`read_all`]: the raw `MzCalibration`, `TimsCalibration`,
/// and per-frame calibration-linkage rows read from a TDF sqlite file.
pub type CalibrationTables = (Vec<MzCalibration>, Vec<TimsCalibration>, Vec<FrameCal>);

/// Read the `MzCalibration`, `TimsCalibration`, and `Frames` tables from a
/// Bruker `analysis.tdf` sqlite file.
///
/// `path` is the path to the `.tdf` file itself (not the enclosing `.d`
/// directory). See the module docs for a caveat about a `-wal` sidecar file
/// that the current turso backend leaves next to `path`.
///
/// # Errors
/// [`CalibrationError::Open`]/[`CalibrationError::Query`] on I/O or sqlite
/// failures, [`CalibrationError::NoCalibration`] if `MzCalibration` is
/// empty, [`CalibrationError::NoFrames`] if `Frames` is empty.
pub fn read_all(path: &str) -> Result<CalibrationTables, CalibrationError> {
    pollster::block_on(read_all_async(path))
}

async fn read_all_async(path: &str) -> Result<CalibrationTables, CalibrationError> {
    let db = turso::Builder::new_local(path)
        .build()
        .await
        .map_err(|e| CalibrationError::Open(e.to_string()))?;
    let conn = db
        .connect()
        .map_err(|e| CalibrationError::Open(e.to_string()))?;

    // helpers for nullable/typed column access
    fn f64_at(row: &turso::Row, i: usize) -> Option<f64> {
        row.get_value(i).ok().and_then(|v| v.as_real().copied())
    }
    fn int_at(row: &turso::Row, i: usize) -> i64 {
        row.get_value(i)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0)
    }

    let mut mz = Vec::new();
    let mut rows = conn
        .query(
            "SELECT Id, ModelType, DigitizerTimebase, DigitizerDelay, T1, T2, dC1, dC2, C0, C1, C2, C3, C4 FROM MzCalibration",
            (),
        )
        .await
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    while let Some(r) = rows
        .next()
        .await
        .map_err(|e| CalibrationError::Query(e.to_string()))?
    {
        mz.push(MzCalibration {
            id: int_at(&r, 0) as u8,
            model_type: int_at(&r, 1) as u8,
            digitizer_timebase: f64_at(&r, 2).unwrap_or(0.0),
            digitizer_delay: f64_at(&r, 3).unwrap_or(0.0),
            t1: f64_at(&r, 4).unwrap_or(0.0),
            t2: f64_at(&r, 5).unwrap_or(0.0),
            dc1: f64_at(&r, 6).unwrap_or(0.0),
            dc2: f64_at(&r, 7).unwrap_or(0.0),
            c0: f64_at(&r, 8),
            c1: f64_at(&r, 9),
            c2: f64_at(&r, 10),
            c3: f64_at(&r, 11),
            c4: f64_at(&r, 12),
        });
    }

    let mut tims = Vec::new();
    let mut trows = conn
        .query(
            "SELECT Id, ModelType, C0, C1, C2, C3, C4, C5, C6, C7, C8, C9 FROM TimsCalibration",
            (),
        )
        .await
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    while let Some(r) = trows
        .next()
        .await
        .map_err(|e| CalibrationError::Query(e.to_string()))?
    {
        tims.push(TimsCalibration {
            id: int_at(&r, 0) as u8,
            model_type: int_at(&r, 1) as u8,
            c0: f64_at(&r, 2),
            c1: f64_at(&r, 3),
            c2: f64_at(&r, 4),
            c3: f64_at(&r, 5),
            c4: f64_at(&r, 6),
            c5: f64_at(&r, 7),
            c6: f64_at(&r, 8),
            c7: f64_at(&r, 9),
            c8: f64_at(&r, 10),
            c9: f64_at(&r, 11),
        });
    }

    let mut frames = Vec::new();
    let mut frows = conn
        .query(
            "SELECT Id, T1, MzCalibration, TimsCalibration FROM Frames",
            (),
        )
        .await
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    while let Some(r) = frows
        .next()
        .await
        .map_err(|e| CalibrationError::Query(e.to_string()))?
    {
        frames.push(FrameCal {
            frame_id: int_at(&r, 0) as usize,
            t1: f64_at(&r, 1).unwrap_or(0.0),
            mz_cal_id: int_at(&r, 2) as u8,
            tims_cal_id: int_at(&r, 3) as u8,
        });
    }

    if mz.is_empty() {
        return Err(CalibrationError::NoCalibration);
    }
    if frames.is_empty() {
        return Err(CalibrationError::NoFrames);
    }
    Ok((mz, tims, frames))
}
