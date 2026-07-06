use crate::CalibrationError;

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

#[derive(Clone, Debug, PartialEq)]
pub struct FrameCal {
    pub frame_id: usize,
    pub t1: f64,
    pub cal_id: u8,
}

pub fn read_all(
    path: &str,
) -> Result<(Vec<MzCalibration>, Vec<TimsCalibration>, Vec<FrameCal>), CalibrationError> {
    pollster::block_on(read_all_async(path))
}

async fn read_all_async(
    path: &str,
) -> Result<(Vec<MzCalibration>, Vec<TimsCalibration>, Vec<FrameCal>), CalibrationError> {
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
        .query("SELECT Id, T1, MzCalibration FROM Frames", ())
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
            cal_id: int_at(&r, 2) as u8,
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
