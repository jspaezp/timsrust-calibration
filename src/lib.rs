//! Physical (M2) calibration converters for timsTOF TDF data.

pub mod im;
pub mod mz;
pub mod run;
pub mod sql;

pub use im::CalibratedScan2ImConverter;
pub use mz::CalibratedTof2MzConverter;
pub use run::RunCalibration;

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
}
