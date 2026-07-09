#!/usr/bin/env python3
"""Generate small synthetic .tdf (SQLite) fixtures for timsrust-calibration's
tests, using only the Python standard library (sqlite3).

We deliberately do NOT adapt the Bruker tdf_simulator.ipynb notebooks here:
`RunCalibration::from_path` (via `src/sql.rs::read_all`) only ever reads three
tables (MzCalibration, TimsCalibration, Frames) with a fixed set of columns,
so a tiny hand-rolled generator is sufficient and avoids pulling in the heavy
Bruker-simulation notebook dependencies.

Produces:
  tests/fixtures/flat_t1.d/analysis.tdf         -- all frames share one T1
  tests/fixtures/spread_t1.d/analysis.tdf       -- frames span a wide T1 range
  tests/fixtures/null_mz_field.d/analysis.tdf   -- MzCalibration.DigitizerTimebase NULL
  tests/fixtures/null_frame_t1.d/analysis.tdf   -- Frames.T1 NULL
  tests/fixtures/missing_mz_coeffs.d/analysis.tdf   -- MzCalibration.C0 NULL
  tests/fixtures/missing_im_coeffs.d/analysis.tdf   -- TimsCalibration.C6 NULL
  tests/fixtures/bad_mz_model.d/analysis.tdf    -- MzCalibration.ModelType unsupported
  tests/fixtures/bad_im_model.d/analysis.tdf    -- TimsCalibration.ModelType unsupported
  tests/fixtures/bad_cal_id.d/analysis.tdf      -- Frames.MzCalibration FK dangles
  tests/fixtures/not_a_tdf.d/analysis.tdf       -- valid sqlite, no MzCalibration table

Run with:
  uv run --no-project python tests/fixtures/generate.py
"""

from __future__ import annotations

import sqlite3
from pathlib import Path

# --- known-good MzCalibration coefficients (from src/mz.rs unit test) ------
MZ_MODEL_TYPE = 1
DIGITIZER_TIMEBASE = 0.125
DIGITIZER_DELAY = 25741.0
MZ_T1 = 20.9410989491122
MZ_T2 = 24.8706161298104
MZ_DC1 = 20.0
MZ_DC2 = 0.0
MZ_C0 = 286.065160463331
MZ_C1 = 154317.348188993
# C2/C3/C4 are NULL for this model.

# --- known-good TimsCalibration coefficients (from src/im.rs unit test) ---
TIMS_MODEL_TYPE = 2
TIMS_C0 = 1.0
TIMS_C1 = 708.0
TIMS_C2 = 241.751905250524
TIMS_C3 = 99.2437539638487
TIMS_C4 = 33.9622641509434
TIMS_C5 = 1.0
TIMS_C6 = 0.0071422641733084
TIMS_C7 = 164.998795925213
TIMS_C8 = 16.3705403907576
TIMS_C9 = 2553.11607142569

NUM_FRAMES = 10


def create_schema(conn: sqlite3.Connection) -> None:
    conn.execute(
        """
        CREATE TABLE MzCalibration (
            Id INTEGER PRIMARY KEY,
            ModelType INTEGER,
            DigitizerTimebase REAL,
            DigitizerDelay REAL,
            T1 REAL,
            T2 REAL,
            dC1 REAL,
            dC2 REAL,
            C0 REAL,
            C1 REAL,
            C2 REAL,
            C3 REAL,
            C4 REAL
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE TimsCalibration (
            Id INTEGER PRIMARY KEY,
            ModelType INTEGER,
            C0 REAL,
            C1 REAL,
            C2 REAL,
            C3 REAL,
            C4 REAL,
            C5 REAL,
            C6 REAL,
            C7 REAL,
            C8 REAL,
            C9 REAL
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE Frames (
            Id INTEGER PRIMARY KEY,
            T1 REAL,
            MzCalibration INTEGER,
            TimsCalibration INTEGER
        )
        """
    )


def insert_mz_calibration(
    conn: sqlite3.Connection,
    *,
    id: int = 1,
    model_type: int | None = MZ_MODEL_TYPE,
    digitizer_timebase: float | None = DIGITIZER_TIMEBASE,
    digitizer_delay: float | None = DIGITIZER_DELAY,
    t1: float | None = MZ_T1,
    t2: float | None = MZ_T2,
    dc1: float | None = MZ_DC1,
    dc2: float | None = MZ_DC2,
    c0: float | None = MZ_C0,
    c1: float | None = MZ_C1,
    c2: float | None = None,
    c3: float | None = None,
    c4: float | None = None,
) -> None:
    """Insert one `MzCalibration` row. Defaults are the known-good
    coefficients from `src/mz.rs`'s unit test; pass a keyword override of
    `None` (or an unsupported value) to build a malformed fixture."""
    conn.execute(
        """
        INSERT INTO MzCalibration
            (Id, ModelType, DigitizerTimebase, DigitizerDelay, T1, T2, dC1, dC2, C0, C1, C2, C3, C4)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (id, model_type, digitizer_timebase, digitizer_delay, t1, t2, dc1, dc2, c0, c1, c2, c3, c4),
    )


def insert_tims_calibration(
    conn: sqlite3.Connection,
    *,
    id: int = 2,
    model_type: int | None = TIMS_MODEL_TYPE,
    c0: float | None = TIMS_C0,
    c1: float | None = TIMS_C1,
    c2: float | None = TIMS_C2,
    c3: float | None = TIMS_C3,
    c4: float | None = TIMS_C4,
    c5: float | None = TIMS_C5,
    c6: float | None = TIMS_C6,
    c7: float | None = TIMS_C7,
    c8: float | None = TIMS_C8,
    c9: float | None = TIMS_C9,
) -> None:
    """Insert one `TimsCalibration` row. Defaults are the known-good
    coefficients from `src/im.rs`'s unit test; pass a keyword override of
    `None` (or an unsupported value) to build a malformed fixture."""
    conn.execute(
        """
        INSERT INTO TimsCalibration
            (Id, ModelType, C0, C1, C2, C3, C4, C5, C6, C7, C8, C9)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (id, model_type, c0, c1, c2, c3, c4, c5, c6, c7, c8, c9),
    )


def insert_frame(
    conn: sqlite3.Connection,
    frame_id: int,
    t1: float | None,
    *,
    mz_cal_id: int | None = 1,
    tims_cal_id: int | None = 2,
) -> None:
    # MzCalibration id (1) and TimsCalibration id (2) are deliberately
    # different by default so a regression that swaps which FK feeds the
    # m/z vs. IM converter is caught as a CalIdNotFound error rather than
    # silently resolving to the same (coincidentally shared) row.
    conn.execute(
        "INSERT INTO Frames (Id, T1, MzCalibration, TimsCalibration) VALUES (?, ?, ?, ?)",
        (frame_id, t1, mz_cal_id, tims_cal_id),
    )


def _new_db(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    conn = sqlite3.connect(path)
    create_schema(conn)
    return conn


def build(path: Path, t1_values: list[float]) -> None:
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn)
        insert_tims_calibration(conn)
        for frame_id, t1 in enumerate(t1_values, start=1):
            insert_frame(conn, frame_id, t1)
        conn.commit()
    finally:
        conn.close()


def build_null_mz_field(path: Path) -> None:
    """MzCalibration.DigitizerTimebase is NULL. Per Bruker's schema this
    column is NOT NULL, so this exercises
    `CalibrationError::UnexpectedNull { table: "MzCalibration",
    column: "DigitizerTimebase" }`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn, digitizer_timebase=None)
        insert_tims_calibration(conn)
        insert_frame(conn, 1, MZ_T1)
        conn.commit()
    finally:
        conn.close()


def build_null_frame_t1(path: Path) -> None:
    """Frames.T1 is NULL. Per Bruker's schema this column is NOT NULL, so
    this exercises `CalibrationError::UnexpectedNull { table: "Frames",
    column: "T1" }`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn)
        insert_tims_calibration(conn)
        insert_frame(conn, 1, None)
        conn.commit()
    finally:
        conn.close()


def build_missing_mz_coeffs(path: Path) -> None:
    """MzCalibration.C0 is NULL while ModelType=1 (which requires it).
    C0/C1 are schema-nullable, so this is read successfully by `read_all`
    and only fails once `CalibratedTof2MzConverter::try_from_calibration`
    is called, with `CalibrationError::MissingMzCoefficients`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn, c0=None)
        insert_tims_calibration(conn)
        insert_frame(conn, 1, MZ_T1)
        conn.commit()
    finally:
        conn.close()


def build_missing_im_coeffs(path: Path) -> None:
    """TimsCalibration.C6 is NULL while ModelType=2 (which requires it) ->
    `CalibrationError::MissingImCoefficients`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn)
        insert_tims_calibration(conn, c6=None)
        insert_frame(conn, 1, MZ_T1)
        conn.commit()
    finally:
        conn.close()


def build_bad_mz_model(path: Path) -> None:
    """MzCalibration.ModelType is an unsupported value ->
    `CalibrationError::UnsupportedMzModel`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn, model_type=99)
        insert_tims_calibration(conn)
        insert_frame(conn, 1, MZ_T1)
        conn.commit()
    finally:
        conn.close()


def build_bad_im_model(path: Path) -> None:
    """TimsCalibration.ModelType is an unsupported value ->
    `CalibrationError::UnsupportedImModel`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn)
        insert_tims_calibration(conn, model_type=99)
        insert_frame(conn, 1, MZ_T1)
        conn.commit()
    finally:
        conn.close()


def build_bad_cal_id(path: Path) -> None:
    """Frames.MzCalibration references an id (999) absent from
    MzCalibration -> `CalibrationError::CalIdNotFound`."""
    conn = _new_db(path)
    try:
        insert_mz_calibration(conn)  # id=1
        insert_tims_calibration(conn)  # id=2
        insert_frame(conn, 1, MZ_T1, mz_cal_id=999, tims_cal_id=2)
        conn.commit()
    finally:
        conn.close()


def build_not_a_tdf(path: Path) -> None:
    """A valid sqlite file that is NOT a TDF: it has a single dummy table and
    no `MzCalibration` table at all. `read_all` opens it fine but the first
    prepare (`SELECT ... FROM MzCalibration`) fails, exercising
    `CalibrationError::NotATdf`."""
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    conn = sqlite3.connect(path)
    try:
        conn.execute("CREATE TABLE NotACalibration (Id INTEGER PRIMARY KEY, Value REAL)")
        conn.execute("INSERT INTO NotACalibration (Id, Value) VALUES (1, 42.0)")
        conn.commit()
    finally:
        conn.close()


def main() -> None:
    fixtures_dir = Path(__file__).resolve().parent

    # flat_t1.d: every frame shares the same T1 as the calibration reference
    # T1 -> dc1 correction is exactly zero for every frame, so median == per-frame.
    flat_t1_values = [MZ_T1] * NUM_FRAMES
    build(fixtures_dir / "flat_t1.d" / "analysis.tdf", flat_t1_values)

    # spread_t1.d: frames span a wide T1 range -> median T1 != frame-1 T1,
    # so the dc1 correction differs between the median and per-frame converters.
    lo, hi = 20.90, 21.05
    step = (hi - lo) / (NUM_FRAMES - 1)
    spread_t1_values = [lo + i * step for i in range(NUM_FRAMES)]
    build(fixtures_dir / "spread_t1.d" / "analysis.tdf", spread_t1_values)

    malformed = {
        "null_mz_field.d": build_null_mz_field,
        "null_frame_t1.d": build_null_frame_t1,
        "missing_mz_coeffs.d": build_missing_mz_coeffs,
        "missing_im_coeffs.d": build_missing_im_coeffs,
        "bad_mz_model.d": build_bad_mz_model,
        "bad_im_model.d": build_bad_im_model,
        "bad_cal_id.d": build_bad_cal_id,
        "not_a_tdf.d": build_not_a_tdf,
    }
    for name, builder in malformed.items():
        builder(fixtures_dir / name / "analysis.tdf")

    for name in [
        "flat_t1.d",
        "spread_t1.d",
        *malformed.keys(),
    ]:
        print("wrote", fixtures_dir / name / "analysis.tdf")


if __name__ == "__main__":
    main()
