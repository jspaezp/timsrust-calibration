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
//! source, and by `tests/failure_paths.rs`'s
//! `read_only_open_leaves_no_wal_sidecar`, an integration test that asserts
//! no `-wal`/`-shm` file appears after a read-only open of a real fixture).
//!
//! [`read_all`] therefore:
//! - canonicalizes `path` and percent-encodes it into a
//!   `file:<abs-path>?mode=ro&immutable=1` SQLite URI (`immutable=1` tells
//!   SQLite the file is on read-only media, which additionally lets it skip
//!   some locking that would otherwise fail on a read-only mount),
//! - opens it via `turso_core::Connection::from_uri`,
//! - drives the resulting `Statement`/`StepResult` state machine directly.
//!   There is no `async`/`.await` anywhere in this path: `turso_core`'s
//!   local-file I/O backend completes synchronously, so `Statement::step`
//!   never actually parks on a real, local `.tdf` file — [`read_all`] is a
//!   plain synchronous function that drives the step loop to completion in
//!   one shot.
//!
//! Consequence: this crate works unmodified against read-only-mounted `.d`
//! data directories, and leaves no stray files behind on any filesystem.
//!
//! # Nullability
//!
//! Bruker's TDF schema declares some columns this crate reads as `NOT
//! NULL` (verified via `PRAGMA table_info` on real files and Bruker's
//! `tdf-schema.sql`): `MzCalibration.ModelType`/`DigitizerTimebase`/
//! `DigitizerDelay`/`T1`/`dC1`, `TimsCalibration.ModelType`, and
//! `Frames.T1`/`MzCalibration`/`TimsCalibration` (the two FK columns). A
//! `NULL` in one of these columns cannot be produced by a well-formed TDF
//! file, so it is treated as file corruption: reading one returns
//! [`CalibrationError::UnexpectedNull`] rather than silently defaulting to
//! `0.0`/`0`, which would otherwise produce catastrophically wrong (but
//! not obviously wrong) calibration output.
//!
//! By contrast the polynomial coefficient columns (`MzCalibration.C0..C1`,
//! `TimsCalibration.C0..C4`/`C6`/`C7`) *are* schema-nullable: whether a
//! given model needs them depends on `ModelType`. A `NULL` there is
//! expected/legal at the schema level and is represented as `Option<f64>`;
//! [`crate::mz::CalibratedTof2MzConverter::try_from_calibration`] and
//! [`crate::im::CalibratedScan2ImConverter::try_from_calibration`] turn a
//! missing-but-required coefficient into
//! [`CalibrationError::MissingMzCoefficients`]/[`CalibrationError::MissingImCoefficients`].
//! Both are "the file told us something is missing" errors, but
//! `UnexpectedNull` means the schema promised a value and didn't deliver
//! (corruption), while `Missing*Coefficients` means the schema always
//! allowed the absence and we simply can't build the requested model
//! without it.

use std::path::Path;

use crate::CalibrationError;

use turso::core as turso_core;

/// One row of the `MzCalibration` table (physical TOF->m/z model).
///
/// Only the columns this crate's converters actually consume are kept; see
/// the module docs for the columns dropped as unused (`T2`, `dC2`, `C2`,
/// `C3`, `C4`).
#[derive(Clone, Debug, PartialEq)]
pub struct MzCalibration {
    /// `MzCalibration.Id` (primary key), referenced by `Frames.MzCalibration`.
    pub id: u32,
    /// `MzCalibration.ModelType`; only `1` is supported by
    /// [`crate::mz::CalibratedTof2MzConverter`].
    pub model_type: u8,
    /// `MzCalibration.DigitizerTimebase`.
    pub digitizer_timebase: f64,
    /// `MzCalibration.DigitizerDelay`.
    pub digitizer_delay: f64,
    /// `MzCalibration.T1`, the reference digitizer temperature the
    /// calibration was fit at.
    pub t1: f64,
    /// `MzCalibration.dC1`, the per-degree drift coefficient for `C1`.
    pub dc1: f64,
    /// `MzCalibration.C0`; schema-nullable (see module docs).
    pub c0: Option<f64>,
    /// `MzCalibration.C1`; schema-nullable (see module docs).
    pub c1: Option<f64>,
}

/// One row of the `TimsCalibration` table (physical scan->1/K0 mobility model).
///
/// Only the columns this crate's converters actually consume are kept; see
/// the module docs for the columns dropped as unused (`C5`, `C8`, `C9`).
#[derive(Clone, Debug, PartialEq)]
pub struct TimsCalibration {
    /// `TimsCalibration.Id` (primary key), referenced by
    /// `Frames.TimsCalibration`.
    pub id: u32,
    /// `TimsCalibration.ModelType`; only `2` is supported by
    /// [`crate::im::CalibratedScan2ImConverter`].
    pub model_type: u8,
    /// `TimsCalibration.C0`; schema-nullable (see module docs).
    pub c0: Option<f64>,
    /// `TimsCalibration.C1`; schema-nullable (see module docs).
    pub c1: Option<f64>,
    /// `TimsCalibration.C2`; schema-nullable (see module docs).
    pub c2: Option<f64>,
    /// `TimsCalibration.C3`; schema-nullable (see module docs).
    pub c3: Option<f64>,
    /// `TimsCalibration.C4`; schema-nullable (see module docs).
    pub c4: Option<f64>,
    /// `TimsCalibration.C6`; schema-nullable (see module docs).
    pub c6: Option<f64>,
    /// `TimsCalibration.C7`; schema-nullable (see module docs).
    pub c7: Option<f64>,
}

/// A single `Frames` row's calibration linkage: which calibration rows a
/// given frame refers to, plus the T1 value used for the digitizer-drift
/// correction.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameCal {
    /// The Bruker `Frames.Id` (1-based instrument frame index).
    pub frame_id: usize,
    /// `Frames.T1`, the digitizer temperature observed for this frame.
    pub t1: f64,
    /// FK into `MzCalibration.Id` (`Frames.MzCalibration`).
    pub mz_cal_id: u32,
    /// FK into `TimsCalibration.Id` (`Frames.TimsCalibration`).
    ///
    /// Bruker's `Frames` table has two *separate* calibration FK columns;
    /// they happen to share the same value on files observed so far, but
    /// must not be assumed equal.
    pub tims_cal_id: u32,
}

/// Return type of [`read_all`]: the raw `MzCalibration`, `TimsCalibration`,
/// and per-frame calibration-linkage rows read from a TDF sqlite file.
#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationTables {
    /// The `MzCalibration` rows (physical TOF->m/z models).
    pub mz: Vec<MzCalibration>,
    /// The `TimsCalibration` rows (physical scan->1/K0 mobility models).
    pub tims: Vec<TimsCalibration>,
    /// The per-frame calibration-linkage rows read from `Frames`.
    pub frames: Vec<FrameCal>,
}

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
/// empty, [`CalibrationError::NoFrames`] if `Frames` is empty,
/// [`CalibrationError::UnexpectedNull`] if a schema-`NOT NULL` column (see
/// module docs) is null.
pub fn read_all(path: impl AsRef<Path>) -> Result<CalibrationTables, CalibrationError> {
    let uri = read_only_uri(path.as_ref())?;
    let (io, conn) = turso_core::Connection::from_uri(&uri, turso_core::DatabaseOpts::new())
        .map_err(|e| CalibrationError::Open(e.to_string()))?;

    let mut mz = Vec::new();
    let stmt = conn
        .prepare(
            "SELECT Id, ModelType, DigitizerTimebase, DigitizerDelay, T1, dC1, C0, C1 FROM MzCalibration",
        )
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    let mut mz_err = None;
    drive_stmt(&io, stmt, |r| {
        if mz_err.is_some() {
            return;
        }
        match read_mz_row(r) {
            Ok(row) => mz.push(row),
            Err(e) => mz_err = Some(e),
        }
    })?;
    if let Some(e) = mz_err {
        return Err(e);
    }

    let mut tims = Vec::new();
    let stmt = conn
        .prepare("SELECT Id, ModelType, C0, C1, C2, C3, C4, C6, C7 FROM TimsCalibration")
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    let mut tims_err = None;
    drive_stmt(&io, stmt, |r| {
        if tims_err.is_some() {
            return;
        }
        match read_tims_row(r) {
            Ok(row) => tims.push(row),
            Err(e) => tims_err = Some(e),
        }
    })?;
    if let Some(e) = tims_err {
        return Err(e);
    }

    let mut frames = Vec::new();
    let stmt = conn
        .prepare("SELECT Id, T1, MzCalibration, TimsCalibration FROM Frames")
        .map_err(|e| CalibrationError::Query(e.to_string()))?;
    let mut frame_err = None;
    drive_stmt(&io, stmt, |r| {
        if frame_err.is_some() {
            return;
        }
        match read_frame_row(r) {
            Ok(row) => frames.push(row),
            Err(e) => frame_err = Some(e),
        }
    })?;
    if let Some(e) = frame_err {
        return Err(e);
    }

    if mz.is_empty() {
        return Err(CalibrationError::NoCalibration);
    }
    if frames.is_empty() {
        return Err(CalibrationError::NoFrames);
    }
    Ok(CalibrationTables { mz, tims, frames })
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
///
/// Returns [`CalibrationError::Open`] (rather than silently lossy-converting)
/// if the canonicalized path isn't valid UTF-8, since the URI it feeds to
/// `turso_core` must be a `str`.
fn read_only_uri(path: &Path) -> Result<String, CalibrationError> {
    let abs = std::fs::canonicalize(path)
        .map_err(|e| CalibrationError::Open(format!("{}: {e}", path.display())))?;
    let abs = abs.to_str().ok_or_else(|| {
        CalibrationError::Open(format!("{}: path is not valid UTF-8", path.display()))
    })?;
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

/// Read column `i` as an `f64`, or `None` if it is `NULL`/non-numeric.
/// Use for schema-nullable columns (the polynomial coefficients).
fn f64_at(row: &turso_core::Row, i: usize) -> Option<f64> {
    match row.get_value(i) {
        turso_core::Value::Numeric(turso_core::Numeric::Float(f)) => Some((*f).into()),
        turso_core::Value::Numeric(turso_core::Numeric::Integer(n)) => Some(*n as f64),
        _ => None,
    }
}

/// Read column `i` as an `i64`, or `None` if it is `NULL`/non-numeric.
/// Use for schema-nullable integer columns.
fn int_at_opt(row: &turso_core::Row, i: usize) -> Option<i64> {
    match row.get_value(i) {
        turso_core::Value::Numeric(turso_core::Numeric::Integer(n)) => Some(*n),
        turso_core::Value::Numeric(turso_core::Numeric::Float(f)) => Some(f64::from(*f) as i64),
        _ => None,
    }
}

/// Read column `i` as an `f64`, erroring with [`CalibrationError::UnexpectedNull`]
/// if it is `NULL`/non-numeric. Use for columns the schema declares `NOT NULL`.
fn required_f64_at(
    row: &turso_core::Row,
    i: usize,
    table: &'static str,
    column: &'static str,
) -> Result<f64, CalibrationError> {
    f64_at(row, i).ok_or(CalibrationError::UnexpectedNull { table, column })
}

/// Read column `i` as an `i64`, erroring with [`CalibrationError::UnexpectedNull`]
/// if it is `NULL`/non-numeric. Use for columns the schema declares `NOT NULL`.
fn required_int_at(
    row: &turso_core::Row,
    i: usize,
    table: &'static str,
    column: &'static str,
) -> Result<i64, CalibrationError> {
    int_at_opt(row, i).ok_or(CalibrationError::UnexpectedNull { table, column })
}

fn read_mz_row(r: &turso_core::Row) -> Result<MzCalibration, CalibrationError> {
    // `Id` is an `INTEGER PRIMARY KEY` (sqlite rowid alias), which cannot be
    // NULL, so it doesn't need the `required_*` NOT-NULL treatment.
    let id = int_at_opt(r, 0).unwrap_or(0) as u32;
    Ok(MzCalibration {
        id,
        model_type: required_int_at(r, 1, "MzCalibration", "ModelType")? as u8,
        digitizer_timebase: required_f64_at(r, 2, "MzCalibration", "DigitizerTimebase")?,
        digitizer_delay: required_f64_at(r, 3, "MzCalibration", "DigitizerDelay")?,
        t1: required_f64_at(r, 4, "MzCalibration", "T1")?,
        dc1: required_f64_at(r, 5, "MzCalibration", "dC1")?,
        c0: f64_at(r, 6),
        c1: f64_at(r, 7),
    })
}

fn read_tims_row(r: &turso_core::Row) -> Result<TimsCalibration, CalibrationError> {
    let id = int_at_opt(r, 0).unwrap_or(0) as u32;
    Ok(TimsCalibration {
        id,
        model_type: required_int_at(r, 1, "TimsCalibration", "ModelType")? as u8,
        c0: f64_at(r, 2),
        c1: f64_at(r, 3),
        c2: f64_at(r, 4),
        c3: f64_at(r, 5),
        c4: f64_at(r, 6),
        c6: f64_at(r, 7),
        c7: f64_at(r, 8),
    })
}

fn read_frame_row(r: &turso_core::Row) -> Result<FrameCal, CalibrationError> {
    let frame_id = int_at_opt(r, 0).unwrap_or(0) as usize;
    Ok(FrameCal {
        frame_id,
        t1: required_f64_at(r, 1, "Frames", "T1")?,
        mz_cal_id: required_int_at(r, 2, "Frames", "MzCalibration")? as u32,
        tims_cal_id: required_int_at(r, 3, "Frames", "TimsCalibration")? as u32,
    })
}
