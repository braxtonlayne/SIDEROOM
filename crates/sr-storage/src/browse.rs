//! Folder-shaped listing and object metadata for the bucket browser (BYOS-SCP-002), and the key check every
//! route runs before a key reaches a presign or an S3 call.

use futures::TryStreamExt;
use object_store::ObjectStoreExt;
use object_store::path::Path;

use crate::error::{Result, StorageError};
use crate::storage::ObjectStoreStorage;

/// Page size of a listing.
pub const PAGE: usize = 1000;
/// Longest key in bytes.
pub use crate::path::MAX_KEY_BYTES;

/// An object in a listing page. Keys are bucket keys; callers strip the mount prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirObject {
    /// Bucket key.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// Last modified, unix ms.
    pub last_modified_ms: i64,
}

/// One page of a listing: sub-folders (full prefixes ending in `/`) and objects, in key order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirPage {
    /// Sub-folder prefixes.
    pub folders: Vec<String>,
    /// Objects.
    pub objects: Vec<DirObject>,
    /// Opaque cursor for the next page, `None` at the end.
    pub next_cursor: Option<String>,
}

/// Metadata of one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMetaInfo {
    /// Size in bytes.
    pub size: u64,
    /// Last modified, unix ms.
    pub last_modified_ms: i64,
    /// Entity tag.
    pub etag: Option<String>,
}

/// The key check is `sr_storage::path::safe_key` (one rule set for every key from user input; NFC
/// normalisation is still missing, it needs the `unicode-normalization` crate this wave may not add).
pub use crate::path::{KeyError, safe_key};

fn dir_object(m: &object_store::ObjectMeta) -> DirObject {
    DirObject {
        key: m.location.to_string(),
        size: m.size,
        last_modified_ms: m.last_modified.timestamp_millis(),
    }
}

impl ObjectStoreStorage {
    /// List one level (`delimiter = true`) or everything below `prefix` (`false`) in pages of `limit`.
    /// `prefix` is a bucket prefix: empty or ending in `/`. The cursor is the last name of the previous page.
    /// The backend walks the whole folder per call (S3 pages it internally); folders beyond a few tens of
    /// thousands of entries are slow, which the beta accepts.
    pub async fn list_dir(
        &self,
        prefix: &str,
        delimiter: bool,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<DirPage> {
        let path = if prefix.is_empty() {
            None
        } else {
            Some(Path::parse(prefix)?)
        };
        // (sort name, object); folders carry no object.
        let mut items: Vec<(String, Option<DirObject>)> = Vec::new();
        if delimiter {
            let r = self.store.list_with_delimiter(path.as_ref()).await?;
            for p in r.common_prefixes {
                items.push((format!("{}/", p.as_ref()), None));
            }
            for m in &r.objects {
                items.push((m.location.to_string(), Some(dir_object(m))));
            }
        } else {
            let metas: Vec<_> = self.store.list(path.as_ref()).try_collect().await?;
            for m in &metas {
                items.push((m.location.to_string(), Some(dir_object(m))));
            }
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let mut page = DirPage::default();
        let mut taken = 0usize;
        let mut last = None;
        for (name, obj) in items {
            if cursor.is_some_and(|c| name.as_str() <= c) {
                continue;
            }
            if taken == limit {
                page.next_cursor = last;
                return Ok(page);
            }
            taken += 1;
            last = Some(name.clone());
            match obj {
                Some(o) => page.objects.push(o),
                None => page.folders.push(name),
            }
        }
        Ok(page)
    }

    /// Size, modification time and etag of one object.
    pub async fn head_meta(&self, key: &str) -> Result<ObjectMetaInfo> {
        let path = Path::parse(key)?;
        let m = self.store.head(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StorageError::NotFound(key.to_owned()),
            other => StorageError::from(other),
        })?;
        Ok(ObjectMetaInfo {
            size: m.size,
            last_modified_ms: m.last_modified.timestamp_millis(),
            etag: m.e_tag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    use crate::storage::ObjectStorage;

    #[test]
    fn keys_refuse_every_forbidden_shape() {
        assert_eq!(safe_key("", false), Err(KeyError::Empty));
        assert_eq!(safe_key("/a", false), Err(KeyError::LeadingSlash));
        assert_eq!(safe_key("../a", false), Err(KeyError::DotSegment));
        assert_eq!(safe_key("a/../b", false), Err(KeyError::DotSegment));
        assert_eq!(safe_key("a/./b", false), Err(KeyError::DotSegment));
        assert_eq!(safe_key("a//b", false), Err(KeyError::EmptySegment));
        assert_eq!(safe_key("a/", false), Err(KeyError::EmptySegment));
        assert_eq!(safe_key("a\\b", false), Err(KeyError::Control));
        assert_eq!(safe_key("a\u{0}b", false), Err(KeyError::Control));
        assert_eq!(safe_key("a\nb", false), Err(KeyError::Control));
        assert_eq!(safe_key("a\u{7f}", false), Err(KeyError::Control));
        assert_eq!(safe_key("a\u{85}", false), Err(KeyError::Control));
        assert_eq!(safe_key(&"a".repeat(1025), false), Err(KeyError::TooLong));
        assert!(safe_key(&"a".repeat(1024), false).is_ok());
        assert!(safe_key("Musik/Björk/Jóga.flac", false).is_ok());
        assert!(safe_key("a/b/", true).is_ok());
        assert_eq!(safe_key("a//", true), Err(KeyError::EmptySegment));
        assert_eq!(safe_key("../", true), Err(KeyError::DotSegment));
    }

    #[test]
    fn keys_fuzz_never_admits_a_traversal() {
        // Every combination of a small alphabet: whatever is admitted has no dot segment, no leading slash,
        // no empty segment and no control character.
        let alphabet = ['.', '/', 'a', '\\', '\u{0}', ' '];
        let mut seen = 0u32;
        for len in 1..=6u32 {
            for n in 0..alphabet.len().pow(len) {
                let mut k = String::new();
                let mut x = n;
                for _ in 0..len {
                    k.push(alphabet[x % alphabet.len()]);
                    x /= alphabet.len();
                }
                if safe_key(&k, true).is_ok() {
                    seen += 1;
                    assert!(!k.starts_with('/'), "{k:?}");
                    assert!(k.split('/').all(|s| s != ".." && s != "."), "{k:?}");
                    assert!(!k.contains("//"), "{k:?}");
                    assert!(!k.chars().any(|c| c.is_control() || c == '\\'), "{k:?}");
                }
            }
        }
        assert!(seen > 0);
    }

    async fn fixture() -> ObjectStoreStorage {
        let s = ObjectStoreStorage::in_memory();
        for k in [
            "m/a.mp3",
            "m/b.mkv",
            "m/albums/x/1.flac",
            "m/albums/y.flac",
            "m/zeta/z.txt",
            "other/q.mp3",
        ] {
            s.put(k, Bytes::from_static(b"12345")).await.unwrap();
        }
        s
    }

    #[tokio::test]
    async fn one_level_shows_folders_and_objects_in_order() {
        let s = fixture().await;
        let p = s.list_dir("m/", true, None, 1000).await.unwrap();
        assert_eq!(p.folders, vec!["m/albums/", "m/zeta/"]);
        assert_eq!(
            p.objects.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            vec!["m/a.mp3", "m/b.mkv"]
        );
        assert_eq!(p.objects[0].size, 5);
        assert!(p.objects[0].last_modified_ms > 0);
        assert_eq!(p.next_cursor, None);
        let sub = s.list_dir("m/albums/", true, None, 1000).await.unwrap();
        assert_eq!(sub.folders, vec!["m/albums/x/"]);
        assert_eq!(sub.objects.len(), 1);
        let root = s.list_dir("", true, None, 1000).await.unwrap();
        assert_eq!(root.folders, vec!["m/", "other/"]);
    }

    #[tokio::test]
    async fn pages_follow_the_cursor_and_cover_everything_once() {
        let s = fixture().await;
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0;
        loop {
            let p = s.list_dir("m/", false, cursor.as_deref(), 2).await.unwrap();
            pages += 1;
            seen.extend(p.objects.iter().map(|o| o.key.clone()));
            match p.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(pages, 3);
        assert_eq!(
            seen,
            vec![
                "m/a.mp3",
                "m/albums/x/1.flac",
                "m/albums/y.flac",
                "m/b.mkv",
                "m/zeta/z.txt"
            ]
        );
    }

    #[tokio::test]
    async fn head_meta_and_missing() {
        let s = fixture().await;
        let m = s.head_meta("m/a.mp3").await.unwrap();
        assert_eq!(m.size, 5);
        assert!(matches!(
            s.head_meta("m/none").await,
            Err(StorageError::NotFound(_))
        ));
    }
}
