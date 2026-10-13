//! Validation and normalisation of Library input. Pure functions.

use crate::error::{LibraryError, LibraryResult};
use crate::model::*;

pub const MAX_TITLE: usize = 300;
pub const MAX_SYNOPSIS: usize = 10_000;
pub const MAX_NAME: usize = 200;
pub const MAX_LIST: usize = 60;
pub const MAX_PAGES: usize = 3000;
pub const MAX_KEY: usize = 400;
pub const MAX_LIMIT: u32 = 100;
pub const MAX_OFFSET: u32 = 100_000;
/// Namespace of every Library object key.
pub const KEY_PREFIX: &str = "library/";

fn invalid<T>(m: impl Into<String>) -> LibraryResult<T> {
    Err(LibraryError::Invalid(m.into()))
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn no_control(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// Trim, collapse whitespace, drop empties and case-insensitive duplicates.
pub fn clean_list(what: &str, items: &[String], max_len: usize) -> LibraryResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for raw in items {
        let v = squash(raw);
        if v.is_empty() {
            continue;
        }
        if v.chars().count() > max_len || !no_control(&v) {
            return invalid(format!(
                "{what}: entries must be at most {max_len} characters"
            ));
        }
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&v) || *o == v) {
            out.push(v);
        }
    }
    if out.len() > MAX_LIST {
        return invalid(format!("{what}: at most {MAX_LIST} entries"));
    }
    Ok(out)
}

/// Genres and tags are matched exactly by filters, so store them lower-cased.
pub fn clean_labels(what: &str, items: &[String]) -> LibraryResult<Vec<String>> {
    let lower: Vec<String> = items.iter().map(|s| s.to_lowercase()).collect();
    clean_list(what, &lower, 60)
}

/// A storage key under `library/` without traversal or odd characters.
pub fn check_key(what: &str, key: &str) -> LibraryResult<()> {
    let ok = key.starts_with(KEY_PREFIX)
        && key.len() <= MAX_KEY
        && !key.ends_with('/')
        && key.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        });
    if ok {
        Ok(())
    } else {
        invalid(format!("{what}: not a valid library storage key"))
    }
}

/// Default reading direction for a kind (spec section 2, D-084).
pub fn default_direction(kind: SeriesKind) -> ReadingDirection {
    match kind {
        SeriesKind::Manga => ReadingDirection::Rtl,
        SeriesKind::Webtoon | SeriesKind::Manhwa | SeriesKind::Manhua => ReadingDirection::Vertical,
        SeriesKind::Comic | SeriesKind::Novel | SeriesKind::Book => ReadingDirection::Ltr,
    }
}

/// Validate a series write and return the normalised input and its direction.
pub fn series_input(i: &SeriesInput) -> LibraryResult<(SeriesInput, ReadingDirection)> {
    let title = squash(&i.title);
    if title.is_empty() || title.chars().count() > MAX_TITLE || !no_control(&title) {
        return invalid(format!("title must be 1 to {MAX_TITLE} characters"));
    }
    if let Some(y) = i.year
        && !(1800..=2200).contains(&y)
    {
        return invalid("year must be between 1800 and 2200");
    }
    if i.synopsis.chars().count() > MAX_SYNOPSIS {
        return invalid(format!(
            "synopsis must be at most {MAX_SYNOPSIS} characters"
        ));
    }
    if let Some(k) = &i.cover_key {
        check_key("cover_key", k)?;
    }
    let direction = i
        .reading_direction
        .unwrap_or_else(|| default_direction(i.kind));
    let out = SeriesInput {
        kind: i.kind,
        title,
        alt_titles: clean_list("alt_titles", &i.alt_titles, MAX_TITLE)?,
        authors: clean_list("authors", &i.authors, MAX_NAME)?,
        artists: clean_list("artists", &i.artists, MAX_NAME)?,
        status: i.status,
        year: i.year,
        synopsis: i.synopsis.trim().to_owned(),
        genres: clean_labels("genres", &i.genres)?,
        tags: clean_labels("tags", &i.tags)?,
        reading_direction: Some(direction),
        age_restricted: i.age_restricted,
        cover_key: i.cover_key.clone(),
    };
    Ok((out, direction))
}

/// A BCP 47 shaped language tag, normalised (`pt-br` becomes `pt-BR`).
pub fn language(raw: &str) -> LibraryResult<String> {
    let parts: Vec<&str> = raw.trim().split('-').collect();
    let ok = !parts.is_empty()
        && (2..=3).contains(&parts[0].len())
        && parts[0].bytes().all(|b| b.is_ascii_alphabetic())
        && parts[1..]
            .iter()
            .all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()));
    if !ok {
        return invalid("language must be a BCP 47 tag such as en or pt-BR");
    }
    let mut out = parts[0].to_ascii_lowercase();
    for p in &parts[1..] {
        out.push('-');
        if p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()) {
            out.push_str(&p.to_ascii_uppercase());
        } else {
            out.push_str(p);
        }
    }
    Ok(out)
}

pub fn edition_input(i: &EditionInput) -> LibraryResult<EditionInput> {
    let label = squash(&i.label);
    if label.is_empty() || label.chars().count() > 120 || !no_control(&label) {
        return invalid("label must be 1 to 120 characters");
    }
    Ok(EditionInput {
        language: language(&i.language)?,
        label,
        is_default: i.is_default,
    })
}

/// Chapter and volume numbers: finite, non-negative, at most two decimals.
pub fn number(n: f64) -> LibraryResult<f64> {
    if !n.is_finite() || !(0.0..=99_999.0).contains(&n) {
        return invalid("number must be between 0 and 99999");
    }
    Ok((n * 100.0).round() / 100.0)
}

pub fn volume_input(i: &VolumeInput) -> LibraryResult<VolumeInput> {
    if let Some(k) = &i.cover_key {
        check_key("cover_key", k)?;
    }
    Ok(VolumeInput {
        number: number(i.number)?,
        title: opt_text("title", i.title.as_deref(), MAX_TITLE)?,
        cover_key: i.cover_key.clone(),
    })
}

fn opt_text(what: &str, v: Option<&str>, max: usize) -> LibraryResult<Option<String>> {
    match v.map(squash).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) if s.chars().count() <= max && no_control(&s) => Ok(Some(s)),
        Some(_) => invalid(format!("{what} must be at most {max} characters")),
    }
}

pub fn chapter_input(i: &ChapterInput) -> LibraryResult<ChapterInput> {
    let source_key = match (i.format, &i.source_key) {
        (ChapterFormat::Images, None) => None,
        (ChapterFormat::Images, Some(_)) => {
            return invalid("source_key is only for pdf and epub chapters");
        }
        (_, Some(k)) => {
            check_key("source_key", k)?;
            Some(k.clone())
        }
        (_, None) => return invalid("pdf and epub chapters need a source_key"),
    };
    Ok(ChapterInput {
        number: number(i.number)?,
        title: opt_text("title", i.title.as_deref(), MAX_TITLE)?,
        volume_id: i.volume_id,
        format: i.format,
        source_key,
        release_group: opt_text("release_group", i.release_group.as_deref(), 120)?,
    })
}

const BLURHASH: &str =
    "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

/// Validate a replacement page list for chapter `chapter` of series `series`.
pub fn pages(series: i64, chapter: i64, input: &[PageInput]) -> LibraryResult<Vec<PageRecord>> {
    if input.len() > MAX_PAGES {
        return invalid(format!("at most {MAX_PAGES} pages per chapter"));
    }
    let prefix = format!("{KEY_PREFIX}{series}/{chapter}/");
    let mut out = Vec::with_capacity(input.len());
    for (i, p) in input.iter().enumerate() {
        check_key("page key", &p.key)?;
        if !p.key.starts_with(&prefix) {
            return invalid(format!("page {i}: key must live under {prefix}"));
        }
        if p.width == 0 || p.height == 0 || p.width > 65_535 || p.height > 200_000 {
            return invalid(format!("page {i}: bad dimensions"));
        }
        if let Some(b) = &p.blurhash
            && (b.len() < 6 || b.len() > 120 || !b.chars().all(|c| BLURHASH.contains(c)))
        {
            return invalid(format!("page {i}: bad blurhash"));
        }
        let mut variants = Vec::new();
        for (w, k) in &p.variants {
            let width: u32 = w
                .parse()
                .ok()
                .filter(|w| (16..=4096).contains(w))
                .ok_or_else(|| LibraryError::Invalid(format!("page {i}: bad variant width")))?;
            check_key("variant key", k)?;
            if !k.starts_with(&prefix) {
                return invalid(format!("page {i}: variant key must live under {prefix}"));
            }
            variants.push((width, k.clone()));
        }
        variants.sort_unstable();
        out.push(PageRecord {
            index: i as i32,
            key: p.key.clone(),
            variants,
            width: p.width as i32,
            height: p.height as i32,
            is_double_spread: p.is_double_spread,
            blurhash: p.blurhash.clone(),
        });
    }
    Ok(out)
}

/// Clamp paging parameters.
pub fn paging(limit: Option<u32>, offset: Option<u32>) -> (u32, u32) {
    (
        limit.unwrap_or(24).clamp(1, MAX_LIMIT),
        offset.unwrap_or(0).min(MAX_OFFSET),
    )
}

/// Normalise the list filters of a browse query (lower-case labels, bounded counts).
pub fn browse(mut q: BrowseQuery) -> LibraryResult<BrowseQuery> {
    q.genres = clean_labels("genres", &q.genres)?;
    q.exclude_genres = clean_labels("exclude_genres", &q.exclude_genres)?;
    q.tags = clean_labels("tags", &q.tags)?;
    if let Some(l) = &q.language {
        q.language = Some(language(l)?);
    }
    if let (Some(a), Some(b)) = (q.year_from, q.year_to)
        && a > b
    {
        return invalid("year_from must not exceed year_to");
    }
    let (limit, offset) = paging(Some(q.limit), Some(q.offset));
    q.limit = limit;
    q.offset = offset;
    q.title_contains = q
        .title_contains
        .as_deref()
        .map(squash)
        .filter(|s| !s.is_empty());
    Ok(q)
}

/// Encode a feed cursor.
pub fn encode_cursor(ms: i64, id: i64) -> String {
    format!("{ms}_{id}")
}

/// Decode a feed cursor produced by [`encode_cursor`].
pub fn decode_cursor(s: &str) -> LibraryResult<(i64, i64)> {
    s.split_once('_')
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .filter(|(a, b)| *a >= 0 && *b > 0)
        .ok_or_else(|| LibraryError::Invalid("before is not a valid cursor".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SeriesInput {
        SeriesInput {
            kind: SeriesKind::Manga,
            title: "  One   Piece ".into(),
            alt_titles: vec!["ONE PIECE".into(), " ".into(), "ワンピース".into()],
            authors: vec!["Oda".into(), "oda".into()],
            artists: vec![],
            status: SeriesStatus::Ongoing,
            year: Some(1997),
            synopsis: " pirates ".into(),
            genres: vec!["Action".into(), "ACTION".into(), "Adventure".into()],
            tags: vec![],
            reading_direction: None,
            age_restricted: false,
            cover_key: None,
        }
    }

    #[test]
    fn series_input_is_normalised() {
        let (n, d) = series_input(&input()).unwrap();
        assert_eq!(n.title, "One Piece");
        assert_eq!(n.authors, ["Oda"]);
        assert_eq!(n.genres, ["action", "adventure"]);
        assert_eq!(n.synopsis, "pirates");
        assert_eq!(d, ReadingDirection::Rtl);
        assert_eq!(n.alt_titles.len(), 3 - 1);
    }

    #[test]
    fn direction_defaults_follow_the_kind() {
        let mut i = input();
        i.kind = SeriesKind::Webtoon;
        assert_eq!(series_input(&i).unwrap().1, ReadingDirection::Vertical);
        i.kind = SeriesKind::Comic;
        assert_eq!(series_input(&i).unwrap().1, ReadingDirection::Ltr);
        i.reading_direction = Some(ReadingDirection::Rtl);
        assert_eq!(series_input(&i).unwrap().1, ReadingDirection::Rtl);
    }

    #[test]
    fn rejects_bad_series_input() {
        let mut i = input();
        i.title = "  ".into();
        assert!(series_input(&i).is_err());
        let mut i = input();
        i.year = Some(1500);
        assert!(series_input(&i).is_err());
        let mut i = input();
        i.cover_key = Some("library/../etc/passwd".into());
        assert!(series_input(&i).is_err());
        let mut i = input();
        i.title = "a\u{0}b".into();
        assert!(series_input(&i).is_err());
    }

    #[test]
    fn keys_must_be_clean_and_namespaced() {
        assert!(check_key("k", "library/1/2/0001.webp").is_ok());
        for bad in [
            "chat/1/2",
            "library/",
            "library//x",
            "library/a/../b",
            "library/a b",
            "library/a\\b",
            "library/a/",
            "/library/a",
        ] {
            assert!(check_key("k", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn languages_normalise() {
        assert_eq!(language("EN").unwrap(), "en");
        assert_eq!(language("pt-br").unwrap(), "pt-BR");
        assert_eq!(language("zh-Hant").unwrap(), "zh-Hant");
        assert!(language("e").is_err());
        assert!(language("en_US").is_err());
        assert!(language("").is_err());
    }

    #[test]
    fn numbers_round_to_two_places() {
        assert_eq!(number(10.5).unwrap(), 10.5);
        assert_eq!(number(1.004).unwrap(), 1.0);
        assert!(number(-1.0).is_err());
        assert!(number(f64::NAN).is_err());
        assert!(number(1e9).is_err());
    }

    #[test]
    fn chapters_need_a_source_for_documents() {
        let mut c = ChapterInput {
            number: 1.0,
            title: None,
            volume_id: None,
            format: ChapterFormat::Pdf,
            source_key: None,
            release_group: None,
        };
        assert!(chapter_input(&c).is_err());
        c.source_key = Some("library/1/2/book.pdf".into());
        assert!(chapter_input(&c).is_ok());
        c.format = ChapterFormat::Images;
        assert!(chapter_input(&c).is_err());
    }

    #[test]
    fn pages_must_stay_inside_their_chapter() {
        let mk = |key: &str| PageInput {
            key: key.into(),
            variants: Default::default(),
            width: 800,
            height: 1200,
            is_double_spread: false,
            blurhash: Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into()),
        };
        let ok = pages(
            1,
            2,
            &[mk("library/1/2/0001.webp"), mk("library/1/2/0002.webp")],
        )
        .unwrap();
        assert_eq!(ok[1].index, 1);
        assert!(pages(1, 2, &[mk("library/1/3/0001.webp")]).is_err());
        assert!(pages(1, 2, &[mk("library/9/2/0001.webp")]).is_err());
        let mut p = mk("library/1/2/0001.webp");
        p.variants
            .insert("720".into(), "library/1/2/0001_720.webp".into());
        p.variants
            .insert("99999".into(), "library/1/2/x.webp".into());
        assert!(pages(1, 2, &[p.clone()]).is_err());
        p.variants.remove("99999");
        assert_eq!(pages(1, 2, &[p]).unwrap()[0].variants[0].0, 720);
        let mut p = mk("library/1/2/0001.webp");
        p.blurhash = Some("bad hash!".into());
        assert!(pages(1, 2, &[p]).is_err());
    }

    #[test]
    fn paging_is_clamped() {
        assert_eq!(paging(None, None), (24, 0));
        assert_eq!(paging(Some(0), Some(7)), (1, 7));
        assert_eq!(paging(Some(9999), Some(u32::MAX)), (100, MAX_OFFSET));
    }

    #[test]
    fn cursors_round_trip() {
        assert_eq!(decode_cursor(&encode_cursor(123, 45)).unwrap(), (123, 45));
        for bad in ["", "1", "a_b", "1_0", "-1_5", "1_2_3"] {
            assert!(decode_cursor(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn browse_filters_are_normalised() {
        let q = browse(BrowseQuery {
            genres: vec!["Action ".into()],
            language: Some("EN".into()),
            limit: 1000,
            ..BrowseQuery::default()
        })
        .unwrap();
        assert_eq!(q.genres, ["action"]);
        assert_eq!(q.language.as_deref(), Some("en"));
        assert_eq!(q.limit, 100);
        assert!(
            browse(BrowseQuery {
                year_from: Some(2000),
                year_to: Some(1999),
                ..BrowseQuery::default()
            })
            .is_err()
        );
    }
}
