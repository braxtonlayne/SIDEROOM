//! Tantivy index of published titles (CIN-DATA-004): titles, original and alternative titles,
//! cast and crew text, genres, tags and studios, with prefix matching for the instant search.
//!
//! The index only ranks. The service re-reads every hit from the database with the viewer's
//! [`Scope`](crate::model::Scope), so a stale index can never show an unpublished title. The
//! `include_restricted` flag leaves restricted hits out for viewers who hide them
//! (`cinema.hide_age_restricted`, D-184). The index is rebuilt from Postgres at boot.

use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, Schema, TantivyDocument, TextFieldIndexing,
    TextOptions, Value,
};
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};

use crate::error::{CinemaError, CinemaResult};
use crate::model::{TitleKind, TitleRecord};

const EDGE: &str = "sr_cin_edge";
const MAX_PREFIX: usize = 24;
const TITLE_BOOST: f32 = 3.0;

/// What the index needs to know about a title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedTitle {
    pub id: i64,
    pub kind: TitleKind,
    /// Title, original title and alternative titles.
    pub titles: Vec<String>,
    /// Cast, directors, writers, studios, crew, genres and tags.
    pub facets: Vec<String>,
    pub age_restricted: bool,
}

impl From<&TitleRecord> for IndexedTitle {
    fn from(t: &TitleRecord) -> Self {
        let mut titles = vec![t.title.clone()];
        titles.extend(t.original_title.iter().cloned());
        titles.extend(t.alt_titles.iter().cloned());
        let facets = t
            .cast
            .iter()
            .chain(&t.directors)
            .chain(&t.writers)
            .chain(&t.studios)
            .chain(&t.crew)
            .chain(&t.genres)
            .chain(&t.tags)
            .cloned()
            .collect();
        Self {
            id: t.id,
            kind: t.kind,
            titles,
            facets,
            age_restricted: t.age_restricted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitleHit {
    pub id: i64,
    pub score: f32,
}

#[derive(Clone, Copy)]
struct Fields {
    id: Field,
    kind: Field,
    titles: Field,
    facets: Field,
    adult: Field,
}

const fn kind_code(k: TitleKind) -> u64 {
    match k {
        TitleKind::Movie => 0,
        TitleKind::Series => 1,
        TitleKind::Anime => 2,
    }
}

fn build_schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let edge = || {
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(EDGE)
                .set_index_option(IndexRecordOption::Basic),
        )
    };
    let f = Fields {
        id: b.add_u64_field("id", NumericOptions::default().set_indexed().set_stored()),
        kind: b.add_u64_field("kind", NumericOptions::default().set_indexed()),
        titles: b.add_text_field("titles", edge()),
        facets: b.add_text_field("facets", edge()),
        adult: b.add_u64_field("adult", NumericOptions::default().set_indexed()),
    };
    (b.build(), f)
}

/// Words of a string: runs of alphanumerics.
fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}

fn analyzer() -> CinemaResult<TextAnalyzer> {
    let ngram = NgramTokenizer::prefix_only(1, MAX_PREFIX)?;
    Ok(TextAnalyzer::builder(ngram).filter(LowerCaser).build())
}

pub struct TitleIndex {
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    f: Fields,
}

impl TitleIndex {
    pub fn open_in_dir(dir: &Path) -> CinemaResult<Self> {
        std::fs::create_dir_all(dir).map_err(|e| CinemaError::Search(e.to_string()))?;
        let (schema, _) = build_schema();
        let mmap = tantivy::directory::MmapDirectory::open(dir)
            .map_err(|e| CinemaError::Search(e.to_string()))?;
        Self::from_index(Index::open_or_create(mmap, schema)?)
    }

    pub fn in_ram() -> CinemaResult<Self> {
        let (schema, _) = build_schema();
        Self::from_index(Index::create_in_ram(schema))
    }

    fn from_index(index: Index) -> CinemaResult<Self> {
        index.tokenizers().register(EDGE, analyzer()?);
        let (_, f) = build_schema();
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let writer = index.writer(15_000_000)?;
        Ok(Self {
            reader,
            writer: Mutex::new(writer),
            f,
        })
    }

    fn lock(&self) -> CinemaResult<std::sync::MutexGuard<'_, IndexWriter>> {
        self.writer
            .lock()
            .map_err(|_| CinemaError::Search("writer lock poisoned".into()))
    }

    fn commit(&self, mut w: std::sync::MutexGuard<'_, IndexWriter>) -> CinemaResult<()> {
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    fn id_term(&self, id: i64) -> Term {
        Term::from_field_u64(self.f.id, id as u64)
    }

    /// Insert or replace titles, then commit.
    pub fn upsert(&self, titles: &[IndexedTitle]) -> CinemaResult<()> {
        let w = self.lock()?;
        for t in titles {
            w.delete_term(self.id_term(t.id));
            let mut d = TantivyDocument::default();
            d.add_u64(self.f.id, t.id as u64);
            d.add_u64(self.f.kind, kind_code(t.kind));
            d.add_u64(self.f.adult, u64::from(t.age_restricted));
            for word in t.titles.iter().flat_map(|s| words(s)) {
                d.add_text(self.f.titles, word);
            }
            for word in t.facets.iter().flat_map(|s| words(s)) {
                d.add_text(self.f.facets, word);
            }
            w.add_document(d)?;
        }
        self.commit(w)
    }

    pub fn remove(&self, id: i64) -> CinemaResult<()> {
        let w = self.lock()?;
        w.delete_term(self.id_term(id));
        self.commit(w)
    }

    /// Drop every document (before a full rebuild).
    pub fn clear(&self) -> CinemaResult<()> {
        let w = self.lock()?;
        w.delete_all_documents()?;
        self.commit(w)
    }

    /// Titles whose names or facets start with every word of `text`, best first.
    pub fn search(
        &self,
        text: &str,
        kind: Option<TitleKind>,
        include_restricted: bool,
        limit: usize,
    ) -> CinemaResult<Vec<TitleHit>> {
        let mut an = analyzer()?;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for word in words(text) {
            let w: String = word.chars().take(MAX_PREFIX).collect();
            let mut stream = an.token_stream(&w);
            let mut last = None;
            while stream.advance() {
                last = Some(stream.token().text.clone());
            }
            let Some(term) = last else {
                return Ok(Vec::new());
            };
            let in_titles: Box<dyn Query> = Box::new(BoostQuery::new(
                Box::new(TermQuery::new(
                    Term::from_field_text(self.f.titles, &term),
                    IndexRecordOption::Basic,
                )),
                TITLE_BOOST,
            ));
            let in_facets: Box<dyn Query> = Box::new(TermQuery::new(
                Term::from_field_text(self.f.facets, &term),
                IndexRecordOption::Basic,
            ));
            clauses.push((
                Occur::Must,
                Box::new(BooleanQuery::new(vec![
                    (Occur::Should, in_titles),
                    (Occur::Should, in_facets),
                ])),
            ));
        }
        if clauses.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        if !include_restricted {
            clauses.push((
                Occur::MustNot,
                Box::new(TermQuery::new(
                    Term::from_field_u64(self.f.adult, 1),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if let Some(k) = kind {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_u64(self.f.kind, kind_code(k)),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        let query = BooleanQuery::new(clauses);
        let searcher = self.reader.searcher();
        let top = searcher.search(
            &query,
            &TopDocs::with_limit(limit.min(100)).order_by_score(),
        )?;
        let mut out = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(id) = doc.get_first(self.f.id).and_then(|v| v.as_u64()) {
                out.push(TitleHit {
                    id: id as i64,
                    score,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(
        id: i64,
        kind: TitleKind,
        titles: &[&str],
        facets: &[&str],
        adult: bool,
    ) -> IndexedTitle {
        IndexedTitle {
            id,
            kind,
            titles: titles.iter().map(|s| s.to_string()).collect(),
            facets: facets.iter().map(|s| s.to_string()).collect(),
            age_restricted: adult,
        }
    }

    fn index() -> TitleIndex {
        let ix = TitleIndex::in_ram().unwrap();
        ix.upsert(&[
            doc(
                1,
                TitleKind::Movie,
                &["The Long Goodbye"],
                &["Elliott Gould", "crime"],
                false,
            ),
            doc(
                2,
                TitleKind::Anime,
                &["Cowboy Bebop", "Kaubōi Bibappu"],
                &["Shinichiro Watanabe", "sci-fi"],
                false,
            ),
            doc(3, TitleKind::Series, &["Goodbye Earth"], &["Drama"], true),
        ])
        .unwrap();
        ix
    }

    fn ids(h: Vec<TitleHit>) -> Vec<i64> {
        h.into_iter().map(|h| h.id).collect()
    }

    #[test]
    fn prefix_search_over_titles_and_people() {
        let ix = index();
        assert_eq!(ids(ix.search("cow", None, true, 10).unwrap()), vec![2]);
        assert_eq!(ids(ix.search("watan", None, true, 10).unwrap()), vec![2]);
        assert_eq!(ids(ix.search("kaub", None, true, 10).unwrap()), vec![2]);
        let mut both = ids(ix.search("goodb", None, true, 10).unwrap());
        both.sort_unstable();
        assert_eq!(both, vec![1, 3]);
        assert_eq!(
            ids(ix.search("long good", None, true, 10).unwrap()),
            vec![1]
        );
        assert!(ix.search("zzz", None, true, 10).unwrap().is_empty());
        assert!(ix.search("   ", None, true, 10).unwrap().is_empty());
    }

    #[test]
    fn title_hits_outrank_people_hits() {
        let ix = TitleIndex::in_ram().unwrap();
        ix.upsert(&[
            doc(1, TitleKind::Movie, &["Nolan"], &[], false),
            doc(
                2,
                TitleKind::Movie,
                &["Inception"],
                &["Christopher Nolan"],
                false,
            ),
        ])
        .unwrap();
        assert_eq!(ids(ix.search("nolan", None, true, 10).unwrap()), vec![1, 2]);
    }

    #[test]
    fn kind_and_age_filters() {
        let ix = index();
        assert_eq!(
            ids(ix
                .search("goodb", Some(TitleKind::Series), true, 10)
                .unwrap()),
            vec![3]
        );
        assert_eq!(
            ids(ix.search("goodb", None, false, 10).unwrap()),
            vec![1],
            "restricted left out"
        );
        assert!(
            ix.search("goodb", Some(TitleKind::Series), false, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn upsert_replaces_and_remove_drops() {
        let ix = index();
        ix.upsert(&[doc(2, TitleKind::Anime, &["Samurai Champloo"], &[], false)])
            .unwrap();
        assert!(ix.search("cow", None, true, 10).unwrap().is_empty());
        assert_eq!(ids(ix.search("samur", None, true, 10).unwrap()), vec![2]);
        ix.remove(2).unwrap();
        assert!(ix.search("samur", None, true, 10).unwrap().is_empty());
        ix.clear().unwrap();
        assert!(ix.search("goodb", None, true, 10).unwrap().is_empty());
    }

    #[test]
    fn indexed_title_collects_people_and_labels() {
        let rec = TitleRecord {
            id: 5,
            kind: TitleKind::Movie,
            title: "T".into(),
            original_title: Some("Orig".into()),
            alt_titles: vec!["Alt".into()],
            sort_title: "t".into(),
            year: None,
            runtime_min: None,
            content_rating: None,
            synopsis: String::new(),
            tagline: None,
            genres: vec!["drama".into()],
            tags: vec!["slow".into()],
            languages: vec![],
            subtitle_languages: vec![],
            cast: vec!["A".into()],
            directors: vec!["B".into()],
            writers: vec!["C".into()],
            studios: vec!["D".into()],
            crew: vec!["Composer: E".into()],
            age_restricted: true,
            poster_key: None,
            backdrop_key: None,
            logo_key: None,
            external_ids: Default::default(),
            asset_id: None,
            published: true,
            published_at_ms: None,
            merged_into: None,
            season_count: 0,
            episode_count: 0,
            last_content_at_ms: None,
            has_trailer: false,
            created_at_ms: 0,
            updated_at_ms: 0,
            sort_value: None,
        };
        let d = IndexedTitle::from(&rec);
        assert_eq!(d.titles, vec!["T", "Orig", "Alt"]);
        assert_eq!(d.facets.len(), 7);
        assert!(d.age_restricted);
    }
}
