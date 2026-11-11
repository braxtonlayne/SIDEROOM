//! Reading an object once to fingerprint it (D-583): the sha256 of its bytes, streamed so a film never sits in
//! memory.

use futures::StreamExt;
use sha2::{Digest, Sha256};

use crate::{ObjectStorage, Result};

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Read the whole object `key` and return `(sha256 as lower-case hex, bytes read)`. Any read error fails the
/// whole call: a partial read is never a fingerprint.
pub async fn sha256_object(storage: &dyn ObjectStorage, key: &str) -> Result<(String, u64)> {
    let mut stream = storage.get_stream(key, None).await?;
    let mut h = Sha256::new();
    let mut n = 0_u64;
    while let Some(chunk) = stream.body.next().await {
        let chunk = chunk?;
        n += chunk.len() as u64;
        h.update(&chunk);
    }
    Ok((hex(&h.finalize()), n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjectStoreStorage;
    use bytes::Bytes;

    #[tokio::test]
    async fn hashes_the_bytes_and_counts_them() {
        let s = ObjectStoreStorage::in_memory();
        s.put("a", Bytes::from_static(b"abc")).await.unwrap();
        let (sha, n) = sha256_object(&s, "a").await.unwrap();
        assert_eq!(
            sha,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(n, 3);
        assert!(sha256_object(&s, "missing").await.is_err());
    }
}
