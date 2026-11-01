-- 2130: MOD-VCH-001 (wave 44-B, "Heavy Crown"), profile vouches.
--
-- A vouch is one member saying "I trust this person" on their /~handle profile: one row per pair, revocable,
-- never for oneself. The row is not a score by itself: the read side counts only vouchers in good standing (an
-- active, non-bot account at least three days old), so a banned or deleted voucher stops counting without a
-- cleanup job. The count sets the vouch tier (Citizen 0-49, Pillar 50-499, Elder 500+) that scales automated
-- moderation upward for the vouched member (the accountability multiplier).
--
-- This is NOT arcade_build_vouches (wave 42, hash-pinned vouches for one game build); the tables never mix.
-- The accounts row is never removed by an erasure (account_deletion.rs scrubs it), so ON DELETE CASCADE alone
-- would not remove vouches: the erasure transaction deletes these rows explicitly, both directions.
CREATE TABLE profile_vouches (
    voucher_id BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    vouchee_id BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (voucher_id, vouchee_id),
    CONSTRAINT profile_vouches_no_self CHECK (voucher_id <> vouchee_id)
);
CREATE INDEX profile_vouches_vouchee_idx ON profile_vouches (vouchee_id);
CREATE INDEX profile_vouches_voucher_recent_idx ON profile_vouches (voucher_id, created_at DESC);
