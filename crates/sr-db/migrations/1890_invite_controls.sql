-- 1890: OPS-INV-001, invite controls ("Lock the Doors", quotas, the invites.prune job).
--
-- * The credit seed was a catalog row (`invites.initial_credits`) that registration never read. It is now
--   `invites.default_credits` and registration reads it. An operator's saved value moves with the rename.
-- * `invite_inactive_roots` is the DISPLAY mark of the prune job: an account that redeemed an invite and never
--   activated (email never verified and never seen online) is the root of an inactive subtree in Admin > Invites.
--   It is derived data, rebuilt by every pass; it holds no invite_graph row and deleting from it deletes nothing
--   (`invite_graph` is a view over `platform_invites`, kept forever, docs/09).
-- * `invite_prune_runs` is what the last passes did, for the Admin > Invites card.
UPDATE platform_settings
   SET key = 'invites.default_credits'
 WHERE key = 'invites.initial_credits'
   AND NOT EXISTS (SELECT 1 FROM platform_settings WHERE key = 'invites.default_credits');
DELETE FROM platform_settings WHERE key = 'invites.initial_credits';

CREATE TABLE invite_inactive_roots (
    -- Not a foreign key: the mark is display data and never blocks (or follows) an account's lifecycle.
    root_account_id  BIGINT PRIMARY KEY,
    marked_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The root and everyone below it in the invite graph.
    subtree_size     INTEGER NOT NULL CHECK (subtree_size >= 1)
);

CREATE TABLE invite_prune_runs (
    ran_at         TIMESTAMPTZ PRIMARY KEY DEFAULT now(),
    codes_revoked  INTEGER NOT NULL CHECK (codes_revoked >= 0),
    roots_marked   INTEGER NOT NULL CHECK (roots_marked >= 0),
    roots_cleared  INTEGER NOT NULL CHECK (roots_cleared >= 0),
    -- Redeemed edges in invite_graph when the pass finished (it never changes because of a pass).
    graph_rows     BIGINT NOT NULL CHECK (graph_rows >= 0)
);
