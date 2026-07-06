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
    #[error("MzCalibration {0} missing c0/c1 coefficients")]
    MissingMzCoefficients(u8),
    #[error("TimsCalibration {0} missing coefficients")]
    MissingImCoefficients(u8),
    #[error("calibration id {0} not found")]
    CalIdNotFound(u8),
    /// The requested `frame_id` (a `Frames.Id` value) has no matching row.
    #[error("frame {0} not found")]
    FrameNotFound(usize),
}
