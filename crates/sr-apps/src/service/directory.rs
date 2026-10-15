//! The App Directory (DEV-BOT-012): released applications with a public bot, searchable by
//! text and tag. Nothing here exposes owner-only data.

use super::AppsService;
use crate::{
    error::{AppsError, AppsResult},
    model::{Application, Command, DIRECTORY_PAGE_MAX, DirectoryEntry, ReleaseState},
};

/// Longest search text the directory takes.
pub const DIRECTORY_QUERY_MAX: usize = 100;

impl AppsService {
    /// One page of the directory. `page` counts from 0; `per_page` is clamped to
    /// `1..=`[`DIRECTORY_PAGE_MAX`]. Returns one more row than asked for, so the caller can tell
    /// whether another page exists: it trims the extra row itself.
    pub async fn directory(
        &self,
        query: Option<&str>,
        tag: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> AppsResult<Vec<DirectoryEntry>> {
        let per_page = per_page.clamp(1, DIRECTORY_PAGE_MAX);
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        if query.is_some_and(|q| q.chars().count() > DIRECTORY_QUERY_MAX) {
            return Err(AppsError::field(
                "query",
                "BASE_TYPE_MAX_LENGTH",
                "Search for at most 100 characters.",
            ));
        }
        let tag = tag.map(str::trim).filter(|t| !t.is_empty());
        self.repo
            .directory(
                query,
                tag,
                per_page + 1,
                page.max(0).saturating_mul(per_page),
            )
            .await
    }

    /// The tags in use, with counts.
    pub async fn directory_tags(&self) -> AppsResult<Vec<(String, i64)>> {
        self.repo.directory_tags().await
    }

    /// One listed application with its global commands. An application that is not released,
    /// not public or suspended is `NotFound`, like a missing id.
    pub async fn directory_app(&self, id: i64) -> AppsResult<(DirectoryEntry, Vec<Command>)> {
        let app: Application = self.application(id).await?;
        if app.release_state != ReleaseState::Released || !app.bot_public {
            return Err(AppsError::NotFound("application"));
        }
        let commands = self.repo.commands(app.id, None).await?;
        let installs = self.repo.installs(app.id).await?.len() as i64;
        Ok((
            DirectoryEntry {
                application: app,
                installs,
            },
            commands,
        ))
    }
}
