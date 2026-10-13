//! `series.json` parser (LIB-ING-006): the optional series-level metadata file of
//! `docs/specs/ingest-naming-conventions.md` section 2.4.
//!
//! ```json
//! { "title": "…", "alt_titles": ["…"], "kind": "manga", "language": "en",
//!   "edition": "Official English", "direction": "rtl", "authors": ["…"],
//!   "status": "ongoing", "year": 2020, "genres": ["Fantasy"], "age_restricted": false }
//! ```
//!
//! Only `title` is required. Unknown keys are ignored so newer files keep working.
//! Values are validated through the same rules as the admin API ([`crate::rules`]).

use serde::Deserialize;

use super::IngestError;
use crate::model::*;
use crate::rules;

pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
struct Raw {
    title: Option<String>,
    #[serde(default)]
    alt_titles: Vec<String>,
    kind: Option<String>,
    language: Option<String>,
    edition: Option<String>,
    #[serde(alias = "reading_direction")]
    direction: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    artists: Vec<String>,
    status: Option<String>,
    year: Option<i64>,
    #[serde(alias = "description")]
    synopsis: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    age_restricted: bool,
}

/// Validated series metadata from a `series.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesFile {
    /// Normalised series fields, ready for `LibraryService::create_series`.
    pub series: SeriesInput,
    /// `language` + `edition`, when both are present.
    pub edition: Option<EditionInput>,
}

fn bad(msg: impl std::fmt::Display) -> IngestError {
    IngestError::Metadata(format!("series.json: {msg}"))
}

fn enum_of<T>(
    field: &str,
    raw: Option<&str>,
    parse: fn(&str) -> Option<T>,
) -> Result<Option<T>, IngestError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => parse(&s.to_ascii_lowercase())
            .map(Some)
            .ok_or_else(|| bad(format!("{field}: unknown value {s:?}"))),
    }
}

/// Parse and validate a `series.json`.
pub fn parse(bytes: &[u8]) -> Result<SeriesFile, IngestError> {
    if bytes.len() > MAX_BYTES {
        return Err(bad("file is too large"));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let raw: Raw = serde_json::from_slice(bytes).map_err(bad)?;
    let title = raw.title.unwrap_or_default();
    if title.trim().is_empty() {
        return Err(bad("title is required"));
    }
    let kind =
        enum_of("kind", raw.kind.as_deref(), SeriesKind::parse)?.unwrap_or(SeriesKind::Manga);
    let year = match raw.year {
        None => None,
        Some(y) => Some(i16::try_from(y).map_err(|_| bad("year is out of range"))?),
    };
    let input = SeriesInput {
        kind,
        title,
        alt_titles: raw.alt_titles,
        authors: raw.authors,
        artists: raw.artists,
        status: enum_of("status", raw.status.as_deref(), SeriesStatus::parse)?
            .unwrap_or(SeriesStatus::Ongoing),
        year,
        synopsis: raw.synopsis.unwrap_or_default(),
        genres: raw.genres,
        tags: raw.tags,
        reading_direction: enum_of(
            "direction",
            raw.direction.as_deref(),
            ReadingDirection::parse,
        )?,
        age_restricted: raw.age_restricted,
        cover_key: None,
    };
    let (series, _) = rules::series_input(&input).map_err(bad)?;
    let edition = match (raw.language, raw.edition) {
        (Some(language), Some(label)) => Some(
            rules::edition_input(&EditionInput {
                language,
                label,
                is_default: true,
            })
            .map_err(bad)?,
        ),
        (Some(language), None) => {
            let label = format!("Edition ({})", language.trim());
            Some(
                rules::edition_input(&EditionInput {
                    language,
                    label,
                    is_default: true,
                })
                .map_err(bad)?,
            )
        }
        _ => None,
    };
    Ok(SeriesFile { series, edition })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_EXAMPLE: &str = r#"{ "title": "  Example   Saga ", "alt_titles": ["Saga Example"], "kind": "Manhwa",
        "language": "EN", "edition": "Official English", "direction": "vertical", "authors": ["A. Author"],
        "status": "Completed", "year": 2020, "genres": ["Fantasy", "fantasy"], "age_restricted": false }"#;

    #[test]
    fn the_documented_example_parses() {
        let f = parse(SPEC_EXAMPLE.as_bytes()).unwrap();
        assert_eq!(f.series.title, "Example Saga");
        assert_eq!(f.series.kind, SeriesKind::Manhwa);
        assert_eq!(f.series.status, SeriesStatus::Completed);
        assert_eq!(f.series.reading_direction, Some(ReadingDirection::Vertical));
        assert_eq!(f.series.genres, ["fantasy"]);
        assert_eq!(f.series.year, Some(2020));
        let e = f.edition.unwrap();
        assert_eq!(
            (e.language.as_str(), e.label.as_str()),
            ("en", "Official English")
        );
        assert!(e.is_default);
    }

    #[test]
    fn only_the_title_is_required_and_defaults_apply() {
        let f = parse(br#"{"title":"Bare"}"#).unwrap();
        assert_eq!(f.series.kind, SeriesKind::Manga);
        assert_eq!(f.series.status, SeriesStatus::Ongoing);
        // The normalised direction is filled in from the kind.
        assert_eq!(f.series.reading_direction, Some(ReadingDirection::Rtl));
        assert!(f.edition.is_none());
        let f = parse(br#"{"title":"T","language":"ja","unknown_key":{"x":1}}"#).unwrap();
        assert_eq!(f.edition.unwrap().label, "Edition (ja)");
        assert!(parse(b"\xEF\xBB\xBF{\"title\":\"BOM\"}").is_ok());
    }

    #[test]
    fn invalid_files_name_the_problem() {
        for (body, needle) in [
            (&br#"{}"#[..], "title"),
            (br#"{"title":"  "}"#, "title"),
            (br#"{"title":"T","kind":"podcast"}"#, "kind"),
            (br#"{"title":"T","status":"dead"}"#, "status"),
            (br#"{"title":"T","direction":"up"}"#, "direction"),
            (br#"{"title":"T","year":99999}"#, "year"),
            (br#"{"title":"T","year":1500}"#, "year"),
            (
                br#"{"title":"T","language":"e_n","edition":"x"}"#,
                "language",
            ),
            (br#"{"title": 5}"#, "series.json"),
            (b"not json", "series.json"),
            (b"[]", "series.json"),
        ] {
            let e = parse(body).unwrap_err().to_string();
            assert!(e.contains(needle), "{e}");
        }
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
}
