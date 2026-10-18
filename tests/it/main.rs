//! The root package's (`mashup`) integration tests, as one test binary:
//! `dump` (this package's install inspector, `CARGO_BIN_EXE_dump`). Every
//! other test lives in the crate whose API it needs (docs/OBSERVABILITY.md,
//! section 1); Combat Arms' archive tests are `lithtech_formats`'.

mod dump;
