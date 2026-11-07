-- 2380: soundboard sounds may be WebM/Opus (D-534). 4 = webm, joining 1 mp3, 2 ogg, 3 wav.
-- The server still decodes nothing; the duration is read by the voice-message parser.
ALTER TABLE soundboard_sounds DROP CONSTRAINT IF EXISTS soundboard_sounds_format_type_check;
ALTER TABLE soundboard_sounds
    ADD CONSTRAINT soundboard_sounds_format_type_check CHECK (format_type BETWEEN 1 AND 4);
