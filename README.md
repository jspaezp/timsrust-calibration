# timsrust-calibration

Accurate m/z and ion-mobility calibration for Bruker timsTOF `.tdf` data, as
drop-in [`timsrust_core::Converter`](https://docs.rs/timsrust-core) implementations for
[`timsrust`](https://github.com/MannLabs/timsrust) 0.6.5.

## Why

`timsrust`'s built-in TOF→m/z converter approximates the calibration with a
sqrt-linear fit over the acquisition m/z range. This crate instead applies the
physical calibration Bruker records per run in the `.tdf` — the quadratic TOF
polynomial plus a per-frame temperature correction — for a more accurate TOF→m/z
and scan→(1/K0) conversion. The converters implement `timsrust_core::Converter`,
so they drop in wherever `timsrust`'s own converters are used.

On internal datasets with temperature shifts, this approach produces more
accurate calibration than the stock `timsrust` approximation.

It applies the calibration stored in the file; it does **not** perform empirical
recalibration (e.g. lock-mass or MS1-based correction).


## Install

```toml
[dependencies]
timsrust-calibration = "0.2"
```

Until it lands on crates.io, depend on it via git:

```toml
[dependencies]
timsrust-calibration = { git = "https://github.com/jspaezp/timsrust-calibration" }
```

SQLite reads use the pure-Rust [`turso`](https://crates.io/crates/turso) engine
(no C libsqlite).

## Compatibility

This crate is built against `timsrust-core` **0.6.5** — the line that
`timsrust` **0.6.5** uses. Your project's `timsrust` must resolve to that same
`timsrust-core 0.6.x`, so that a single shared version is in the dependency
tree. The converters implement `timsrust_core::Converter`; if two incompatible
`timsrust-core` versions end up in the tree (e.g. your `timsrust` pulls a `0.7`
core), that trait is a *different type* and the converters silently won't
satisfy `timsrust`'s bounds. Cargo unifies the `0.6.x` line automatically, so
this only bites if `timsrust`/`timsrust-core` make a breaking (`0.7`+) jump.

## Usage

```rust
use timsrust_calibration::RunCalibration;
use timsrust_core::{Converter, TofIndex, ScanIndex};

// `from_path` is opened read-only: your data is never modified, and
// read-only-mounted data works. It accepts any of:

// 1. the analysis.tdf file inside a .d folder directly
let cal = RunCalibration::from_path("/data/run.d/analysis.tdf")?;

// 2. the enclosing .d run directory (analysis.tdf is resolved for you)
let cal = RunCalibration::from_path("/data/run.d")?;

// 3. any `impl AsRef<str>`, in particular a `timsrust::TimsTofPath` — its
//    `AsRef<str>` yields the .d directory, so this works with no extra
//    plumbing and without this crate depending on `timsrust` itself:
use timsrust::TimsTofPath;
let p = TimsTofPath::new("/data/run.d")?;
let cal = RunCalibration::from_path(&p)?;

// One converter per run (median temperature across frames) — the usual choice.
let mz = cal.mz_converter_median()?;
let im = cal.im_converter_median()?;

let mz_value    = mz.convert(TofIndex::try_from(250_000u32)?); // -> Mz
let one_over_k0 = im.convert(ScanIndex::try_from(400u32)?);    // -> Im
```

Note: `from_path` does not auto-detect the acquisition format the way
`timsrust` does — it just looks for `analysis.tdf` at the resolved path/`.d`
directory. Pointing it at a non-TDF acquisition (TSF, miniTDF, Parquet, ...)
returns a `CalibrationError::FileNotFound` when no `analysis.tdf` exists at
the resolved path, or a `CalibrationError::NotATdf` when the file opens as
sqlite but lacks the defining `MzCalibration` table — not a raw underlying
sqlite-engine error.

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
`timsrust`'s frame reader. m/z and intensity are already per-peak; 1/K0 is
per-scan, so you expand it over each scan's peak range to get three aligned
per-peak vectors:

```rust
use timsrust::TimsTofPath;
use timsrust::core::{Converter, ScanIndex};
use timsrust_calibration::RunCalibration;

let cal = RunCalibration::from_path("/data/run.d/analysis.tdf")?;
let mz_conv = cal.mz_converter_median()?;
let im_conv = cal.im_converter_median()?;

let frames = TimsTofPath::new("/data/run.d")?.frame_reader()?;
for index in frames.iter_indices() {
    let frame = frames.get_frame(index)?;
    let ions = frame.ions();

    // already per-peak, in the same order:
    let mz: Vec<f64> = ions.mz_values(&mz_conv).iter().map(|m| f64::from(*m)).collect();
    let intensity: Vec<u32> = ions.intensities().iter().map(|i| u32::from(*i)).collect();

    // per-peak 1/K0: repeat each scan's mobility across that scan's peaks
    let offsets = ions.scan_offsets(); // len = scan_count() + 1
    let mut mobility: Vec<f64> = Vec::with_capacity(mz.len());
    for scan in 0..ions.scan_count() {
        let k0 = f64::from(im_conv.convert(ScanIndex::try_from(scan as u32)?));
        for _ in offsets[scan]..offsets[scan + 1] {
            mobility.push(k0);
        }
    }

    assert_eq!(mz.len(), intensity.len());
    assert_eq!(mz.len(), mobility.len());
    // mz[i], intensity[i], mobility[i] all describe the same peak
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
- IM calibration applies the static `TimsCalibration` polynomial only; it does
  not apply the per-frame pressure compensation Bruker's format allows
  (`Frames.Pressure`, which Bruker's schema documents as "required to perform
  a pressure compensated tims calibration" — we don't have Bruker's
  pressure-comp formula, so it isn't implemented here). It may diverge on runs with significant pressure drift.
- The per-run (`_median`) converters assume a single calibration per run.

## License

Apache-2.0.
