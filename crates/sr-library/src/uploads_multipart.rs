//! Resumable multipart uploads of chapter files (REV-UP-001): the service half. The browser side
//! and the protocol are described in [`crate::multipart`].

use crate::error::{LibraryError, LibraryResult};
use crate::model::*;
use crate::multipart::{
    MultipartSlot, MultipartWiring, PART_URL_TTL, PartUrl, ReceivedPart, UploadParts, check_parts,
};
use crate::service::{Actor, LibraryService};
use crate::uploads::{IngestWiring, to_upload};

fn storage_err(e: impl std::fmt::Display) -> LibraryError {
    LibraryError::Storage(e.to_string())
}

/// Number of parts of an upload of `size` bytes cut into `part_size` parts.
fn part_count_of(size: u64, part_size: u64) -> LibraryResult<u32> {
    u32::try_from(size.div_ceil(part_size.max(1)).max(1)).map_err(|_| storage_err("too many parts"))
}

impl LibraryService {
    fn multipart_wiring(&self) -> LibraryResult<&MultipartWiring> {
        self.multipart.as_ref().ok_or_else(|| {
            LibraryError::Conflict("resumable uploads are not available on this server".into())
        })
    }

    fn part_urls(
        &self,
        m: &MultipartWiring,
        rec: &UploadRecord,
        multipart_id: &str,
        numbers: impl Iterator<Item = u32>,
    ) -> LibraryResult<Vec<PartUrl>> {
        numbers
            .map(|n| {
                m.store
                    .presign_part(&rec.object_key, multipart_id, n, PART_URL_TTL)
                    .map(|url| PartUrl {
                        part_number: n,
                        url,
                    })
                    .map_err(storage_err)
            })
            .collect()
    }

    /// Start the multipart upload behind a slot that was just recorded.
    pub(crate) async fn begin_multipart(
        &self,
        _w: &IngestWiring,
        rec: UploadRecord,
        max_bytes: u64,
    ) -> LibraryResult<UploadSlot> {
        let m = self.multipart_wiring()?;
        let size = u64::try_from(rec.size_bytes).unwrap_or(0);
        let (part_size, part_count) = m.plan(size);
        let started = async {
            let id = m
                .store
                .create(&rec.object_key, content_type_of(&rec.filename))
                .await
                .map_err(storage_err)?;
            self.repo.set_multipart(rec.id, &id, part_size).await?;
            self.part_urls(m, &rec, &id, 1..=part_count)
        }
        .await;
        match started {
            Ok(parts) => Ok(UploadSlot {
                upload: to_upload(rec),
                multipart: Some(MultipartSlot {
                    part_size,
                    part_count,
                    parts,
                }),
                upload_url: String::new(),
                upload_method: "PUT".into(),
                expires_in_secs: PART_URL_TTL.as_secs(),
                max_bytes,
            }),
            Err(e) => {
                let _ = self
                    .repo
                    .transition_upload(
                        rec.id,
                        &[UploadStatus::AwaitingUpload],
                        UploadStatus::Expired,
                    )
                    .await;
                Err(e)
            }
        }
    }

    /// Resume: which parts of a multipart upload arrived, and fresh URLs for the rest.
    pub async fn upload_parts(&self, a: Actor, id: i64) -> LibraryResult<UploadParts> {
        a.publish()?;
        let m = self.multipart_wiring()?;
        let rec = self
            .repo
            .get_upload(id)
            .await?
            .ok_or(LibraryError::NotFound("upload"))?;
        let (multipart_id, part_size) =
            self.repo.get_multipart(id).await?.ok_or_else(|| {
                LibraryError::Conflict("this upload is not a resumable one".into())
            })?;
        if rec.status != UploadStatus::AwaitingUpload {
            return Err(LibraryError::Conflict(format!(
                "this upload is {}; start a new one",
                rec.status.as_str()
            )));
        }
        let size = u64::try_from(rec.size_bytes).unwrap_or(0);
        let part_count = part_count_of(size, part_size)?;
        let stored = m
            .store
            .list_parts(&rec.object_key, &multipart_id)
            .await
            .map_err(storage_err)?;
        let want = |n: u32| {
            if n == part_count {
                size.saturating_sub(part_size * u64::from(part_count - 1))
            } else {
                part_size
            }
        };
        let received: Vec<ReceivedPart> = stored
            .iter()
            .filter(|p| p.number >= 1 && p.number <= part_count && p.size == want(p.number))
            .map(|p| ReceivedPart {
                part_number: p.number,
                size: p.size,
            })
            .collect();
        let missing = self.part_urls(
            m,
            &rec,
            &multipart_id,
            (1..=part_count).filter(|n| !received.iter().any(|r| r.part_number == *n)),
        )?;
        Ok(UploadParts {
            part_size,
            part_count,
            received,
            missing,
            expires_in_secs: PART_URL_TTL.as_secs(),
        })
    }

    /// Give up an upload that was not completed: free the parts and expire the slot.
    pub async fn abort_upload(&self, a: Actor, id: i64) -> LibraryResult<Upload> {
        a.publish()?;
        let rec = self
            .repo
            .get_upload(id)
            .await?
            .ok_or(LibraryError::NotFound("upload"))?;
        if rec.status != UploadStatus::AwaitingUpload {
            return Err(LibraryError::Conflict(format!(
                "this upload is {} and cannot be cancelled",
                rec.status.as_str()
            )));
        }
        if let Some(m) = &self.multipart
            && let Some((multipart_id, _)) = self.repo.get_multipart(id).await?
            && let Err(e) = m.store.abort(&rec.object_key, &multipart_id).await
        {
            tracing::warn!(upload = id, error = %e, "aborting a multipart upload failed");
        }
        let expired = self
            .repo
            .transition_upload(id, &[UploadStatus::AwaitingUpload], UploadStatus::Expired)
            .await?
            .ok_or_else(|| LibraryError::Conflict("the upload changed; reload it".into()))?;
        Ok(to_upload(expired))
    }

    /// Check the parts the store holds and let it assemble the object (called by
    /// `complete_upload` for a multipart slot).
    pub(crate) async fn assemble_multipart(
        &self,
        rec: &UploadRecord,
        multipart_id: &str,
        part_size: u64,
    ) -> LibraryResult<()> {
        let m = self.multipart_wiring()?;
        let size = u64::try_from(rec.size_bytes).unwrap_or(0);
        let part_count = part_count_of(size, part_size)?;
        let stored = m
            .store
            .list_parts(&rec.object_key, multipart_id)
            .await
            .map_err(storage_err)?;
        let parts =
            check_parts(&stored, size, part_size, part_count).map_err(LibraryError::Invalid)?;
        m.store
            .complete(&rec.object_key, multipart_id, &parts)
            .await
            .map_err(|e| {
                tracing::warn!(upload = rec.id, error = %e, "completing a multipart upload failed");
                LibraryError::Invalid("the storage did not accept the uploaded parts".into())
            })
    }
}

/// Content type of an upload by its extension.
fn content_type_of(filename: &str) -> &'static str {
    match filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("epub") => "application/epub+zip",
        Some("pdf") => "application/pdf",
        Some("cbz" | "zip") => "application/zip",
        _ => "application/octet-stream",
    }
}
