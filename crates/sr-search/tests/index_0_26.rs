//! DEP-FU-001 (D-1037): an index directory written by tantivy 0.26.2 (index format 7) opens read-write under
//! 0.27, with no reindex. The fixture is the message index of the wave's seeded sweep world (4 segments, 29
//! documents, about 60 KB), copied unchanged from a server run on 0.26.2.

use std::collections::BTreeSet;
use std::path::Path;

use sr_core::{ChannelId, GuildId, MessageId, UserId};
use sr_search::{Access, MessageIndex};
use sr_search::{AuthorType, IndexedMessage, MessageEvent, SearchRequest};

/// The seeded channel the fixture's messages were posted in.
const CHANNEL: u64 = 116_774_645_906_018_307;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn hits(idx: &MessageIndex, q: &str) -> usize {
    let access = Access::Channels(vec![ChannelId::new(CHANNEL)]);
    idx.search(&SearchRequest::new(q, access)).unwrap().total
}

#[test]
fn an_index_written_by_tantivy_0_26_opens_read_write() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/index-0.26");
    copy_dir(&fixture, dir.path());

    // Read: the old segments answer without any reindex.
    let idx = MessageIndex::open_in_dir(dir.path()).unwrap();
    assert!(idx.num_docs() >= 20, "docs: {}", idx.num_docs());
    assert!(hits(&idx, "welcome") >= 1, "the seeded message is found");
    let before = idx.num_docs();

    // Write: a new message goes into the same directory, is found, and survives a reopen.
    let msg = IndexedMessage {
        id: MessageId::new(990_000_000_000_000_001),
        channel_id: ChannelId::new(CHANNEL),
        guild_id: Some(GuildId::new(1)),
        author_id: UserId::new(7),
        author_type: AuthorType::User,
        content: "zonkwrit written by the newer tantivy".to_owned(),
        has: BTreeSet::new(),
        timestamp_ms: 1_800_000_000_000,
        pinned: false,
        mention_user_ids: vec![],
    };
    idx.apply(&MessageEvent::Created(msg)).unwrap();
    assert_eq!(idx.num_docs(), before + 1);
    assert_eq!(hits(&idx, "zonkwrit"), 1);
    assert!(
        hits(&idx, "welcome") >= 1,
        "old documents still found after the write"
    );
    drop(idx);

    let again = MessageIndex::open_in_dir(dir.path()).unwrap();
    assert_eq!(again.num_docs(), before + 1);
    assert_eq!(hits(&again, "zonkwrit"), 1);
    assert!(hits(&again, "welcome") >= 1);
}
