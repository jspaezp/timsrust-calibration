//! Raw row types and the Bruker TDF sqlite reader for M2 (physical) calibration.
//!
//! # Read-only open, no WAL sidecar
//!
//! [`read_all`] opens the `.tdf` file **read-only** and **never writes a
//! `-wal`/`-shm` sidecar** next to it, including when the enclosing `.d`
//! folder is mounted read-only.
//!
//! This is implemented directly against the low-level [`turso::core`]
//! (`turso_core`) engine rather than the ergonomic `turso::Builder` /
//! `Connection` / `Rows` wrappers, because as of `turso` 0.6.1 those
//! high-level wrappers still have no way to request a read-only open: the
//! `turso::Builder` type has no read-only option, and it never runs a
//! file-backed path through `turso_core`'s SQLite-URI parser, so an
//! attempt to pass a `file:...?mode=ro` URI straight to `Builder::new_local`
//! fails outright (the whole URI string is treated as a literal filesystem
//! path).
//!
//! `turso_core::Connection::from_uri`, by contrast, *does* parse SQLite URIs
//! (see `turso_core::util::OpenOptions::parse`), and correctly turns
//! `mode=ro` into `OpenFlags::ReadOnly`. With that flag set,
//! `turso_core::storage::wal::WalFileShared::open_shared_if_exists` opens
//! the `-wal` path *without* `OpenFlags::Create`: if the sidecar doesn't
//! exist it returns a no-op in-memory WAL instead of creating one on disk.
//! That is the fix this module relies on (confirmed by reading the 0.6.1
//! source, and by an integration test that asserts no `-wal`/`-shm` file
//! appears after a read-only open).
//!
//! [`read_all`] therefore:
//! - canonicalizes `path` and percent-encodes it into a
//!   `file:<abs-path>?mode=ro&immutable=1` SQLite URI (`immutable=1` tells
//!   SQLite the file is on read-only media, which additionally lets it skip
//!   some locking that would otherwise fail on a read-only mount),
//! - opens it via `turso_core::Connection::from_uri`,
//! - drives the resulting `Statement`/`StepResult` state machine directly
//!   (there is no `async`/`.await` anywhere in this path: `turso_core`'s
//!   local-file I/O backend completes synchronously, so `Statement::step`
//!   never actually parks — `pollster::block_on` around the (still
//!   `async fn`) [`read_all_async`] just runs it to completion in one shot).
//!
//! Consequence: this crate works unmodified against read-only-mounted `.d`
//! data directories, and leaves no stray files behind on any filesystem.

use crate::CalibrationError;

use turso::core as turso_core;

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
/// directory). The file is opened strictly read-only; see the module docs
/// for why this never creates a `-wal`/`-shm` sidecar.
///
/// # Errors
/// [`CalibrationError::Open`]/[`CalibrationError::Query`] on I/O or sqlite
/// failures, [`CalibrationError::NoCalibration`] if `MzCalibration` is
/// empty, [`CalibrationError::NoFrames`] if `Frames` is empty.
pub fn read_all(path: &str) -> Result<CalibrationTables, CalibrationError> {
    pollster::block_on(read_all_async(path))
}

/// Percent-encode a filesystem path for embedding in a SQLite `file:` URI
/// path component. Keeps unreserved characters and `/` (the path separator)
/// literal; encodes everything else byte-by-byte, matching the decoder in
/// `turso_core::util::decode_percent`. Encoding `?`/`#`/`%` is required for
/// correctness (they are URI metacharacters); encoding e.g. spaces is not
/// strictly required by the parser but keeps the result a well-formed URI.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// Build a read-only, no-WAL-sidecar SQLite URI for `path`.
fn read_only_uri(path: &str) -> Result<String, CalibrationError> {
    let abs =
        std::fs::canonicalize(path).map_err(|e| CalibrationError::Open(format!("{path}: {e}")))?;
    let abs = abs
        .to_str()
        .ok_or_else(|| CalibrationError::Open(format!("{path}: path is not valid UTF-8")))?;
    Ok(format!(
        "file:{}?mode=ro&immutable=1",
        percent_encode_path(abs)
    ))
}

/// Drive a prepared [`turso_core::Statement`] to completion, calling
/// `on_row` for each row produced.
///
/// `turso_core`'s local-file I/O backend never actually yields (see module
/// docs); `StepResult::IO` is still handled per the documented `step()`
/// contract so this stays correct if a different `IO` backend is ever
/// substituted in.
fn drive_stmt(
    io: &std::sync::Arc<dyn turso_core::IO>,
    mut stmt: turso_core::Statement,
    mut on_row: impl FnMut(&turso_core::Row),
) -> Result<(), CalibrationError> {
    loop {
        match stmt
            .step()
            .map_err(|e| CalibrationError::Query(e.to_string()))?
        {
            turso_core::StepResult::Row => {
                let row = stmt.row().expect("StepResult::Row implies a row exists");
                on_row(row);
            }
            turso_core::StepResult::IO => {
                io.step()
                    .map_err(|e| CalibrationError::Query(e.to_string()))?;
            }
            turso_core::StepResult::Done => break,
            turso_core::StepResult::Busy => {
                return Err(CalibrationError::Query(
                    "database busy on a read-only single-connection query".to_string(),
                ));
            }
            turso_core::StepResult::Interrupt => {
                return Err(CalibrationError::Query("query interrupted".to_string()));
            }
        }
    }
    Ok(())
}

// helpers for nullable/typed column access against turso_core's Value
fn f64_at(row: &turso_core::Row, i: usize) -> Option<f64> {
    match row.get_value(i) {
        turso_core::Value::Numeric(turso_core::Numeric::Float(f)) => Some((*f).into()),
        turso_core::Value::Numeric(turso_core::Numeric::Integer(n)) => Some(*n as f64),
        _ => None,
    }
}
fn int_at(row: &turso_core::Row, i: usize) -> i64 {
    match row.get_value(i) {
        turso_core::Value::Numeric(turso_core::Numeric::Integer(n)) => *n,
        turso_core::Value::Numeric(turso_core::Numeric::Float(f)) => f64::from(*f) as i64,
        _ => 0,
    }
}

async fn read_all_async(path: &str) -> Result<CalibrationTables, CalibrationError> {
    let uri = read_only_uri(path)?;
    let (io, conn) = turso_core::Connection::from_uri(&uri, turso_core::DatabaseOpts::new())
        .map_err(|e| CalibrationError::Open(e.to_string()))?;

    let mut mz = Vec::new();
    let stmt = conn
        .prepare(
            "SELECT Id, ModelType, DigitizerTimebase, DigitizerDelay, T1, T2, dC1, dC2, C0, C1, C2, C3, C4 FROM MzCalibration",
        )
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    drive_stmt(&io, stmt, |r| {
        mz.push(MzCalibration {
            id: int_at(r, 0) as u8,
            model_type: int_at(r, 1) as u8,
            digitizer_timebase: f64_at(r, 2).unwrap_or(0.0),
            digitizer_delay: f64_at(r, 3).unwrap_or(0.0),
            t1: f64_at(r, 4).unwrap_or(0.0),
            t2: f64_at(r, 5).unwrap_or(0.0),
            dc1: f64_at(r, 6).unwrap_or(0.0),
            dc2: f64_at(r, 7).unwrap_or(0.0),
            c0: f64_at(r, 8),
            c1: f64_at(r, 9),
            c2: f64_at(r, 10),
            c3: f64_at(r, 11),
            c4: f64_at(r, 12),
        });
    })?;

    let mut tims = Vec::new();
    let stmt = conn
        .prepare(
            "SELECT Id, ModelType, C0, C1, C2, C3, C4, C5, C6, C7, C8, C9 FROM TimsCalibration",
        )
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    drive_stmt(&io, stmt, |r| {
        tims.push(TimsCalibration {
            id: int_at(r, 0) as u8,
            model_type: int_at(r, 1) as u8,
            c0: f64_at(r, 2),
            c1: f64_at(r, 3),
            c2: f64_at(r, 4),
            c3: f64_at(r, 5),
            c4: f64_at(r, 6),
            c5: f64_at(r, 7),
            c6: f64_at(r, 8),
            c7: f64_at(r, 9),
            c8: f64_at(r, 10),
            c9: f64_at(r, 11),
        });
    })?;

    let mut frames = Vec::new();
    let stmt = conn
        .prepare("SELECT Id, T1, MzCalibration, TimsCalibration FROM Frames")
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    drive_stmt(&io, stmt, |r| {
        frames.push(FrameCal {
            frame_id: int_at(r, 0) as usize,
            t1: f64_at(r, 1).unwrap_or(0.0),
            mz_cal_id: int_at(r, 2) as u8,
            tims_cal_id: int_at(r, 3) as u8,
        });
    })?;

    if mz.is_empty() {
        return Err(CalibrationError::NoCalibration);
    }
    if frames.is_empty() {
        return Err(CalibrationError::NoFrames);
    }
    Ok((mz, tims, frames))
}
