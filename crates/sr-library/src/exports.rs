//! Chapter file export for Patrons (LIB-RDR-016): a CBZ of the converted pages for an image
//! chapter, the kept original for a PDF or EPUB chapter. The entitlement and the daily limit are
//! checked by the caller (the API layer, D-075); this module builds and caches the file.
//!
//! The CBZ is built in the request (pages stored uncompressed: they are WebP already) and cached
//! under `library/exports/{chapter}/{fingerprint}.cbz`; the fingerprint changes whenever the page
//! list does, so a replaced page produces a new file. Old exports are removed by the storage
//! lifecycle of the `library/exports/` prefix.

use std::io::{Cursor, Write};

use serde::Serialize;
use utoipa::ToSchema;

use crate::error::{LibraryError, LibraryResult};
use crate::model::*;
use crate::service::{LibraryService, Viewer};

/// The most pages one CBZ holds.
pub const MAX_EXPORT_PAGES: usize = 600;
/// The most bytes one CBZ holds (it is assembled in memory).
pub const MAX_EXPORT_BYTES: u64 = 200 * 1024 * 1024;

/// A file ready to download.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = LibraryChapterExport)]
pub struct ChapterExport {
    /// `cbz`, `pdf` or `epub`.
    pub format: String,
    /// The suggested file name.
    pub filename: String,
    /// A signed link, valid for a few hours.
    pub url: String,
    pub size_bytes: u64,
    pub expires_at: u64,
}

fn fingerprint(keys: &[&str]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for k in keys {
        for b in k.bytes().chain(std::iter::once(0)) {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// A file-system safe name from a title: letters and digits kept, the rest collapsed to `-`.
pub fn safe_name(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let t = out.trim_end_matches('-');
    let t: String = t.chars().take(80).collect();
    if t.is_empty() { "chapter".into() } else { t }
}

fn number_text(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

impl LibraryService {
    /// The downloadable file of a chapter the viewer may open (published, age confirmed).
    pub async fn export_chapter(&self, v: Viewer, chapter: i64) -> LibraryResult<ChapterExport> {
        let c = self
            .repo
            .get_chapter(chapter, Some(v.user_id))
            .await?
            .filter(|c| Self::can_see(c, Scope::Public))
            .ok_or(LibraryError::NotFound("chapter"))?;
        if c.series_age_restricted && !v.adult {
            return Err(LibraryError::AgeConfirmationRequired);
        }
        let storage = self
            .ingest
            .as_ref()
            .map(|i| i.storage.clone())
            .ok_or_else(|| LibraryError::Conflict("downloads are not configured".into()))?;
        let title = self
            .repo
            .series_by_ids(&[c.series_id], Scope::Public)
            .await?
            .into_iter()
            .next()
            .map(|s| s.title)
            .unwrap_or_default();
        let stem = format!("{}-ch-{}", safe_name(&title), number_text(c.number));
        let exp = self.expires_at();
        match c.format {
            ChapterFormat::Pdf | ChapterFormat::Epub => {
                let key = c
                    .source_key
                    .ok_or(LibraryError::Conflict("this chapter has no file".into()))?;
                let info = storage
                    .head(&key)
                    .await
                    .map_err(|e| LibraryError::Storage(e.to_string()))?;
                let ext = if c.format == ChapterFormat::Pdf {
                    "pdf"
                } else {
                    "epub"
                };
                Ok(ChapterExport {
                    format: ext.into(),
                    filename: format!("{stem}.{ext}"),
                    url: self.url(&key, exp),
                    size_bytes: info.size,
                    expires_at: exp,
                })
            }
            ChapterFormat::Images => {
                let pages = self.repo.list_pages(chapter).await?;
                if pages.is_empty() {
                    return Err(LibraryError::Conflict("this chapter has no pages".into()));
                }
                if pages.len() > MAX_EXPORT_PAGES {
                    return Err(LibraryError::Conflict(format!(
                        "this chapter has more than {MAX_EXPORT_PAGES} pages"
                    )));
                }
                let keys: Vec<&str> = pages.iter().map(|p| p.key.as_str()).collect();
                let export_key = format!("library/exports/{chapter}/{}.cbz", fingerprint(&keys));
                let size = match storage.head(&export_key).await {
                    Ok(info) => info.size,
                    Err(_) => {
                        let bytes = self.build_cbz(&*storage, &pages).await?;
                        let size = bytes.len() as u64;
                        storage
                            .put(&export_key, bytes.into())
                            .await
                            .map_err(|e| LibraryError::Storage(e.to_string()))?;
                        size
                    }
                };
                Ok(ChapterExport {
                    format: "cbz".into(),
                    filename: format!("{stem}.cbz"),
                    url: self.url(&export_key, exp),
                    size_bytes: size,
                    expires_at: exp,
                })
            }
        }
    }

    async fn build_cbz(
        &self,
        storage: &dyn sr_storage::ObjectStorage,
        pages: &[PageRecord],
    ) -> LibraryResult<Vec<u8>> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let width = pages.len().to_string().len().max(3);
        let mut total = 0_u64;
        for p in pages {
            let data = storage
                .get(&p.key)
                .await
                .map_err(|e| LibraryError::Storage(e.to_string()))?;
            total += data.len() as u64;
            if total > MAX_EXPORT_BYTES {
                return Err(LibraryError::Conflict(
                    "this chapter is too large to export".into(),
                ));
            }
            let ext = p
                .key
                .rsplit('.')
                .next()
                .filter(|e| e.len() <= 4)
                .unwrap_or("webp");
            zip.start_file(format!("{:0width$}.{ext}", p.index + 1), opts)
                .map_err(|e| LibraryError::Storage(e.to_string()))?;
            zip.write_all(&data)
                .map_err(|e| LibraryError::Storage(e.to_string()))?;
        }
        let cursor = zip
            .finish()
            .map_err(|e| LibraryError::Storage(e.to_string()))?;
        Ok(cursor.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_file_safe() {
        assert_eq!(
            safe_name("Frieren: Beyond Journey's End"),
            "Frieren-Beyond-Journey-s-End"
        );
        assert_eq!(safe_name("///"), "chapter");
        assert_eq!(number_text(36.0), "36");
        assert_eq!(number_text(36.5), "36.5");
    }

    #[test]
    fn fingerprints_follow_the_page_list() {
        assert_eq!(fingerprint(&["a", "b"]), fingerprint(&["a", "b"]));
        assert_ne!(fingerprint(&["a", "b"]), fingerprint(&["b", "a"]));
        assert_ne!(fingerprint(&["ab"]), fingerprint(&["a", "b"]));
    }
}
