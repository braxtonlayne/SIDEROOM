//! Audio tags, cover art and lyrics for the music library (MUS-WASM-001).
//!
//! No tag-parsing dependency is used: the readers are hand-written (ID3v2.2-2.4, ID3v1, FLAC, Ogg
//! Vorbis/Opus, MP4 `ilst`). The public Rust API ([`read_tags`], [`read_tags_tail`],
//! [`lrc::parse_lrc`], [`lrc::lrc_sidecar_names`]) and the wasm-bindgen surface are the stable
//! shape; `lofty` can replace the internals later without changing either.
//!
//! The client passes the first 1 MiB of a file (never the whole file) and, for MP3 without an
//! ID3v2 head, the last 128 bytes. Every reader returns an error on truncated or garbage input.

use wasm_bindgen::prelude::*;

pub mod flac;
pub mod id3;
pub mod lofty_path;
pub mod lrc;
pub mod mp4;
pub mod ogg;
pub mod tags;

pub use lrc::{LrcLine, lrc_sidecar_names, parse_lrc};
pub use tags::{Cover, TagError, Tags};

/// The hand-written readers, picking one by magic bytes. Fallback behind [`read_tags`] and the
/// source of the stream duration (TLEN, FLAC STREAMINFO, MP4 mvhd) that lofty cannot give for a prefix.
pub fn read_tags_builtin(head: &[u8]) -> Result<Tags, TagError> {
    if head.starts_with(b"ID3") {
        id3::read_v2(head)
    } else if head.starts_with(b"fLaC") {
        flac::read(head)
    } else if head.starts_with(b"OggS") {
        ogg::read(head)
    } else if head.get(4..8) == Some(b"ftyp") {
        mp4::read(head)
    } else if head.len() >= 128 && id3::read_v1(head).is_ok() {
        // A small MP3 passed whole, with only an ID3v1 tag.
        id3::read_v1(head)
    } else {
        Err(TagError::Unsupported)
    }
}

/// Read tags from the head of a file. lofty is tried first (MP3, FLAC, Ogg Vorbis/Opus, M4A, WAV);
/// the hand-written reader fills gaps (stream duration, a cover lofty skipped) and takes over when
/// lofty cannot read the prefix.
pub fn read_tags(head: &[u8]) -> Result<Tags, TagError> {
    let builtin = read_tags_builtin(head);
    match (lofty_path::read(head), builtin) {
        (Some(mut l), Ok(b)) => {
            l.fill_from(b);
            Ok(l)
        }
        (Some(l), Err(_)) => Ok(l),
        (None, b) => b,
    }
}

/// Read an ID3v1 tag from the last 128 bytes of a file.
pub fn read_tags_tail(tail: &[u8]) -> Result<Tags, TagError> {
    id3::read_v1(tail)
}

fn js(e: TagError) -> JsError {
    JsError::new(&e.to_string())
}

/// Tags as JSON (`coverMime` instead of the picture; see [`read_cover`]).
#[wasm_bindgen(js_name = readTags)]
pub fn read_tags_json(head: &[u8]) -> Result<String, JsError> {
    read_tags(head).map(|t| t.to_json()).map_err(js)
}

/// The picture bytes of `head`, empty when there is none.
#[wasm_bindgen(js_name = readCover)]
pub fn read_cover(head: &[u8]) -> Vec<u8> {
    read_tags(head)
        .ok()
        .and_then(|t| t.cover)
        .map(|c| c.bytes)
        .unwrap_or_default()
}

#[wasm_bindgen(js_name = readTagsTail)]
pub fn read_tags_tail_json(tail: &[u8]) -> Result<String, JsError> {
    read_tags_tail(tail).map(|t| t.to_json()).map_err(js)
}

/// LRC text as JSON `[{ms, line}]`.
#[wasm_bindgen(js_name = parseLrc)]
pub fn parse_lrc_json(text: &str) -> String {
    let lines: Vec<serde_json::Value> = parse_lrc(text)
        .into_iter()
        .map(|l| serde_json::json!({ "ms": l.ms, "line": l.line }))
        .collect();
    serde_json::Value::Array(lines).to_string()
}

#[wasm_bindgen(js_name = lrcSidecarNames)]
pub fn lrc_sidecar_names_js(track_key: &str) -> Vec<String> {
    lrc_sidecar_names(track_key)
}

#[cfg(test)]
mod tests;

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    /// The `JsError` path is only constructible on the wasm target.
    #[wasm_bindgen_test]
    fn garbage_throws_js_error() {
        assert!(read_tags_json(b"not audio").is_err());
        assert!(read_tags_tail_json(b"short").is_err());
        assert!(read_cover(b"not audio").is_empty());
    }

    #[wasm_bindgen_test]
    fn lrc_round_trips_through_json() {
        let out = parse_lrc_json("[00:01.50]hi");
        assert!(out.contains("\"ms\":1500") && out.contains("\"line\":\"hi\""));
        assert_eq!(lrc_sidecar_names_js("a/b.mp3")[0], "a/b.lrc");
    }
}
