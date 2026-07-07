# timsrust-calibration

Physical (**M2**) m/z and ion-mobility calibration converters for Bruker timsTOF
`.tdf` data, built to plug into the new-API (0.5.x) crate-split
[`timsrust`](https://github.com/MannLabs/timsrust).

## Why

`timsrust`'s built-in TOF→m/z converter (`UncalibratedTof2MzConverter`) is a
sqrt-linear fit from the acquisition m/z bounds. This crate instead applies the
**physical calibration model Bruker stores per run** in the `.tdf`
(`MzCalibration` / `TimsCalibration` tables): the quadratic TOF polynomial
(`c0`/`c1`) plus digitizer timebase/delay, with a **per-frame temperature
correction** (`c1` scaled by `dc1·(T1_ref − T1_frame)`). The result is a
materially more accurate TOF→m/z and scan→(1/K0) conversion.

Converters implement [`timsrust_core::Converter`], so they are drop-in wherever
you use `timsrust`'s own converters.

## Status

Experimental. Scope is **M2 only** — the physical per-run calibration model.
Empirical/lock-mass recalibration (matching unfragmented precursors, "M3") is
**not** included. Targets new-API `timsrust` 0.5.x.

## Install

Not yet published to crates.io. Add as a path/git dependency. It needs
`timsrust-core` (from the same `timsrust` workspace you build against):

```toml
[dependencies]
timsrust-calibration = { path = "../timsrust-calibration" }
timsrust-core = { path = "../timsrust/crates/timsrust-core" }
```

Requires Rust 2021. SQLite reads go through the pure-Rust
[`turso`](https://crates.io/crates/turso) engine (no C libsqlite).

## Usage

```rust
use timsrust_calibration::RunCalibration;
use timsrust_core::{Converter, TofIndex, ScanIndex};

// Point at the analysis.tdf *file* inside a .d folder. Opened strictly
// read-only: no -wal/-shm sidecar is written, works on read-only mounts.
let cal = RunCalibration::from_path("/data/run.d/analysis.tdf")?;

// One converter per run using the median T1 across all frames. Good default
// when intra-run temperature drift is small — check cal.t1_spread() to decide.
let mz = cal.mz_converter_median()?;
let im = cal.im_converter_median()?;

let mz_value  = mz.convert(TofIndex::try_from(250_000u32)?); // -> timsrust_core::Mz
let one_over_k0 = im.convert(ScanIndex::try_from(400u32)?);  // -> timsrust_core::Im
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Per-frame (maximal accuracy)

Temperature (`T1`) varies frame to frame; for the exact per-frame calibration,
build a converter for a specific `Frames.Id` (1-based instrument frame index):

```rust
# use timsrust_calibration::RunCalibration;
# use timsrust_core::{Converter, TofIndex};
# let cal = RunCalibration::from_path("/data/run.d/analysis.tdf")?;
let frame_id = 1; // Bruker Frames.Id, NOT a 0-based offset
let mz = cal.mz_converter(frame_id)?;
let im = cal.im_converter(frame_id)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Median vs per-frame

- **`*_median`** — one converter for the whole run (median `T1`). Fits the
  usual "one converter per file" pipeline; negligible error when temperature is
  stable within the run.
- **per-frame** — `mz_converter(frame_id)` / `im_converter(frame_id)` rebuild
  the T1 correction for each frame. Use when accuracy matters and/or
  `t1_spread()` is large.
- **`t1_spread()`** returns `max(T1) − min(T1)` across the run so you can pick.

## API

| Item | Purpose |
|------|---------|
| `RunCalibration::from_path(path) -> Result<Self, CalibrationError>` | Read the cal tables + per-frame T1 from an `analysis.tdf` (read-only). |
| `.mz_converter(frame_id)` / `.mz_converter_median()` | `CalibratedTof2MzConverter` (`Converter<TofIndex, Mz>` + inverse). |
| `.im_converter(frame_id)` / `.im_converter_median()` | `CalibratedScan2ImConverter` (`Converter<ScanIndex, Im>` + inverse). |
| `.t1_spread() -> f64` | Intra-run T1 range, to choose median vs per-frame. |
| `CalibrationError` | Open/query failures, missing rows, unsupported model types, missing coefficients, `FrameNotFound`. |

`frame_id` is always the Bruker `Frames.Id` (1-based), not a 0-based offset.

## How it works

- Reads `MzCalibration`, `TimsCalibration`, and `Frames` (`Id`, `T1`, and the
  separate `MzCalibration` and `TimsCalibration` foreign keys) straight from the
  `.tdf` sqlite. New-API `timsrust` does not expose these tables or per-frame
  `T1`, so this crate reads the file directly.
- TOF→m/z: `mz = c1·(tof − c0)² / 1e12`, `tof = idx·timebase + delay`, with
  `c1` temperature-corrected per frame. IM: `TimsCalibration` model_type 2.
- **Read-only, no sidecar:** the `.tdf` is opened as
  `file:<abs-path>?mode=ro&immutable=1` via `turso_core` (turso ≥ 0.6), so no
  `-wal`/`-shm` file is ever created next to your data, and read-only-mounted
  `.d` directories work. (turso ≤ 0.1.5 could not do this; the higher-level
  `turso::Builder` still has no read-only option, so the low-level engine is
  used — see `src/sql.rs` module docs.)

## Limitations

- **M2 only** — no empirical/lock-mass recalibration.
- Supports `MzCalibration` model_type 1 and `TimsCalibration` model_type 2;
  other model types return an error.
- IM calibration requires the `TimsCalibration` coefficients to be populated.
  Some real files (e.g. certain DIA `.tdf`) have a model-type-2 row with **null**
  coefficients, or no `TimsCalibration` row at all — `im_converter*` then returns
  `MissingImCoefficients` / `CalIdNotFound`. The m/z path is unaffected.
- The `_median` methods assume a **single calibration per run** (they use the
  first frame's cal ids); they do not reconcile multiple distinct calibrations
  within one run.

## Testing

Unit tests and golden fixtures need no data. Integration tests that read a real
`.tdf` are gated on an env var and skip if it is unset:

```bash
TIMSRUST_CAL_TEST_TDF=/data/run.d/analysis.tdf cargo test
```

Synthetic fixtures under `tests/fixtures/` are generated by `generate.py`
(stdlib `sqlite3`, run via `uv run --no-project python tests/fixtures/generate.py`).

## License

Apache-2.0.
