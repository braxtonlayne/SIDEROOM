//! Upload slots (`POST /channels/{id}/attachments`, FND-STOR-004): size limits come from
//! the entitlements policy, the client PUTs straight to object storage through a
//! presigned URL, and the content type is sniffed from the stored bytes when the
//! upload is completed (explicitly, or implicitly when a message references it).

use super::deps::MessagesDeps;
use super::error::{ApiJson, MsgError, MsgResult, invalid, parse_id};
use super::model::{AttachmentObject, UploadSlot, UploadSlotsBody, UploadSlotsResponse};
use super::render::actor;
use super::store::{Attachment, UploadState};
use crate::routes::Ext;
use crate::routes::MaybeExt;
use crate::routes::social::SocialDeps;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use sr_chat::ChatError;
use sr_core::permissions::Permissions;
use sr_core::{AttachmentId, ChannelId, ErrorCode, UserId};
use sr_library::multipart::{
    MultipartSlot, MultipartWiring, PART_URL_TTL, PartUrl, ReceivedPart, UploadParts, check_parts,
};
use sr_storage::StorageError;
use sr_storage::storage::PresignMethod;
use sr_storage::upload::sniff::is_strippable_image;
use sr_storage::upload::{SizeError, Surface, strip_metadata};

/// Bytes read from the start of the object for sniffing.
const SNIFF_BYTES: u64 = 512;

/// Strip path components and control characters; never empty, at most 255 bytes.
pub fn sanitize_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut name: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | ':' | '|' | '?' | '*'))
        .collect();
    name = name.trim().trim_start_matches('.').to_owned();
    while name.len() > 255 {
        name.pop();
    }
    if name.is_empty() {
        "file".to_owned()
    } else {
        name
    }
}

/// Discord's filename marker for spoiler attachments (D-177).
pub const SPOILER_PREFIX: &str = "SPOILER_";

/// The stored filename of a spoiler attachment: `SPOILER_` once in front, still at
/// most 255 bytes. A name that already carries the prefix is kept as is.
pub fn spoiler_filename(name: &str) -> String {
    if name.starts_with(SPOILER_PREFIX) {
        return name.to_owned();
    }
    let mut out = format!("{SPOILER_PREFIX}{name}");
    while out.len() > 255 {
        out.pop();
    }
    out
}

/// `{upload_id}/{filename}`, what the client passes back as `uploaded_filename`.
pub fn upload_filename(a: &Attachment) -> String {
    format!("{}/{}", a.id, a.filename)
}

/// Request an upload slot for each file.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/attachments",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = UploadSlotsBody,
    responses(
        (status = 200, body = UploadSlotsResponse),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 413, body = ErrorBody, description = "File larger than the caller's limit"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn request_slots(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<UploadSlotsBody>,
) -> MsgResult<Json<UploadSlotsResponse>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(Permissions::ATTACH_FILES)?;
    // Forum and media channels take slots too: the starter message of a post carries the files (H-38).
    if !acc.channel.kind.accepts_messages() && !acc.channel.kind.is_forum_like() {
        return Err(invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Cannot upload to this channel type.",
        ));
    }
    if body.files.is_empty() || body.files.len() > deps.limits.max_attachments {
        return Err(invalid(
            "files",
            "BASE_TYPE_BAD_LENGTH",
            &format!(
                "Must provide between 1 and {} files.",
                deps.limits.max_attachments
            ),
        ));
    }

    let entitled = deps.upload_limit(user, acc.channel.guild_id).await;
    let max_size = deps.size_policy.effective_limit(entitled);
    let now = deps.now_ms();
    // Validate everything before issuing any slot.
    let mut decisions = Vec::with_capacity(body.files.len());
    for (i, f) in body.files.iter().enumerate() {
        check_image_meta(i, f)?;
        match deps.size_policy.check(f.file_size, entitled) {
            Ok(d) => decisions.push(d),
            Err(SizeError::Empty) => {
                return Err(invalid(
                    &format!("files.{i}.file_size"),
                    "NUMBER_TYPE_MIN",
                    "Files cannot be empty.",
                ));
            }
            Err(SizeError::ExceedsUserLimit { limit }) => {
                return Err(MsgError::Api(sr_core::ApiError::with_message(
                    ErrorCode::RequestEntityTooLarge,
                    format!("File exceeds your upload limit of {limit} bytes."),
                )));
            }
        }
    }

    // A conversation in someone's Message Requests under `explicit_media_filter_dms` delivers
    // every attachment with the spoiler flag; nothing is scanned (D-224).
    let force_spoiler = match (&social, acc.channel.guild_id) {
        (Some(s), None) => s
            .service
            .spoiler_attachments(user, channel_id)
            .await
            .unwrap_or(false),
        _ => false,
    };
    let mut slots = Vec::with_capacity(body.files.len());
    for (f, d) in body.files.iter().zip(decisions) {
        let id = deps.next_attachment_id();
        // Discord's convention: a `SPOILER_` name is a spoiler, and `spoiler: true`
        // adds the prefix (D-177), so `AttachmentObject.filename` carries it.
        let name = sanitize_filename(&f.filename);
        let spoiler =
            force_spoiler || f.spoiler.unwrap_or(false) || name.starts_with(SPOILER_PREFIX);
        let filename = if spoiler {
            spoiler_filename(&name)
        } else {
            name
        };
        let att = Attachment {
            id,
            channel_id,
            uploader: user,
            key: format!("chat/{channel_id}/{id}/{filename}"),
            filename,
            size: f.file_size,
            declared_content_type: f.content_type.clone(),
            content_type: None,
            description: f.description.clone().filter(|d| d.chars().count() <= 1024),
            spoiler,
            state: UploadState::Pending,
            message_id: None,
            expires_at_ms: d
                .expires_after
                .map(|t| now + i64::try_from(t.as_millis()).unwrap_or(i64::MAX / 2)),
            created_at_ms: now,
            duration_ms: None,
            waveform: None,
            width: f.width,
            height: f.height,
            blurhash: f.blurhash.clone(),
        };
        deps.store.insert_attachment(&att).await?;
        // Files above the multipart threshold go up in parts that can be resumed (LNG-CMP-007).
        let (url, multipart, ttl) = match (&deps.multipart, d.multipart) {
            (Some(m), true) => {
                let slot = begin_multipart(&deps, m, &att).await?;
                (String::new(), Some(slot), PART_URL_TTL)
            }
            _ => {
                let url = deps
                    .storage
                    .presign(PresignMethod::Put, &att.key, deps.upload_url_ttl)
                    .await?;
                (url, None, deps.upload_url_ttl)
            }
        };
        slots.push(UploadSlot {
            id: f.id.clone().unwrap_or_else(|| id.to_string()),
            upload_id: id.to_string(),
            upload_url: url,
            upload_filename: upload_filename(&att),
            upload_url_expires_in: ttl.as_secs(),
            max_size,
            expires_after: d.expires_after.map(|t| t.as_secs()),
            multipart,
        });
    }
    Ok(Json(UploadSlotsResponse { attachments: slots }))
}

fn storage_err(e: impl std::fmt::Display) -> MsgError {
    tracing::warn!(error = %e, "multipart upload storage call failed");
    ErrorCode::SrInternal.into()
}

fn part_urls(
    m: &MultipartWiring,
    att: &Attachment,
    multipart_id: &str,
    numbers: impl Iterator<Item = u32>,
) -> MsgResult<Vec<PartUrl>> {
    numbers
        .map(|n| {
            m.store
                .presign_part(&att.key, multipart_id, n, PART_URL_TTL)
                .map(|url| PartUrl {
                    part_number: n,
                    url,
                })
                .map_err(storage_err)
        })
        .collect()
}

/// Start the multipart upload behind a freshly recorded slot.
async fn begin_multipart(
    deps: &MessagesDeps,
    m: &MultipartWiring,
    att: &Attachment,
) -> MsgResult<MultipartSlot> {
    let (part_size, part_count) = m.plan(att.size);
    let content_type = att
        .declared_content_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    let id = m
        .store
        .create(&att.key, content_type)
        .await
        .map_err(storage_err)?;
    if let Err(e) = deps
        .store
        .set_attachment_multipart(att.id, &id, part_size)
        .await
    {
        let _ = m.store.abort(&att.key, &id).await;
        return Err(e.into());
    }
    let parts = part_urls(m, att, &id, 1..=part_count)?;
    Ok(MultipartSlot {
        part_size,
        part_count,
        parts,
    })
}

fn part_count_of(size: u64, part_size: u64) -> u32 {
    u32::try_from(size.div_ceil(part_size.max(1)).max(1)).unwrap_or(u32::MAX)
}

/// Let the store assemble the parts it holds into the object. Idempotent: once the object
/// exists nothing is done.
async fn assemble(deps: &MessagesDeps, att: &Attachment) -> MsgResult<()> {
    let Some((multipart_id, part_size)) = deps.store.attachment_multipart(att.id).await? else {
        return Ok(());
    };
    if deps.storage.head(&att.key).await.is_ok() {
        deps.store.clear_attachment_multipart(att.id).await?;
        return Ok(());
    }
    let m = deps.multipart.as_ref().ok_or_else(|| {
        invalid(
            "attachments",
            "UPLOAD_INVALID",
            "Resumable uploads are not available on this server.",
        )
    })?;
    let part_count = part_count_of(att.size, part_size);
    let stored = m
        .store
        .list_parts(&att.key, &multipart_id)
        .await
        .map_err(storage_err)?;
    let parts = check_parts(&stored, att.size, part_size, part_count)
        .map_err(|e| invalid("attachments", "UPLOAD_INVALID", &e))?;
    m.store
        .complete(&att.key, &multipart_id, &parts)
        .await
        .map_err(|e| {
            tracing::warn!(upload = %att.id, error = %e, "completing a multipart upload failed");
            invalid(
                "attachments",
                "UPLOAD_INVALID",
                "The storage did not accept the uploaded parts.",
            )
        })?;
    deps.store.clear_attachment_multipart(att.id).await?;
    Ok(())
}

/// A truncated read may end mid-character; that is still UTF-8 so far.
fn utf8_prefix_ok(b: &[u8]) -> bool {
    match std::str::from_utf8(b) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

/// Verify the uploaded object and sniff its content type. Idempotent. On a rejected
/// upload the object is deleted and the slot dropped from use.
pub async fn finalize(deps: &MessagesDeps, att: &mut Attachment) -> MsgResult<()> {
    if att.state != UploadState::Pending {
        return Ok(());
    }
    let bad = |msg: &str| invalid("attachments", "UPLOAD_INVALID", msg);
    assemble(deps, att).await?;
    let info = match deps.storage.head(&att.key).await {
        Ok(i) => i,
        Err(StorageError::NotFound(_)) => return Err(bad("The file has not been uploaded.")),
        Err(e) => return Err(e.into()),
    };
    let guild = deps.channel_guild(att.channel_id).await;
    let limit = deps
        .size_policy
        .effective_limit(deps.upload_limit(att.uploader, guild).await);
    if info.size != att.size || info.size > limit {
        let _ = deps.storage.delete(&att.key).await;
        return Err(bad("The uploaded file does not match the declared size."));
    }
    let head = deps
        .storage
        .get_range(&att.key, 0, info.size.min(SNIFF_BYTES))
        .await?;
    // Markup (SVG, HTML, XML) is kept as an opaque download, whatever the client declared
    // (XC-SEC-002): it gets no sniffed type and is stored as application/octet-stream.
    let active = sr_storage::upload::is_active_content(&head);
    let sniffed = Surface::ChatAttachment.check(&head).unwrap_or(None);
    let declared = att.declared_content_type.as_deref().map(|s| {
        s.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    });
    let content_type = match (sniffed, declared.as_deref()) {
        // Declared media types must match the bytes; otherwise the file could be
        // rendered inline as something it is not.
        (got, Some(d)) if d.starts_with("image/") || d.starts_with("video/") => {
            let same = got.is_some_and(|g| g == d || (g == "video/mp4" && d == "video/quicktime"));
            if !same {
                let _ = deps.storage.delete(&att.key).await;
                return Err(bad("The file contents do not match its declared type."));
            }
            got.map(str::to_owned)
        }
        (Some(got), _) => Some(got.to_owned()),
        // Plain text has no magic bytes: accept it only when it really is UTF-8.
        (None, Some("text/plain")) if !active && utf8_prefix_ok(&head) => {
            Some("text/plain".to_owned())
        }
        (None, _) => None,
    };
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_owned());
    // Privacy: drop EXIF/XMP/text chunks (GPS etc.) from JPEG and PNG before anyone can fetch them.
    if is_strippable_image(&content_type) && info.size <= deps.size_policy.multipart_threshold {
        let whole = deps.storage.get(&att.key).await?;
        match strip_metadata(&whole) {
            Ok(clean) => {
                if clean.len() != whole.len() {
                    att.size = clean.len() as u64;
                    deps.storage.put(&att.key, clean.into()).await?;
                }
            }
            Err(_) => {
                let _ = deps.storage.delete(&att.key).await;
                return Err(bad("The image is corrupt."));
            }
        }
    }
    // Voice messages: the server measures the duration and builds the waveform itself
    // (LNG-MSG-019); the values the client declares only fill in when it cannot.
    if content_type == "audio/ogg" && info.size <= super::rich_create::MAX_VOICE_BYTES {
        let whole = deps.storage.get(&att.key).await?;
        if let Some(m) = sr_chat::voice_meta::analyze_ogg_opus(&whole) {
            att.duration_ms = Some(m.duration_ms);
            att.waveform = Some(data_encoding::BASE64.encode(&m.waveform));
        }
    }
    att.content_type = Some(content_type);
    att.state = UploadState::Ready;
    deps.store.update_attachment(att).await?;
    Ok(())
}

/// The client-declared picture size and BlurHash: both sides or neither, in range, and a plausible hash.
fn check_image_meta(i: usize, f: &super::model::FileSpec) -> MsgResult<()> {
    let side_ok = |n: u32| (1..=sr_chat::repo::MAX_IMAGE_SIDE).contains(&n);
    match (f.width, f.height) {
        (None, None) => {}
        (Some(w), Some(h)) if side_ok(w) && side_ok(h) => {}
        _ => {
            return Err(invalid(
                &format!("files.{i}.width"),
                "NUMBER_TYPE_RANGE",
                "Width and height must both be given, between 1 and 16384.",
            ));
        }
    }
    if let Some(h) = &f.blurhash
        && !sr_chat::repo::valid_blurhash(h)
    {
        return Err(invalid(
            &format!("files.{i}.blurhash"),
            "BASE_TYPE_BAD_FORMAT",
            "Not a valid BlurHash.",
        ));
    }
    Ok(())
}

/// Mark an uploaded file complete and learn its verified content type.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/attachments/{upload_id}/complete",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("upload_id" = String, Path, description = "Upload id from the slot response"),
    ),
    responses(
        (status = 200, body = AttachmentObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn complete_upload(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel_id, upload_id)): Path<(String, String)>,
) -> MsgResult<Json<AttachmentObject>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let upload_id: AttachmentId = parse_id(&upload_id, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(Permissions::ATTACH_FILES)?;
    let mut att = load_upload(&deps, user, channel_id, upload_id).await?;
    finalize(&deps, &mut att).await?;
    Ok(Json(super::model::attachment_object(&att)))
}

/// Resume a multipart upload: the parts that arrived and fresh URLs for the missing ones.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/attachments/{upload_id}/parts",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("upload_id" = String, Path, description = "Upload id from the slot response"),
    ),
    responses(
        (status = 200, body = UploadParts),
        (status = 400, body = ErrorBody, description = "Not a multipart upload, or already completed"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn upload_parts(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel_id, upload_id)): Path<(String, String)>,
) -> MsgResult<Json<UploadParts>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let upload_id: AttachmentId = parse_id(&upload_id, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(Permissions::ATTACH_FILES)?;
    let att = load_upload(&deps, user, channel_id, upload_id).await?;
    let not_resumable = || {
        invalid(
            "upload_id",
            "UPLOAD_INVALID",
            "This upload is not a resumable one.",
        )
    };
    let m = deps.multipart.as_ref().ok_or_else(not_resumable)?;
    if att.state != UploadState::Pending {
        return Err(not_resumable());
    }
    let (multipart_id, part_size) = deps
        .store
        .attachment_multipart(att.id)
        .await?
        .ok_or_else(not_resumable)?;
    let part_count = part_count_of(att.size, part_size);
    let stored = m
        .store
        .list_parts(&att.key, &multipart_id)
        .await
        .map_err(storage_err)?;
    let want = |n: u32| {
        if n == part_count {
            att.size
                .saturating_sub(part_size * u64::from(part_count - 1))
        } else {
            part_size
        }
    };
    let received: Vec<ReceivedPart> = stored
        .iter()
        .filter(|s| s.number >= 1 && s.number <= part_count && s.size == want(s.number))
        .map(|s| ReceivedPart {
            part_number: s.number,
            size: s.size,
        })
        .collect();
    let missing = part_urls(
        m,
        &att,
        &multipart_id,
        (1..=part_count).filter(|n| !received.iter().any(|r| r.part_number == *n)),
    )?;
    Ok(Json(UploadParts {
        part_size,
        part_count,
        received,
        missing,
        expires_in_secs: PART_URL_TTL.as_secs(),
    }))
}

/// Cancel an upload that was not attached to a message: frees the uploaded parts or the stored object.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/attachments/{upload_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("upload_id" = String, Path, description = "Upload id from the slot response"),
    ),
    responses(
        (status = 204, description = "Upload discarded"),
        (status = 400, body = ErrorBody, description = "Already attached to a message"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn abort_upload(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel_id, upload_id)): Path<(String, String)>,
) -> MsgResult<axum::http::StatusCode> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let upload_id: AttachmentId = parse_id(&upload_id, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(Permissions::ATTACH_FILES)?;
    let att = load_upload(&deps, user, channel_id, upload_id).await?;
    if att.state == UploadState::Attached || att.message_id.is_some() {
        return Err(invalid(
            "upload_id",
            "UPLOAD_INVALID",
            "This file is already part of a message.",
        ));
    }
    if let (Some(m), Some((multipart_id, _))) = (
        &deps.multipart,
        deps.store.attachment_multipart(att.id).await?,
    ) && let Err(e) = m.store.abort(&att.key, &multipart_id).await
    {
        tracing::warn!(upload = %att.id, error = %e, "aborting a multipart upload failed");
    }
    let _ = deps.storage.delete(&att.key).await;
    deps.store.forget_pending_upload(att.id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// An upload the caller owns in this channel (404 otherwise, so ids do not leak).
pub async fn load_upload(
    deps: &MessagesDeps,
    user: UserId,
    channel: ChannelId,
    id: AttachmentId,
) -> MsgResult<Attachment> {
    deps.store
        .get_attachments(&[id])
        .await?
        .into_iter()
        .find(|a| a.channel_id == channel && a.uploader == user)
        .ok_or_else(|| ChatError::UnknownChannel.into())
}

#[cfg(test)]
mod tests {
    use super::{sanitize_filename, spoiler_filename};

    #[test]
    fn spoiler_prefix_is_added_once_and_bounded() {
        assert_eq!(spoiler_filename("cat.png"), "SPOILER_cat.png");
        assert_eq!(spoiler_filename("SPOILER_cat.png"), "SPOILER_cat.png");
        // Only the exact marker counts; it is not a case-folded match.
        assert_eq!(
            spoiler_filename("spoiler_cat.png"),
            "SPOILER_spoiler_cat.png"
        );
        let long = spoiler_filename(&"x".repeat(255));
        assert_eq!(long.len(), 255);
        assert!(long.starts_with("SPOILER_x"));
    }

    #[test]
    fn filenames_are_sanitized() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("C:\\x\\cat.png"), "cat.png");
        assert_eq!(sanitize_filename("  .hidden "), "hidden");
        assert_eq!(sanitize_filename("a\u{0}b<c>.txt"), "abc.txt");
        assert_eq!(sanitize_filename(""), "file");
        assert_eq!(sanitize_filename(&"x".repeat(400)).len(), 255);
    }
}
