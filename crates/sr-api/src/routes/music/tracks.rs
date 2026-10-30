//! `GET|PATCH /music/tracks/{id}` and `POST /music/tracks/{id}/presign` (MUS-IDX-001).

use super::{MusicDeps, may_write, not_found, reaches};
use crate::AppState;
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, MsgResult, invalid};
use crate::routes::storage::objects::{PresignBody, PresignView};
use crate::routes::storage::shelves::{Signer, store_failed};
use crate::routes::storage::{ScopeDeps, facts_failed, parse_row, uid};
use crate::routes::{Ext, MaybeExt};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Deserializer, Serialize};
use sr_core::ErrorCode;
use sr_storage::index::audio::{
    MAX_TEXT, TagSource, TagWrite, TagsState, Track, clean_genres, clean_text, cover_hash_of,
};
use sr_storage::index::shelf::{PatchError, Reach};
use sr_storage::scopes::ScopeFacts;
use utoipa::ToSchema;

/// One track.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrackView {
    /// Row snowflake.
    pub id: String,
    /// The mount it lives on (snowflake).
    pub mount_id: String,
    /// Mount-relative key of the audio file.
    pub key: String,
    /// Size in bytes.
    pub size: i64,
    /// Container (`mp3`, `flac`, `ogg`, `m4a`, `wav`, ...).
    pub container: String,
    /// Plays in a browser as is.
    pub web_native: bool,
    /// Title.
    pub title: String,
    /// Artist.
    pub artist: String,
    /// Album.
    pub album: String,
    /// Album artist.
    pub album_artist: String,
    /// Track number.
    pub track_no: Option<i32>,
    /// Disc number.
    pub disc_no: Option<i32>,
    /// Year.
    pub year: Option<i16>,
    /// Length in milliseconds, once a tag read saw it.
    pub duration_ms: Option<i32>,
    /// Genres.
    pub genres: Vec<String>,
    /// Cover key on the same mount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_key: Option<String>,
    /// Short-lived URL of the cover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// `pending`, `read`, `none` or `failed`.
    pub tags_state: String,
    /// The caller may write tags (owner, or `upload` on a server's shelf).
    pub can_edit: bool,
}

pub(crate) async fn view_of(t: Track, can_edit: bool, signer: &mut Signer<'_>) -> TrackView {
    let cover_url = match &t.cover_key {
        Some(k) => signer.url(t.mount_id, k).await,
        None => None,
    };
    TrackView {
        id: t.id.to_string(),
        mount_id: t.mount_id.to_string(),
        key: t.key,
        size: t.size,
        container: t.container,
        web_native: t.web_native,
        title: t.title,
        artist: t.artist,
        album: t.album,
        album_artist: t.album_artist,
        track_no: t.track_no,
        disc_no: t.disc_no,
        year: t.year,
        duration_ms: t.duration_ms,
        genres: t.genres,
        cover_key: t.cover_key,
        cover_url,
        tags_state: t.tags_state,
        can_edit,
    }
}

fn de_opt<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Body of the tag write. Every field present is written.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TrackPatchBody {
    /// Title (1 to 300 characters).
    pub title: Option<String>,
    /// Artist.
    pub artist: Option<String>,
    /// Album.
    pub album: Option<String>,
    /// Album artist.
    pub album_artist: Option<String>,
    /// Track number (0 to 9999); `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<i32>)]
    pub track_no: Option<Option<i32>>,
    /// Disc number (0 to 999); `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<i32>)]
    pub disc_no: Option<Option<i32>>,
    /// Year (1000 to 2200); `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<i16>)]
    pub year: Option<Option<i16>>,
    /// Length in milliseconds; `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<i32>)]
    pub duration_ms: Option<Option<i32>>,
    /// Genres (at most 10).
    pub genres: Option<Vec<String>>,
    /// Cover: exactly `covers/<hash>.jpg` on the same mount; `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<String>)]
    pub cover_key: Option<Option<String>>,
    /// `file` (the lazy tagger: values read from the file, never replacing a field a member edited) or `edit`
    /// (default: a member's edit, which a rescan then keeps).
    pub source: Option<String>,
    /// `read`, `none` or `failed`: where the tag read ended (the tagger sends it). Defaults to `read` for a
    /// `file` write.
    pub tags_state: Option<String>,
}

fn range<T: PartialOrd + Copy>(field: &str, v: Option<T>, lo: T, hi: T) -> MsgResult<()> {
    if let Some(v) = v
        && (v < lo || v > hi)
    {
        return Err(invalid(
            field,
            "BASE_TYPE_BAD_RANGE",
            "That number is out of range.",
        ));
    }
    Ok(())
}

pub(crate) fn write_from(b: TrackPatchBody) -> MsgResult<(TagWrite, TagSource)> {
    let source = match b.source.as_deref() {
        None | Some("edit") => TagSource::Edit,
        Some("file") => TagSource::File,
        Some(_) => return Err(invalid("source", "ENUM_TYPE_COERCE", "Use file or edit.")),
    };
    let mut w = TagWrite::default();
    if let Some(t) = &b.title {
        let t = clean_text(t, MAX_TEXT);
        if t.is_empty() {
            return Err(invalid(
                "title",
                "BASE_TYPE_BAD_LENGTH",
                "Use 1 to 300 characters.",
            ));
        }
        w.title = Some(t);
    }
    w.artist = b.artist.as_deref().map(|s| clean_text(s, MAX_TEXT));
    w.album = b.album.as_deref().map(|s| clean_text(s, MAX_TEXT));
    w.album_artist = b.album_artist.as_deref().map(|s| clean_text(s, MAX_TEXT));
    range("track_no", b.track_no.flatten(), 0, 9999)?;
    range("disc_no", b.disc_no.flatten(), 0, 999)?;
    range("year", b.year.flatten(), 1000, 2200)?;
    range("duration_ms", b.duration_ms.flatten(), 0, 86_400_000)?;
    w.track_no = b.track_no;
    w.disc_no = b.disc_no;
    w.year = b.year;
    w.duration_ms = b.duration_ms;
    if let Some(g) = &b.genres {
        w.genres = Some(clean_genres(g));
    }
    w.cover = match &b.cover_key {
        None => None,
        Some(None) => Some(None),
        Some(Some(k)) => {
            let h = cover_hash_of(k).ok_or_else(|| {
                invalid(
                    "cover_key",
                    "KEY_INVALID",
                    "A cover is covers/<hash>.jpg on the same bucket.",
                )
            })?;
            Some(Some((k.clone(), h.to_owned())))
        }
    };
    w.state = match b.tags_state.as_deref() {
        None => None,
        Some(s) => match TagsState::parse(s) {
            Some(TagsState::Pending) | None => {
                return Err(invalid(
                    "tags_state",
                    "ENUM_TYPE_COERCE",
                    "Use read, none or failed.",
                ));
            }
            st => st,
        },
    };
    if source == TagSource::File && w.state.is_none() {
        w.state = Some(TagsState::Read);
    }
    if w.is_empty() {
        return Err(invalid("body", "BASE_TYPE_REQUIRED", "Nothing to write."));
    }
    Ok((w, source))
}

/// The track `id` when `user` reaches its mount and it is not delisted (the voice jukebox queues through this).
pub(crate) async fn find_track(
    d: &ScopeDeps,
    m: &MusicDeps,
    user: i64,
    id: i64,
) -> MsgResult<Track> {
    find(d, m, user, id).await.map(|(_, t)| t)
}

async fn find(d: &ScopeDeps, m: &MusicDeps, user: i64, id: i64) -> MsgResult<(Vec<Reach>, Track)> {
    let reach = reaches(d, m, user).await?;
    let ids: Vec<i64> = reach.iter().map(|r| r.mount.id).collect();
    let t = m
        .store
        .get(id, &ids)
        .await
        .map_err(store_failed)?
        .ok_or_else(not_found)?;
    // The per-key rules: a delisted file is gone for everyone who reaches it.
    let facts: &dyn ScopeFacts = &*d.store;
    let hidden = facts
        .delisted(t.mount_id, std::slice::from_ref(&t.key))
        .await
        .map_err(facts_failed)?;
    if !hidden.is_empty() {
        return Err(not_found());
    }
    Ok((reach, t))
}

async fn one(d: &ScopeDeps, user: i64, reach: &[Reach], t: Track) -> MsgResult<Json<TrackView>> {
    let mount = reach
        .iter()
        .find(|r| r.mount.id == t.mount_id)
        .map(|r| r.mount.clone())
        .ok_or_else(not_found)?;
    let edit = may_write(d, user, &mount).await?;
    let mut signer = Signer::new(d, reach);
    Ok(Json(view_of(t, edit, &mut signer).await))
}

/// One track.
#[utoipa::path(
    get,
    path = "/music/tracks/{track_id}",
    params(("track_id" = String, Path, description = "Track snowflake")),
    responses((status = 200, body = TrackView), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_track(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    Path(track_id): Path<String>,
) -> MsgResult<Json<TrackView>> {
    let user = uid(&p);
    let (reach, t) = find(&d, &m, user, parse_row(&track_id)?).await?;
    one(&d, user, &reach, t).await
}

/// Write a track's tags. Needs the mount's owner or, on a server's shelf, the `upload` grant; a friend who can
/// only listen gets 403, a viewer who does not reach the mount (and platform staff) 404.
#[utoipa::path(
    patch,
    path = "/music/tracks/{track_id}",
    params(("track_id" = String, Path, description = "Track snowflake")),
    request_body = TrackPatchBody,
    responses(
        (status = 200, body = TrackView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn patch_track(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    Path(track_id): Path<String>,
    ApiJson(body): ApiJson<TrackPatchBody>,
) -> MsgResult<Json<TrackView>> {
    let user = uid(&p);
    let id = parse_row(&track_id)?;
    let (w, source) = write_from(body)?;
    let (reach, t) = find(&d, &m, user, id).await?;
    let mount = reach
        .iter()
        .find(|r| r.mount.id == t.mount_id)
        .map(|r| r.mount.clone())
        .ok_or_else(not_found)?;
    if !may_write(&d, user, &mount).await? {
        return Err(ErrorCode::MissingPermissions.into());
    }
    m.store.write(id, &w, source).await.map_err(|e| match e {
        PatchError::NotFound => not_found(),
        PatchError::Invalid(msg) => invalid("body", "ALREADY_TAKEN", &msg),
        PatchError::Store(e) => store_failed(e),
    })?;
    let (reach, t) = find(&d, &m, user, id).await?;
    one(&d, user, &reach, t).await
}

/// Body of the track presign.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TrackPresignBody {
    /// A request range the client will send, `bytes=a-b`.
    #[serde(default)]
    pub range: Option<String>,
}

/// A presigned `GET` for the track's file: the wave-27 presign (`POST /storage/mounts/{id}/presign`) with the
/// mount and key taken from the track, so the player streams by range straight from the bucket.
#[utoipa::path(
    post,
    path = "/music/tracks/{track_id}/presign",
    params(("track_id" = String, Path, description = "Track snowflake")),
    request_body = TrackPresignBody,
    responses(
        (status = 200, body = PresignView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY"),
    ),
    security(("session" = []))
)]
#[allow(clippy::too_many_arguments)]
pub async fn presign(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(track_id): Path<String>,
    ApiJson(body): ApiJson<TrackPresignBody>,
) -> MsgResult<Json<PresignView>> {
    let (_, t) = find(&d, &m, uid(&p), parse_row(&track_id)?).await?;
    crate::routes::storage::objects::presign(
        State(state),
        SessionOnly(p),
        Ext(d),
        MaybeExt(admin),
        headers,
        Path(t.mount_id.to_string()),
        ApiJson(PresignBody {
            key: t.key,
            method: "GET".into(),
            range: body.range,
        }),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: &str) -> TrackPatchBody {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_member_edit_is_the_default_and_a_file_write_reads() {
        let (w, s) = write_from(body(r#"{"title":" Hi  there "}"#)).unwrap();
        assert_eq!(
            (s, w.title.as_deref(), w.state),
            (TagSource::Edit, Some("Hi there"), None)
        );
        let (w, s) = write_from(body(r#"{"source":"file","album":"A"}"#)).unwrap();
        assert_eq!((s, w.state), (TagSource::File, Some(TagsState::Read)));
        let (w, _) = write_from(body(r#"{"source":"file","tags_state":"none"}"#)).unwrap();
        assert_eq!(w.state, Some(TagsState::None));
    }

    #[test]
    fn bad_values_are_refused() {
        for j in [
            r#"{}"#,
            r#"{"title":"  "}"#,
            r#"{"year":5000}"#,
            r#"{"track_no":10000}"#,
            r#"{"source":"x","title":"a"}"#,
            r#"{"tags_state":"pending"}"#,
            r#"{"cover_key":"covers/../x.jpg"}"#,
            r#"{"cover_key":"other/0123abcd.jpg"}"#,
        ] {
            assert!(write_from(body(j)).is_err(), "{j}");
        }
        let (w, _) =
            write_from(body(r#"{"cover_key":"covers/0123abcd.jpg","year":null}"#)).unwrap();
        assert_eq!(
            w.cover,
            Some(Some(("covers/0123abcd.jpg".into(), "0123abcd".into())))
        );
        assert_eq!(w.year, Some(None));
    }
}
