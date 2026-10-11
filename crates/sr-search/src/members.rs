//! Guild member index: prefix search over usernames, display names and nicknames
//! (member list search, LNG-SSET-027, mention autocomplete).

use std::path::Path;
use std::sync::Mutex;

use sr_core::{GuildId, RoleId, UserId};
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{BooleanQuery, EmptyQuery, Occur, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, Schema, TantivyDocument, TextFieldIndexing,
    TextOptions, Value,
};
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};

use crate::error::{SearchError, SearchResult};

const EDGE: &str = "sr_edge";
const MAX_PREFIX: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedMember {
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub username: String,
    pub display_name: Option<String>,
    pub nick: Option<String>,
    pub role_ids: Vec<RoleId>,
}

#[derive(Debug, Clone)]
pub struct MemberQuery {
    pub guild_id: GuildId,
    /// Free text; every word must prefix-match some name.
    pub text: String,
    /// Members must hold this role.
    pub role: Option<RoleId>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberHit {
    pub user_id: UserId,
}

#[derive(Clone, Copy)]
struct Fields {
    key: Field,
    guild: Field,
    user: Field,
    names: Field,
    role: Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let names = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(EDGE)
            .set_index_option(IndexRecordOption::Basic),
    );
    let idx = NumericOptions::default().set_indexed().set_stored();
    let f = Fields {
        // guild_id:user_id composite, for replace-on-upsert.
        key: b.add_text_field(
            "key",
            TextOptions::default().set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("raw")
                    .set_index_option(IndexRecordOption::Basic),
            ),
        ),
        guild: b.add_u64_field("guild_id", idx.clone()),
        user: b.add_u64_field("user_id", idx),
        names: b.add_text_field("names", names),
        role: b.add_u64_field("role", NumericOptions::default().set_indexed()),
    };
    (b.build(), f)
}

fn key(g: GuildId, u: UserId) -> String {
    format!("{}:{}", g.get(), u.get())
}

pub struct MemberIndex {
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    f: Fields,
}

impl MemberIndex {
    pub fn open_in_dir(dir: &Path) -> SearchResult<Self> {
        std::fs::create_dir_all(dir)?;
        let (schema, _) = build_schema();
        Self::from_index(Index::open_or_create(
            tantivy::directory::MmapDirectory::open(dir)?,
            schema,
        )?)
    }

    pub fn in_ram() -> SearchResult<Self> {
        let (schema, _) = build_schema();
        Self::from_index(Index::create_in_ram(schema))
    }

    fn edge_analyzer() -> SearchResult<TextAnalyzer> {
        let ngram = NgramTokenizer::prefix_only(1, MAX_PREFIX)?;
        Ok(TextAnalyzer::builder(ngram).filter(LowerCaser).build())
    }

    fn from_index(index: Index) -> SearchResult<Self> {
        index.tokenizers().register(EDGE, Self::edge_analyzer()?);
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

    fn lock(&self) -> SearchResult<std::sync::MutexGuard<'_, IndexWriter>> {
        self.writer
            .lock()
            .map_err(|_| SearchError::Index("writer lock poisoned".into()))
    }

    /// Insert or replace members, then commit.
    pub fn upsert(&self, members: &[IndexedMember]) -> SearchResult<()> {
        let mut w = self.lock()?;
        for m in members {
            let k = key(m.guild_id, m.user_id);
            w.delete_term(Term::from_field_text(self.f.key, &k));
            let mut d = TantivyDocument::default();
            d.add_text(self.f.key, &k);
            d.add_u64(self.f.guild, m.guild_id.get());
            d.add_u64(self.f.user, m.user_id.get());
            // Edge n-grams are per word, so index each word of each name.
            let names = std::iter::once(m.username.as_str())
                .chain(m.display_name.as_deref())
                .chain(m.nick.as_deref());
            for word in names.flat_map(str::split_whitespace) {
                d.add_text(self.f.names, word);
            }
            for r in &m.role_ids {
                d.add_u64(self.f.role, r.get());
            }
            w.add_document(d)?;
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    pub fn remove(&self, guild: GuildId, user: UserId) -> SearchResult<()> {
        let mut w = self.lock()?;
        w.delete_term(Term::from_field_text(self.f.key, &key(guild, user)));
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    /// Remove every member of a guild (guild deleted).
    pub fn remove_guild(&self, guild: GuildId) -> SearchResult<()> {
        let mut w = self.lock()?;
        w.delete_term(Term::from_field_u64(self.f.guild, guild.get()));
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    /// Returns `(total, hits)`; hits are ordered by relevance (shorter names rank higher).
    pub fn search(&self, q: &MemberQuery) -> SearchResult<(usize, Vec<MemberHit>)> {
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_u64(self.f.guild, q.guild_id.get()),
                IndexRecordOption::Basic,
            )),
        )];
        let mut analyzer = Self::edge_analyzer()?;
        for word in q.text.split_whitespace() {
            // A word longer than the max prefix is matched on its first MAX_PREFIX chars.
            let w: String = word.to_lowercase().chars().take(MAX_PREFIX).collect();
            let mut stream = analyzer.token_stream(&w);
            let mut last = None;
            while stream.advance() {
                last = Some(stream.token().text.clone());
            }
            let Some(term) = last else {
                return Ok((0, Vec::new()));
            };
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.f.names, &term),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if let Some(r) = q.role {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_u64(self.f.role, r.get()),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        let query: Box<dyn Query> = if q.limit == 0 {
            Box::new(EmptyQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };
        let searcher = self.reader.searcher();
        let (count, top) = searcher.search(
            &query,
            &(
                Count,
                TopDocs::with_limit(q.limit.clamp(1, 100)).order_by_score(),
            ),
        )?;
        let mut hits = Vec::with_capacity(top.len());
        for (_, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(u) = doc.get_first(self.f.user).and_then(|v| v.as_u64()) {
                hits.push(MemberHit {
                    user_id: UserId::new(u),
                });
            }
        }
        Ok((count, hits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(g: u64, u: u64, name: &str, nick: Option<&str>, roles: &[u64]) -> IndexedMember {
        IndexedMember {
            guild_id: GuildId::new(g),
            user_id: UserId::new(u),
            username: name.to_owned(),
            display_name: None,
            nick: nick.map(str::to_owned),
            role_ids: roles.iter().map(|r| RoleId::new(*r)).collect(),
        }
    }

    fn q(g: u64, text: &str) -> MemberQuery {
        MemberQuery {
            guild_id: GuildId::new(g),
            text: text.to_owned(),
            role: None,
            limit: 10,
        }
    }

    fn users(r: (usize, Vec<MemberHit>)) -> Vec<u64> {
        let mut v: Vec<u64> = r.1.iter().map(|h| h.user_id.get()).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn prefix_search_scoped_to_guild() {
        let dir = tempfile::tempdir().unwrap();
        let idx = MemberIndex::open_in_dir(dir.path()).unwrap();
        idx.upsert(&[
            m(1, 10, "alice", Some("Ally Cat"), &[5]),
            m(1, 11, "bob", None, &[5, 6]),
            m(2, 12, "alicia", None, &[]),
        ])
        .unwrap();
        assert_eq!(users(idx.search(&q(1, "ali")).unwrap()), vec![10]);
        assert_eq!(users(idx.search(&q(1, "ALLY")).unwrap()), vec![10]);
        assert_eq!(users(idx.search(&q(1, "ally cat")).unwrap()), vec![10]);
        assert_eq!(users(idx.search(&q(2, "ali")).unwrap()), vec![12]);
        assert_eq!(users(idx.search(&q(1, "zzz")).unwrap()), Vec::<u64>::new());
        let mut by_role = q(1, "");
        by_role.role = Some(RoleId::new(6));
        assert_eq!(users(idx.search(&by_role).unwrap()), vec![11]);
    }

    #[test]
    fn upsert_replaces_and_remove_works() {
        let idx = MemberIndex::in_ram().unwrap();
        idx.upsert(&[m(1, 10, "alice", None, &[])]).unwrap();
        idx.upsert(&[m(1, 10, "carol", None, &[])]).unwrap();
        assert_eq!(users(idx.search(&q(1, "ali")).unwrap()), Vec::<u64>::new());
        assert_eq!(users(idx.search(&q(1, "car")).unwrap()), vec![10]);
        idx.remove(GuildId::new(1), UserId::new(10)).unwrap();
        assert_eq!(users(idx.search(&q(1, "car")).unwrap()), Vec::<u64>::new());
        idx.upsert(&[m(3, 20, "dave", None, &[])]).unwrap();
        idx.remove_guild(GuildId::new(3)).unwrap();
        assert_eq!(users(idx.search(&q(3, "dav")).unwrap()), Vec::<u64>::new());
    }
}
