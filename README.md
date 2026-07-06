# timsrust-calibration

Physical (M2) m/z and mobility calibration converters for timsTOF TDF data.

## Known limitation: WAL sidecar file on open

`RunCalibration::from_path` / `sql::read_all` open the `.tdf` sqlite file
through the [`turso`](https://crates.io/crates/turso) crate (currently
pinned to `0.1`). That version of turso has **no read-only or no-WAL open
mode**: opening a file-backed database — even purely to read from it —
unconditionally creates a `<path>-wal` sidecar file next to it (typically
0 bytes, since this crate never writes). This is a limitation of turso
itself (`turso_core::storage::wal::WalFileShared::open_shared_if_exists`
always opens the wal path with `OpenFlags::Create`, and the public
`turso::Builder` does not expose any way to override this), not something
this crate can currently disable.

Practical implications:
- Every real Bruker `.d` folder opened via this crate will end up with an
  `analysis.tdf-wal` file next to `analysis.tdf`.
- If the `.d` folder's filesystem is genuinely read-only, opening will
  fail outright (turso cannot create the sidecar), not just leave a stray
  file.

If a future turso release adds a read-only/no-WAL mode, `sql::read_all`
should be updated to use it.
