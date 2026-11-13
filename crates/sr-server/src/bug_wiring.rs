//! The bug report store behind `POST /bug-reports` and the staff routes (SEC-AUD-166 and 167): `sr-api` defines the
//! trait, this adapts the Postgres table of migration 2790 to it.

use sr_api::routes::bugs::{BugPerson, BugRow, BugStore, NewBug};
use sr_db::bug_reports::{BugRow as DbRow, NewReport, Person, PgBugReports};

pub struct PgBugStore(pub PgBugReports);

fn person(p: Person) -> BugPerson {
    BugPerson {
        id: p.id,
        username: p.username,
    }
}

fn row(r: DbRow) -> BugRow {
    BugRow {
        id: r.id,
        created_ms: r.created_ms,
        status: r.status,
        route: r.route,
        commit_sha: r.commit_sha,
        reporter: person(r.reporter),
        has_screenshot: r.has_screenshot,
        description: r.description,
        bundle: r.bundle,
        closed_ms: r.closed_ms,
        closed_by: r.closed_by.map(person),
    }
}

#[async_trait::async_trait]
impl BugStore for PgBugStore {
    async fn insert(&self, new: &NewBug) -> Result<i64, String> {
        self.0
            .insert(&NewReport {
                reporter_id: new.reporter_id,
                commit_sha: &new.commit_sha,
                route: &new.route,
                description: &new.description,
                bundle: &new.bundle,
                screenshot: new.screenshot.as_deref(),
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn list(&self, status: Option<&str>, limit: i64) -> Result<(Vec<BugRow>, i64), String> {
        let (rows, open) = self
            .0
            .list(status, limit)
            .await
            .map_err(|e| e.to_string())?;
        Ok((rows.into_iter().map(row).collect(), open))
    }

    async fn get(&self, id: i64) -> Result<Option<BugRow>, String> {
        let r = self.0.get(id).await.map_err(|e| e.to_string())?;
        Ok(r.map(row))
    }

    async fn screenshot(&self, id: i64) -> Result<Option<Vec<u8>>, String> {
        self.0.screenshot(id).await.map_err(|e| e.to_string())
    }

    async fn close(&self, id: i64, by: i64) -> Result<bool, String> {
        self.0.close(id, by).await.map_err(|e| e.to_string())
    }
}
