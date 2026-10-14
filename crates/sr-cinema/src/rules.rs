//! Validation and normalisation of Cinema input. Pure functions.

use std::collections::BTreeMap;

use crate::error::{CinemaError, CinemaResult};
use crate::model::*;

pub const MAX_TITLE: usize = 300;
pub const MAX_SYNOPSIS: usize = 10_000;
pub const MAX_NAME: usize = 200;
pub const MAX_LIST: usize = 100;
pub const MAX_KEY: usize = 400;
pub const MAX_LIMIT: u32 = 100;
pub const MAX_OFFSET: u32 = 100_000;
/// Longest row a request may ask for.
pub const MAX_ROW: u32 = 50;
/// Largest episode list page.
pub const MAX_EPISODE_LIMIT: u32 = 500;
/// Titles in one collection.
pub const MAX_COLLECTION_ITEMS: usize = 200;
/// Skip windows end within 24 hours.
pub const MAX_MARKER_MS: i32 = 86_400_000;
/// Namespace of every Cinema object key.
pub const KEY_PREFIX: &str = "cinema/";

fn invalid<T>(m: impl Into<String>) -> CinemaResult<T> {
    Err(CinemaError::Invalid(m.into()))
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn no_control(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// Trim, collapse whitespace, drop empties and case-insensitive duplicates.
pub fn clean_list(what: &str, items: &[String], max_len: usize) -> CinemaResult<Vec<String>> {
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
        if !out.iter().any(|o| o.to_lowercase() == v.to_lowercase()) {
            out.push(v);
        }
    }
    if out.len() > MAX_LIST {
        return invalid(format!("{what}: at most {MAX_LIST} entries"));
    }
    Ok(out)
}

/// Genres and tags are matched exactly by filters, so store them lower-cased.
pub fn clean_labels(what: &str, items: &[String]) -> CinemaResult<Vec<String>> {
    let lower: Vec<String> = items.iter().map(|s| s.to_lowercase()).collect();
    clean_list(what, &lower, 60)
}

/// An optional single-line text, trimmed; empty becomes `None`.
pub fn opt_text(what: &str, v: &Option<String>, max: usize) -> CinemaResult<Option<String>> {
    let Some(raw) = v else { return Ok(None) };
    let s = squash(raw);
    if s.is_empty() {
        return Ok(None);
    }
    if s.chars().count() > max || !no_control(&s) {
        return invalid(format!("{what} must be at most {max} characters"));
    }
    Ok(Some(s))
}

/// A required single-line text of 1 to `max` characters.
pub fn req_text(what: &str, raw: &str, max: usize) -> CinemaResult<String> {
    let s = squash(raw);
    if s.is_empty() || s.chars().count() > max || !no_control(&s) {
        return invalid(format!("{what} must be 1 to {max} characters"));
    }
    Ok(s)
}

fn long_text(what: &str, raw: &str, max: usize) -> CinemaResult<String> {
    let s = raw.trim();
    if s.chars().count() > max || s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return invalid(format!("{what} must be at most {max} characters"));
    }
    Ok(s.to_owned())
}

/// A storage key under `cinema/` without traversal or odd characters.
pub fn check_key(what: &str, key: &str) -> CinemaResult<()> {
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
        invalid(format!("{what}: not a valid cinema storage key"))
    }
}

fn opt_key(what: &str, k: &Option<String>) -> CinemaResult<Option<String>> {
    match k.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(k) => {
            check_key(what, k)?;
            Ok(Some(k.to_owned()))
        }
    }
}

/// A BCP 47 shaped language tag, normalised (`pt-br` becomes `pt-BR`).
pub fn language(raw: &str) -> CinemaResult<String> {
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

fn languages(what: &str, items: &[String]) -> CinemaResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for raw in items.iter().filter(|s| !s.trim().is_empty()) {
        let l = language(raw)?;
        if !out.contains(&l) {
            out.push(l);
        }
    }
    if out.len() > 40 {
        return invalid(format!("{what}: at most 40 entries"));
    }
    Ok(out)
}

/// The title sort key: lower-case without a leading article.
pub fn sort_title(title: &str) -> String {
    let low = squash(title).to_lowercase();
    for article in ["the ", "an ", "a "] {
        if let Some(rest) = low.strip_prefix(article)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    low
}

fn external_ids(m: &BTreeMap<String, String>) -> CinemaResult<BTreeMap<String, String>> {
    if m.len() > 20 {
        return invalid("external_ids: at most 20 entries");
    }
    let mut out = BTreeMap::new();
    for (k, v) in m {
        let key = k.trim().to_lowercase();
        let val = v.trim();
        let key_ok = (1..=32).contains(&key.len())
            && key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !key_ok || val.is_empty() || val.chars().count() > 100 || !no_control(val) {
            return invalid("external_ids: keys are lower-case words, values 1 to 100 characters");
        }
        out.insert(key, val.to_owned());
    }
    Ok(out)
}

/// Validate a title write and return the normalised input.
pub fn title_input(i: &TitleInput) -> CinemaResult<TitleInput> {
    if let Some(y) = i.year
        && !(1880..=2200).contains(&y)
    {
        return invalid("year must be between 1880 and 2200");
    }
    if let Some(r) = i.runtime_min
        && !(1..=6000).contains(&r)
    {
        return invalid("runtime_min must be between 1 and 6000");
    }
    let content_rating =
        opt_text("content_rating", &i.content_rating, 16)?.map(|s| s.to_uppercase());
    Ok(TitleInput {
        kind: i.kind,
        title: req_text("title", &i.title, MAX_TITLE)?,
        original_title: opt_text("original_title", &i.original_title, MAX_TITLE)?,
        alt_titles: clean_list("alt_titles", &i.alt_titles, MAX_TITLE)?,
        year: i.year,
        runtime_min: i.runtime_min,
        content_rating,
        synopsis: long_text("synopsis", &i.synopsis, MAX_SYNOPSIS)?,
        tagline: opt_text("tagline", &i.tagline, 300)?,
        genres: clean_labels("genres", &i.genres)?,
        tags: clean_labels("tags", &i.tags)?,
        languages: languages("languages", &i.languages)?,
        subtitle_languages: languages("subtitle_languages", &i.subtitle_languages)?,
        cast: clean_list("cast", &i.cast, MAX_NAME)?,
        directors: clean_list("directors", &i.directors, MAX_NAME)?,
        writers: clean_list("writers", &i.writers, MAX_NAME)?,
        studios: clean_list("studios", &i.studios, MAX_NAME)?,
        crew: clean_list("crew", &i.crew, MAX_NAME)?,
        age_restricted: i.age_restricted,
        poster_key: opt_key("poster_key", &i.poster_key)?,
        backdrop_key: opt_key("backdrop_key", &i.backdrop_key)?,
        logo_key: opt_key("logo_key", &i.logo_key)?,
        external_ids: external_ids(&i.external_ids)?,
    })
}

pub fn season_input(i: &SeasonInput) -> CinemaResult<SeasonInput> {
    if !(0..=999).contains(&i.number) {
        return invalid("season number must be between 0 and 999");
    }
    Ok(SeasonInput {
        number: i.number,
        name: opt_text("name", &i.name, MAX_NAME)?,
        synopsis: match &i.synopsis {
            Some(s) if !s.trim().is_empty() => Some(long_text("synopsis", s, MAX_SYNOPSIS)?),
            _ => None,
        },
        poster_key: opt_key("poster_key", &i.poster_key)?,
    })
}

/// Parse and validate `YYYY-MM-DD`.
pub fn air_date(raw: &str) -> CinemaResult<String> {
    let bad = || CinemaError::Invalid("air_date must be YYYY-MM-DD".into());
    let mut it = raw.trim().split('-');
    let (Some(y), Some(m), Some(d), None) = (it.next(), it.next(), it.next(), it.next()) else {
        return Err(bad());
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return Err(bad());
    }
    let (y, m, d): (i32, u32, u32) = (
        y.parse().map_err(|_| bad())?,
        m.parse().map_err(|_| bad())?,
        d.parse().map_err(|_| bad())?,
    );
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(bad()),
    };
    if !(1800..=2200).contains(&y) || d == 0 || d > days {
        return Err(bad());
    }
    Ok(format!("{y:04}-{m:02}-{d:02}"))
}

pub fn episode_input(i: &EpisodeInput) -> CinemaResult<EpisodeInput> {
    if !(0..=9999).contains(&i.number) {
        return invalid("episode number must be between 0 and 9999");
    }
    if let Some(a) = i.absolute_number
        && !(0..=99_999).contains(&a)
    {
        return invalid("absolute_number must be between 0 and 99999");
    }
    if let Some(r) = i.runtime_min
        && !(1..=6000).contains(&r)
    {
        return invalid("runtime_min must be between 1 and 6000");
    }
    let title = squash(&i.title);
    if title.chars().count() > MAX_TITLE || !no_control(&title) {
        return invalid(format!("title must be at most {MAX_TITLE} characters"));
    }
    Ok(EpisodeInput {
        number: i.number,
        absolute_number: i.absolute_number,
        title,
        synopsis: long_text("synopsis", &i.synopsis, MAX_SYNOPSIS)?,
        air_date: match i.air_date.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(d) => Some(air_date(d)?),
        },
        runtime_min: i.runtime_min,
        still_key: opt_key("still_key", &i.still_key)?,
    })
}

pub fn variant_input(i: &VariantInput) -> CinemaResult<VariantInput> {
    if let Some(r) = i.runtime_min
        && !(1..=6000).contains(&r)
    {
        return invalid("runtime_min must be between 1 and 6000");
    }
    if matches!(i.asset_id, Some(a) if a <= 0) {
        return invalid("asset_id must be a positive id");
    }
    Ok(VariantInput {
        label: req_text("label", &i.label, 120)?,
        notes: opt_text("notes", &i.notes, 1000)?,
        runtime_min: i.runtime_min,
        asset_id: i.asset_id,
        position: i.position,
    })
}

pub fn trailer_input(i: &TrailerInput) -> CinemaResult<TrailerInput> {
    if matches!(i.asset_id, Some(a) if a <= 0) {
        return invalid("asset_id must be a positive id");
    }
    Ok(TrailerInput {
        label: req_text("label", &i.label, 120)?,
        kind: i.kind,
        language: match i.language.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(l) => Some(language(l)?),
        },
        asset_id: i.asset_id,
    })
}

/// Validate a full set of skip windows: ordered inside 24 hours, one per kind.
pub fn markers(input: &[MarkerInput]) -> CinemaResult<Vec<MarkerInput>> {
    let mut out: Vec<MarkerInput> = Vec::new();
    for m in input {
        if m.start_ms < 0 || m.end_ms <= m.start_ms || m.end_ms > MAX_MARKER_MS {
            return invalid("a marker must satisfy 0 <= start_ms < end_ms <= 86400000");
        }
        if out.iter().any(|o| o.kind == m.kind) {
            return invalid("at most one marker of each kind");
        }
        out.push(*m);
    }
    out.sort_by_key(|m| (m.start_ms, m.end_ms));
    Ok(out)
}

pub fn collection_input(i: &CollectionInput) -> CinemaResult<CollectionInput> {
    let slug = i.slug.trim().to_lowercase();
    let slug_ok = (1..=63).contains(&slug.len())
        && slug.as_bytes()[0] != b'-'
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !slug_ok {
        return invalid("slug must be 1 to 63 lower-case letters, digits or dashes");
    }
    let mut surfaces: Vec<Surface> = Vec::new();
    for s in &i.surfaces {
        if !surfaces.contains(s) {
            surfaces.push(*s);
        }
    }
    if !(-100_000..=100_000).contains(&i.position) {
        return invalid("position is out of range");
    }
    Ok(CollectionInput {
        slug,
        name: req_text("name", &i.name, 120)?,
        eyebrow: opt_text("eyebrow", &i.eyebrow, 120)?,
        blurb: opt_text("blurb", &i.blurb, 1000)?,
        display: i.display,
        surfaces,
        position: i.position,
        published: i.published,
    })
}

/// The ids of a collection, without duplicates, within the cap.
pub fn collection_items(ids: &[i64]) -> CinemaResult<Vec<i64>> {
    if ids.len() > MAX_COLLECTION_ITEMS {
        return invalid(format!(
            "a collection holds at most {MAX_COLLECTION_ITEMS} titles"
        ));
    }
    let mut out: Vec<i64> = Vec::with_capacity(ids.len());
    for id in ids {
        if *id <= 0 {
            return invalid("title_ids must be positive ids");
        }
        if !out.contains(id) {
            out.push(*id);
        }
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

/// Normalise the filters of a browse query (lower-case labels, bounded counts).
pub fn browse(mut q: BrowseQuery) -> CinemaResult<BrowseQuery> {
    q.genres = clean_labels("genres", &q.genres)?;
    q.exclude_genres = clean_labels("exclude_genres", &q.exclude_genres)?;
    q.tags = clean_labels("tags", &q.tags)?;
    q.ratings = q
        .ratings
        .iter()
        .map(|r| squash(r).to_uppercase())
        .filter(|r| !r.is_empty())
        .collect();
    if q.ratings.len() > 20 || q.ratings.iter().any(|r| r.chars().count() > 16) {
        return invalid("rating: at most 20 values of up to 16 characters");
    }
    if let Some(l) = &q.language {
        q.language = Some(language(l)?);
    }
    if let Some(l) = &q.subtitle_language {
        q.subtitle_language = Some(language(l)?);
    }
    if let (Some(a), Some(b)) = (q.year_from, q.year_to)
        && a > b
    {
        return invalid("year_from must not exceed year_to");
    }
    if let (Some(a), Some(b)) = (q.runtime_min, q.runtime_max)
        && a > b
    {
        return invalid("runtime_min must not exceed runtime_max");
    }
    if q.sort == SortKey::Curated && q.collection.is_none() {
        return invalid("sort=curated needs a collection");
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

// ---- cursors -----------------------------------------------------------------

/// Encode the keyset cursor of the last row of a browse page.
pub fn encode_cursor(sort: SortKey, descending: bool, value: &SortValue, id: i64) -> String {
    let v = match value {
        SortValue::Int(n) => format!("i{n}"),
        SortValue::Text(t) => {
            let hex: String = t.bytes().map(|b| format!("{b:02x}")).collect();
            format!("t{hex}")
        }
    };
    let dir = if descending { 'd' } else { 'a' };
    format!("{}.{dir}.{v}.{id}", sort.as_str())
}

/// Decode a browse cursor; it must have been made for the same sort and direction.
pub fn decode_cursor(s: &str, sort: SortKey, descending: bool) -> CinemaResult<Cursor> {
    let bad = || CinemaError::Invalid("cursor is not valid for this request".into());
    let parts: Vec<&str> = s.split('.').collect();
    let [sort_s, dir, val, id] = parts.as_slice() else {
        return Err(bad());
    };
    if *sort_s != sort.as_str() || *dir != if descending { "d" } else { "a" } {
        return Err(bad());
    }
    let id: i64 = id.parse().ok().filter(|i| *i > 0).ok_or_else(bad)?;
    let value = match val.split_at_checked(1) {
        Some(("i", n)) => SortValue::Int(n.parse().map_err(|_| bad())?),
        Some(("t", hex)) => {
            if hex.len() % 2 != 0 || !hex.is_ascii() {
                return Err(bad());
            }
            let bytes: Result<Vec<u8>, _> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                .collect();
            SortValue::Text(String::from_utf8(bytes.map_err(|_| bad())?).map_err(|_| bad())?)
        }
        _ => return Err(bad()),
    };
    let int_sort = !matches!(sort, SortKey::Title);
    if int_sort != matches!(value, SortValue::Int(_)) {
        return Err(bad());
    }
    Ok(Cursor { value, id })
}

/// Cursor of an episode list: `(season number, episode number, id)`.
pub fn encode_episode_cursor(season: i32, number: i32, id: i64) -> String {
    format!("{season}_{number}_{id}")
}

pub fn decode_episode_cursor(s: &str) -> CinemaResult<(i32, i32, i64)> {
    let mut it = s.split('_');
    let (Some(a), Some(b), Some(c), None) = (it.next(), it.next(), it.next(), it.next()) else {
        return invalid("after is not a valid cursor");
    };
    match (a.parse(), b.parse(), c.parse()) {
        (Ok(a), Ok(b), Ok(c)) if c > 0 => Ok((a, b, c)),
        _ => invalid("after is not a valid cursor"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> TitleInput {
        TitleInput {
            kind: TitleKind::Movie,
            title: "  The   Long  Goodbye ".into(),
            original_title: Some("  ".into()),
            alt_titles: vec!["Goodbye".into(), "goodbye".into(), " ".into()],
            year: Some(1973),
            runtime_min: Some(112),
            content_rating: Some(" r ".into()),
            synopsis: "  A detective.  ".into(),
            tagline: None,
            genres: vec!["Crime".into(), "NEO-NOIR".into(), "crime".into()],
            tags: vec![],
            languages: vec!["EN".into(), "pt-br".into(), "en".into()],
            subtitle_languages: vec![],
            cast: vec!["Elliott Gould".into()],
            directors: vec![],
            writers: vec![],
            studios: vec![],
            crew: vec![],
            age_restricted: false,
            poster_key: Some("cinema/1/poster.webp".into()),
            backdrop_key: None,
            logo_key: Some(" ".into()),
            external_ids: BTreeMap::from([(" IMDB ".into(), " tt0070334 ".into())]),
        }
    }

    #[test]
    fn title_input_is_normalised() {
        let out = title_input(&input()).unwrap();
        assert_eq!(out.title, "The Long Goodbye");
        assert_eq!(out.original_title, None);
        assert_eq!(out.alt_titles, vec!["Goodbye"]);
        assert_eq!(out.content_rating.as_deref(), Some("R"));
        assert_eq!(out.synopsis, "A detective.");
        assert_eq!(out.genres, vec!["crime", "neo-noir"]);
        assert_eq!(out.languages, vec!["en", "pt-BR"]);
        assert_eq!(out.logo_key, None);
        assert_eq!(
            out.external_ids.get("imdb").map(String::as_str),
            Some("tt0070334")
        );
    }

    #[test]
    fn title_input_refuses_bad_values() {
        let mut i = input();
        i.title = "  ".into();
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.year = Some(1700);
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.runtime_min = Some(0);
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.poster_key = Some("library/1/poster.webp".into());
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.poster_key = Some("cinema/../x".into());
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.languages = vec!["english-language-tag".into()];
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.external_ids = BTreeMap::from([("Bad Key".into(), "x".into())]);
        assert!(title_input(&i).is_err());
        let mut i = input();
        i.cast = (0..=MAX_LIST).map(|n| format!("Actor {n}")).collect();
        assert!(title_input(&i).is_err());
    }

    #[test]
    fn sort_title_drops_articles() {
        assert_eq!(sort_title("The Matrix"), "matrix");
        assert_eq!(sort_title("A  Quiet Place"), "quiet place");
        assert_eq!(sort_title("An Education"), "education");
        assert_eq!(sort_title("The"), "the");
        assert_eq!(sort_title("Anatomy of a Fall"), "anatomy of a fall");
    }

    #[test]
    fn air_dates() {
        assert_eq!(air_date("2024-02-29").unwrap(), "2024-02-29");
        assert!(air_date("2023-02-29").is_err());
        assert!(air_date("2024-13-01").is_err());
        assert!(air_date("2024-1-01").is_err());
        assert!(air_date("yesterday").is_err());
        assert!(air_date("2024-04-31").is_err());
    }

    #[test]
    fn episode_and_season_input() {
        let e = EpisodeInput {
            number: 3,
            absolute_number: Some(30),
            title: " Pilot ".into(),
            synopsis: String::new(),
            air_date: Some(" ".into()),
            runtime_min: Some(24),
            still_key: None,
        };
        let out = episode_input(&e).unwrap();
        assert_eq!(out.title, "Pilot");
        assert_eq!(out.air_date, None);
        assert!(
            episode_input(&EpisodeInput {
                number: -1,
                ..e.clone()
            })
            .is_err()
        );
        assert!(
            season_input(&SeasonInput {
                number: 1000,
                name: None,
                synopsis: None,
                poster_key: None
            })
            .is_err()
        );
    }

    #[test]
    fn markers_are_validated() {
        let m = |kind, start_ms, end_ms| MarkerInput {
            kind,
            start_ms,
            end_ms,
        };
        let ok = markers(&[
            m(MarkerKind::Credits, 5000, 6000),
            m(MarkerKind::Intro, 10, 90),
        ])
        .unwrap();
        assert_eq!(ok[0].kind, MarkerKind::Intro, "sorted by start");
        assert!(markers(&[m(MarkerKind::Intro, 10, 10)]).is_err());
        assert!(markers(&[m(MarkerKind::Intro, -1, 10)]).is_err());
        assert!(markers(&[m(MarkerKind::Intro, 0, MAX_MARKER_MS + 1)]).is_err());
        assert!(markers(&[m(MarkerKind::Intro, 0, 5), m(MarkerKind::Intro, 6, 9)]).is_err());
    }

    #[test]
    fn collection_input_rules() {
        let c = CollectionInput {
            slug: " A-Good-Place ".into(),
            name: "A good place to start".into(),
            eyebrow: None,
            blurb: None,
            display: CollectionDisplay::Row,
            surfaces: vec![Surface::Discover, Surface::Discover],
            position: 1,
            published: true,
        };
        let out = collection_input(&c).unwrap();
        assert_eq!(out.slug, "a-good-place");
        assert_eq!(out.surfaces, vec![Surface::Discover]);
        for bad in ["", "-x", "has space", "Ünï"] {
            assert!(
                collection_input(&CollectionInput {
                    slug: bad.into(),
                    ..c.clone()
                })
                .is_err(),
                "{bad}"
            );
        }
        assert_eq!(collection_items(&[3, 1, 3, 2]).unwrap(), vec![3, 1, 2]);
        assert!(collection_items(&[0]).is_err());
        assert!(collection_items(&vec![1; MAX_COLLECTION_ITEMS + 1]).is_err());
    }

    #[test]
    fn browse_normalisation() {
        let q = browse(BrowseQuery {
            genres: vec!["Drama".into()],
            ratings: vec![" pg-13 ".into(), "".into()],
            language: Some("EN".into()),
            limit: 9999,
            offset: u32::MAX,
            ..BrowseQuery::default()
        })
        .unwrap();
        assert_eq!(q.genres, vec!["drama"]);
        assert_eq!(q.ratings, vec!["PG-13"]);
        assert_eq!(q.language.as_deref(), Some("en"));
        assert_eq!((q.limit, q.offset), (MAX_LIMIT, MAX_OFFSET));
        assert!(
            browse(BrowseQuery {
                year_from: Some(2000),
                year_to: Some(1990),
                ..BrowseQuery::default()
            })
            .is_err()
        );
        assert!(
            browse(BrowseQuery {
                sort: SortKey::Curated,
                ..BrowseQuery::default()
            })
            .is_err()
        );
        assert!(
            browse(BrowseQuery {
                runtime_min: Some(100),
                runtime_max: Some(50),
                ..BrowseQuery::default()
            })
            .is_err()
        );
    }

    #[test]
    fn cursors_round_trip_and_bind_to_the_request() {
        let v = SortValue::Int(1_700_000_000_000);
        let c = encode_cursor(SortKey::Updated, true, &v, 42);
        let back = decode_cursor(&c, SortKey::Updated, true).unwrap();
        assert_eq!(back, Cursor { value: v, id: 42 });
        assert!(
            decode_cursor(&c, SortKey::Newest, true).is_err(),
            "other sort"
        );
        assert!(
            decode_cursor(&c, SortKey::Updated, false).is_err(),
            "other direction"
        );
        let t = SortValue::Text("amélie, the".into());
        let c = encode_cursor(SortKey::Title, false, &t, 7);
        assert_eq!(decode_cursor(&c, SortKey::Title, false).unwrap().value, t);
        for junk in [
            "",
            "x",
            "updated.d.i1",
            "updated.d.x1.2",
            "updated.d.i1.0",
            "updated.d.iabc.3",
        ] {
            assert!(
                decode_cursor(junk, SortKey::Updated, true).is_err(),
                "{junk}"
            );
        }
        assert!(decode_cursor("title.a.tzz.3", SortKey::Title, false).is_err());
        assert!(decode_cursor("title.a.i5.3", SortKey::Title, false).is_err());
    }

    #[test]
    fn episode_cursor() {
        let c = encode_episode_cursor(1, 12, 99);
        assert_eq!(decode_episode_cursor(&c).unwrap(), (1, 12, 99));
        assert!(decode_episode_cursor("1_2").is_err());
        assert!(decode_episode_cursor("a_b_c").is_err());
        assert!(decode_episode_cursor("1_2_0").is_err());
    }
}
