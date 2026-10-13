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
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use sr_chat::ChatError;
use sr_core::permissions::Permissions;
use sr_core::{AttachmentId, ChannelId, ErrorCode, UserId};
use sr_storage::StorageError;
use sr_storage::storage::PresignMethod;
use sr_storage::upload::sniff::is_strippable_image;
use sr_storage::upload::{SizeError, sniff_mime, strip_metadata};

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
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<UploadSlotsBody>,
) -> MsgResult<Json<UploadSlotsResponse>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(Permissions::ATTACH_FILES)?;
    if !acc.channel.kind.accepts_messages() {
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

    let entitled = deps.upload_limit(user).await;
    let max_size = deps.size_policy.effective_limit(entitled);
    let now = deps.now_ms();
    // Validate everything before issuing any slot.
    let mut decisions = Vec::with_capacity(body.files.len());
    for (i, f) in body.files.iter().enumerate() {
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

    let mut slots = Vec::with_capacity(body.files.len());
    for (f, d) in body.files.iter().zip(decisions) {
        let id = deps.next_attachment_id();
        let filename = sanitize_filename(&f.filename);
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
            spoiler: f.spoiler.unwrap_or(false),
            state: UploadState::Pending,
            message_id: None,
            expires_at_ms: d
                .expires_after
                .map(|t| now + i64::try_from(t.as_millis()).unwrap_or(i64::MAX / 2)),
            created_at_ms: now,
            duration_ms: None,
            waveform: None,
        };
        let url = deps
            .storage
            .presign(PresignMethod::Put, &att.key, deps.upload_url_ttl)
            .await?;
        deps.store.insert_attachment(&att).await?;
        slots.push(UploadSlot {
            id: f.id.clone().unwrap_or_else(|| id.to_string()),
            upload_id: id.to_string(),
            upload_url: url,
            upload_filename: upload_filename(&att),
            upload_url_expires_in: deps.upload_url_ttl.as_secs(),
            max_size,
            expires_after: d.expires_after.map(|t| t.as_secs()),
        });
    }
    Ok(Json(UploadSlotsResponse { attachments: slots }))
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
    let info = match deps.storage.head(&att.key).await {
        Ok(i) => i,
        Err(StorageError::NotFound(_)) => return Err(bad("The file has not been uploaded.")),
        Err(e) => return Err(e.into()),
    };
    let limit = deps
        .size_policy
        .effective_limit(deps.upload_limit(att.uploader).await);
    if info.size != att.size || info.size > limit {
        let _ = deps.storage.delete(&att.key).await;
        return Err(bad("The uploaded file does not match the declared size."));
    }
    let head = deps
        .storage
        .get_range(&att.key, 0, info.size.min(SNIFF_BYTES))
        .await?;
    let sniffed = sniff_mime(&head);
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
        (None, Some("text/plain")) if utf8_prefix_ok(&head) => Some("text/plain".to_owned()),
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
    att.content_type = Some(content_type);
    att.state = UploadState::Ready;
    deps.store.update_attachment(att).await?;
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
    use super::sanitize_filename;

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
