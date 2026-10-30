//! Okapi BM25 over a small in-memory document set (the tools' task phrases).

use std::collections::HashMap;

const K1: f64 = 1.2;
const B: f64 = 0.75;

#[derive(Default)]
pub struct Bm25 {
    /// term -> (document, term frequency)
    postings: HashMap<String, Vec<(usize, u32)>>,
    lengths: Vec<u32>,
    avg_len: f64,
}

impl Bm25 {
    pub fn build(docs: &[Vec<String>]) -> Self {
        let mut postings: HashMap<String, Vec<(usize, u32)>> = HashMap::new();
        let mut lengths = Vec::with_capacity(docs.len());
        for (i, doc) in docs.iter().enumerate() {
            let mut tf: HashMap<&str, u32> = HashMap::new();
            for t in doc {
                *tf.entry(t.as_str()).or_default() += 1;
            }
            for (t, n) in tf {
                postings.entry(t.to_owned()).or_default().push((i, n));
            }
            lengths.push(doc.len() as u32);
        }
        let total: u64 = lengths.iter().map(|l| u64::from(*l)).sum();
        let avg_len = if docs.is_empty() {
            0.0
        } else {
            total as f64 / docs.len() as f64
        };
        Self {
            postings,
            lengths,
            avg_len,
        }
    }

    /// Scores of every document that matches at least one query term, best first.
    pub fn score(&self, query: &[String]) -> Vec<(usize, f64)> {
        let n = self.lengths.len() as f64;
        let mut acc: HashMap<usize, f64> = HashMap::new();
        for term in query {
            let Some(list) = self.postings.get(term) else {
                continue;
            };
            let df = list.len() as f64;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            for &(doc, tf) in list {
                let tf = f64::from(tf);
                let len = f64::from(self.lengths[doc]);
                let norm = 1.0 - B + B * len / self.avg_len.max(1.0);
                *acc.entry(doc).or_default() += idf * tf * (K1 + 1.0) / (tf + K1 * norm);
            }
        }
        let mut out: Vec<(usize, f64)> = acc.into_iter().collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }
}
