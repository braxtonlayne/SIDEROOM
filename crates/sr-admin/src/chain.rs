//! The hash chain of `staff_access_log` (doc 07 section 5, doc 09):
//! `hash = sha256(prev_hash || canonical(row))`, starting from 32 zero bytes.
//!
//! The canonical form length-prefixes every field, so no two different rows
//! share an encoding. Verification recomputes every hash in sequence order and
//! also requires the sequence numbers to be contiguous from 1, so a deleted or
//! reordered row breaks the chain. Truncating the newest rows cannot be seen
//! from the table alone: compare `head_hash` with a copy kept elsewhere.

use crate::model::{AccessLogEntry, AuditEntry, ChainReport};
use sha2::{Digest, Sha256};

/// `prev_hash` of the first row.
pub const GENESIS: [u8; 32] = [0u8; 32];

fn field(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(value.len().to_string().as_bytes());
    out.push(b':');
    out.extend_from_slice(value.as_bytes());
    out.push(b';');
}

fn opt_field(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(v) => {
            out.push(b'+');
            field(out, v);
        }
        None => out.extend_from_slice(b"-;"),
    }
}

/// Canonical bytes of a row (everything except the two hashes).
pub fn canonical(seq: i64, at_us: i64, e: &AuditEntry) -> Vec<u8> {
    let mut out = Vec::with_capacity(128);
    field(&mut out, &seq.to_string());
    field(&mut out, &at_us.to_string());
    field(&mut out, &e.actor_id.to_string());
    field(&mut out, e.kind.as_str());
    field(&mut out, &e.action);
    opt_field(&mut out, e.target_kind.as_deref());
    opt_field(&mut out, e.target_id.map(|t| t.to_string()).as_deref());
    field(&mut out, &e.reason);
    opt_field(&mut out, e.ip.as_deref());
    field(&mut out, &e.detail);
    out
}

/// `sha256(prev || canonical)`.
pub fn row_hash(prev: &[u8; 32], canonical: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prev);
    h.update(canonical);
    h.finalize().into()
}

pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

pub fn from_hex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// Build a stored row for `entry` chained after `prev`.
pub fn seal(prev: &[u8; 32], seq: i64, at_us: i64, e: &AuditEntry) -> AccessLogEntry {
    let hash = row_hash(prev, &canonical(seq, at_us, e));
    AccessLogEntry {
        seq,
        at_us,
        actor_id: e.actor_id,
        kind: e.kind,
        action: e.action.clone(),
        target_kind: e.target_kind.clone(),
        target_id: e.target_id,
        reason: e.reason.clone(),
        ip: e.ip.clone(),
        detail: e.detail.clone(),
        prev_hash: to_hex(prev),
        hash: to_hex(&hash),
    }
}

fn as_entry(r: &AccessLogEntry) -> AuditEntry {
    AuditEntry {
        actor_id: r.actor_id,
        kind: r.kind,
        action: r.action.clone(),
        target_kind: r.target_kind.clone(),
        target_id: r.target_id,
        reason: r.reason.clone(),
        ip: r.ip.clone(),
        detail: r.detail.clone(),
    }
}

/// Incremental verifier: feed rows in ascending `seq` order.
#[derive(Debug)]
pub struct ChainVerifier {
    expected_prev: [u8; 32],
    next_seq: i64,
    rows: i64,
    head: Option<String>,
    bad: Option<(i64, String)>,
}

impl Default for ChainVerifier {
    fn default() -> Self {
        Self {
            expected_prev: GENESIS,
            next_seq: 1,
            rows: 0,
            head: None,
            bad: None,
        }
    }
}

impl ChainVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once a problem was found; further rows are ignored.
    pub fn failed(&self) -> bool {
        self.bad.is_some()
    }

    pub fn feed(&mut self, r: &AccessLogEntry) {
        if self.bad.is_some() {
            return;
        }
        self.rows += 1;
        if r.seq != self.next_seq {
            self.bad = Some((
                r.seq,
                format!(
                    "sequence gap: expected {}, found {} (a row was removed or reordered)",
                    self.next_seq, r.seq
                ),
            ));
            return;
        }
        if from_hex(&r.prev_hash) != Some(self.expected_prev) {
            self.bad = Some((
                r.seq,
                "prev_hash does not match the previous row".to_owned(),
            ));
            return;
        }
        let want = to_hex(&row_hash(
            &self.expected_prev,
            &canonical(r.seq, r.at_us, &as_entry(r)),
        ));
        if want != r.hash {
            self.bad = Some((r.seq, "row contents do not match its hash".to_owned()));
            return;
        }
        self.expected_prev = from_hex(&r.hash).unwrap_or(GENESIS);
        self.next_seq = r.seq + 1;
        self.head = Some(r.hash.clone());
    }

    pub fn finish(self) -> ChainReport {
        match self.bad {
            None => ChainReport {
                ok: true,
                rows_checked: self.rows,
                first_bad_seq: None,
                problem: None,
                head_hash: self.head,
            },
            Some((seq, problem)) => ChainReport {
                ok: false,
                rows_checked: self.rows,
                first_bad_seq: Some(seq),
                problem: Some(problem),
                head_hash: self.head,
            },
        }
    }
}

/// Verify a complete, ordered slice of rows.
pub fn verify_rows(rows: &[AccessLogEntry]) -> ChainReport {
    let mut v = ChainVerifier::new();
    for r in rows {
        v.feed(r);
    }
    v.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AccessKind;

    fn entry(n: i64) -> AuditEntry {
        AuditEntry {
            actor_id: n,
            kind: AccessKind::AdminAction,
            action: format!("test.{n}"),
            target_kind: Some("account".into()),
            target_id: Some(n * 2),
            reason: "because".into(),
            ip: None,
            detail: "{}".into(),
        }
    }

    fn build(n: i64) -> Vec<AccessLogEntry> {
        let mut prev = GENESIS;
        (1..=n)
            .map(|i| {
                let row = seal(&prev, i, 1_000 * i, &entry(i));
                prev = from_hex(&row.hash).unwrap();
                row
            })
            .collect()
    }

    #[test]
    fn intact_chain_verifies() {
        let r = verify_rows(&build(5));
        assert!(r.ok);
        assert_eq!(r.rows_checked, 5);
        assert!(r.head_hash.is_some());
        assert!(verify_rows(&[]).ok);
    }

    #[test]
    fn edited_row_is_detected() {
        let mut rows = build(5);
        rows[2].reason = "edited".into();
        let r = verify_rows(&rows);
        assert!(!r.ok);
        assert_eq!(r.first_bad_seq, Some(3));
    }

    #[test]
    fn removed_and_reordered_rows_are_detected() {
        let mut rows = build(5);
        rows.remove(1);
        assert_eq!(verify_rows(&rows).first_bad_seq, Some(3));
        let mut rows = build(5);
        rows.swap(1, 2);
        assert!(!verify_rows(&rows).ok);
    }

    #[test]
    fn canonical_form_is_unambiguous() {
        let mut a = entry(1);
        let mut b = entry(1);
        a.action = "ab".into();
        a.reason = "c".into();
        b.action = "a".into();
        b.reason = "bc".into();
        assert_ne!(canonical(1, 1, &a), canonical(1, 1, &b));
        let mut c = entry(1);
        let mut d = entry(1);
        c.target_kind = None;
        d.target_kind = Some(String::new());
        assert_ne!(canonical(1, 1, &c), canonical(1, 1, &d));
    }

    #[test]
    fn hex_round_trip() {
        let h = row_hash(&GENESIS, b"x");
        assert_eq!(from_hex(&to_hex(&h)), Some(h));
        assert_eq!(from_hex("zz"), None);
    }
}
