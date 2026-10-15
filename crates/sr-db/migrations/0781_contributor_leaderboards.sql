-- 0781: contributor leaderboards (DON-LB-001, G-08). A contributor can opt out of the boards built over
-- contribution_credits; the donor flags in this table stay as they are.
ALTER TABLE donor_preferences ADD COLUMN hide_contributions BOOLEAN NOT NULL DEFAULT FALSE;
