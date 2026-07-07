# timsrust-calibration

Accurate m/z and ion-mobility calibration for Bruker timsTOF `.tdf` data, as
drop-in [`timsrust_core::Converter`] implementations for
[`timsrust`](https://github.com/MannLabs/timsrust) 0.5.x.

## Why

`timsrust`'s built-in TOF→m/z converter approximates the calibration with a
sqrt-linear fit over the acquisition m/z range. This crate instead applies the
physical calibration Bruker records per run in the `.tdf` — the quadratic TOF
polynomial plus a per-frame temperature correction — for a more accurate TOF→m/z
and scan→(1/K0) conversion. The converters implement `timsrust_core::Converter`,
so they drop in wherever `timsrust`'s own converters are used.

It applies the calibration stored in the file; it does **not** perform empirical
recalibration (e.g. lock-mass or MS1-based correction).

## Install

Not yet on crates.io. Add as a path/git dependency alongside `timsrust-core`
from the `timsrust` workspace you build against:

```toml
[dependencies]
timsrust-calibration = { path = "../timsrust-calibration" }
timsrust-core = { path = "../timsrust/crates/timsrust-core" }
```

SQLite reads use the pure-Rust [`turso`](https://crates.io/crates/turso) engine
(no C libsqlite).

## Usage

```rust
use timsrust_calibration::RunCalibration;
use timsrust_core::{Converter, TofIndex, ScanIndex};

// Point at the analysis.tdf file inside a .d folder. Opened read-only:
// no sidecar files are written; read-only-mounted data works.
let cal = RunCalibration::from_path("/data/run.d/analysis.tdf")?;

// One converter per run (median temperature across frames) — the usual choice.
let mz = cal.mz_converter_median()?;
let im = cal.im_converter_median()?;

let mz_value    = mz.convert(TofIndex::try_from(250_000u32)?); // -> Mz
let one_over_k0 = im.convert(ScanIndex::try_from(400u32)?);    // -> Im
```

For maximum accuracy, build a converter for a specific frame (temperature
varies frame to frame). `frame_id` is the Bruker `Frames.Id` (1-based):

```rust
let mz = cal.mz_converter(frame_id)?;
let im = cal.im_converter(frame_id)?;
```

Use the per-run (`_median`) converters unless the run's temperature drifts;
`cal.t1_spread()` (max − min temperature across frames) tells you which to pick.

## API

| Item | Purpose |
|------|---------|
| `RunCalibration::from_path(path)` | Read the calibration from an `analysis.tdf`. |
| `.mz_converter(frame_id)` / `.mz_converter_median()` | TOF→m/z converter (`Converter<TofIndex, Mz>` + inverse). |
| `.im_converter(frame_id)` / `.im_converter_median()` | scan→ion-mobility converter (`Converter<ScanIndex, Im>` + inverse). |
| `.t1_spread()` | Temperature range across the run. |
| `CalibrationError` | Read failures, missing rows/coefficients, unsupported models. |

`frame_id` is the Bruker `Frames.Id` (1-based), not a 0-based offset.

## Notes & limitations

- Reads the calibration tables directly from the `.tdf` sqlite, opened
  read-only — no `-wal`/`-shm` files are created next to your data.
- Applies the stored physical calibration only; no empirical/lock-mass
  recalibration.
- Ion-mobility calibration needs the mobility coefficients to be present in the
  file. Some acquisitions store none; `im_converter*` then returns an error
  while the m/z path still works.
- The per-run (`_median`) converters assume a single calibration per run.

## License

Apache-2.0.
