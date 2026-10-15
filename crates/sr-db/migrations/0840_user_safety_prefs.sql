-- LNG-USET-012/013: sensitive-media handling per DM source, the DM spam filter and per-server
-- DM overrides. They sit on the privacy row. Filter codes: 0 show, 1 blur, 2 block.
ALTER TABLE user_privacy
    ADD COLUMN media_friends       SMALLINT NOT NULL DEFAULT 0 CHECK (media_friends BETWEEN 0 AND 2),
    ADD COLUMN media_others        SMALLINT NOT NULL DEFAULT 1 CHECK (media_others BETWEEN 0 AND 2),
    ADD COLUMN media_servers       SMALLINT NOT NULL DEFAULT 0 CHECK (media_servers BETWEEN 0 AND 2),
    ADD COLUMN dm_spam_filter      BOOLEAN NOT NULL DEFAULT FALSE,
    -- {"<guild id>": true|false}
    ADD COLUMN server_dm_overrides JSONB NOT NULL DEFAULT '{}'::jsonb;
