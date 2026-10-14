-- 0580: moderation hooks reserved ahead of Phase 7 (XC-MOD-001; D-122, D-126, D-031;
-- docs/09 section 5, docs/specs/moderation-compliance.md).
--
-- `moderation_holds` (0011) and `messages.hold_id` (0012) already exist. This migration
-- extends the same hold to the other things a case can freeze, and adds a finer reason
-- taxonomy to platform bans. Nothing here is written by product code yet: holds are
-- created by the Phase 7 case tooling. The retention jobs already honour them (a held
-- row is never purged; see sr-server jobs.rs).

-- Attachments: a hold on an attachment keeps the row and its stored object even when the
-- parent message is purged or a large attachment would expire (docs/09 section 1).
ALTER TABLE attachments
    ADD COLUMN hold_id BIGINT REFERENCES moderation_holds (id);
CREATE INDEX attachments_hold_idx ON attachments (hold_id) WHERE hold_id IS NOT NULL;

-- Library/cinema submissions: rejected submissions have their files purged 14 days after
-- rejection (docs/09); a hold keeps them for a case.
ALTER TABLE submissions
    ADD COLUMN hold_id BIGINT REFERENCES moderation_holds (id);
CREATE INDEX submissions_hold_idx ON submissions (hold_id) WHERE hold_id IS NOT NULL;

-- Ban reason taxonomy. `person_bans.category` stays the two-level severity that drives
-- inviter suspicion (D-031: 'serious' propagates to the inviter, 'standard' does not).
-- `reason_category` says why, in a fixed vocabulary staff pick from, so reports and
-- dashboards can group bans. It is nullable: bans made before this column have none, and
-- the ban dialog may leave it unset for 'standard' bans.
CREATE TYPE ban_reason_category AS ENUM (
    'spam',               -- unsolicited bulk content, scams
    'harassment',         -- harassment, hate, threats
    'illegal_content',    -- content unlawful to host (takedown / legal obligation)
    'child_safety',       -- sexual content involving minors, grooming
    'ban_evasion',        -- returning after a ban
    'fraud',              -- payment fraud, chargeback abuse, impersonation
    'platform_abuse',     -- attacks on the service, scraping, exploit abuse
    'nsfw_policy',        -- age-restricted content outside allowed places
    'other'
);
ALTER TABLE person_bans
    ADD COLUMN reason_category ban_reason_category;
CREATE INDEX person_bans_reason_idx ON person_bans (reason_category, id DESC)
    WHERE reason_category IS NOT NULL;
