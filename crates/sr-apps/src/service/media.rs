//! Application emoji and Rich Presence art assets (DEV-PORT-003). The service owns names,
//! caps, ownership and the audit trail; the API layer decodes and stores the image bytes and
//! passes the finished object key.

use super::AppsService;
use crate::{
    error::{AppsError, AppsResult},
    model::{AppAsset, AppEmoji, MAX_APP_ASSETS, MAX_APP_EMOJIS},
};
use serde_json::json;

/// An emoji name: 2 to 32 letters, digits and underscores.
pub fn valid_emoji_name(name: &str) -> bool {
    (2..=32).contains(&name.chars().count())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// An asset name: 1 to 32 of lowercase letters, digits, `_` and `-` (the key an activity names).
pub fn valid_asset_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl AppsService {
    /// A fresh id for an emoji or asset, so the caller can build the object key first.
    pub fn new_media_id(&self) -> i64 {
        self.next_id()
    }

    /// Emoji of an owned application.
    pub async fn list_app_emojis(&self, owner: i64, app: i64) -> AppsResult<Vec<AppEmoji>> {
        let app = self.owned(owner, app).await?;
        self.repo.app_emojis(app.id).await
    }

    /// One emoji of an owned application.
    pub async fn app_emoji(&self, owner: i64, app: i64, id: i64) -> AppsResult<AppEmoji> {
        let app = self.owned(owner, app).await?;
        self.repo
            .app_emoji(app.id, id)
            .await?
            .ok_or(AppsError::NotFound("emoji"))
    }

    /// Record an emoji whose image is stored under `media_key` already.
    pub async fn add_app_emoji(
        &self,
        owner: i64,
        app: i64,
        id: i64,
        name: &str,
        animated: bool,
        size_bytes: i64,
        media_key: String,
    ) -> AppsResult<AppEmoji> {
        let app = self.owned(owner, app).await?;
        if !valid_emoji_name(name) {
            return Err(AppsError::field(
                "name",
                "STRING_TYPE_REGEX",
                "Use 2 to 32 letters, digits or underscores.",
            ));
        }
        let e = AppEmoji {
            id,
            application_id: app.id,
            name: name.to_owned(),
            animated,
            media_key,
            size_bytes,
            created_at_ms: self.now_ms(),
        };
        let audit = self.audit(
            app.id,
            owner,
            "app.emoji_added",
            json!({"emoji_id": id.to_string(), "name": name}),
        );
        self.repo.insert_app_emoji(&e, audit).await
    }

    pub async fn rename_app_emoji(
        &self,
        owner: i64,
        app: i64,
        id: i64,
        name: &str,
    ) -> AppsResult<AppEmoji> {
        let app = self.owned(owner, app).await?;
        if !valid_emoji_name(name) {
            return Err(AppsError::field(
                "name",
                "STRING_TYPE_REGEX",
                "Use 2 to 32 letters, digits or underscores.",
            ));
        }
        let audit = self.audit(
            app.id,
            owner,
            "app.emoji_renamed",
            json!({"emoji_id": id.to_string(), "name": name}),
        );
        self.repo.rename_app_emoji(app.id, id, name, audit).await
    }

    /// Delete an emoji; the returned row names the image to remove.
    pub async fn delete_app_emoji(&self, owner: i64, app: i64, id: i64) -> AppsResult<AppEmoji> {
        let app = self.owned(owner, app).await?;
        let audit = self.audit(
            app.id,
            owner,
            "app.emoji_deleted",
            json!({"emoji_id": id.to_string()}),
        );
        self.repo
            .delete_app_emoji(app.id, id, audit)
            .await?
            .ok_or(AppsError::NotFound("emoji"))
    }

    /// Rich Presence assets of an owned application.
    pub async fn list_app_assets(&self, owner: i64, app: i64) -> AppsResult<Vec<AppAsset>> {
        let app = self.owned(owner, app).await?;
        self.repo.app_assets(app.id).await
    }

    pub async fn app_asset(&self, owner: i64, app: i64, id: i64) -> AppsResult<AppAsset> {
        let app = self.owned(owner, app).await?;
        self.repo
            .app_asset(app.id, id)
            .await?
            .ok_or(AppsError::NotFound("asset"))
    }

    pub async fn add_app_asset(
        &self,
        owner: i64,
        app: i64,
        id: i64,
        name: &str,
        kind: i16,
        media_key: String,
    ) -> AppsResult<AppAsset> {
        let app = self.owned(owner, app).await?;
        if !valid_asset_name(name) {
            return Err(AppsError::field(
                "name",
                "STRING_TYPE_REGEX",
                "Use 1 to 32 lowercase letters, digits, underscores or dashes.",
            ));
        }
        if !matches!(kind, 1 | 2) {
            return Err(AppsError::field(
                "type",
                "BASE_TYPE_CHOICES",
                "Must be 1 (large image) or 2 (small image).",
            ));
        }
        let a = AppAsset {
            id,
            application_id: app.id,
            name: name.to_owned(),
            kind,
            media_key,
            created_at_ms: self.now_ms(),
        };
        let audit = self.audit(
            app.id,
            owner,
            "app.asset_added",
            json!({"asset_id": id.to_string(), "name": name}),
        );
        self.repo.insert_app_asset(&a, audit).await
    }

    pub async fn delete_app_asset(&self, owner: i64, app: i64, id: i64) -> AppsResult<AppAsset> {
        let app = self.owned(owner, app).await?;
        let audit = self.audit(
            app.id,
            owner,
            "app.asset_deleted",
            json!({"asset_id": id.to_string()}),
        );
        self.repo
            .delete_app_asset(app.id, id, audit)
            .await?
            .ok_or(AppsError::NotFound("asset"))
    }

    /// Emoji one application may own.
    pub const fn max_app_emojis() -> i64 {
        MAX_APP_EMOJIS
    }

    /// Assets one application may own.
    pub const fn max_app_assets() -> i64 {
        MAX_APP_ASSETS
    }
}

#[cfg(test)]
mod tests {
    use super::{valid_asset_name, valid_emoji_name};

    #[test]
    fn names_follow_the_rules() {
        assert!(valid_emoji_name("ok") && valid_emoji_name("Party_Time9"));
        for bad in ["a", "has space", "dash-ed", &"x".repeat(33), ""] {
            assert!(!valid_emoji_name(bad), "{bad}");
        }
        assert!(valid_asset_name("logo") && valid_asset_name("big-art_2"));
        for bad in ["", "Upper", "sp ace", &"x".repeat(33)] {
            assert!(!valid_asset_name(bad), "{bad}");
        }
    }
}
