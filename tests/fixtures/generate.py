#!/usr/bin/env python3
"""Generate small synthetic .tdf (SQLite) fixtures for timsrust-calibration's
golden test, using only the Python standard library (sqlite3).

We deliberately do NOT adapt the Bruker tdf_simulator.ipynb notebooks here:
`RunCalibration::from_path` (via `src/sql.rs::read_all`) only ever reads three
tables (MzCalibration, TimsCalibration, Frames) with a fixed set of columns,
so a tiny hand-rolled generator is sufficient and avoids pulling in the heavy
Bruker-simulation notebook dependencies.

Produces:
  tests/fixtures/flat_t1.d/analysis.tdf   -- all frames share one T1
  tests/fixtures/spread_t1.d/analysis.tdf -- frames span a wide T1 range

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


def insert_calibrations(conn: sqlite3.Connection) -> None:
    conn.execute(
        """
        INSERT INTO MzCalibration
            (Id, ModelType, DigitizerTimebase, DigitizerDelay, T1, T2, dC1, dC2, C0, C1, C2, C3, C4)
        VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL)
        """,
        (
            MZ_MODEL_TYPE,
            DIGITIZER_TIMEBASE,
            DIGITIZER_DELAY,
            MZ_T1,
            MZ_T2,
            MZ_DC1,
            MZ_DC2,
            MZ_C0,
            MZ_C1,
        ),
    )
    conn.execute(
        """
        INSERT INTO TimsCalibration
            (Id, ModelType, C0, C1, C2, C3, C4, C5, C6, C7, C8, C9)
        VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TIMS_MODEL_TYPE,
            TIMS_C0,
            TIMS_C1,
            TIMS_C2,
            TIMS_C3,
            TIMS_C4,
            TIMS_C5,
            TIMS_C6,
            TIMS_C7,
            TIMS_C8,
            TIMS_C9,
        ),
    )


def insert_frames(conn: sqlite3.Connection, t1_values: list[float]) -> None:
    for frame_id, t1 in enumerate(t1_values, start=1):
        conn.execute(
            "INSERT INTO Frames (Id, T1, MzCalibration, TimsCalibration) VALUES (?, ?, 1, 1)",
            (frame_id, t1),
        )


def build(path: Path, t1_values: list[float]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    conn = sqlite3.connect(path)
    try:
        create_schema(conn)
        insert_calibrations(conn)
        insert_frames(conn, t1_values)
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

    print("wrote", fixtures_dir / "flat_t1.d" / "analysis.tdf")
    print("wrote", fixtures_dir / "spread_t1.d" / "analysis.tdf")


if __name__ == "__main__":
    main()
