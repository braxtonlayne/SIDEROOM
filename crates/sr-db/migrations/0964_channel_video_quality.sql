-- 0964: per-channel video quality for voice and stage channels (P14-LNG-7, J-11).
-- 1 = auto (the adaptive default), 2 = full (camera tracks may use the 720p tier whatever the
-- occupancy). The column dates from 0011 as a nullable SMALLINT nothing wrote; it becomes
-- NOT NULL DEFAULT 1 with a check. `rtc_region` (same migration) stays unused: the API
-- returns null (one voice region).
UPDATE channels SET video_quality_mode = 1 WHERE video_quality_mode IS NULL OR video_quality_mode NOT IN (1, 2);
ALTER TABLE channels
    ALTER COLUMN video_quality_mode SET DEFAULT 1,
    ALTER COLUMN video_quality_mode SET NOT NULL,
    ADD CONSTRAINT channels_video_quality_mode_check CHECK (video_quality_mode IN (1, 2));
