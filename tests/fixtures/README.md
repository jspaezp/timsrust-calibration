# Synthetic `.tdf` fixtures

These are small, hand-generated SQLite files that satisfy exactly what
`src/sql.rs::read_all` reads from a real Bruker `analysis.tdf`: the
`MzCalibration`, `TimsCalibration`, and `Frames` tables (only the columns
that are queried). They exist purely to make the golden test
(`tests/golden.rs`) runnable in CI without a real, multi-gigabyte Bruker
`.d` folder.

## Deviation from the task brief

The brief originally suggested adapting Bruker's `tdf_simulator.ipynb`
notebook. We did not do that: `read_all` only ever touches three tables
with a fixed column list, so pulling in the notebook's full simulation
stack (which builds realistic frame/scan/peak binary blobs, TIMS
calibration state machines, etc.) would add a heavy dependency for no
benefit. Instead we use a small deterministic generator,
[`generate.py`](./generate.py), written against the Python standard
library's `sqlite3` module only (no third-party dependencies). The
resulting files are opened successfully by `turso` (the SQLite engine
`timsrust-calibration` uses) and are consumed by the same
`RunCalibration::from_path` code path used against real Bruker data.

## Regenerating

```bash
cd timsrust-calibration
uv run --no-project python tests/fixtures/generate.py
```

This overwrites both `.tdf` files in place. Nothing else needs to be
regenerated afterward; the golden test reads the files directly via
relative paths, so no environment variables are required.

## Coefficients

Both fixtures share the same `MzCalibration` (Id=1, ModelType=1) and
`TimsCalibration` (Id=2, ModelType=2) rows, taken verbatim from the
crate's existing unit tests (`src/mz.rs`, `src/im.rs`) so they are known
to produce physically sane converters:

`MzCalibration`:

| column | value |
| --- | --- |
| `DigitizerTimebase` | `0.125` |
| `DigitizerDelay` | `25741.0` |
| `T1` | `20.9410989491122` |
| `T2` | `24.8706161298104` |
| `dC1` | `20.0` |
| `dC2` | `0.0` |
| `C0` | `286.065160463331` |
| `C1` | `154317.348188993` |
| `C2`, `C3`, `C4` | `NULL` (unused by `ModelType=1`) |

`TimsCalibration`:

| column | value |
| --- | --- |
| `C0` | `1.0` |
| `C1` | `708.0` |
| `C2` | `241.751905250524` |
| `C3` | `99.2437539638487` |
| `C4` | `33.9622641509434` |
| `C5` | `1.0` |
| `C6` | `0.0071422641733084` |
| `C7` | `164.998795925213` |
| `C8` | `16.3705403907576` |
| `C9` | `2553.11607142569` |

## `Frames` / T1 setup

Both files have 10 frames (`Id` 1..10), all referencing `MzCalibration`
id 1 via `Frames.MzCalibration` **and** `TimsCalibration` id 2 via the
separate `Frames.TimsCalibration` column (Bruker's schema has two distinct
FK columns on `Frames`). The generator deliberately gives `MzCalibration`
and `TimsCalibration` *different* ids (1 vs. 2, instead of letting both
tables coincidentally use id 1) so that a regression which swaps which FK
feeds the m/z converter vs. the IM converter is actually caught: with
mismatched ids, looking up the wrong table for a given id fails with
`CalibrationError::CalIdNotFound` instead of silently succeeding against
the same row. Because both FKs point at real, well-formed rows in their
respective tables, these are the only fixtures where
`RunCalibration::im_converter`/`im_converter_median` succeed end-to-end
(see the `flat_file_im_converter_in_sane_range` golden test) — real DDA
files observed so far have an empty `TimsCalibration` table, and real DIA
files observed so far have `NULL` `TimsCalibration` coefficients.

- **`flat_t1.d/analysis.tdf`**: every frame's `T1` equals the calibration's
  reference `T1` (`20.9410989491122`), so the per-frame drift correction
  (`dc1 * (cal.t1 - frame.t1)`, see `CalibratedTof2MzConverter::try_from_calibration`
  in `src/mz.rs`) is exactly zero for every frame. `t1_spread()` is `0.0`
  and `mz_converter_median()` is numerically identical to
  `mz_converter(1)` (and any other frame) at every TOF index.

- **`spread_t1.d/analysis.tdf`**: frame `T1` values are linearly spaced
  from `20.90` to `21.05` across the 10 frames
  (`20.90, 20.9167, 20.9333, ..., 21.05`). This gives `t1_spread() ≈ 0.15`
  (well above the `0.01` threshold used in the golden test) and makes the
  median-T1 converter (using the median of all 10 frame T1 values) diverge
  from the frame-1 converter (`T1 = 20.90`, the low end of the range) once
  the `dc1` correction is folded into `C1`. The divergence is small in
  absolute terms at low TOF but becomes numerically observable at high TOF
  (the golden test checks at TOF index `500000`, near the top of the TDF's
  practical TOF range) because the `C1` correction scales with `tof^2`.

## Malformed fixtures (`tests/failure_paths.rs`)

`generate.py` also emits a handful of deliberately-broken `.d` folders, each
built from the same base rows with exactly one thing wrong, used by
`tests/failure_paths.rs` to exercise every `CalibrationError` variant against
a real (synthetic) TDF file rather than only via in-memory unit tests:

| fixture | what's wrong | error exercised |
| --- | --- | --- |
| `null_mz_field.d` | `MzCalibration.DigitizerTimebase` is `NULL` | `UnexpectedNull { table: "MzCalibration", .. }` |
| `null_frame_t1.d` | `Frames.T1` is `NULL` | `UnexpectedNull { table: "Frames", .. }` |
| `missing_mz_coeffs.d` | `MzCalibration.C0` is `NULL` (schema-nullable, but `ModelType=1` needs it) | `MissingMzCoefficients` |
| `missing_im_coeffs.d` | `TimsCalibration.C6` is `NULL` (schema-nullable, but `ModelType=2` needs it) | `MissingImCoefficients` |
| `bad_mz_model.d` | `MzCalibration.ModelType = 99` | `UnsupportedMzModel` |
| `bad_im_model.d` | `TimsCalibration.ModelType = 99` | `UnsupportedImModel` |
| `bad_cal_id.d` | `Frames.MzCalibration` points at id `999`, which has no row | `CalIdNotFound` |

`FrameNotFound` and the inverse-converter panic-safety edge cases
(NaN/±Inf/huge input) don't need a fixture — they're exercised against
`flat_t1.d` (an out-of-range `frame_id`) and as pure in-memory unit tests in
`src/mz.rs`/`src/im.rs` respectively, since they don't depend on anything
read from a TDF file.

## Why the golden test doesn't hard-code absolute m/z values

`tests/golden.rs` only makes *relative* assertions (median vs. per-frame
agreement/disagreement at specific TOF indices, plus `t1_spread()`
thresholds). This avoids needing to hand-derive an absolute expected m/z
for a synthetic file while still exercising the exact behavior the task
cares about: does the per-frame vs. median T1 correction actually change
the converter's output when frame T1s disagree, and does it *not* change
it when they agree.
