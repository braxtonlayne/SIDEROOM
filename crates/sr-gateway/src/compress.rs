//! Optional `compress=zstd-stream` transport compression.
//!
//! Pure-Rust (`ruzstd`), so no C toolchain is needed. Each gateway message is
//! emitted as one self-contained zstd frame in a binary WebSocket frame.

use ruzstd::encoding::{CompressionLevel, compress_to_vec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    #[default]
    None,
    ZstdStream,
}

impl Compression {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "" | "none" => Some(Self::None),
            "zstd-stream" => Some(Self::ZstdStream),
            _ => None,
        }
    }
}

pub fn zstd_compress(text: &str) -> Vec<u8> {
    compress_to_vec(text.as_bytes(), CompressionLevel::Fastest)
}
