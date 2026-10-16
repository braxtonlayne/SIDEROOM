-- 0961: guild members carry Discord's `flags` (P14-LNG-4, D-245).
-- The column dates from 0010 (`guild_members.flags INTEGER NOT NULL DEFAULT 0`) and nothing wrote it
-- until now. Bit 0 DID_REJOIN, bit 1 COMPLETED_ONBOARDING, bit 2 BYPASSES_VERIFICATION,
-- bit 3 STARTED_ONBOARDING; only COMPLETED_ONBOARDING has a setter today. This migration makes sure
-- the column is there and keeps it a non-negative bit field.
ALTER TABLE guild_members
    ADD COLUMN IF NOT EXISTS flags INTEGER NOT NULL DEFAULT 0;
ALTER TABLE guild_members
    ADD CONSTRAINT guild_members_flags_nonneg CHECK (flags >= 0);
