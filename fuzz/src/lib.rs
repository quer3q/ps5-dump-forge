//! Helpers shared by the fuzz targets.

pub mod sparse;

/// At most this many files are read back per input, at most `READ_CAP` bytes each: enough to
/// reach every data path (direct, indirect, FAT chain, compressed block) without letting one
/// input spend seconds copying.
pub const MAX_FILES: usize = 16;
pub const READ_CAP: usize = 256 * 1024;
