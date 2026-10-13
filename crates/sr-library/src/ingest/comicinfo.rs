//! `ComicInfo.xml` parser (LIB-ING-003): the de-facto metadata file inside CBZ archives
//! (the ComicRack / Anansi schema).
//!
//! The parser is deliberately small and strict about hostile input: UTF-8 only, no
//! DTDs (so no entity expansion), bounded size, depth, event count, field length and
//! page count. Unknown elements are ignored; unparsable numbers become `None`.

use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};

use super::IngestError;
use crate::model::ReadingDirection;

pub const MAX_BYTES: usize = 1024 * 1024;
const MAX_EVENTS: usize = 50_000;
const MAX_DEPTH: usize = 8;
const MAX_FIELD_CHARS: usize = 20_000;
const MAX_PAGES: usize = 5000;

/// The `Manga` element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Manga {
    #[default]
    Unknown,
    No,
    Yes,
    YesAndRightToLeft,
}

/// A `<Page>` entry; `image` is the zero-based page index.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComicPage {
    pub image: u32,
    pub kind: Option<String>,
    pub double_page: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComicInfo {
    pub title: Option<String>,
    pub series: Option<String>,
    /// The raw `Number` ("12", "12.5", "Annual 1").
    pub number: Option<String>,
    pub volume: Option<u32>,
    pub summary: Option<String>,
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub writers: Vec<String>,
    pub pencillers: Vec<String>,
    pub inkers: Vec<String>,
    pub colorists: Vec<String>,
    pub letterers: Vec<String>,
    pub cover_artists: Vec<String>,
    pub publisher: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub web: Option<String>,
    /// `LanguageISO`, as written.
    pub language: Option<String>,
    pub manga: Manga,
    pub age_rating: Option<String>,
    pub page_count: Option<u32>,
    pub pages: Vec<ComicPage>,
}

fn bad(msg: &str) -> IngestError {
    IngestError::Metadata(format!("ComicInfo.xml: {msg}"))
}

fn list(v: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in v.split([',', ';']) {
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if !s.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&s)) {
            out.push(s);
        }
    }
    out
}

fn text(v: &str) -> Option<String> {
    let t = v.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

fn flag(v: Option<&str>) -> bool {
    v.is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

impl ComicInfo {
    fn set(&mut self, name: &str, v: &str) {
        match name.to_ascii_lowercase().as_str() {
            "title" => self.title = text(v),
            "series" => self.series = text(v),
            "number" => self.number = text(v),
            "volume" => self.volume = v.trim().parse().ok(),
            "summary" => self.summary = text(v),
            "year" => self.year = v.trim().parse().ok(),
            "month" => self.month = v.trim().parse().ok().filter(|m| (1..=12).contains(m)),
            "day" => self.day = v.trim().parse().ok().filter(|d| (1..=31).contains(d)),
            "writer" => self.writers = list(v),
            "penciller" => self.pencillers = list(v),
            "inker" => self.inkers = list(v),
            "colorist" => self.colorists = list(v),
            "letterer" => self.letterers = list(v),
            "coverartist" => self.cover_artists = list(v),
            "publisher" => self.publisher = text(v),
            "genre" => self.genres = list(v),
            "tags" => self.tags = list(v),
            "web" => self.web = text(v),
            "languageiso" => self.language = text(v),
            "agerating" => self.age_rating = text(v),
            "pagecount" => self.page_count = v.trim().parse().ok(),
            "manga" => {
                self.manga = match v.trim().to_ascii_lowercase().as_str() {
                    "yesandrighttoleft" => Manga::YesAndRightToLeft,
                    "yes" => Manga::Yes,
                    "no" => Manga::No,
                    _ => Manga::Unknown,
                }
            }
            _ => {}
        }
    }

    fn page(&mut self, e: &BytesStart<'_>) {
        let mut p = ComicPage::default();
        let mut have_image = false;
        for a in e.attributes().flatten() {
            let key = a.key.as_ref().to_ascii_lowercase();
            let val: &str = a.value.as_ref();
            match key.as_str() {
                "image" => {
                    if let Ok(i) = val.trim().parse() {
                        p.image = i;
                        have_image = true;
                    }
                }
                "type" => p.kind = text(val),
                "doublepage" => p.double_page = flag(Some(val)),
                "imagewidth" => p.width = val.trim().parse().ok(),
                "imageheight" => p.height = val.trim().parse().ok(),
                _ => {}
            }
        }
        if have_image && self.pages.len() < MAX_PAGES {
            self.pages.push(p);
        }
    }

    /// Reading direction implied by `Manga`: manga is right to left, `No` is left to right.
    pub fn reading_direction(&self) -> Option<ReadingDirection> {
        match self.manga {
            Manga::Yes | Manga::YesAndRightToLeft => Some(ReadingDirection::Rtl),
            Manga::No => Some(ReadingDirection::Ltr),
            Manga::Unknown => None,
        }
    }

    /// Whether `AgeRating` names an adults-only (18+) rating. "Mature 17+" is not auto-restricted.
    pub fn is_age_restricted(&self) -> bool {
        self.age_rating.as_deref().is_some_and(|r| {
            let r = r.to_ascii_lowercase();
            r.contains("18+") || r.contains("r18") || r.contains("x18") || r.contains("adults only")
        })
    }

    /// The numeric chapter number when `Number` is a plain decimal.
    pub fn chapter_number(&self) -> Option<f64> {
        self.number
            .as_deref()
            .and_then(|n| n.trim().parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n >= 0.0)
    }

    /// Zero-based indexes of pages marked as double-page spreads.
    pub fn double_page_indexes(&self) -> Vec<u32> {
        self.pages
            .iter()
            .filter(|p| p.double_page)
            .map(|p| p.image)
            .collect()
    }

    /// Everyone credited on the art side, de-duplicated.
    pub fn artists(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in self
            .pencillers
            .iter()
            .chain(&self.inkers)
            .chain(&self.cover_artists)
        {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(a)) {
                out.push(a.clone());
            }
        }
        out
    }
}

/// Parse a `ComicInfo.xml` document.
pub fn parse(bytes: &[u8]) -> Result<ComicInfo, IngestError> {
    if bytes.len() > MAX_BYTES {
        return Err(bad("file is too large"));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let src = std::str::from_utf8(bytes).map_err(|_| bad("only UTF-8 is supported"))?;
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;

    let mut info = ComicInfo::default();
    let (mut depth, mut events) = (0usize, 0usize);
    let mut saw_root = false;
    let mut current: Option<(String, String)> = None;
    let mut in_pages = false;
    loop {
        events += 1;
        if events > MAX_EVENTS {
            return Err(bad("too many elements"));
        }
        let ev = reader.read_event().map_err(|e| bad(&e.to_string()))?;
        match ev {
            Event::Start(e) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(bad("nested too deeply"));
                }
                let name = e.local_name().as_ref().to_owned();
                match depth {
                    1 => {
                        if !name.eq_ignore_ascii_case("ComicInfo") {
                            return Err(bad("the root element is not ComicInfo"));
                        }
                        saw_root = true;
                    }
                    2 => {
                        in_pages = name.eq_ignore_ascii_case("Pages");
                        current = Some((name, String::new()));
                    }
                    3 if in_pages && name.eq_ignore_ascii_case("Page") => info.page(&e),
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let name = e.local_name().as_ref().to_owned();
                if depth == 0 {
                    if !name.eq_ignore_ascii_case("ComicInfo") {
                        return Err(bad("the root element is not ComicInfo"));
                    }
                    saw_root = true;
                } else if depth == 2 && in_pages && name.eq_ignore_ascii_case("Page") {
                    info.page(&e);
                }
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
                // Only the five predefined entities and character references are honoured;
                // anything else (custom entities cannot be declared: no DTD) is dropped.
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
            Event::DocType(_) => return Err(bad("DTDs are not allowed")),
            Event::End(_) => {
                if depth == 2 {
                    if let Some((name, value)) = current.take()
                        && !name.eq_ignore_ascii_case("Pages")
                    {
                        info.set(&name, &value);
                    }
                    in_pages = false;
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err(bad("no ComicInfo element"));
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Title>The Beginning &amp; the End</Title>
  <Series>Example Saga</Series>
  <Number>12.5</Number>
  <Volume>3</Volume>
  <Summary>
    Line one.
    A &lt;tag&gt; and a snowman &#9731;.
  </Summary>
  <Year>2021</Year><Month>7</Month><Day>31</Day>
  <Writer>Alice A, Bob B; alice a</Writer>
  <Penciller>Carol C</Penciller>
  <Inker>Carol C, Dan D</Inker>
  <CoverArtist>Eve E</CoverArtist>
  <Publisher>Example Press</Publisher>
  <Genre>Fantasy, Adventure</Genre>
  <Tags>slow burn; found family</Tags>
  <Web>https://example.test/saga</Web>
  <LanguageISO>en</LanguageISO>
  <Manga>YesAndRightToLeft</Manga>
  <AgeRating>Teen</AgeRating>
  <PageCount>3</PageCount>
  <Unknown>ignored</Unknown>
  <Pages>
    <Page Image="0" Type="FrontCover" ImageWidth="1600" ImageHeight="2400"/>
    <Page Image="1" DoublePage="True"/>
    <Page Image="2"></Page>
    <Page Type="NoImageAttr"/>
  </Pages>
</ComicInfo>"#;

    #[test]
    fn parses_a_full_document() {
        let c = parse(FULL.as_bytes()).unwrap();
        assert_eq!(c.title.as_deref(), Some("The Beginning & the End"));
        assert_eq!(c.series.as_deref(), Some("Example Saga"));
        assert_eq!(c.chapter_number(), Some(12.5));
        assert_eq!(c.volume, Some(3));
        assert_eq!((c.year, c.month, c.day), (Some(2021), Some(7), Some(31)));
        assert_eq!(c.writers, ["Alice A", "Bob B"]);
        assert_eq!(c.artists(), ["Carol C", "Dan D", "Eve E"]);
        assert_eq!(c.genres, ["Fantasy", "Adventure"]);
        assert_eq!(c.tags, ["slow burn", "found family"]);
        assert_eq!(c.language.as_deref(), Some("en"));
        assert_eq!(c.manga, Manga::YesAndRightToLeft);
        assert_eq!(c.reading_direction(), Some(ReadingDirection::Rtl));
        assert!(!c.is_age_restricted());
        assert_eq!(c.page_count, Some(3));
        let summary = c.summary.clone().unwrap();
        assert!(
            summary.contains("A <tag> and a snowman \u{2603}."),
            "{summary}"
        );
        assert_eq!(c.pages.len(), 3);
        assert_eq!(c.pages[0].kind.as_deref(), Some("FrontCover"));
        assert_eq!(
            (c.pages[0].width, c.pages[0].height),
            (Some(1600), Some(2400))
        );
        assert!(c.pages[1].double_page);
        assert_eq!(c.double_page_indexes(), [1]);
    }

    #[test]
    fn manga_values_and_ratings() {
        let doc = |m: &str, r: &str| {
            format!("<ComicInfo><Manga>{m}</Manga><AgeRating>{r}</AgeRating></ComicInfo>")
        };
        let c = parse(doc("No", "Mature 17+").as_bytes()).unwrap();
        assert_eq!(c.reading_direction(), Some(ReadingDirection::Ltr));
        assert!(!c.is_age_restricted());
        let c = parse(doc("Yes", "Adults Only 18+").as_bytes()).unwrap();
        assert_eq!(c.reading_direction(), Some(ReadingDirection::Rtl));
        assert!(c.is_age_restricted());
        let c = parse(doc("Unknown", "R18+").as_bytes()).unwrap();
        assert_eq!(c.reading_direction(), None);
        assert!(c.is_age_restricted());
    }

    #[test]
    fn lenient_about_content_strict_about_structure() {
        let c = parse(b"<ComicInfo><Number>Annual 1</Number><Year>abc</Year><Month>13</Month><Volume>-1</Volume></ComicInfo>").unwrap();
        assert_eq!(c.number.as_deref(), Some("Annual 1"));
        assert_eq!(c.chapter_number(), None);
        assert_eq!((c.year, c.month, c.volume), (None, None, None));
        // BOM and an empty root are fine.
        assert!(parse(b"\xEF\xBB\xBF<ComicInfo/>").is_ok());
        for bad in [
            &b"not xml"[..],
            b"<Other/>",
            b"<ComicInfo><Title>unterminated</ComicInfo>",
            b"",
            b"\xFF\xFE<\0C\0",
        ] {
            assert!(parse(bad).is_err(), "{:?}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn dtds_and_entity_bombs_are_refused() {
        let lol = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">]>
<ComicInfo><Title>&lol2;</Title></ComicInfo>"#;
        assert!(parse(lol.as_bytes()).is_err());
        // An undeclared entity reference is dropped, not expanded.
        let c = parse(b"<ComicInfo><Title>a&nbsp;b</Title></ComicInfo>").unwrap();
        assert_eq!(c.title.as_deref(), Some("ab"));
        let ext = r#"<!DOCTYPE x [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><ComicInfo><Title>&xxe;</Title></ComicInfo>"#;
        assert!(parse(ext.as_bytes()).is_err());
    }

    #[test]
    fn resource_limits() {
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
        let deep = format!(
            "<ComicInfo>{}{}</ComicInfo>",
            "<a>".repeat(20),
            "</a>".repeat(20)
        );
        assert!(parse(deep.as_bytes()).is_err());
        let many = format!(
            "<ComicInfo><Pages>{}</Pages></ComicInfo>",
            "<Page Image=\"1\"/>".repeat(6000)
        );
        assert_eq!(parse(many.as_bytes()).unwrap().pages.len(), MAX_PAGES);
        let long = format!(
            "<ComicInfo><Summary>{}</Summary></ComicInfo>",
            "x".repeat(500_000)
        );
        let c = parse(long.as_bytes()).unwrap();
        assert!(c.summary.unwrap().chars().count() <= MAX_FIELD_CHARS + 500_000);
        let events = format!("<ComicInfo>{}</ComicInfo>", "<a/>".repeat(60_000));
        assert!(parse(events.as_bytes()).is_err());
    }
}
