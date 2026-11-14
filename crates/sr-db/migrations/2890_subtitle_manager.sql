-- 2890 (SEC-AUD-200, D-685): the Cinema subtitles manager.
--
-- * `source`: `embedded` for a track the transcode extracted from the source file (every existing row), `uploaded`
--   for a file a staff member added. A re-transcode replaces the embedded rows and never touches the uploaded ones.
-- * `hidden`: a track staff hide is in neither the playback answer nor the playlists.
-- * `source_ordinal`: the stream the track came from (`-map 0:s:<n>`, the job plan's `source_ordinal`), so a
--   staff-edited embedded track can be recognised again after a re-transcode. NULL for rows written before this
--   migration (they have no recorded stream and nothing staff edited; the next re-transcode fills it) and for
--   uploaded rows.
-- * `edited`: staff changed the label, language or flags of this embedded track; only then are those fields
--   carried over to the new row of the same stream at the next re-transcode.
-- * `job_id` becomes nullable: an uploaded track belongs to no job.
-- Uploaded files live under `subtitles/{asset}/` (never under a transcode output prefix, which the collector
-- lists once an asset points at a newer one), with the paths `u/{track}.vtt` and `u/{track}.ass` relative to the
-- asset playback base, which the CDN maps to that prefix.
ALTER TABLE video_subtitle_tracks
    ADD COLUMN source TEXT NOT NULL DEFAULT 'embedded' CHECK (source IN ('embedded', 'uploaded')),
    ADD COLUMN hidden BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN source_ordinal INTEGER,
    ADD COLUMN edited BOOLEAN NOT NULL DEFAULT FALSE,
    ALTER COLUMN job_id DROP NOT NULL;

-- Rows written before this migration have no recorded stream. The ladder job that wrote them kept its plan in
-- `transcode_jobs.spec` (`plan.subtitles[].source_ordinal` with the `vtt_path` of the file), so the stream is read
-- back from the newest finished ladder job whose plan names the same file; a row no plan names stays NULL (its
-- edits cannot be carried over at the next re-transcode, which then writes the stream for it).
UPDATE video_subtitle_tracks t
SET source_ordinal = j.src
FROM (
    SELECT DISTINCT ON (jobs.asset_id, s ->> 'vtt_path')
           jobs.asset_id, s ->> 'vtt_path' AS vtt, (s ->> 'source_ordinal')::int AS src
    FROM transcode_jobs jobs
    CROSS JOIN LATERAL jsonb_array_elements(
        CASE WHEN jsonb_typeof(jobs.spec -> 'plan' -> 'subtitles') = 'array'
             THEN jobs.spec -> 'plan' -> 'subtitles' ELSE '[]'::jsonb END) AS s
    WHERE jobs.kind = 'ladder' AND jobs.state = 'succeeded' AND jobs.asset_id IS NOT NULL
      AND s ->> 'source_ordinal' ~ '^[0-9]+$'
    ORDER BY jobs.asset_id, s ->> 'vtt_path', jobs.id DESC
) j
WHERE t.asset_id = j.asset_id AND t.vtt_path = j.vtt AND t.source = 'embedded' AND t.source_ordinal IS NULL;
