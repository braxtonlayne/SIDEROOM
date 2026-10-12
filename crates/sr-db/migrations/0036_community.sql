-- 0036: community, onboarding, welcome screen and security flags.
-- docs/04 section 4, LNG-SSET-063..067. Rules/updates channels live here (not on
-- `guilds`) so the whole wizard state is one row.
CREATE TABLE guild_community (
    guild_id                    BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    community_enabled           BOOLEAN NOT NULL DEFAULT FALSE,
    rules_channel_id            BIGINT REFERENCES channels (id) ON DELETE SET NULL,
    updates_channel_id          BIGINT REFERENCES channels (id) ON DELETE SET NULL,
    safety_alerts_channel_id    BIGINT REFERENCES channels (id) ON DELETE SET NULL,
    description                 TEXT,
    preferred_locale            TEXT NOT NULL DEFAULT 'en-US',
    onboarding_enabled          BOOLEAN NOT NULL DEFAULT FALSE,
    welcome_screen_enabled      BOOLEAN NOT NULL DEFAULT FALSE,
    guide_enabled               BOOLEAN NOT NULL DEFAULT FALSE,
    invites_paused              BOOLEAN NOT NULL DEFAULT FALSE,
    raid_alert_joins_per_minute INTEGER NOT NULL DEFAULT 0 CHECK (raid_alert_joins_per_minute >= 0),
    age_restricted              BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE guild_onboarding (
    guild_id             BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    enabled              BOOLEAN NOT NULL DEFAULT FALSE,
    default_channel_ids  BIGINT[] NOT NULL DEFAULT '{}',
    -- serde form of Vec<sr_chat::settings::community::OnboardingPrompt>.
    prompts              JSONB NOT NULL DEFAULT '[]'::jsonb,
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE guild_welcome_screens (
    guild_id     BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    enabled      BOOLEAN NOT NULL DEFAULT FALSE,
    description  TEXT CHECK (char_length(description) <= 140),
    -- serde form of Vec<sr_chat::settings::community::WelcomeChannel> (at most 5).
    channels     JSONB NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_array_length(channels) <= 5),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
