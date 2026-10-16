-- 0961: guild members carry Discord's `flags` (P14-LNG-4, D-245).
-- Bit 0 DID_REJOIN, bit 1 COMPLETED_ONBOARDING, bit 2 BYPASSES_VERIFICATION,
-- bit 3 STARTED_ONBOARDING. Only COMPLETED_ONBOARDING has a setter today.
ALTER TABLE guild_members
    ADD COLUMN flags INTEGER NOT NULL DEFAULT 0 CHECK (flags >= 0);
