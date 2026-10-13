//! EPUB validation, metadata and cover extraction (LIB-ING-005). The file itself is kept
//! as uploaded; this only reads enough of it to accept it and to describe it.
//!
//! An EPUB is a ZIP, so it gets the same treatment as a comic archive ([`ArchiveLimits`]):
//! entry count, declared sizes and compression ratios are checked from the central
//! directory, names are never used as paths (traversal is rejected), encrypted entries are
//! refused, and every entry that is read is read through a hard byte cap. Nothing inside is
//! executed or rendered, and the XML parsers refuse DTDs.
//!
//! Validation is the structural minimum the reader needs: a `mimetype` entry reading
//! `application/epub+zip`, `META-INF/container.xml` naming a package document, a package
//! document that parses and has a title and a non-empty spine. Metadata comes from the
//! package document (`dc:title`, `dc:creator`, `dc:language`, `dc:description`,
//! `dc:identifier`). The cover is the manifest item carrying the `cover-image` property
//! (EPUB 3) or named by `<meta name="cover">` (EPUB 2); it must be a raster image.

use std::io::{Read, Seek, SeekFrom};

use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use zip::ZipArchive;

use super::IngestError;
use super::archive::{ArchiveLimits, safe_name};

const MIMETYPE: &str = "application/epub+zip";
const MAX_XML_BYTES: u64 = 4 * 1024 * 1024;
/// Largest cover image we will pull out.
pub const MAX_COVER_BYTES: u64 = 16 * 1024 * 1024;
const MAX_EVENTS: usize = 200_000;
const MAX_DEPTH: usize = 12;
const MAX_FIELD_CHARS: usize = 20_000;
const MAX_LIST: usize = 100;
const MAX_ITEMS: usize = 20_000;
const COVER_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubCover {
    /// Path of the image inside the EPUB.
    pub name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EpubInfo {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub identifier: Option<String>,
    /// Number of spine items (reading order entries).
    pub spine_len: usize,
    pub cover: Option<EpubCover>,
    /// Things that are wrong but not fatal (a misplaced `mimetype`, an unusable cover).
    pub warnings: Vec<String>,
}

fn bad(msg: impl std::fmt::Display) -> IngestError {
    IngestError::Metadata(format!("EPUB: {msg}"))
}

fn zip_err(e: impl std::fmt::Display) -> IngestError {
    IngestError::Archive(e.to_string())
}

fn read_capped<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    cap: u64,
) -> Result<Vec<u8>, IngestError> {
    let f = zip
        .by_name(name)
        .map_err(|_| bad(format!("missing {name}")))?;
    if f.size() > cap {
        return Err(IngestError::EntryTooLarge(name.to_owned()));
    }
    let mut out = Vec::new();
    f.take(cap + 1)
        .read_to_end(&mut out)
        .map_err(|e| IngestError::Archive(format!("{name}: {e}")))?;
    if out.len() as u64 > cap {
        return Err(IngestError::EntryTooLarge(name.to_owned()));
    }
    Ok(out)
}

/// Resolve `href` (percent-encoded, relative to the package document's folder) to an
/// archive path, or `None` when it would leave the archive or is not a plain path.
fn resolve(opf_dir: &str, href: &str) -> Option<String> {
    let href = href.split(['#', '?']).next()?;
    if href.contains("://") || href.starts_with('/') || href.starts_with('\\') {
        return None;
    }
    let b = href.as_bytes();
    let mut decoded = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(b[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(decoded).ok()?.replace('\\', "/");
    if decoded.trim_matches('/').is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = opf_dir.split('/').filter(|p| !p.is_empty()).collect();
    for c in decoded.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A budgeted pull parser: counts events and depth and refuses DTDs.
struct Walker<'a> {
    reader: Reader<&'a [u8]>,
    events: usize,
    depth: usize,
}

impl<'a> Walker<'a> {
    fn new(bytes: &'a [u8], what: &str) -> Result<Self, IngestError> {
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        // UTF-8 is what EPUB mandates; refuse anything else instead of guessing.
        std::str::from_utf8(bytes).map_err(|_| bad(format!("{what} is not UTF-8")))?;
        let mut reader = Reader::from_reader(bytes);
        reader.config_mut().check_end_names = true;
        Ok(Self {
            reader,
            events: 0,
            depth: 0,
        })
    }

    fn next(&mut self) -> Result<Event<'a>, IngestError> {
        self.events += 1;
        if self.events > MAX_EVENTS {
            return Err(bad("too many XML elements"));
        }
        let ev = self
            .reader
            .read_event()
            .map_err(|e| bad(format!("XML: {e}")))?;
        match &ev {
            Event::DocType(_) => return Err(bad("DTDs are not allowed")),
            Event::Start(_) => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(bad("XML nested too deeply"));
                }
            }
            Event::End(_) => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        Ok(ev)
    }
}

fn attr(e: &BytesStart<'_>, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (a.key.local_name().as_ref() == key)
            .then(|| {
                a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()
                    .map(|v| v.into_owned())
            })
            .flatten()
    })
}

/// `META-INF/container.xml` -> the package document's path.
fn rootfile(xml: &[u8]) -> Result<String, IngestError> {
    let mut w = Walker::new(xml, "container.xml")?;
    loop {
        match w.next()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "rootfile" => {
                if let Some(p) = attr(&e, "full-path") {
                    return safe_name(&p).map_err(|_| bad("the package path is not safe"));
                }
            }
            Event::Eof => return Err(bad("container.xml names no package document")),
            _ => {}
        }
    }
}

#[derive(Default)]
struct Package {
    title: Option<String>,
    authors: Vec<String>,
    language: Option<String>,
    description: Option<String>,
    identifier: Option<String>,
    spine_len: usize,
    /// EPUB 2 `<meta name="cover" content="item-id"/>`.
    cover_id: Option<String>,
    /// Manifest: (id, href, media-type, properties).
    items: Vec<(String, String, String, String)>,
}

fn text_of(buf: &str) -> Option<String> {
    let t = buf.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// Elements whose data is all in attributes (`item`, `itemref`, `meta`).
fn attributes_only(p: &mut Package, name: &str, e: &BytesStart<'_>) {
    match name {
        "item" if p.items.len() < MAX_ITEMS => {
            if let (Some(id), Some(href)) = (attr(e, "id"), attr(e, "href")) {
                p.items.push((
                    id,
                    href,
                    attr(e, "media-type").unwrap_or_default(),
                    attr(e, "properties").unwrap_or_default(),
                ));
            }
        }
        "itemref" => p.spine_len += 1,
        "meta" if attr(e, "name").as_deref() == Some("cover") => {
            p.cover_id = attr(e, "content");
        }
        _ => {}
    }
}

fn package(xml: &[u8]) -> Result<Package, IngestError> {
    let mut w = Walker::new(xml, "package document")?;
    let mut p = Package::default();
    let mut current: Option<(String, String)> = None;
    let mut saw_root = false;
    loop {
        match w.next()? {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_owned();
                if w.depth == 1 {
                    if name != "package" {
                        return Err(bad("the root element is not package"));
                    }
                    saw_root = true;
                }
                attributes_only(&mut p, &name, &e);
                if matches!(
                    name.as_str(),
                    "title" | "creator" | "language" | "description" | "identifier"
                ) {
                    current = Some((name, String::new()));
                }
            }
            Event::Empty(e) => {
                if w.depth == 0 {
                    return Err(bad("the package document is empty"));
                }
                let name = e.local_name().as_ref().to_owned();
                attributes_only(&mut p, &name, &e);
            }
            Event::Text(t) => {
                if let Some((_, buf)) = &mut current
                    && buf.chars().count() < MAX_FIELD_CHARS
                {
                    buf.push_str(&t.xml10_content());
                }
            }
            Event::CData(c) => {
                if let Some((_, buf)) = &mut current
                    && buf.chars().count() < MAX_FIELD_CHARS
                {
                    buf.push_str(&c.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some((_, buf)) = &mut current {
                    let name: &str = &r;
                    let piece = match name {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => r.resolve_char_ref().ok().flatten(),
                    };
                    if let Some(c) = piece {
                        buf.push(c);
                    }
                }
            }
            Event::End(e) => {
                let local = e.local_name().as_ref().to_owned();
                if let Some((name, buf)) = current.take_if(|(n, _)| *n == local) {
                    let v = text_of(&buf);
                    match (name.as_str(), v) {
                        ("title", Some(v)) if p.title.is_none() => p.title = Some(v),
                        ("creator", Some(v)) if p.authors.len() < MAX_LIST => p.authors.push(v),
                        ("language", Some(v)) if p.language.is_none() => p.language = Some(v),
                        ("description", Some(v)) if p.description.is_none() => {
                            p.description = Some(v);
                        }
                        ("identifier", Some(v)) if p.identifier.is_none() => {
                            p.identifier = Some(v);
                        }
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err(bad("no package element"));
    }
    Ok(p)
}

/// Validate an EPUB and read its metadata and cover.
pub fn inspect<R: Read + Seek>(
    mut reader: R,
    limits: &ArchiveLimits,
) -> Result<EpubInfo, IngestError> {
    let len = reader.seek(SeekFrom::End(0)).map_err(zip_err)?;
    if len > limits.max_archive_bytes {
        return Err(IngestError::TooLarge("archive file"));
    }
    reader.seek(SeekFrom::Start(0)).map_err(zip_err)?;
    let mut zip = ZipArchive::new(reader).map_err(zip_err)?;
    if zip.len() > limits.max_entries {
        return Err(IngestError::TooManyEntries);
    }
    let mut warnings = Vec::new();
    let mut first_name = None;
    let mut declared = 0u64;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(zip_err)?;
        let raw = f.name().map_err(zip_err)?.into_owned();
        let name = safe_name(&raw)?;
        if f.encrypted() {
            return Err(IngestError::Encrypted(name));
        }
        if first_name.is_none() {
            first_name = Some(name.clone());
        }
        if f.is_dir() || f.is_symlink() {
            continue;
        }
        let (size, packed) = (f.size(), f.compressed_size());
        if size > limits.max_entry_bytes {
            return Err(IngestError::EntryTooLarge(name));
        }
        if size >= limits.ratio_floor && size / packed.max(1) > limits.max_ratio {
            return Err(IngestError::SuspiciousRatio(name));
        }
        declared = declared.saturating_add(size);
        if declared > limits.max_total_bytes {
            return Err(IngestError::TooLarge("total uncompressed size"));
        }
    }
    let mime = read_capped(&mut zip, "mimetype", 256)?;
    if String::from_utf8_lossy(&mime).trim() != MIMETYPE {
        return Err(bad("mimetype is not application/epub+zip"));
    }
    if first_name.as_deref() != Some("mimetype") {
        warnings.push("mimetype is not the first entry in the archive".to_owned());
    }
    let container = read_capped(&mut zip, "META-INF/container.xml", MAX_XML_BYTES)?;
    let opf_path = rootfile(&container)?;
    let opf = read_capped(&mut zip, &opf_path, MAX_XML_BYTES)?;
    let p = package(&opf)?;
    let title = p
        .title
        .clone()
        .ok_or_else(|| bad("the book has no title"))?;
    if p.spine_len == 0 {
        return Err(bad("the book has an empty reading order"));
    }
    let opf_dir = opf_path.rsplit_once('/').map_or("", |(d, _)| d);

    // EPUB 3 `cover-image` property first, then the EPUB 2 `<meta name="cover">` id.
    let cover_item = p
        .items
        .iter()
        .find(|i| i.3.split_whitespace().any(|t| t == "cover-image"))
        .or_else(|| {
            p.cover_id
                .as_deref()
                .and_then(|id| p.items.iter().find(|i| i.0 == id))
        });
    let mut cover = None;
    if let Some((_, href, media, _)) = cover_item {
        let media = media.to_ascii_lowercase();
        match resolve(opf_dir, href) {
            Some(path) if COVER_TYPES.contains(&media.as_str()) => {
                match read_capped(&mut zip, &path, MAX_COVER_BYTES) {
                    Ok(bytes) => {
                        cover = Some(EpubCover {
                            name: path,
                            media_type: media,
                            bytes,
                        });
                    }
                    Err(e) => warnings.push(format!("cover not read: {e}")),
                }
            }
            Some(_) => warnings.push(format!("cover is not a raster image ({media})")),
            None => warnings.push("cover path is not usable".to_owned()),
        }
    }
    Ok(EpubInfo {
        title,
        authors: p.authors,
        language: p.language,
        description: p.description,
        identifier: p.identifier,
        spine_len: p.spine_len,
        cover,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    fn opf(meta: &str, manifest: &str, spine: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">{meta}</metadata>
 <manifest>{manifest}</manifest>
 <spine>{spine}</spine>
</package>"#
        )
    }

    const META: &str = r#"<dc:title>The  Example
 Book &amp; Co</dc:title><dc:creator>Alice A</dc:creator><dc:creator>Bob B</dc:creator>
        <dc:language>en</dc:language><dc:identifier id="id">urn:uuid:1234</dc:identifier>
        <dc:description>A short <![CDATA[<b>blurb</b>]]>.</dc:description>
        <meta name="cover" content="cov"/>"#;
    const MANIFEST: &str = r#"<item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
        <item id="cov" href="images/cover%20art.jpg" media-type="image/jpeg"/>"#;
    const SPINE: &str = r#"<itemref idref="c1"/>"#;

    fn epub(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (n, d) in files {
            let m = if *n == "mimetype" {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            w.start_file(*n, SimpleFileOptions::default().compression_method(m))
                .unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn good(opf_xml: &str) -> Vec<u8> {
        epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf_xml.as_bytes()),
            ("OEBPS/ch1.xhtml", b"<html/>"),
            ("OEBPS/images/cover art.jpg", b"\xFF\xD8\xFFcover"),
        ])
    }

    fn inspect_bytes(b: Vec<u8>) -> Result<EpubInfo, IngestError> {
        inspect(Cursor::new(b), &ArchiveLimits::default())
    }

    #[test]
    fn metadata_and_epub2_cover_are_read() {
        let info = inspect_bytes(good(&opf(META, MANIFEST, SPINE))).unwrap();
        assert_eq!(info.title, "The Example Book & Co");
        assert_eq!(info.authors, ["Alice A", "Bob B"]);
        assert_eq!(info.language.as_deref(), Some("en"));
        assert_eq!(info.identifier.as_deref(), Some("urn:uuid:1234"));
        assert_eq!(info.description.as_deref(), Some("A short <b>blurb</b>."));
        assert_eq!(info.spine_len, 1);
        let c = info.cover.unwrap();
        assert_eq!(c.name, "OEBPS/images/cover art.jpg");
        assert_eq!(c.media_type, "image/jpeg");
        assert_eq!(c.bytes, b"\xFF\xD8\xFFcover");
        assert!(info.warnings.is_empty());
    }

    #[test]
    fn epub3_cover_image_property_wins() {
        let manifest = r#"<item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
          <item id="old" href="old.jpg" media-type="image/jpeg"/>
          <item id="new" href="images/cover art.jpg" media-type="image/jpeg" properties="cover-image"/>"#;
        let info = inspect_bytes(good(&opf(META, manifest, SPINE))).unwrap();
        assert_eq!(info.cover.unwrap().name, "OEBPS/images/cover art.jpg");
    }

    #[test]
    fn a_book_without_a_cover_is_fine() {
        let manifest = r#"<item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>"#;
        let meta = "<dc:title>T</dc:title>";
        let info = inspect_bytes(good(&opf(meta, manifest, SPINE))).unwrap();
        assert!(info.cover.is_none() && info.authors.is_empty());
    }

    #[test]
    fn unusable_covers_are_warnings_not_failures() {
        let cases = [
            // SVG is not a raster image.
            r#"<item id="cov" href="c.svg" media-type="image/svg+xml"/>"#,
            // Points outside the archive.
            r#"<item id="cov" href="../../../etc/passwd" media-type="image/png"/>"#,
            // A remote URL.
            r#"<item id="cov" href="https://evil.test/c.png" media-type="image/png"/>"#,
            // Missing file.
            r#"<item id="cov" href="gone.png" media-type="image/png"/>"#,
        ];
        for item in cases {
            let manifest = format!(
                r#"<item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>{item}"#
            );
            let info = inspect_bytes(good(&opf(META, &manifest, SPINE))).unwrap();
            assert!(info.cover.is_none(), "{item}");
            assert_eq!(info.warnings.len(), 1, "{item}: {:?}", info.warnings);
        }
    }

    #[test]
    fn structural_problems_are_rejected() {
        let ok_opf = opf(META, MANIFEST, SPINE);
        let m = |e: Result<EpubInfo, IngestError>| matches!(e, Err(IngestError::Metadata(_)));
        // Wrong mimetype.
        let b = epub(&[
            ("mimetype", b"application/zip"),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", ok_opf.as_bytes()),
        ]);
        assert!(m(inspect_bytes(b)));
        // No container.
        let b = epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("OEBPS/content.opf", ok_opf.as_bytes()),
        ]);
        assert!(m(inspect_bytes(b)));
        // Container names a package that is not there.
        let b = epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
        ]);
        assert!(m(inspect_bytes(b)));
        // No title, empty spine, wrong root.
        assert!(m(inspect_bytes(good(&opf("", MANIFEST, SPINE)))));
        assert!(m(inspect_bytes(good(&opf(META, MANIFEST, "")))));
        assert!(m(inspect_bytes(good("<html><body/></html>"))));
        // Not a ZIP at all.
        assert!(matches!(
            inspect_bytes(b"%PDF-1.7".to_vec()),
            Err(IngestError::Archive(_))
        ));
    }

    #[test]
    fn a_misplaced_mimetype_is_only_a_warning() {
        let b = epub(&[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("mimetype", MIMETYPE.as_bytes()),
            ("OEBPS/content.opf", opf(META, MANIFEST, SPINE).as_bytes()),
            ("OEBPS/images/cover art.jpg", b"img"),
        ]);
        let info = inspect_bytes(b).unwrap();
        assert!(info.warnings.iter().any(|w| w.contains("mimetype")));
    }

    #[test]
    fn dtds_and_entity_tricks_are_refused() {
        let body = opf(META, MANIFEST, SPINE);
        let (_, rest) = body.split_once("?>").unwrap();
        let evil = format!(
            "<?xml version=\"1.0\"?><!DOCTYPE package [<!ENTITY x SYSTEM \"file:///etc/passwd\">]>{rest}"
        );
        assert!(matches!(
            inspect_bytes(good(&evil)),
            Err(IngestError::Metadata(m)) if m.contains("DTD")
        ));
        let evil_container = r#"<!DOCTYPE c [<!ENTITY a "aaaa">]><container/>"#;
        let b = epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("META-INF/container.xml", evil_container.as_bytes()),
        ]);
        assert!(matches!(inspect_bytes(b), Err(IngestError::Metadata(_))));
    }

    #[test]
    fn hostile_archives_are_refused_like_comics() {
        let ok_opf = opf(META, MANIFEST, SPINE);
        // Path traversal is refused from the directory alone.
        let b = epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", ok_opf.as_bytes()),
            ("../evil.xhtml", b"x"),
        ]);
        assert!(matches!(inspect_bytes(b), Err(IngestError::UnsafePath(_))));
        // A package path with `..` is refused.
        let c = CONTAINER.replace("OEBPS/content.opf", "../content.opf");
        let b = epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("META-INF/container.xml", c.as_bytes()),
        ]);
        assert!(matches!(inspect_bytes(b), Err(IngestError::Metadata(_))));
        // A compression bomb entry.
        let zeros = vec![0u8; 12 * 1024 * 1024];
        let b = epub(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", ok_opf.as_bytes()),
            ("OEBPS/bomb.bin", &zeros),
        ]);
        assert!(matches!(
            inspect_bytes(b),
            Err(IngestError::SuspiciousRatio(_))
        ));
        // Entry-count, size and total limits.
        let b = good(&ok_opf);
        let few = ArchiveLimits {
            max_entries: 2,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            inspect(Cursor::new(b.clone()), &few),
            Err(IngestError::TooManyEntries)
        ));
        let small = ArchiveLimits {
            max_entry_bytes: 10,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            inspect(Cursor::new(b.clone()), &small),
            Err(IngestError::EntryTooLarge(_))
        ));
        let total = ArchiveLimits {
            max_total_bytes: 100,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            inspect(Cursor::new(b), &total),
            Err(IngestError::TooLarge(_))
        ));
    }

    #[test]
    fn href_resolution_stays_inside_the_archive() {
        assert_eq!(resolve("OEBPS", "a/b.png").unwrap(), "OEBPS/a/b.png");
        assert_eq!(resolve("OEBPS", "../img/c.png").unwrap(), "img/c.png");
        assert_eq!(resolve("", "c%20d.png").unwrap(), "c d.png");
        assert_eq!(resolve("OEBPS", "x.png#frag").unwrap(), "OEBPS/x.png");
        assert!(resolve("OEBPS", "../../x.png").is_none());
        assert!(resolve("OEBPS", "/abs.png").is_none());
        assert!(resolve("OEBPS", "http://h/x.png").is_none());
        assert!(resolve("OEBPS", "bad%zz.png").is_none());
        assert!(resolve("OEBPS", "").is_none());
    }
}
