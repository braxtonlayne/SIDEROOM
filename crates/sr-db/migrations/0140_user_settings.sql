-- 0140: per-user settings (LNG-USET-002, LNG-NTF-001).
-- `settings` is the client-owned synced blob (appearance, keybinds, toggles); the server
-- caps its size and treats it as opaque. `version` bumps on every save so concurrent
-- sessions merge instead of overwriting each other. `default_notification_level` is the
-- user's own default (0 all, 1 only mentions, 2 nothing); NULL follows each server's default.
CREATE TABLE user_settings (
    user_id                    BIGINT PRIMARY KEY REFERENCES accounts (id),
    settings                   JSONB NOT NULL DEFAULT '{}'::jsonb
                               CHECK (jsonb_typeof(settings) = 'object'
                                      AND octet_length(settings::text) <= 65536),
    version                    BIGINT NOT NULL DEFAULT 0 CHECK (version >= 0),
    default_notification_level SMALLINT CHECK (default_notification_level IN (0, 1, 2)),
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT now()
);
