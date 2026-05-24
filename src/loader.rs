//! Load `geo.bin` from disk into a [`Geocoder`].
//!
//! Reads the whole file in one syscall, leaks the buffer to `&'static [u8]`,
//! and hands it to the parser. No copy after the read; pages stay resident
//! → sub-µs lookups, no page-fault overhead.

#![warn(missing_docs)]

use std::fs;
use std::io;
use std::path::Path;

use crate::database::Geocoder;

/// Database filename written by [`crate::builder::build`].
pub const DEFAULT_FILENAME: &str = "geo.bin";

/// Load a `geo.bin` file into a ready-to-query [`Geocoder`].
pub fn load_from_file(path: impl AsRef<Path>) -> io::Result<Geocoder> {
    let bytes: &'static [u8] = Box::leak(fs::read(path)?.into_boxed_slice());
    Ok(Geocoder::from_bytes(bytes))
}

/// `true` if a non-empty file exists at `path`.
pub fn exists(path: impl AsRef<Path>) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
}
