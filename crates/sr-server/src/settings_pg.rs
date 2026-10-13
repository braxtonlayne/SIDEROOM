//! Postgres-backed pieces of the settings and search wiring.

use async_trait::async_trait;
use sr_api::routes::library::LibraryPrefs;
use sr_api::routes::search::AgeGate;
use sr_core::UserId;
use sr_db::PgPool;
use std::hash::{BuildHasher, Hasher};

/// Reads `accounts.age_confirmed_at` (open question 131). Any failure counts as
/// "not confirmed", which hides NSFW channels rather than showing them.
pub struct PgAgeGate {
    pool: PgPool,
}

impl PgAgeGate {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgeGate for PgAgeGate {
    async fn age_confirmed(&self, user: UserId) -> bool {
        let id = i64::try_from(user.get()).unwrap_or(0);
        match sqlx::query_scalar::<_, bool>(
            "SELECT age_confirmed_at IS NOT NULL FROM accounts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(v) => v.unwrap_or(false),
            Err(e) => {
                tracing::warn!(error = %e, "age lookup failed; treating the account as unconfirmed");
                false
            }
        }
    }
}

/// Reads `library.hide_age_restricted` from the user settings blob (D-184):
/// `{"library": {"hide_age_restricted": true}}`. Absent or unreadable means false.
pub struct PgLibraryPrefs {
    pool: PgPool,
}

impl PgLibraryPrefs {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LibraryPrefs for PgLibraryPrefs {
    async fn hide_age_restricted(&self, user: UserId) -> bool {
        let id = i64::try_from(user.get()).unwrap_or(0);
        match sqlx::query_scalar::<_, Option<bool>>(
            "SELECT (settings #>> '{library,hide_age_restricted}') = 'true' \
             FROM user_settings WHERE user_id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(v) => v.flatten().unwrap_or(false),
            Err(e) => {
                tracing::warn!(error = %e, "library preference lookup failed; using the default");
                false
            }
        }
    }
}

/// 64 bits from the OS random source, for invite codes and webhook tokens.
/// If the OS source fails (it practically never does) the fallback is the
/// randomly keyed hasher std seeds from the OS, so output is never constant.
pub fn secure_entropy() -> u64 {
    match sr_auth::random::bytes::<8>() {
        Ok(b) => u64::from_le_bytes(b),
        Err(e) => {
            tracing::error!(error = %e, "OS randomness failed; using the std hasher seed");
            std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_varies() {
        let a: std::collections::HashSet<u64> = (0..16).map(|_| secure_entropy()).collect();
        assert!(a.len() > 8);
    }
}
