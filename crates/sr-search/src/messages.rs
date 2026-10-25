//! The message index (LNG-SRC-001..003).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

use sr_core::{ChannelId, GuildId, MessageId, UserId};
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{
    BooleanQuery, EmptyQuery, Occur, PhraseQuery, Query, RangeQuery, TermQuery, TermSetQuery,
};
use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, STORED, Schema, TantivyDocument, TextFieldIndexing,
    TextOptions, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::{DocAddress, Index, IndexReader, IndexWriter, Order, ReloadPolicy, Term};

use crate::error::{SearchError, SearchResult};
use crate::query::{ParsedQuery, parse_query};

const WRITER_HEAP: usize = 50_000_000;
const SNIPPET_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HasKind {
    Link,
    Embed,
    File,
    Image,
    Video,
    Sound,
    Sticker,
    Poll,
}

impl HasKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Embed => "embed",
            Self::File => "file",
            Self::Image => "image",
            Self::Video => "video",
            Self::Sound => "sound",
            Self::Sticker => "sticker",
            Self::Poll => "poll",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "link" => Self::Link,
            "embed" => Self::Embed,
            "file" => Self::File,
            "image" => Self::Image,
            "video" => Self::Video,
            "sound" => Self::Sound,
            "sticker" => Self::Sticker,
            "poll" => Self::Poll,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorType {
    User,
    Bot,
    Webhook,
}

impl AuthorType {
    const fn code(self) -> u64 {
        match self {
            Self::User => 0,
            Self::Bot => 1,
            Self::Webhook => 2,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "user" => Self::User,
            "bot" => Self::Bot,
            "webhook" => Self::Webhook,
            _ => return None,
        })
    }
}

/// What the index stores about a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedMessage {
    pub id: MessageId,
    pub channel_id: ChannelId,
    /// `None` for DMs.
    pub guild_id: Option<GuildId>,
    pub author_id: UserId,
    pub author_type: AuthorType,
    /// Plain text (markdown stripped by the caller, LNG-MD-004).
    pub content: String,
    pub has: BTreeSet<HasKind>,
    pub timestamp_ms: i64,
    pub pinned: bool,
    pub mention_user_ids: Vec<UserId>,
}

/// Message lifecycle events the indexer consumes (LNG-SRC-002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageEvent {
    Created(IndexedMessage),
    /// Replaces the stored document (edit, pin change).
    Updated(IndexedMessage),
    /// Deleted by the author or a moderator: never searchable again.
    Deleted(MessageId),
    /// Bulk delete / channel delete.
    ChannelDeleted(ChannelId),
    /// The guild was deleted: remove every message indexed under it.
    GuildDeleted(GuildId),
}

/// The channels the searcher may read (VIEW_CHANNEL + READ_MESSAGE_HISTORY, or
/// DM membership). Computed server-side by the caller; the index never returns
/// anything outside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Channels(Vec<ChannelId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Relevance,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// Raw search-box text, parsed by [`parse_query`].
    pub query: String,
    pub access: Access,
    /// Restrict to one guild (guild scope); `None` for a DM / channel scope.
    pub guild_id: Option<GuildId>,
    pub sort: Sort,
    pub offset: usize,
    pub limit: usize,
}

impl SearchRequest {
    pub fn new(query: impl Into<String>, access: Access) -> Self {
        Self {
            query: query.into(),
            access,
            guild_id: None,
            sort: Sort::Newest,
            offset: 0,
            limit: 25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    /// Byte ranges into `text` to highlight.
    pub highlights: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub message_id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
    pub author_id: UserId,
    pub timestamp_ms: i64,
    pub score: f32,
    pub snippet: Snippet,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub total: usize,
    pub offset: usize,
    pub hits: Vec<Hit>,
}

#[derive(Clone, Copy)]
struct Fields {
    id: Field,
    channel: Field,
    guild: Field,
    author: Field,
    author_type: Field,
    content: Field,
    has: Field,
    ts: Field,
    pinned: Field,
    mention: Field,
}

fn num() -> NumericOptions {
    NumericOptions::default()
        .set_indexed()
        .set_fast()
        .set_stored()
}

fn build_schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let raw = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("raw")
            .set_index_option(IndexRecordOption::Basic),
    );
    let content = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let f = Fields {
        id: b.add_u64_field("id", num()),
        channel: b.add_u64_field("channel_id", num()),
        guild: b.add_u64_field("guild_id", num()),
        author: b.add_u64_field("author_id", num()),
        author_type: b.add_u64_field("author_type", num()),
        content: b.add_text_field("content", content),
        has: b.add_text_field("has", raw),
        ts: b.add_u64_field("ts", num()),
        pinned: b.add_u64_field("pinned", num()),
        mention: b.add_u64_field("mention", NumericOptions::default().set_indexed()),
    };
    let _ = STORED;
    (b.build(), f)
}

/// Tantivy-backed message index.
pub struct MessageIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    f: Fields,
}

fn ts_u64(ms: i64) -> u64 {
    u64::try_from(ms).unwrap_or(0)
}

impl MessageIndex {
    /// Open the index in `dir`, creating it when empty.
    pub fn open_in_dir(dir: &Path) -> SearchResult<Self> {
        std::fs::create_dir_all(dir)?;
        let (schema, _) = build_schema();
        let index = Index::open_or_create(tantivy::directory::MmapDirectory::open(dir)?, schema)?;
        Self::from_index(index)
    }

    /// RAM-only index (tests, tools).
    pub fn in_ram() -> SearchResult<Self> {
        let (schema, _) = build_schema();
        Self::from_index(Index::create_in_ram(schema))
    }

    fn from_index(index: Index) -> SearchResult<Self> {
        let (_, f) = build_schema();
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let writer = index.writer(WRITER_HEAP)?;
        Ok(Self {
            index,
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

    fn document(&self, m: &IndexedMessage) -> TantivyDocument {
        let f = &self.f;
        let mut d = TantivyDocument::default();
        d.add_u64(f.id, m.id.get());
        d.add_u64(f.channel, m.channel_id.get());
        d.add_u64(f.guild, m.guild_id.map_or(0, GuildId::get));
        d.add_u64(f.author, m.author_id.get());
        d.add_u64(f.author_type, m.author_type.code());
        d.add_text(f.content, &m.content);
        for h in &m.has {
            d.add_text(f.has, h.as_str());
        }
        d.add_u64(f.ts, ts_u64(m.timestamp_ms));
        d.add_u64(f.pinned, u64::from(m.pinned));
        for u in &m.mention_user_ids {
            d.add_u64(f.mention, u.get());
        }
        d
    }

    /// Apply one event and commit (LNG-SRC-002). Use [`Self::apply_batch`] for bulk work.
    pub fn apply(&self, event: &MessageEvent) -> SearchResult<()> {
        self.apply_batch(std::slice::from_ref(event))
    }

    /// Apply events in order, then commit and refresh the reader once.
    pub fn apply_batch(&self, events: &[MessageEvent]) -> SearchResult<()> {
        let mut w = self.lock()?;
        for e in events {
            match e {
                MessageEvent::Created(m) | MessageEvent::Updated(m) => {
                    // Delete-then-add makes replays and edits idempotent.
                    w.delete_term(Term::from_field_u64(self.f.id, m.id.get()));
                    w.add_document(self.document(m))?;
                }
                MessageEvent::Deleted(id) => {
                    w.delete_term(Term::from_field_u64(self.f.id, id.get()));
                }
                MessageEvent::ChannelDeleted(c) => {
                    w.delete_term(Term::from_field_u64(self.f.channel, c.get()));
                }
                MessageEvent::GuildDeleted(g) => {
                    w.delete_term(Term::from_field_u64(self.f.guild, g.get()));
                }
            }
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    /// Add documents without deleting earlier versions and without committing (RDY-PERF-003, `search.reindex`).
    /// For a rebuild into an emptied index only: the caller commits with [`Self::commit_bulk`] every few
    /// thousand documents instead of once per page, which is what made a 5 million message reindex spend its time
    /// writing segments.
    pub fn add_bulk(&self, events: &[MessageEvent]) -> SearchResult<()> {
        let w = self.lock()?;
        for e in events {
            if let MessageEvent::Created(m) | MessageEvent::Updated(m) = e {
                w.add_document(self.document(m))?;
            }
        }
        Ok(())
    }

    /// Commit what [`Self::add_bulk`] added and refresh the reader.
    pub fn commit_bulk(&self) -> SearchResult<()> {
        let mut w = self.lock()?;
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    /// Drop everything and index `messages` (full reindex command, LNG-SRC-002).
    pub fn rebuild(&self, messages: impl IntoIterator<Item = IndexedMessage>) -> SearchResult<()> {
        let mut w = self.lock()?;
        w.delete_all_documents()?;
        for m in messages {
            w.add_document(self.document(&m))?;
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    fn term_query(&self, field: Field, v: u64) -> Box<dyn Query> {
        Box::new(TermQuery::new(
            Term::from_field_u64(field, v),
            IndexRecordOption::Basic,
        ))
    }

    fn any_of(&self, field: Field, values: &[u64]) -> Box<dyn Query> {
        let terms: Vec<Term> = values
            .iter()
            .map(|v| Term::from_field_u64(field, *v))
            .collect();
        Box::new(TermSetQuery::new(terms))
    }

    /// Analyze `text` with the content tokenizer into lowercase terms.
    fn analyze(&self, text: &str) -> SearchResult<Vec<String>> {
        let mut analyzer = self
            .index
            .tokenizers()
            .get("default")
            .ok_or_else(|| SearchError::Index("default tokenizer missing".into()))?;
        let mut stream = analyzer.token_stream(text);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.clone());
        }
        Ok(out)
    }

    fn text_clauses(&self, q: &ParsedQuery) -> SearchResult<Vec<(Occur, Box<dyn Query>)>> {
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let content = self.f.content;
        let mut words = Vec::new();
        for w in &q.words {
            words.extend(self.analyze(w)?);
        }
        for w in words {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(content, &w),
                    IndexRecordOption::WithFreqs,
                )),
            ));
        }
        for p in &q.phrases {
            let toks = self.analyze(p)?;
            match toks.len() {
                0 => {}
                1 => clauses.push((
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(content, &toks[0]),
                        IndexRecordOption::WithFreqs,
                    )),
                )),
                _ => clauses.push((
                    Occur::Must,
                    Box::new(PhraseQuery::new(
                        toks.iter()
                            .map(|t| Term::from_field_text(content, t))
                            .collect(),
                    )),
                )),
            }
        }
        Ok(clauses)
    }

    /// Build the full query: text AND filters AND access.
    fn build_query(&self, req: &SearchRequest, q: &ParsedQuery) -> SearchResult<Box<dyn Query>> {
        let Access::Channels(allowed) = &req.access;
        if allowed.is_empty() {
            return Ok(Box::new(EmptyQuery));
        }
        let f = &self.f;
        let mut clauses = self.text_clauses(q)?;
        // Permission filter: always applied, intersected with `in:`.
        let allowed_ids: Vec<u64> = allowed.iter().map(|c| c.get()).collect();
        clauses.push((Occur::Must, self.any_of(f.channel, &allowed_ids)));
        if !q.in_channels.is_empty() {
            clauses.push((Occur::Must, self.any_of(f.channel, &q.in_channels)));
        }
        if let Some(g) = req.guild_id {
            clauses.push((Occur::Must, self.term_query(f.guild, g.get())));
        }
        if !q.from.is_empty() {
            clauses.push((Occur::Must, self.any_of(f.author, &q.from)));
        }
        if !q.mentions.is_empty() {
            clauses.push((Occur::Must, self.any_of(f.mention, &q.mentions)));
        }
        for h in &q.has {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(f.has, h.as_str()),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if !q.author_types.is_empty() {
            let codes: Vec<u64> = q.author_types.iter().map(|a| a.code()).collect();
            clauses.push((Occur::Must, self.any_of(f.author_type, &codes)));
        }
        if let Some(p) = q.pinned {
            clauses.push((Occur::Must, self.term_query(f.pinned, u64::from(p))));
        }
        if q.before_ms.is_some() || q.after_ms.is_some() {
            use std::ops::Bound;
            let lower = q.after_ms.map_or(Bound::Unbounded, |ms| {
                Bound::Included(Term::from_field_u64(f.ts, ts_u64(ms)))
            });
            let upper = q.before_ms.map_or(Bound::Unbounded, |ms| {
                Bound::Excluded(Term::from_field_u64(f.ts, ts_u64(ms)))
            });
            clauses.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
        }
        Ok(Box::new(BooleanQuery::new(clauses)))
    }

    /// Run a search (LNG-SRC-003).
    pub fn search(&self, req: &SearchRequest) -> SearchResult<Page> {
        let parsed = parse_query(&req.query)?;
        let query = self.build_query(req, &parsed)?;
        let searcher = self.reader.searcher();
        let limit = req.limit.clamp(1, 100);

        let relevance = req.sort == Sort::Relevance && parsed.has_text();
        let (total, addrs): (usize, Vec<(f32, DocAddress)>) = if relevance {
            let (count, top) = searcher.search(
                &query,
                &(
                    Count,
                    TopDocs::with_limit(limit)
                        .and_offset(req.offset)
                        .order_by_score(),
                ),
            )?;
            (count, top)
        } else {
            let order = if req.sort == Sort::Oldest {
                Order::Asc
            } else {
                Order::Desc
            };
            let (count, top) = searcher.search(
                &query,
                &(
                    Count,
                    TopDocs::with_limit(limit)
                        .and_offset(req.offset)
                        .order_by_fast_field::<u64>("id", order),
                ),
            )?;
            (count, top.into_iter().map(|(_, a)| (0.0_f32, a)).collect())
        };

        let generator = if parsed.has_text() {
            Some(SnippetGenerator::create(
                &searcher,
                &*self.text_only_query(&parsed)?,
                self.f.content,
            )?)
        } else {
            None
        };

        let mut hits = Vec::with_capacity(addrs.len());
        for (score, addr) in addrs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let get_u64 = |f: Field| doc.get_first(f).and_then(|v| v.as_u64()).unwrap_or(0);
            let content = doc
                .get_first(self.f.content)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let guild = get_u64(self.f.guild);
            hits.push(Hit {
                message_id: MessageId::new(get_u64(self.f.id)),
                channel_id: ChannelId::new(get_u64(self.f.channel)),
                guild_id: (guild != 0).then(|| GuildId::new(guild)),
                author_id: UserId::new(get_u64(self.f.author)),
                timestamp_ms: i64::try_from(get_u64(self.f.ts)).unwrap_or(i64::MAX),
                score,
                snippet: make_snippet(generator.as_ref(), content),
            });
        }
        Ok(Page {
            total,
            offset: req.offset,
            hits,
        })
    }

    fn text_only_query(&self, q: &ParsedQuery) -> SearchResult<Box<dyn Query>> {
        Ok(Box::new(BooleanQuery::new(self.text_clauses(q)?)))
    }
}

fn truncate_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

fn make_snippet(generator: Option<&SnippetGenerator>, content: &str) -> Snippet {
    if let Some(g) = generator {
        let s = g.snippet(content);
        if !s.fragment().is_empty() {
            return Snippet {
                text: s.fragment().to_owned(),
                highlights: s.highlighted().iter().map(|r| (r.start, r.end)).collect(),
            };
        }
    }
    Snippet {
        text: truncate_chars(content, SNIPPET_CHARS).to_owned(),
        highlights: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    fn msg(id: u64, channel: u64, author: u64, text: &str, ts: i64) -> IndexedMessage {
        IndexedMessage {
            id: MessageId::new(id),
            channel_id: ChannelId::new(channel),
            guild_id: Some(GuildId::new(1)),
            author_id: UserId::new(author),
            author_type: AuthorType::User,
            content: text.to_owned(),
            has: BTreeSet::new(),
            timestamp_ms: ts,
            pinned: false,
            mention_user_ids: vec![],
        }
    }

    fn all(channels: &[u64]) -> Access {
        Access::Channels(channels.iter().map(|c| ChannelId::new(*c)).collect())
    }

    fn ids(p: &Page) -> Vec<u64> {
        p.hits.iter().map(|h| h.message_id.get()).collect()
    }

    fn seeded(dir: Option<&Path>) -> MessageIndex {
        let idx = match dir {
            Some(d) => MessageIndex::open_in_dir(d).unwrap(),
            None => MessageIndex::in_ram().unwrap(),
        };
        let mut a = msg(10, 100, 7, "the quick brown fox", 3 * DAY);
        a.has.insert(HasKind::Link);
        let mut b = msg(11, 100, 8, "lazy dog sleeps", 4 * DAY);
        b.pinned = true;
        b.mention_user_ids = vec![UserId::new(7)];
        b.has.insert(HasKind::Image);
        let mut c = msg(12, 200, 7, "quick secret plans", 5 * DAY);
        c.author_type = AuthorType::Bot;
        idx.apply_batch(&[
            MessageEvent::Created(a),
            MessageEvent::Created(b),
            MessageEvent::Created(c),
        ])
        .unwrap();
        idx
    }

    #[test]
    fn text_search_with_snippet_and_highlights() {
        let idx = seeded(None);
        let p = idx
            .search(&SearchRequest::new("quick", all(&[100, 200])))
            .unwrap();
        assert_eq!(p.total, 2);
        assert_eq!(ids(&p), vec![12, 10]); // newest first
        let s = &p.hits[1].snippet;
        let (a, b) = s.highlights[0];
        assert_eq!(&s.text[a..b], "quick");
        let phrase = idx
            .search(&SearchRequest::new("\"brown fox\"", all(&[100])))
            .unwrap();
        assert_eq!(ids(&phrase), vec![10]);
    }

    #[test]
    fn permission_filtering_hides_unreadable_channels() {
        let idx = seeded(None);
        let p = idx
            .search(&SearchRequest::new("quick", all(&[100])))
            .unwrap();
        assert_eq!(ids(&p), vec![10]);
        // in: cannot widen access.
        let p = idx
            .search(&SearchRequest::new("quick in:200", all(&[100])))
            .unwrap();
        assert_eq!(p.total, 0);
        let none = idx.search(&SearchRequest::new("quick", all(&[]))).unwrap();
        assert_eq!(none.total, 0);
    }

    #[test]
    fn filters() {
        let idx = seeded(None);
        let run = |q: &str| {
            ids(&idx
                .search(&SearchRequest::new(q, all(&[100, 200])))
                .unwrap())
        };
        assert_eq!(run("from:7"), vec![12, 10]);
        assert_eq!(run("mentions:7"), vec![11]);
        assert_eq!(run("has:link"), vec![10]);
        assert_eq!(run("has:image pinned:true"), vec![11]);
        assert_eq!(run("pinned:false"), vec![12, 10]);
        assert_eq!(run("authorType:bot"), vec![12]);
        assert_eq!(run("in:200"), vec![12]);
        assert_eq!(run("from:7 in:100"), vec![10]);
        // 1970-01-05 is day 4: during matches only message 11.
        assert_eq!(run("during:1970-01-05"), vec![11]);
        assert_eq!(run("before:1970-01-05"), vec![10]);
        assert_eq!(run("after:1970-01-05"), vec![12]);
        assert_eq!(run("lazy from:7"), Vec::<u64>::new());
    }

    #[test]
    fn edit_delete_and_channel_delete() {
        let idx = seeded(None);
        let mut edited = msg(10, 100, 7, "now about pandas", 3 * DAY);
        edited.pinned = true;
        idx.apply(&MessageEvent::Updated(edited)).unwrap();
        let run = |q: &str| {
            ids(&idx
                .search(&SearchRequest::new(q, all(&[100, 200])))
                .unwrap())
        };
        assert_eq!(run("brown"), Vec::<u64>::new());
        assert_eq!(run("pandas pinned:true"), vec![10]);
        idx.apply(&MessageEvent::Deleted(MessageId::new(10)))
            .unwrap();
        assert_eq!(run("pandas"), Vec::<u64>::new());
        idx.apply(&MessageEvent::ChannelDeleted(ChannelId::new(200)))
            .unwrap();
        assert_eq!(run("quick"), Vec::<u64>::new());
        assert_eq!(idx.num_docs(), 1);
    }

    #[test]
    fn deleting_a_guild_purges_its_messages_only() {
        let idx = MessageIndex::in_ram().unwrap();
        let mut other = msg(2, 300, 1, "kept words", 2000);
        other.guild_id = Some(GuildId::new(2));
        let mut dm = msg(3, 400, 1, "dm words", 3000);
        dm.guild_id = None;
        idx.apply_batch(&[
            MessageEvent::Created(msg(1, 100, 1, "gone words", 1000)),
            MessageEvent::Created(other),
            MessageEvent::Created(dm),
        ])
        .unwrap();
        assert_eq!(idx.num_docs(), 3);
        idx.apply(&MessageEvent::GuildDeleted(GuildId::new(1)))
            .unwrap();
        assert_eq!(idx.num_docs(), 2);
    }

    #[test]
    fn paging_and_sorting() {
        let idx = MessageIndex::in_ram().unwrap();
        let events: Vec<_> = (1..=7)
            .map(|i| MessageEvent::Created(msg(i, 100, 1, "hello world", i as i64 * 1000)))
            .collect();
        idx.apply_batch(&events).unwrap();
        let mut req = SearchRequest::new("hello", all(&[100]));
        req.limit = 3;
        let p1 = idx.search(&req).unwrap();
        assert_eq!((p1.total, ids(&p1)), (7, vec![7, 6, 5]));
        req.offset = 3;
        assert_eq!(ids(&idx.search(&req).unwrap()), vec![4, 3, 2]);
        req.offset = 6;
        assert_eq!(ids(&idx.search(&req).unwrap()), vec![1]);
        req.offset = 0;
        req.sort = Sort::Oldest;
        assert_eq!(ids(&idx.search(&req).unwrap()), vec![1, 2, 3]);
    }

    #[test]
    fn relevance_prefers_denser_matches() {
        let idx = MessageIndex::in_ram().unwrap();
        idx.apply_batch(&[
            MessageEvent::Created(msg(1, 100, 1, "cat dog bird fish horse", 1000)),
            MessageEvent::Created(msg(2, 100, 1, "cat cat cat", 2000)),
        ])
        .unwrap();
        let mut req = SearchRequest::new("cat", all(&[100]));
        req.sort = Sort::Relevance;
        assert_eq!(ids(&idx.search(&req).unwrap())[0], 2);
    }

    #[test]
    fn filter_only_search_and_bad_filter() {
        let idx = seeded(None);
        let p = idx
            .search(&SearchRequest::new("has:link", all(&[100])))
            .unwrap();
        assert_eq!(p.hits[0].snippet.text, "the quick brown fox");
        assert!(p.hits[0].snippet.highlights.is_empty());
        assert!(
            idx.search(&SearchRequest::new("from:nobody", all(&[100])))
                .is_err()
        );
    }

    #[test]
    fn persists_in_tempdir_and_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _ = seeded(Some(dir.path()));
        }
        let idx = MessageIndex::open_in_dir(dir.path()).unwrap();
        assert_eq!(idx.num_docs(), 3);
        idx.rebuild([msg(99, 100, 1, "fresh start", DAY)]).unwrap();
        assert_eq!(idx.num_docs(), 1);
        let p = idx
            .search(&SearchRequest::new("fresh", all(&[100])))
            .unwrap();
        assert_eq!(ids(&p), vec![99]);
    }
}
