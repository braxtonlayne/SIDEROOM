-- 0964: per-channel video quality for voice and stage channels (P14-LNG-7, J-11).
-- 1 = auto (the adaptive default), 2 = full (camera tracks may use the 720p tier whatever
-- the occupancy).
ALTER TABLE channels
    ADD COLUMN video_quality_mode SMALLINT NOT NULL DEFAULT 1
        CHECK (video_quality_mode IN (1, 2));
