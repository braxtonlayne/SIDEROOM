//! Tantivy index of published series (LIB-DATA-004): titles, alternative titles,
//! authors and artists, genres and tags, with prefix matching for the instant
//! search dropdown.
//!
//! The index only ranks. The service re-reads every hit from the database with
//! the viewer's [`Scope`](crate::model::Scope), so a stale index can never show
//! an unpublished or age-restricted series. The `adult` filter here is a second
//! line of defence and keeps restricted hits from using up result slots.

use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, EmptyQuery, Occur, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, Schema, TantivyDocument, TextFieldIndexing,
    TextOptions, Value,
};
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};

use crate::error::{LibraryError, LibraryResult};
use crate::model::SeriesRecord;

const EDGE: &str = "sr_lib_edge";
const MAX_PREFIX: usize = 24;
const TITLE_BOOST: f32 = 3.0;

/// What the index needs to know about a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedSeries {
    pub id: i64,
    /// Title and alternative titles.
    pub titles: Vec<String>,
    /// Authors, artists, genres and tags.
    pub facets: Vec<String>,
    pub age_restricted: bool,
}

impl From<&SeriesRecord> for IndexedSeries {
    fn from(s: &SeriesRecord) -> Self {
        let mut titles = vec![s.title.clone()];
        titles.extend(s.alt_titles.iter().cloned());
        let facets = s
            .authors
            .iter()
            .chain(&s.artists)
            .chain(&s.genres)
            .chain(&s.tags)
            .cloned()
            .collect();
        Self {
            id: s.id,
            titles,
            facets,
            age_restricted: s.age_restricted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeriesHit {
    pub id: i64,
    pub score: f32,
}

#[derive(Clone, Copy)]
struct Fields {
    id: Field,
    titles: Field,
    facets: Field,
    adult: Field,
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

fn analyzer() -> LibraryResult<TextAnalyzer> {
    let ngram = NgramTokenizer::prefix_only(1, MAX_PREFIX)?;
    Ok(TextAnalyzer::builder(ngram).filter(LowerCaser).build())
}

pub struct SeriesIndex {
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    f: Fields,
}

impl SeriesIndex {
    pub fn open_in_dir(dir: &Path) -> LibraryResult<Self> {
        std::fs::create_dir_all(dir).map_err(|e| LibraryError::Search(e.to_string()))?;
        let (schema, _) = build_schema();
        let mmap = tantivy::directory::MmapDirectory::open(dir)
            .map_err(|e| LibraryError::Search(e.to_string()))?;
        Self::from_index(Index::open_or_create(mmap, schema)?)
    }

    pub fn in_ram() -> LibraryResult<Self> {
        let (schema, _) = build_schema();
        Self::from_index(Index::create_in_ram(schema))
    }

    fn from_index(index: Index) -> LibraryResult<Self> {
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

    fn lock(&self) -> LibraryResult<std::sync::MutexGuard<'_, IndexWriter>> {
        self.writer
            .lock()
            .map_err(|_| LibraryError::Search("writer lock poisoned".into()))
    }

    fn commit(&self, mut w: std::sync::MutexGuard<'_, IndexWriter>) -> LibraryResult<()> {
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    fn id_term(&self, id: i64) -> Term {
        Term::from_field_u64(self.f.id, id as u64)
    }

    /// Insert or replace series, then commit.
    pub fn upsert(&self, series: &[IndexedSeries]) -> LibraryResult<()> {
        let w = self.lock()?;
        for s in series {
            w.delete_term(self.id_term(s.id));
            let mut d = TantivyDocument::default();
            d.add_u64(self.f.id, s.id as u64);
            d.add_u64(self.f.adult, u64::from(s.age_restricted));
            for word in s.titles.iter().flat_map(|t| words(t)) {
                d.add_text(self.f.titles, word);
            }
            for word in s.facets.iter().flat_map(|t| words(t)) {
                d.add_text(self.f.facets, word);
            }
            w.add_document(d)?;
        }
        self.commit(w)
    }

    pub fn remove(&self, id: i64) -> LibraryResult<()> {
        let w = self.lock()?;
        w.delete_term(self.id_term(id));
        self.commit(w)
    }

    /// Drop every document (before a full rebuild).
    pub fn clear(&self) -> LibraryResult<()> {
        let w = self.lock()?;
        w.delete_all_documents()?;
        self.commit(w)
    }

    /// Series whose titles or facets start with every word of `text`, best first.
    pub fn search(&self, text: &str, adult: bool, limit: usize) -> LibraryResult<Vec<SeriesHit>> {
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
        if !adult {
            clauses.push((
                Occur::MustNot,
                Box::new(TermQuery::new(
                    Term::from_field_u64(self.f.adult, 1),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        let query: Box<dyn Query> = if clauses.is_empty() {
            Box::new(EmptyQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };
        let searcher = self.reader.searcher();
        let top = searcher.search(
            &query,
            &TopDocs::with_limit(limit.min(100)).order_by_score(),
        )?;
        let mut out = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(id) = doc.get_first(self.f.id).and_then(|v| v.as_u64()) {
                out.push(SeriesHit {
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

    fn doc(id: i64, titles: &[&str], facets: &[&str], adult: bool) -> IndexedSeries {
        IndexedSeries {
            id,
            titles: titles.iter().map(|s| (*s).to_owned()).collect(),
            facets: facets.iter().map(|s| (*s).to_owned()).collect(),
            age_restricted: adult,
        }
    }

    fn ids(h: Vec<SeriesHit>) -> Vec<i64> {
        h.into_iter().map(|h| h.id).collect()
    }

    fn sample() -> SeriesIndex {
        let ix = SeriesIndex::in_ram().unwrap();
        ix.upsert(&[
            doc(
                1,
                &["One Piece", "ワンピース"],
                &["Eiichiro Oda", "adventure"],
                false,
            ),
            doc(
                2,
                &["Re:Zero", "Re Zero kara"],
                &["Tappei Nagatsuki", "isekai"],
                false,
            ),
            doc(3, &["Adult Tale"], &["oda fan", "mature"], true),
            doc(4, &["Piece of Cake"], &["romance"], false),
        ])
        .unwrap();
        ix
    }

    #[test]
    fn prefix_match_on_titles_and_alt_titles() {
        let ix = sample();
        assert_eq!(ids(ix.search("one pi", false, 10).unwrap()), [1]);
        assert_eq!(ids(ix.search("ワン", false, 10).unwrap()), [1]);
        assert_eq!(ids(ix.search("zero", false, 10).unwrap()), [2]);
        assert_eq!(ids(ix.search("re:ze", false, 10).unwrap()), [2]);
    }

    #[test]
    fn matches_authors_genres_and_tags() {
        let ix = sample();
        assert_eq!(ids(ix.search("eiichi", false, 10).unwrap()), [1]);
        assert_eq!(ids(ix.search("isek", false, 10).unwrap()), [2]);
    }

    #[test]
    fn title_hits_outrank_facet_hits() {
        let ix = SeriesIndex::in_ram().unwrap();
        ix.upsert(&[
            doc(1, &["Something"], &["piece"], false),
            doc(2, &["Piece"], &[], false),
        ])
        .unwrap();
        assert_eq!(ids(ix.search("piece", false, 10).unwrap()), [2, 1]);
    }

    #[test]
    fn age_restricted_hits_need_adult() {
        let ix = sample();
        assert_eq!(ids(ix.search("oda", false, 10).unwrap()), [1]);
        let mut both = ids(ix.search("oda", true, 10).unwrap());
        both.sort_unstable();
        assert_eq!(both, [1, 3]);
    }

    #[test]
    fn upsert_replaces_and_remove_deletes() {
        let ix = sample();
        ix.upsert(&[doc(1, &["Renamed"], &[], false)]).unwrap();
        assert!(ix.search("one", false, 10).unwrap().is_empty());
        assert_eq!(ids(ix.search("renam", false, 10).unwrap()), [1]);
        ix.remove(1).unwrap();
        assert!(ix.search("renam", false, 10).unwrap().is_empty());
        ix.clear().unwrap();
        assert!(ix.search("piece", false, 10).unwrap().is_empty());
    }

    #[test]
    fn empty_or_symbol_only_queries_match_nothing() {
        let ix = sample();
        assert!(ix.search("", false, 10).unwrap().is_empty());
        assert!(ix.search("  !! ", false, 10).unwrap().is_empty());
        assert!(ix.search("piece", false, 0).unwrap().is_empty());
    }
}
