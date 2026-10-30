//! The `lofty` reader (MUS-WASM-001): the primary path for MP3, FLAC, Ogg Vorbis/Opus, M4A and WAV.
//!
//! It reads from an in-memory `Cursor` over the head bytes the client fetched, never a file. The
//! stream duration is not taken from lofty (for MP3 and Ogg it is estimated from the file length,
//! which is wrong for a prefix); WAV is the exception because its header states the data size.

use crate::tags::{Cover, Tags, clean, normalise_mime};
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use std::io::Cursor;

fn parse(bytes: &[u8], properties: bool) -> Option<lofty::file::TaggedFile> {
    let opts = ParseOptions::new().read_properties(properties);
    Probe::new(Cursor::new(bytes))
        .options(opts)
        .guess_file_type()
        .ok()?
        .read()
        .ok()
}

fn from_tag(tag: &Tag, t: &mut Tags) {
    let own = |c: std::borrow::Cow<'_, str>| clean(&c);
    t.title = t.title.take().or_else(|| tag.title().and_then(own));
    t.artist = t.artist.take().or_else(|| tag.artist().and_then(own));
    t.album = t.album.take().or_else(|| tag.album().and_then(own));
    t.genre = t.genre.take().or_else(|| tag.genre().and_then(own));
    t.album_artist = t
        .album_artist
        .take()
        .or_else(|| tag.get_string(ItemKey::AlbumArtist).and_then(clean));
    t.track = t.track.or(tag.track().filter(|n| *n > 0));
    t.disc = t.disc.or(tag.disk().filter(|n| *n > 0));
    t.year = t
        .year
        .or(tag.date().map(|d| u32::from(d.year)).filter(|y| *y > 0));
    if t.cover.is_none() {
        let pics = tag.pictures();
        let pick = pics
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| pics.first());
        if let Some(p) = pick
            && !p.data().is_empty()
        {
            let raw = p.mime_type().map(|m| m.as_str()).unwrap_or_default();
            t.cover = Some(Cover {
                mime: normalise_mime(raw, p.data()),
                bytes: p.data().to_vec(),
            });
        }
    }
}

fn is_empty(t: &Tags) -> bool {
    *t == Tags::default()
}

/// `None` when lofty cannot read the buffer or finds no tag fields in it.
pub fn read(bytes: &[u8]) -> Option<Tags> {
    let file = parse(bytes, false)?;
    let mut t = Tags::default();
    // Primary tag first, then any other tag the file carries (e.g. ID3v1 behind ID3v2).
    if let Some(tag) = file.primary_tag() {
        from_tag(tag, &mut t);
    }
    for tag in file.tags() {
        from_tag(tag, &mut t);
    }
    if file.file_type() == FileType::Wav
        && let Some(full) = parse(bytes, true)
    {
        let d = full.properties().duration().as_millis();
        t.duration_hint = u64::try_from(d).ok().filter(|d| *d > 0);
    }
    (!is_empty(&t)).then_some(t)
}
