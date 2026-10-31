-- ACT-CH-001: an Activity bundle may declare capabilities in `sideroom.json` at its root
-- (`{"capabilities": ["datachannel"]}`). Staff see them when they review the bundle, and an instance of the
-- reviewed bundle is told which it has: `datachannel` lets the Activity use `sdk.state` and `sdk.input`
-- (the SFU-relayed, client-encrypted data channel). A development build gets every capability (only its
-- owner and testers can run it). Forward-only.
ALTER TABLE application_activity_bundles
    ADD COLUMN capabilities TEXT[] NOT NULL DEFAULT '{}';
