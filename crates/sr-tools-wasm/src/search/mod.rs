//! Three-stage search for the utility drawer (UTL-SRC-001), in the browser with no model.
//!
//! 1. **Prefix**: the query is an exact name, a name prefix or the prefix of a word of the name or of an
//!    alias.
//! 2. **Fuzzy**: edit distance (Damerau-Levenshtein) and Jaro-Winkler from `strsim` over the names, with
//!    a "Did you mean" suggestion when nothing matched in stage 1 and the best fuzzy hit is close.
//! 3. **BM25** over the build-time corpus of task phrases per tool (`apps/web/src/tools/corpus.json`,
//!    embedded below), so "make my image smaller" finds the image studio. Tools only.
//!
//! Channels and people come from the client's cache (names and aliases only) and take part in
//! stages 1 and 2. A leading `>`, `#` or `@` limits the search to tools, channels or people.

mod bm25;
pub mod norm;

use bm25::Bm25;
use serde_json::{Value, json};
use std::collections::HashSet;
use strsim::{damerau_levenshtein, jaro_winkler};
use wasm_bindgen::prelude::*;

/// The task-phrase corpus, one entry per tool (checked by `scripts/check-tools-corpus.mjs`).
pub const CORPUS_JSON: &str = include_str!("../../../../apps/web/src/tools/corpus.json");

pub const KIND_TOOL: &str = "tool";

/// One searchable thing: a tool, a channel or a person.
#[derive(Debug, Clone)]
struct Entry {
    kind: String,
    id: String,
    label: String,
    /// Normalised label first, then the normalised aliases.
    names: Vec<String>,
}

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub kind: String,
    pub id: String,
    pub label: String,
    /// 1 prefix, 2 fuzzy, 3 BM25.
    pub stage: u8,
    pub score: f64,
}

/// The results of one query.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Results {
    pub hits: Vec<Hit>,
    /// Set when stage 1 found nothing and the best fuzzy hit is close: "Did you mean ...?".
    pub did_you_mean: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusError(pub String);

impl std::fmt::Display for CorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub struct SearchIndex {
    tools: Vec<Entry>,
    entities: Vec<Entry>,
    bm25: Bm25,
}

fn entry(kind: &str, id: &str, label: &str, aliases: impl Iterator<Item = String>) -> Entry {
    let mut names = vec![norm::normalize(label)];
    names.extend(aliases.map(|a| norm::normalize(&a)));
    names.retain(|n| !n.is_empty());
    names.dedup();
    Entry {
        kind: kind.to_owned(),
        id: id.to_owned(),
        label: label.to_owned(),
        names,
    }
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

impl SearchIndex {
    /// Build from a corpus document `{"tools":[{"id","name","aliases":[],"phrases":[]}]}`.
    pub fn from_corpus(json: &str) -> Result<Self, CorpusError> {
        let v: Value = serde_json::from_str(json).map_err(|e| CorpusError(e.to_string()))?;
        let list = v
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| CorpusError("corpus has no \"tools\" array".into()))?;
        let mut tools = Vec::with_capacity(list.len());
        let mut docs = Vec::with_capacity(list.len());
        for t in list {
            let id = t.get("id").and_then(Value::as_str).unwrap_or_default();
            let name = t.get("name").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() || name.is_empty() {
                return Err(CorpusError("a tool needs an id and a name".into()));
            }
            let aliases = strings(t, "aliases");
            let phrases = strings(t, "phrases");
            let mut doc = norm::tokens(name);
            for a in &aliases {
                doc.extend(norm::tokens(a));
            }
            for p in &phrases {
                doc.extend(norm::tokens(p));
            }
            docs.push(doc);
            tools.push(entry(KIND_TOOL, id, name, aliases.into_iter()));
        }
        let bm25 = Bm25::build(&docs);
        Ok(Self {
            tools,
            entities: Vec::new(),
            bm25,
        })
    }

    /// The index over the embedded corpus.
    pub fn embedded() -> Self {
        Self::from_corpus(CORPUS_JSON).expect("the embedded corpus parses (checked by the tests)")
    }

    /// Replace the channels and people: `[{"kind":"channel","id":"..","name":"..","aliases":[".."]}]`.
    pub fn set_entities(&mut self, json: &str) -> Result<usize, CorpusError> {
        let v: Value = serde_json::from_str(json).map_err(|e| CorpusError(e.to_string()))?;
        let list = v
            .as_array()
            .ok_or_else(|| CorpusError("entities must be an array".into()))?;
        self.entities = list
            .iter()
            .filter_map(|e| {
                let kind = e.get("kind")?.as_str()?;
                let id = e.get("id")?.as_str()?;
                let name = e.get("name")?.as_str()?;
                Some(entry(kind, id, name, strings(e, "aliases").into_iter()))
            })
            .filter(|e| !e.names.is_empty())
            .collect();
        Ok(self.entities.len())
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn search(&self, query: &str, limit: usize) -> Results {
        let mut q = norm::clamp(query).trim_start();
        let filter = match q.chars().next() {
            Some('>') => Some("tool"),
            Some('#') => Some("channel"),
            Some('@') => Some("person"),
            _ => None,
        };
        if filter.is_some() {
            q = &q[1..];
        }
        let nq = norm::normalize(q);
        let limit = limit.min(100);
        if nq.is_empty() || limit == 0 {
            return Results::default();
        }
        let all = || {
            self.tools
                .iter()
                .chain(self.entities.iter())
                .filter(|e| filter.is_none_or(|k| e.kind == k))
        };

        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        let mut hits: Vec<Hit> = Vec::new();
        let hit = |e: &Entry, stage: u8, score: f64| Hit {
            kind: e.kind.clone(),
            id: e.id.clone(),
            label: e.label.clone(),
            stage,
            score,
        };

        // Stage 1: prefix.
        for e in all() {
            if let Some(s) = e
                .names
                .iter()
                .filter_map(|n| prefix_score(&nq, n))
                .reduce(f64::max)
            {
                seen.insert((&e.kind, &e.id));
                hits.push(hit(e, 1, s));
            }
        }
        let stage1 = hits.len();

        // Stage 2: fuzzy.
        let mut fuzzy: Vec<Hit> = Vec::new();
        let mut best: Option<(f64, &Entry)> = None;
        for e in all().filter(|e| !seen.contains(&(e.kind.as_str(), e.id.as_str()))) {
            if let Some(s) = e
                .names
                .iter()
                .filter_map(|n| fuzzy_score(&nq, n))
                .reduce(f64::max)
            {
                if best.is_none_or(|(b, _)| s > b) {
                    best = Some((s, e));
                }
                fuzzy.push(hit(e, 2, s));
            }
        }
        let fuzzy_keys: Vec<(String, String)> = fuzzy
            .iter()
            .map(|h| (h.kind.clone(), h.id.clone()))
            .collect();
        hits.extend(fuzzy);
        let did_you_mean = if stage1 == 0 {
            best.filter(|(s, _)| *s >= 0.85)
                .map(|(_, e)| e.label.clone())
        } else {
            None
        };

        // Stage 3: BM25 over the tools' task phrases.
        if filter.is_none_or(|k| k == "tool") && nq.chars().count() >= 2 {
            let qt = norm::tokens(&nq);
            for (i, s) in self.bm25.score(&qt) {
                let e = &self.tools[i];
                let taken = seen.contains(&(e.kind.as_str(), e.id.as_str()))
                    || fuzzy_keys.iter().any(|(k, id)| *k == e.kind && *id == e.id);
                if !taken {
                    hits.push(hit(e, 3, s));
                }
            }
        }

        hits.sort_by(|a, b| {
            a.stage
                .cmp(&b.stage)
                .then(b.score.total_cmp(&a.score))
                .then(a.label.chars().count().cmp(&b.label.chars().count()))
                .then(a.label.cmp(&b.label))
                .then(a.id.cmp(&b.id))
        });
        hits.truncate(limit);
        Results { hits, did_you_mean }
    }
}

/// Stage 1 score of `q` against one normalised name: an exact match, a prefix, or the prefix of a
/// later word ("converter" finds "unit converter").
fn prefix_score(q: &str, name: &str) -> Option<f64> {
    if q == name {
        return Some(1.0);
    }
    let tightness = q.chars().count() as f64 / name.chars().count().max(1) as f64;
    if name.starts_with(q) {
        return Some(0.8 + 0.1 * tightness);
    }
    name.match_indices(' ')
        .map(|(i, _)| &name[i + 1..])
        .any(|rest| rest.starts_with(q))
        .then_some(0.6 + 0.1 * tightness)
}

/// How many edits a query of `len` characters may be off by.
fn allowed_edits(len: usize) -> usize {
    match len {
        0..=2 => 0,
        3..=7 => 1,
        _ => 2,
    }
}

/// Stage 2 score (0..1) of `q` against one normalised name, if close enough: the whole name, one of its
/// words, or the name's leading characters (a typo inside a partly typed name).
fn fuzzy_score(q: &str, name: &str) -> Option<f64> {
    let ql = q.chars().count();
    let allowed = allowed_edits(ql);
    let mut best: Option<f64> = None;
    let mut bump = |s: f64| best = Some(best.map_or(s, |b| b.max(s)));
    let mut consider = |cand: &str| {
        let cl = cand.chars().count();
        if cl.abs_diff(ql) > allowed.max(1) {
            return;
        }
        let d = damerau_levenshtein(q, cand);
        let jw = jaro_winkler(q, cand);
        if d <= allowed || (ql >= 6 && jw >= 0.94) {
            // A closer match scores higher.
            bump(jw.min(1.0 - 0.1 * d as f64));
        }
    };
    consider(name);
    if !q.contains(' ') {
        for w in name.split(' ') {
            consider(w);
        }
    }
    // The same number of leading characters: "calcualt" against "calculator".
    if ql >= 4 && name.chars().count() > ql {
        let head: String = name.chars().take(ql).collect();
        let d = damerau_levenshtein(q, &head);
        if d <= allowed {
            bump((0.9 - 0.1 * d as f64).max(0.0));
        }
    }
    best
}

// ---- wasm-bindgen surface ----

/// The browser-facing index: the embedded tool corpus plus the channels and people the client sets.
#[wasm_bindgen(js_name = ToolSearch)]
pub struct ToolSearch {
    index: SearchIndex,
}

#[wasm_bindgen(js_class = ToolSearch)]
impl ToolSearch {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ToolSearch {
        ToolSearch {
            index: SearchIndex::embedded(),
        }
    }

    /// Replace the channels and people (names and aliases only). Returns how many were indexed.
    #[wasm_bindgen(js_name = setEntities)]
    pub fn set_entities(&mut self, json: &str) -> Result<usize, JsError> {
        self.index
            .set_entities(json)
            .map_err(|e| JsError::new(&e.0))
    }

    /// `{"didYouMean":string|null,"hits":[{"kind","id","label","stage","score"}]}` as JSON.
    pub fn search(&self, query: &str, limit: usize) -> String {
        results_json(&self.index.search(query, limit))
    }
}

impl Default for ToolSearch {
    fn default() -> Self {
        Self::new()
    }
}

/// The JSON the facade parses.
pub fn results_json(r: &Results) -> String {
    json!({
        "didYouMean": r.did_you_mean,
        "hits": r.hits.iter().map(|h| json!({
            "kind": h.kind, "id": h.id, "label": h.label, "stage": h.stage, "score": h.score,
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

#[cfg(test)]
mod tests;
