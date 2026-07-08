//! Physical (M2) calibration converters for timsTOF TDF data.
//!
//! This crate reads the physical calibration tables (`MzCalibration`,
//! `TimsCalibration`, `Frames`) directly out of a Bruker `analysis.tdf`
//! sqlite file and builds [`timsrust_core::Converter`] implementations for
//! TOF-index -> m/z ([`CalibratedTof2MzConverter`]) and
//! scan-index -> ion mobility (1/K0) ([`CalibratedScan2ImConverter`]).
//!
//! "M2" scope: these converters implement the *physical* calibration model
//! (the polynomial models Bruker stores per-run in the TDF cal tables), as
//! opposed to any downstream recalibration (e.g. lock-mass/MS1-based
//! recalibration) that might be layered on top elsewhere.
//!
//! The entry point is [`RunCalibration::from_path`].

pub mod im;
pub mod mz;
pub mod run;
pub mod sql;

pub use im::CalibratedScan2ImConverter;
pub use mz::CalibratedTof2MzConverter;
pub use run::RunCalibration;

/// Errors returned while reading TDF calibration tables or building
/// converters from them.
#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    #[error("failed to open TDF sqlite: {0}")]
    Open(String),
    #[error("query failed: {0}")]
    Query(String),
    #[error("no MzCalibration rows")]
    NoCalibration,
    #[error("no frames")]
    NoFrames,
    #[error("unsupported MzCalibration model_type {0}")]
    UnsupportedMzModel(u8),
    #[error("unsupported TimsCalibration model_type {0}")]
    UnsupportedImModel(u8),
    /// A model-required polynomial coefficient (`C0`..`C4`/`C6`/`C7`) was
    /// `NULL`. These columns are schema-nullable (whether they're needed
    /// depends on `ModelType`); this error means the *requested* model
    /// needs one that's absent. Contrast with [`Self::UnexpectedNull`],
    /// which is for columns the schema declares `NOT NULL`.
    #[error("MzCalibration {0} missing c0/c1 coefficients")]
    MissingMzCoefficients(u32),
    /// See [`Self::MissingMzCoefficients`]; the `TimsCalibration` analogue.
    #[error("TimsCalibration {0} missing coefficients")]
    MissingImCoefficients(u32),
    #[error("calibration id {0} not found")]
    CalIdNotFound(u32),
    /// The requested `frame_id` (a `Frames.Id` value) has no matching row.
    #[error("frame {0} not found")]
    FrameNotFound(usize),
    /// A column the Bruker TDF schema declares `NOT NULL` was `NULL`. This
    /// indicates a corrupt/unexpected file (as opposed to
    /// [`Self::MissingMzCoefficients`]/[`Self::MissingImCoefficients`],
    /// which are for schema-nullable columns the requested model happens to
    /// need). See the [`crate::sql`] module docs for the exact NOT-NULL
    /// column list.
    #[error("{table}.{column} is NULL but the schema declares it NOT NULL")]
    UnexpectedNull {
        table: &'static str,
        column: &'static str,
    },
}

/// Map an inverse-conversion float result to a `u32` that is always safely
/// constructible into a `timsrust_core` index type (`TofIndex`/`ScanIndex`,
/// both backed by `NonZeroU32(x + 1)`).
///
/// `Converter::convert` is an infallible trait, but the underlying algebra
/// (division by a coefficient that can be zero, `sqrt` of a value that can
/// be negative before being driven to `+Inf`/`NaN` by upstream garbage,
/// `as u32` casts that saturate `+Inf`/huge floats to `u32::MAX`) can easily
/// produce non-finite or out-of-range results. Since there is no `Result`
/// to return, we clamp deterministically instead of panicking, mirroring
/// Rust's own saturating `as` float->int cast semantics: `NaN` and negative
/// values (including `-Inf`) -> `0`; `+Inf` and anything at or above
/// `u32::MAX - 1` (the largest value `NonZeroU32::new(x + 1)` can
/// represent) -> `u32::MAX - 1`.
pub(crate) fn clamp_f64_to_index(x: f64) -> u32 {
    const MAX_INDEX: f64 = (u32::MAX - 1) as f64;
    if x.is_nan() || x < 0.0 {
        0
    } else if x >= MAX_INDEX {
        // Also catches `+Inf`, which compares `>=` any finite value.
        u32::MAX - 1
    } else {
        x.round() as u32
    }
}
