-- K-20 (D-262): a staff member moves a Discord identity between accounts of one Person. The old row is kept as
-- history (soft-unlinked: `unlinked_at`, and where it went: `moved_to_account_id`) and a new live row is
-- written for the target account. Only live rows are unique per Discord user; the pair stays unique across
-- accounts, deleted ones included (D-030), among the live rows.
ALTER TABLE external_identities
    ADD COLUMN unlinked_at         TIMESTAMPTZ,
    ADD COLUMN moved_to_account_id BIGINT REFERENCES accounts (id);
ALTER TABLE external_identities DROP CONSTRAINT external_identities_provider_provider_user_id_key;
CREATE UNIQUE INDEX external_identities_live_uidx
    ON external_identities (provider, provider_user_id) WHERE unlinked_at IS NULL;
CREATE INDEX external_identities_provider_user_idx ON external_identities (provider, provider_user_id);
