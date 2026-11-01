-- 2120: MOD-GRD-001, graduated moderation and subtree cases (wave 44-A, D-395, D-420 to D-424).
--
-- * `account_activity`   voice seconds per account (D-392 needs "0 voice seconds"; voice sessions were never
--                        persisted). Written by `Moderation::record_voice_seconds`; an absent row means 0.
-- * `member_restrictions` every per-member staff act that stands for a while and can be appealed: a warning (kept
--                        on record), a timeout, an invite freeze, a case freeze and an inviter sterilization.
--                        The enforcement checks read it ("is there an active row of this kind for the account").
-- * `moderation_subtree_cases` / `_nodes`  the case a serious ban opens on the target's invite subtree.
-- * `moderation_requests` the two-person step of a sterilization and of the Operator's branch action.
-- * `moderation_notices`  the anonymised notice behind the MOTD and the monthly transparency page. It has no
--                        column that could hold a handle, an account id, the tree shape or an inviter: category
--                        and counts only, by construction.
-- No foreign key to `accounts`: like `invite_inactive_roots`, these rows never block (or follow) an account's life.

CREATE TABLE account_activity (
    account_id     BIGINT PRIMARY KEY,
    voice_seconds  BIGINT NOT NULL DEFAULT 0 CHECK (voice_seconds >= 0),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE moderation_subtree_cases (
    id               BIGINT PRIMARY KEY,
    -- The ban that opened it.
    ban_id           BIGINT UNIQUE,
    root_account_id  BIGINT NOT NULL,
    -- The ban_reason_category value that made the ban serious ('serious' when the ban named none).
    category         TEXT NOT NULL CHECK (category <> ''),
    state            TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    opened_by        BIGINT NOT NULL,
    opened_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at        TIMESTAMPTZ,
    closed_by        BIGINT,
    close_reason     TEXT,
    -- The subtree was larger than the case lists (the nodes kept are the nearest ones).
    nodes_truncated  BOOLEAN NOT NULL DEFAULT FALSE,
    notice_id        BIGINT
);
CREATE INDEX moderation_subtree_cases_open_idx ON moderation_subtree_cases (id DESC) WHERE state = 'open';
CREATE INDEX moderation_subtree_cases_root_idx ON moderation_subtree_cases (root_account_id);

CREATE TABLE moderation_subtree_nodes (
    case_id            BIGINT NOT NULL REFERENCES moderation_subtree_cases (id) ON DELETE CASCADE,
    account_id         BIGINT NOT NULL,
    -- 'root' the banned account, 'descendant' below it, 'inviter' who invited it, 'inviter2' who invited that one.
    role               TEXT NOT NULL CHECK (role IN ('root', 'descendant', 'inviter', 'inviter2')),
    -- 0 for the root, 1.. below it, -1 and -2 for the two inviters.
    depth              INTEGER NOT NULL,
    -- The issuer of the invite this account redeemed: the lineage a staff member reads.
    parent_account_id  BIGINT,
    registered_at      TIMESTAMPTZ NOT NULL,
    messages           BIGINT NOT NULL DEFAULT 0,
    voice_seconds      BIGINT NOT NULL DEFAULT 0,
    -- D-392 at the time the case was opened: the account is at least 7 days old with 0 messages and 0 voice seconds.
    unactivated        BOOLEAN NOT NULL,
    -- 'auto_frozen' by the case, 'review' waits for a staff member, 'frozen' / 'banned' / 'sterilized' /
    -- 'cleared' were decided by staff on this node's own evidence, 'removed' the 30 days ran out,
    -- 'not_active' the account was not active when the case opened (already banned or deleted).
    status             TEXT NOT NULL CHECK (status IN
        ('auto_frozen', 'review', 'frozen', 'banned', 'sterilized', 'cleared', 'removed', 'not_active')),
    decided_by         BIGINT,
    decided_at         TIMESTAMPTZ,
    note               TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (case_id, account_id)
);
CREATE INDEX moderation_subtree_nodes_account_idx ON moderation_subtree_nodes (account_id);

CREATE TABLE member_restrictions (
    id                  BIGINT PRIMARY KEY,
    account_id          BIGINT NOT NULL,
    kind                TEXT NOT NULL CHECK (kind IN ('warn', 'timeout', 'invite_freeze', 'case_freeze', 'sterilization')),
    reason              TEXT NOT NULL CHECK (reason <> ''),
    case_id             BIGINT REFERENCES moderation_subtree_cases (id),
    -- The staff member, or the system user for the automatic freeze of an unactivated descendant.
    imposed_by          BIGINT NOT NULL,
    approved_by         BIGINT,
    -- Strike weight: 1.0 times the account's accountability multiplier (44-B) when the act was recorded.
    weight              REAL NOT NULL DEFAULT 1.0 CHECK (weight >= 1.0),
    starts_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    ends_at             TIMESTAMPTZ NOT NULL,
    -- The 30-day freeze of an unactivated descendant ends in the account's removal unless cleared.
    removes_account     BOOLEAN NOT NULL DEFAULT FALSE,
    state               TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'lifted', 'overturned', 'expired', 'removed')),
    lifted_by           BIGINT,
    lifted_at           TIMESTAMPTZ,
    lift_reason         TEXT,
    -- One appeal per restriction, filed by the person it is on (they can log in while frozen).
    appeal_message      TEXT,
    appealed_at         TIMESTAMPTZ,
    appeal_state        TEXT CHECK (appeal_state IN ('open', 'upheld', 'overturned')),
    appeal_decided_by   BIGINT,
    appeal_decided_at   TIMESTAMPTZ,
    appeal_reason       TEXT,
    CHECK (ends_at >= starts_at),
    CHECK ((appeal_state IS NULL) = (appealed_at IS NULL))
);
CREATE INDEX member_restrictions_active_idx ON member_restrictions (account_id, kind) WHERE state = 'active';
CREATE INDEX member_restrictions_case_idx ON member_restrictions (case_id) WHERE case_id IS NOT NULL;
CREATE INDEX member_restrictions_due_idx ON member_restrictions (ends_at) WHERE state = 'active';
CREATE INDEX member_restrictions_appeal_idx ON member_restrictions (appealed_at) WHERE appeal_state = 'open';

CREATE TABLE moderation_requests (
    id                BIGINT PRIMARY KEY,
    kind              TEXT NOT NULL CHECK (kind IN ('sterilize', 'branch_action')),
    case_id           BIGINT NOT NULL REFERENCES moderation_subtree_cases (id),
    -- 'sterilize': the one inviter account. 'branch_action': NULL (every node of the case subtree).
    target_account_id BIGINT,
    -- 'branch_action' only: 'freeze' or 'ban'.
    action            TEXT CHECK (action IN ('freeze', 'ban')),
    reason            TEXT NOT NULL CHECK (reason <> ''),
    proposed_by       BIGINT NOT NULL,
    approved_by       BIGINT,
    state             TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'approved', 'executed', 'rejected', 'cancelled')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at        TIMESTAMPTZ,
    executed_at       TIMESTAMPTZ,
    -- Nodes the branch action reached (set when it executes).
    nodes_actioned    INTEGER,
    CHECK ((kind = 'sterilize') = (target_account_id IS NOT NULL)),
    CHECK ((kind = 'branch_action') = (action IS NOT NULL)),
    CHECK (approved_by IS NULL OR approved_by <> proposed_by)
);
CREATE INDEX moderation_requests_state_idx ON moderation_requests (state, id DESC);

CREATE TABLE moderation_notices (
    id                BIGINT PRIMARY KEY,
    kind              TEXT NOT NULL CHECK (kind IN ('serious_ban', 'branch_action')),
    category          TEXT NOT NULL CHECK (category <> ''),
    accounts_actioned INTEGER NOT NULL CHECK (accounts_actioned >= 0),
    accounts_frozen   INTEGER NOT NULL CHECK (accounts_frozen >= 0),
    announcement_id   BIGINT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX moderation_notices_month_idx ON moderation_notices (created_at);
