//! Staff subtitle management (D-685, SEC-AUD-200): list the tracks of an asset, upload a subtitle file, edit a
//! track, delete an uploaded one.
//!
//! An upload is untrusted input. It is parsed by [`crate::subtitles::convert_upload`] and written out again as
//! WebVTT (an ASS/SSA file is also kept for styled rendering, as for extracted tracks); a file with no readable
//! cue is refused. The files are stored under `subtitles/{asset}/{track}.vtt` (see
//! [`crate::assets::upload_object_key`]), outside every prefix a transcode job writes and the collector lists,
//! so a re-transcode, which replaces the embedded rows only, never touches them.

use super::{MediaService, storage_err};
use crate::assets::*;
use crate::subtitles::{self, SubtitleError};

/// The most characters of a track label.
pub const MAX_LABEL_CHARS: usize = 80;

/// What an upload says about its file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubtitleUploadMeta {
    /// Shown in the player's menu; the language, or "Subtitles", when empty.
    pub label: Option<String>,
    /// A BCP 47 tag, already validated and normalised by the caller.
    pub language: Option<String>,
    pub forced: bool,
    pub hearing_impaired: bool,
}

/// A label as stored: trimmed, no control characters, 1 to [`MAX_LABEL_CHARS`] characters.
pub fn clean_label(raw: &str) -> MediaResult<String> {
    let label: String = raw.trim().chars().filter(|c| !c.is_control()).collect();
    let n = label.chars().count();
    if n == 0 || n > MAX_LABEL_CHARS {
        return Err(MediaError::Invalid(format!(
            "the label must be 1 to {MAX_LABEL_CHARS} characters"
        )));
    }
    Ok(label)
}

fn converter_error(e: SubtitleError) -> MediaError {
    MediaError::Invalid(e.to_string())
}

impl MediaService {
    /// Every track of the asset, hidden ones included: embedded tracks first, then the uploaded ones.
    pub async fn subtitle_tracks(&self, asset_id: i64) -> MediaResult<Vec<SubtitleTrack>> {
        self.repo
            .get_asset(asset_id)
            .await?
            .ok_or(MediaError::NotFound("asset"))?;
        Ok(self
            .repo
            .subtitle_tracks(asset_id)
            .await?
            .into_iter()
            .map(|s| s.track)
            .collect())
    }

    /// Add a staff-uploaded track. The format is read from the content, never from a file name.
    pub async fn upload_subtitle(
        &self,
        asset_id: i64,
        meta: SubtitleUploadMeta,
        file: &[u8],
    ) -> MediaResult<SubtitleTrack> {
        let asset = self
            .repo
            .get_asset(asset_id)
            .await?
            .ok_or(MediaError::NotFound("asset"))?;
        if matches!(asset.state, AssetState::Removed | AssetState::Rejected) {
            return Err(MediaError::Conflict(
                "this video was removed; subtitles can no longer be added".into(),
            ));
        }
        if asset.mount_id != sr_storage::mounts::INSTANCE_MOUNT_ID {
            return Err(MediaError::Conflict(
                "this video's storage does not take subtitle uploads".into(),
            ));
        }
        let converted = subtitles::convert_upload(file).map_err(converter_error)?;
        let label = match meta
            .label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            Some(l) => clean_label(l)?,
            None => meta
                .language
                .clone()
                .unwrap_or_else(|| "Subtitles".to_owned()),
        };
        let id = (self.ids)();
        let vtt_path = format!("{UPLOAD_REL_PREFIX}{id}.vtt");
        let ass_path = converted
            .ass
            .as_ref()
            .map(|_| format!("{UPLOAD_REL_PREFIX}{id}.ass"));
        let vtt_key = upload_object_key(asset_id, &vtt_path)
            .ok_or_else(|| MediaError::Backend("subtitle key".into()))?;
        let ass_key = ass_path
            .as_deref()
            .and_then(|p| upload_object_key(asset_id, p));
        self.storage
            .put(&vtt_key, bytes::Bytes::from(converted.vtt.into_bytes()))
            .await
            .map_err(storage_err)?;
        if let (Some(key), Some(ass)) = (&ass_key, converted.ass) {
            if let Err(e) = self.storage.put(key, bytes::Bytes::from(ass)).await {
                self.drop_objects(&[vtt_key]).await;
                return Err(storage_err(e));
            }
        }
        let added = self
            .repo
            .add_uploaded_subtitle(
                asset_id,
                &NewUploadedSubtitle {
                    id,
                    label,
                    language: meta.language,
                    forced: meta.forced,
                    hearing_impaired: meta.hearing_impaired,
                    vtt_path,
                    ass_path,
                },
            )
            .await;
        match added {
            Ok(Some(stored)) => Ok(stored.track),
            other => {
                // No row: do not leave the files behind.
                let keys: Vec<String> = [Some(vtt_key), ass_key].into_iter().flatten().collect();
                self.drop_objects(&keys).await;
                match other {
                    Ok(_) => Err(MediaError::NotFound("asset")),
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// Edit a track (label, language, flags, hidden); a default clears the other defaults.
    pub async fn update_subtitle(
        &self,
        asset_id: i64,
        track_id: i64,
        patch: SubtitlePatch,
    ) -> MediaResult<SubtitleTrack> {
        let patch = SubtitlePatch {
            label: patch.label.as_deref().map(clean_label).transpose()?,
            ..patch
        };
        self.repo
            .update_subtitle(asset_id, track_id, &patch)
            .await?
            .map(|s| s.track)
            .ok_or(MediaError::NotFound("subtitle track"))
    }

    /// Delete an uploaded track with its files. An embedded track is hidden instead (400).
    pub async fn delete_subtitle(&self, asset_id: i64, track_id: i64) -> MediaResult<()> {
        match self
            .repo
            .delete_uploaded_subtitle(asset_id, track_id)
            .await?
        {
            SubtitleDelete::Deleted(stored) => {
                self.drop_subtitle_objects(asset_id, &[stored]).await;
                Ok(())
            }
            SubtitleDelete::Embedded => Err(MediaError::Invalid(
                "this track came from the video file and cannot be deleted; hide it instead".into(),
            )),
            SubtitleDelete::Missing => Err(MediaError::NotFound("subtitle track")),
        }
    }

    /// Delete the uploaded tracks of an asset that was removed, rows and files.
    pub(super) async fn drop_uploaded_subtitles(&self, asset_id: i64) {
        match self.repo.remove_uploaded_subtitles(asset_id).await {
            Ok(rows) => self.drop_subtitle_objects(asset_id, &rows).await,
            Err(e) => {
                tracing::warn!(error = %e, asset = asset_id, "could not remove the uploaded subtitles of a removed asset");
            }
        }
    }

    async fn drop_subtitle_objects(&self, asset_id: i64, rows: &[StoredSubtitle]) {
        let mut keys = Vec::new();
        for r in rows {
            keys.extend(upload_object_key(asset_id, &r.vtt_path));
            if let Some(ass) = &r.ass_path {
                keys.extend(upload_object_key(asset_id, ass));
            }
        }
        self.drop_objects(&keys).await;
    }

    /// Delete objects; a failure is logged (the object is only orphaned), never an error.
    async fn drop_objects(&self, keys: &[String]) {
        for key in keys {
            match self.storage.delete(key).await {
                Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => {}
                Err(e) => tracing::warn!(error = %e, key, "could not delete a subtitle object"),
            }
        }
    }
}
