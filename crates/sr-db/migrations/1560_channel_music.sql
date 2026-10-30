-- 1560: voice channel music settings (MUS-JB-001).
-- `music_allowed` (default on): members with SPEAK may play a stereo music track into the channel.
-- Turning it off makes the SFU drop every music line of the room (the voice itself is untouched).
-- `music_bitrate_kbps` is the channel setting `voice.music_bitrate_kbps`: the Opus bitrate of the
-- music track, 32 to 320, default 256, the same for every channel whatever its owner's plan.
-- Only voice-like channels use them; text channels keep the defaults.
ALTER TABLE channels
    ADD COLUMN IF NOT EXISTS music_allowed boolean NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS music_bitrate_kbps smallint NOT NULL DEFAULT 256;
ALTER TABLE channels
    DROP CONSTRAINT IF EXISTS channels_music_bitrate_check;
ALTER TABLE channels
    ADD CONSTRAINT channels_music_bitrate_check CHECK (music_bitrate_kbps BETWEEN 32 AND 320);
