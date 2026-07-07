# timsrust-calibration

Physical (M2) m/z and mobility calibration converters for timsTOF TDF data.

## Read-only open, no WAL sidecar

`RunCalibration::from_path` / `sql::read_all` open the `.tdf` sqlite file
through the [`turso`](https://crates.io/crates/turso) crate (`0.6`),
strictly **read-only**, and never write a `-wal`/`-shm` sidecar next to it —
including when the enclosing `.d` folder's filesystem is mounted read-only.

This is implemented against the low-level `turso::core` (`turso_core`)
engine rather than the ergonomic `turso::Builder`/`Connection`/`Rows`
wrappers: as of `turso` 0.6.1 those high-level wrappers have no read-only
option and don't parse `file:` URIs for the main database path. The path is
canonicalized and opened as `file:<abs-path>?mode=ro&immutable=1` via
`turso_core::Connection::from_uri`, which does parse SQLite URIs and
correctly turns `mode=ro` into `OpenFlags::ReadOnly`. With that flag set,
`WalFileShared::open_shared_if_exists` opens the `-wal` path *without*
`OpenFlags::Create`, returning a no-op in-memory WAL when the sidecar
doesn't already exist, instead of creating one on disk.

Practical implications:
- Opening a real Bruker `.d` folder via this crate leaves **no** stray
  files behind, and works against a genuinely read-only-mounted data
  directory.

See `src/sql.rs` module docs for the exact URI/API details.
