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
// your data is never modified, and read-only-mounted data works.
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

### With timsrust frames

The converters are ordinary `timsrust_core::Converter`s, so they slot into
`timsrust`'s frame reader — e.g. calibrated m/z for every ion in each frame:

```rust
use timsrust::TimsTofPath;
use timsrust_calibration::RunCalibration;

let cal = RunCalibration::from_path("/data/run.d/analysis.tdf")?;
let mz = cal.mz_converter_median()?;

let frames = TimsTofPath::new("/data/run.d")?.frame_reader()?;
for index in frames.iter_indices() {
    let frame = frames.get_frame(index)?;
    let mz_values = frame.ions().mz_values(&mz); // Vec<Mz>, calibrated
    // pair with frame.ions().intensities(), scan offsets, etc.
}
```

### Calibrating MS2 spectra

`timsrust`'s facade `SpectrumReader` converts each spectrum to m/z with the
**stock** converter before you get it, so you can't inject calibration there.
Instead read spectra at the tdf level, where they arrive as `Spectrum<TofIndex>`
(raw TOF), and convert with this crate's converter:

```rust
// reader: a timsrust::tdf spectrum reader → yields Spectrum<TofIndex>
let spectrum = reader.get(index)?;
let calibrated_mz = spectrum.mz_values(&mz); // Vec<Mz>, calibrated fragments
```

Calibrate **fragment peaks only** — the precursor m/z in a TDF is a value the
instrument stored (not TOF-derived), so leave it as-is. (See the `sage`
integration for a full worked example, including selecting stock-vs-calibrated
converters at runtime.)

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

- Opens the `.tdf` strictly read-only: it never modifies your data files and
  works on read-only-mounted directories.
- Applies the stored physical calibration only; no empirical/lock-mass
  recalibration.
- Ion-mobility calibration needs the mobility coefficients to be present in the
  file. Some acquisitions store none; `im_converter*` then returns an error
  while the m/z path still works.
- The per-run (`_median`) converters assume a single calibration per run.

## License

Apache-2.0.
