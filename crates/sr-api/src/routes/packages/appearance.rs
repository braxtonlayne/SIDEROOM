//! Server Extensions & Themes (LNG-SSET-050, EXT-PKG-006): the server-wide theme and accent colour that every
//! member's client applies, and adding or taking away the emoji, stickers and sounds of an approved pack.
//!
//! * `GET /guilds/{id}/appearance` (any member): the theme tokens and the accent colour to apply;
//! * `PATCH /guilds/{id}/appearance` (`MANAGE_GUILD`): the accent colour;
//! * `POST /guilds/{id}/packages/{package}/import` (`MANAGE_GUILD`, and the expression permissions the item
//!   routes ask for): turn an emoji, sticker or sound pack on and copy its items into the server's own lists.
//!
//! A theme is chosen with the existing `PUT /guilds/{id}/packages/{package}`; every change announces
//! `SR_GUILD_APPEARANCE_UPDATE` so open clients read the appearance again. Taking a pack away
//! (`DELETE /guilds/{id}/packages/{package}`) removes exactly the items it added.

use super::{PackagesDeps, pkg};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        guilds::safety::MfaGate,
        messages::error::{ApiJson, MsgResult},
        settings::{
            SettingsDeps, SettingsEvent,
            expressions::{
                EMOJI_PX, Image, STICKER_PX, fit_image, publish_emojis, publish_stickers, sniff,
                store,
            },
            soundboard::publish_sounds,
        },
    },
};
use axum::{Json, extract::Path};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_chat::settings::expressions::StickerFormat;
use sr_chat::settings::repo::ExpressionRepository;
use sr_chat::settings::service::{NewEmoji, NewSound, NewSticker};
use sr_chat::settings::soundboard::inspect_sound;
use sr_core::{ApiError, ErrorCode, GuildId, UserId};
use sr_packages::{
    PackageKind,
    service::{Appearance, Enablement, PackItemData},
};
use utoipa::ToSchema;

fn gi(g: GuildId) -> i64 {
    i64::try_from(g.get()).unwrap_or(0)
}

/// Tell the members' clients to read the server appearance again.
pub(super) async fn announce(settings: &SettingsDeps, guild: GuildId) {
    settings
        .publish(SettingsEvent::AppearanceChanged {
            guild,
            payload: json!({ "guild_id": guild.to_string() }),
        })
        .await;
}

/// The theme tokens and accent colour a member's client applies in this server (any member).
#[utoipa::path(
    get, path = "/guilds/{guild_id}/appearance", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Appearance), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_appearance(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Appearance>> {
    let g = super::handlers::gid(&guild)?;
    settings
        .guild_access(UserId::new(u64::try_from(p.user_id).unwrap_or(0)), g)
        .await?;
    Ok(Json(deps.service.appearance(gi(g)).await.map_err(pkg)?))
}

/// Body of `PATCH /guilds/{id}/appearance`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = PackageAppearanceBody)]
pub struct AppearanceBody {
    /// The accent colour as `0xRRGGBB`; `null` clears it. Absent keeps it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<i32>)]
    pub accent_color: Option<Option<i32>>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Set or clear the server accent colour (`MANAGE_GUILD`).
#[utoipa::path(
    patch, path = "/guilds/{guild_id}/appearance", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = AppearanceBody,
    responses((status = 200, body = Appearance), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_appearance(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<AppearanceBody>,
) -> MsgResult<Json<Appearance>> {
    let g = super::handlers::gid(&guild)?;
    super::handlers::require_manage(&settings, p.user_id, g, Some(&gate)).await?;
    if let Some(color) = body.accent_color {
        deps.service
            .set_accent(gi(g), color, p.user_id)
            .await
            .map_err(pkg)?;
        announce(&settings, g).await;
    }
    Ok(Json(deps.service.appearance(gi(g)).await.map_err(pkg)?))
}

/// One item copied into the server.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = PackImportAdded)]
pub struct Added {
    pub name: String,
    pub id: String,
}

/// An item that was not copied, and why.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = PackImportSkipped)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = PackImportResult)]
pub struct ImportResult {
    pub enablement: Enablement,
    pub added: Vec<Added>,
    pub skipped: Vec<Skipped>,
}

/// Add an approved emoji, sticker or sound pack to the server: the pack is turned on and its items become
/// the server's own emoji, stickers or sounds (`MANAGE_GUILD` and the expression permissions). Items that do
/// not fit (a name the server already uses, no free slot, a file type the list does not take) are listed
/// under `skipped`; the rest are added. Take the pack away again with `DELETE /guilds/{id}/packages/{package}`.
#[utoipa::path(
    post, path = "/guilds/{guild_id}/packages/{package_id}/import", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake"),
           ("package_id" = String, Path, description = "Package snowflake")),
    responses((status = 200, body = ImportResult), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn import_pack(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Path((guild, package)): Path<(String, String)>,
) -> MsgResult<Json<ImportResult>> {
    let g = super::handlers::gid(&guild)?;
    let acc = super::handlers::require_manage(&settings, p.user_id, g, Some(&gate)).await?;
    let package = super::handlers::id(&package)?;
    let view = deps.service.get(package).await.map_err(pkg)?;
    if !matches!(
        view.kind,
        PackageKind::EmojiPack | PackageKind::StickerPack | PackageKind::SoundPack
    ) {
        return Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "Only emoji, sticker and sound packs are added to a server.",
        )
        .into());
    }
    let user = UserId::new(u64::try_from(p.user_id).unwrap_or(0));
    let en = deps
        .service
        .enable(
            "guild",
            gi(g),
            package,
            "all",
            serde_json::Value::Object(serde_json::Map::new()),
            p.user_id,
        )
        .await
        .map_err(pkg)?;
    let version = super::handlers::id(&en.version_id)?;
    let items = deps.service.pack_contents(version).await.map_err(pkg)?;
    let mut ctx = Ctx {
        settings: &settings,
        perms: acc.perms,
        user,
        guild: g,
        added: Vec::new(),
        skipped: Vec::new(),
    };
    let ids = match view.kind {
        PackageKind::EmojiPack => ctx.emoji(items).await,
        PackageKind::StickerPack => ctx.stickers(items).await,
        _ => ctx.sounds(items).await,
    };
    let kind = match view.kind {
        PackageKind::EmojiPack => "emoji",
        PackageKind::StickerPack => "sticker",
        _ => "sound",
    };
    let (added, skipped) = (ctx.added, ctx.skipped);
    deps.service
        .record_pack_items(gi(g), package, kind, &ids, p.user_id)
        .await
        .map_err(pkg)?;
    if !added.is_empty() {
        match view.kind {
            PackageKind::EmojiPack => publish_emojis(&settings, g).await,
            PackageKind::StickerPack => publish_stickers(&settings, g).await,
            _ => publish_sounds(&settings, g).await,
        }
    }
    Ok(Json(ImportResult {
        enablement: en,
        added,
        skipped,
    }))
}

/// Take a pack away from a server: delete the items it added, then forget the pack. What the server uploaded
/// itself, and items an owner already deleted, are left alone.
pub(super) async fn remove_pack(
    settings: &SettingsDeps,
    deps: &PackagesDeps,
    acc: &crate::routes::settings::deps::GuildAccess,
    user: UserId,
    guild: GuildId,
    package: i64,
) -> MsgResult<()> {
    let recorded = deps
        .service
        .pack_items(gi(guild), package)
        .await
        .map_err(pkg)?;
    let repo = settings.service.repo();
    let (mut emojis, mut stickers, mut sounds) = (false, false, false);
    for (kind, raw) in recorded {
        let Ok(raw) = u64::try_from(raw) else {
            continue;
        };
        match kind.as_str() {
            "emoji" => {
                let id = sr_core::EmojiId::new(raw);
                if let Ok(Some(e)) = repo.get_emoji(guild, id).await
                    && settings
                        .service
                        .delete_emoji(acc.perms, user, guild, id)
                        .await
                        .is_ok()
                {
                    let _ = settings.storage.delete(&e.media_key).await;
                    emojis = true;
                }
            }
            "sticker" => {
                let id = sr_core::StickerId::new(raw);
                if let Ok(Some(s)) = repo.get_sticker(guild, id).await
                    && settings
                        .service
                        .delete_sticker(acc.perms, user, guild, id)
                        .await
                        .is_ok()
                {
                    let _ = settings.storage.delete(&s.media_key).await;
                    stickers = true;
                }
            }
            _ => {
                let id = sr_core::SoundId::new(raw);
                if let Ok(gone) = settings
                    .service
                    .delete_sound(acc.perms, user, guild, id)
                    .await
                {
                    let _ = settings.storage.delete(&gone.media_key).await;
                    sounds = true;
                }
            }
        }
    }
    deps.service
        .forget_pack_items(gi(guild), package)
        .await
        .map_err(pkg)?;
    if emojis {
        publish_emojis(settings, guild).await;
    }
    if stickers {
        publish_stickers(settings, guild).await;
    }
    if sounds {
        publish_sounds(settings, guild).await;
    }
    Ok(())
}

/// What an import is working with.
struct Ctx<'a> {
    settings: &'a SettingsDeps,
    perms: sr_core::permissions::Permissions,
    user: UserId,
    guild: GuildId,
    added: Vec<Added>,
    skipped: Vec<Skipped>,
}

impl Ctx<'_> {
    fn skip(&mut self, name: &str, reason: impl Into<String>) {
        self.skipped.push(Skipped {
            name: name.to_owned(),
            reason: reason.into(),
        });
    }

    fn add(&mut self, name: &str, id: u64, ids: &mut Vec<i64>) {
        ids.push(i64::try_from(id).unwrap_or(0));
        self.added.push(Added {
            name: name.to_owned(),
            id: id.to_string(),
        });
    }

    async fn emoji(&mut self, items: Vec<PackItemData>) -> Vec<i64> {
        let mut ids = Vec::new();
        let taken: Vec<String> = self
            .settings
            .service
            .repo()
            .list_emoji(self.guild)
            .await
            .map(|l| l.into_iter().map(|e| e.name).collect())
            .unwrap_or_default();
        for it in items {
            if taken.contains(&it.name) {
                self.skip(&it.name, "The server already has an emoji with this name.");
                continue;
            }
            let img = match sniff(&it.bytes) {
                Some(
                    i @ (Image::Png { .. } | Image::Jpeg | Image::Gif { .. } | Image::Webp { .. }),
                ) => i,
                _ => {
                    self.skip(&it.name, "Not a PNG, JPEG, GIF or WebP image.");
                    continue;
                }
            };
            let original = it.bytes.len() as u64;
            let (data, ext) = fit_image(it.bytes, img.extension(), EMOJI_PX);
            let key = format!(
                "emoji/{}/{}.{ext}",
                self.guild,
                self.settings.service.new_id()
            );
            match self
                .settings
                .service
                .create_emoji(
                    self.perms,
                    self.user,
                    self.guild,
                    NewEmoji {
                        name: it.name.clone(),
                        animated: img.animated(),
                        size_bytes: original,
                        media_key: key.clone(),
                        role_ids: Vec::new(),
                    },
                )
                .await
            {
                Ok(e) => {
                    if store(self.settings, &key, data).await.is_err() {
                        let _ = self
                            .settings
                            .service
                            .repo()
                            .delete_emoji(self.guild, e.id)
                            .await;
                        self.skip(&it.name, "The image could not be stored.");
                    } else {
                        self.add(&it.name, e.id.get(), &mut ids);
                    }
                }
                Err(e) => self.skip(&it.name, e.to_string()),
            }
        }
        ids
    }

    async fn stickers(&mut self, items: Vec<PackItemData>) -> Vec<i64> {
        let mut ids = Vec::new();
        let taken: Vec<String> = self
            .settings
            .service
            .repo()
            .list_stickers(self.guild)
            .await
            .map(|l| l.into_iter().map(|s| s.name).collect())
            .unwrap_or_default();
        for it in items {
            if taken.contains(&it.name) {
                self.skip(&it.name, "The server already has a sticker with this name.");
                continue;
            }
            let (format, img) = match sniff(&it.bytes) {
                Some(i @ Image::Png { animated: false }) => (StickerFormat::Png, i),
                Some(i @ Image::Png { animated: true }) => (StickerFormat::Apng, i),
                Some(i @ Image::Gif { .. }) => (StickerFormat::Gif, i),
                _ => {
                    self.skip(&it.name, "Stickers are PNG, APNG or GIF files.");
                    continue;
                }
            };
            let original = it.bytes.len() as u64;
            let (data, ext): (Bytes, &str) =
                if matches!(format, StickerFormat::Png | StickerFormat::Gif) {
                    fit_image(it.bytes, img.extension(), STICKER_PX)
                } else {
                    (it.bytes, img.extension())
                };
            let key = format!(
                "sticker/{}/{}.{ext}",
                self.guild,
                self.settings.service.new_id()
            );
            match self
                .settings
                .service
                .create_sticker(
                    self.perms,
                    self.user,
                    self.guild,
                    NewSticker {
                        name: it.name.clone(),
                        description: None,
                        tags: it.name.clone(),
                        format,
                        size_bytes: original,
                        media_key: key.clone(),
                    },
                )
                .await
            {
                Ok(s) => {
                    if store(self.settings, &key, data).await.is_err() {
                        let _ = self
                            .settings
                            .service
                            .repo()
                            .delete_sticker(self.guild, s.id)
                            .await;
                        self.skip(&it.name, "The image could not be stored.");
                    } else {
                        self.add(&it.name, s.id.get(), &mut ids);
                    }
                }
                Err(e) => self.skip(&it.name, e.to_string()),
            }
        }
        ids
    }

    async fn sounds(&mut self, items: Vec<PackItemData>) -> Vec<i64> {
        let mut ids = Vec::new();
        let taken: Vec<String> = self
            .settings
            .service
            .repo()
            .list_sounds(self.guild)
            .await
            .map(|l| l.into_iter().map(|s| s.name).collect())
            .unwrap_or_default();
        for it in items {
            if taken.contains(&it.name) {
                self.skip(&it.name, "The server already has a sound with this name.");
                continue;
            }
            let Some(info) = inspect_sound(&it.bytes) else {
                self.skip(&it.name, "Not an MP3, Ogg or WAV file.");
                continue;
            };
            let key = format!(
                "sound/{}/{}.{}",
                self.guild,
                self.settings.service.new_id(),
                info.format.extension()
            );
            match self
                .settings
                .service
                .create_sound(
                    self.perms,
                    self.user,
                    self.guild,
                    NewSound {
                        name: it.name.clone(),
                        volume: 1.0,
                        emoji_id: None,
                        emoji_name: None,
                        format: info.format,
                        size_bytes: it.bytes.len() as u64,
                        duration_ms: info.duration_ms,
                        media_key: key.clone(),
                    },
                )
                .await
            {
                Ok(s) => {
                    if self.settings.storage.put(&key, it.bytes).await.is_err() {
                        let _ = self
                            .settings
                            .service
                            .repo()
                            .delete_sound(self.guild, s.id)
                            .await;
                        self.skip(&it.name, "The sound could not be stored.");
                    } else {
                        self.add(&it.name, s.id.get(), &mut ids);
                    }
                }
                Err(e) => self.skip(&it.name, e.to_string()),
            }
        }
        ids
    }
}
