//! Keeps the library and cinema search indexes in step with a staff change of the age flag
//! (MOD-AGE-001): [`sr_moderation::ItemIndexSink`] over the two catalog services, so a series or title marked 18+
//! (or cleared) is filtered in the very next search instead of after the next rebuild at boot.

use async_trait::async_trait;
use sr_cinema::CinemaService;
use sr_library::LibraryService;
use sr_moderation::{AgeTargetKind, ItemIndexSink};

pub struct CatalogIndexSink {
    pub library: LibraryService,
    pub cinema: CinemaService,
}

#[async_trait]
impl ItemIndexSink for CatalogIndexSink {
    async fn age_changed(&self, kind: AgeTargetKind, id: i64) {
        let res = match kind {
            AgeTargetKind::LibrarySeries => self
                .library
                .reindex_one(id)
                .await
                .map_err(|e| e.to_string()),
            AgeTargetKind::CinemaTitle => {
                self.cinema.reindex_one(id).await.map_err(|e| e.to_string())
            }
            AgeTargetKind::Channel => return,
        };
        if let Err(e) = res {
            // The flag itself is already committed and the gate reads the column; the index catches up at boot.
            tracing::warn!(kind = kind.as_str(), id, error = %e, "search index refresh after an age change failed");
        }
    }
}
