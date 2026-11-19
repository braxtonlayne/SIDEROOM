# Deployment-day checks (D-1038)

The tests the project chose not to run on a developer's machine, in the order to run them on the first real server. Each says what it proves, when it fits in [GO-LIVE.md](../../deploy/GO-LIVE.md), who does what, what is needed, where the steps are, and where the result is written. This page replaces nothing: each entry points to its own runbook.

Rules for everybody: **the owner types every secret, in the app's own form or on the host, never in a chat and never to an agent.** The commander (or any agent) only reads output the owner pastes with secrets removed, or runs a spec that holds none. Live tests wait for a deployed server (the owner's rule, 2026-11-18). D-1038: a behaviour that needs Safari's own media stack is checked by hand on a real Safari, not in Playwright's WebKit.

Order. GO-LIVE's sections 1 to 4 come first. The backup restore is GO-LIVE section 5, so it comes before the checks that need a signed-in Owner (section 7); that is why it is entry 2 here and not last. Entries 3 to 5 need the Owner signed in and, for the calls, a second account (make one with `sr-cli invite create --note "test"`; GO-LIVE section 9 starts only after these are done). Entries 3 to 6 sit between GO-LIVE section 7 and section 9, "The first invites".

## 1. GO-LIVE sections 1 to 4

- **Proves:** the host, DNS, install and `sr-cli preflight` are green.
- **When:** GO-LIVE 1 to 4, "Preflight is green".
- **Who:** the owner at the keyboard (the installer asks for secrets and writes them); the commander reads the preflight table.
- **Needed:** a server, a domain, the owner's accounts ([owner-checklist.md](owner-checklist.md)).
- **Steps:** [GO-LIVE.md](../../deploy/GO-LIVE.md), sections 1 to 4. Nothing is copied here.
- **Result:** the "Done when" lines in GO-LIVE; anything that needed a workaround goes in [FAQ.md](../../deploy/FAQ.md).

## 2. The backup restore

- **Proves:** a backup can be restored (GO-LIVE section 5; RDY-OPS-002 is the CI version on the smoke stack).
- **When:** GO-LIVE 5, then GO-LIVE 6 (monitoring) and 7 (Setup checklist) before entries 3 to 6.
- **Who:** the owner at the keyboard on the host.
- **Needed:** the host, a scratch database, the off-host repository (`SR_BACKUP_REPOSITORY`).
- **Steps:** [GO-LIVE.md](../../deploy/GO-LIVE.md) section 5 and [BACKUP.md](../../deploy/BACKUP.md). One more sentence: a restore brings back the database, not the search folder (`search.dir`). If search is empty or stale afterwards, `sr-cli reindex` queues a rebuild (`crates/sr-cli/src/ops.rs`, `reindex`). `docs/12-deployment-and-operations.md` (D-1037) says only that an upgrade needs no reindex; it says nothing about a restore.
- **Result:** GO-LIVE's "Done when" (restore succeeds, `restore-drill.sh` exits 0); write the date in the closing table.

## 3. The R2 test (only if the owner uses a bucket of his own)

- **Proves:** real uploads, CORS, links, a shelf item, listing and deletes against a real Cloudflare R2 bucket (D-1029).
- **When:** after GO-LIVE 7 (the Owner is signed in and the storage test is green).
- **Who:** the owner at the keyboard (makes the token, sets the CORS rule, types the keys into User settings, Storage); the commander runs the spec with no secret in it, after the owner says "connected".
- **Needed:** an R2 bucket, an object-scoped token for that bucket (D-1032), the mount labelled `sideroom-test` with prefix `sideroom-test/`.
- **Steps:** [r2-byos-test.md](r2-byos-test.md), whose origin, stack start and clean-up are rewritten for the server first (see its top note). Its spec signs in with the e2e Owner of a test stack; on a real server that sign-in does not exist, so the commander and owner settle how the spec signs in before the run, still without any credential in the spec.
- **Garage instead:** with Garage on the server this entry is not needed. [storage-live-test.md](storage-live-test.md) is a developer and CI test (it runs against the compose Garage in `storage.yml`); on the server, Garage is covered by the storage row of `sr-cli preflight` (GO-LIVE 4) and the storage test of the Setup checklist (GO-LIVE 7). [media-pipeline-test.md](media-pipeline-test.md) also runs in CI and does not wait for a server.
- **Result:** the commander's report to the owner; no inventory row ticks (D-1029 is the decision). Revoke the token afterwards.

## 4. Voice on the real network

- **Proves:** voice works across the internet, and TURN relays when a direct path is not possible.
- **When:** after GO-LIVE 7 (the Setup checklist's voice and TURN row is green).
- **Who:** the owner and a second person, each at their own device.
- **Needed:** two accounts; two networks, one of them a phone on mobile data (this is what exercises TURN).
- **Steps:** (1) Both sign in and join the same voice channel, one from home Wi-Fi, one from the phone on mobile data. (2) Each says a sentence; the other hears it. (3) Each turns the camera on and off once. (4) The mobile one switches Wi-Fi on, then off again; the call comes back by itself. (5) Leave and rejoin; both hear each other again. If a step fails, write which one, on which network.
- **The soak (optional, RDY-VOX-002):** `.github/workflows/voice-soak.yml` cannot target a deployed host. It builds the web app, starts its own `sr-server` on `http://localhost:8080` on the CI runner and runs ten Chromium contexts against that. So on a real server the hand check above is the row's evidence; the soak stays a CI dispatch.
- **Result:** the date and the two networks in the closing table; RDY-VOX-002 keeps its text and gains "the two-network call on the real server: see `docs/dev/deployment-day-checks.md`".

## 5. Safari, by hand (D-1038)

Needs a real Safari (a Mac, or an iPhone) and a second member in the same call or channel. The owner does both.

**5a. Music in a call (MUS-JB-001).** What it proves: the stereo music track reaches a Safari listener through Safari's own media stack.
1. On another device, a member joins a voice channel and starts a track from the Jukebox (Add tracks, then play).
2. On the Safari, sign in and join the same voice channel.
3. Listen: the music is heard along with the voices. The call shows a "Music in the call" group with a mute button and a volume control. With headphones or two speakers, the music is in stereo (left and right differ), not the same on both sides.
4. Mute the music with its button: the voices continue and the music stops. Unmute: it returns.
5. Change network (turn Wi-Fi off so the device goes to mobile data, or the other way). The voices and the music come back within about half a minute without you rejoining.

**5b. Live TV's discontinuity (TV-HLS-005).** What it proves: when one programme ends and the next starts, Safari carries on playing without stalling or going blank. The server side is already tested (`both_clients_see_the_discontinuity_at_the_programme_boundary` in `crates/sr-tv/tests/hls_sync.rs`: the playlist marks the new programme with `#EXT-X-DISCONTINUITY`, and the numbering carries on across the boundary). Only Safari's playback is left.
1. As the Owner, put two short programmes back to back on a Live TV channel (the second starting about two minutes after you begin watching), or find a channel where a boundary is a few minutes away.
2. On the Safari, open Live TV and the channel; the picture plays.
3. Watch the change of programme at the boundary (the player must not be paused).
4. Pass: the picture and sound continue into the new programme within a few seconds, with no frozen frame, no blank screen and no error. Fail: it stalls, loops the old programme, or needs a reload.
5. Open the same channel on a second device. The two show the same programme and are within a few seconds of each other (the two-client sync, TV-HLS-005's first half).

- **Result:** the date, device, Safari version and pass or fail go in the closing table. The rows' text changes are proposed in the handoff.

## 6. The payment walk (only if donations are to be on)

- **Proves:** an invoice paid from a signet wallet is confirmed and unlocks Patron (RDY-PAY-001).
- **When:** after GO-LIVE 7 (the Setup checklist's wallet row), before real donations are switched on.
- **Who:** the owner at the keyboard (the wallet, the watch-only descriptor, the signet payment).
- **Needed:** a funded signet wallet and a second signet wallet's watch-only descriptor; the signet Esplora URL in the server's environment ([payments-live-test.md](payments-live-test.md), "Before you start"); an ordinary member account.
- **Steps:** [payments-live-test.md](payments-live-test.md), "The end-to-end walk" (7 steps).
- **Result:** a row in that page's "Recorded runs" table. RDY-PAY-001 ticks on the first recorded run. Afterwards follow the page's "How to undo it" before the first real invite.

## 7. The playbooks (RDY-DOC-003)

- **Proves:** each playbook in [RUNBOOK.md](../../deploy/RUNBOOK.md) works when followed on a running stack.
- **When:** after GO-LIVE 6 and 7, as each situation can be staged; "Restore from backup" is walked with entry 2. RUNBOOK's own status line asks the owner to walk them during the go-live rehearsal.
- **Who:** the owner at the keyboard; the commander may read output.
- **Steps:** each playbook, from its anchor. When all are walked, RUNBOOK's "Status of the walk-through" line is replaced with the date and the stack.
- **Result:** the date column below; RDY-DOC-003 ticks when every line has a date.

| Playbook (anchor in RUNBOOK.md) | Walked on (date) |
|---|---|
| `#job-backlog` | |
| `#voice-egress` | |
| `#invoice-stuck` | |
| `#storage-usage` | |
| `#backup-missing-or-stale` | |
| `#wal-archive-lag` | |
| `#restore-drill-failed` | |
| `#csp-reports-spike` | |
| `#suspicion-case-opened` | |
| `#takedown-notice-received` | |
| `#data-subject-request-received` | |
| `#suspected-compromised-staff-account` | |
| `#lost-owner-second-factor` | |
| `#disk-full` | |
| `#restore-from-backup` | |
| `#a-member-reports-csam` | |

## What no case and no CI job proves (wave 113)

Watch these first on the day; they are the faults wave 113 fixed on paper only.

- A real systemd start of `sr-server` with the hardening on (`ProtectSystem=strict`, read-only root): the index opens under `/var/lib/sideroom/search` (SEC-AUD-566).
- A real timer firing: `sideroom-backup-compose`, `sideroom-restore-drill-compose`, `sideroom-basebackup`; the dump lands in `pg-backups` and Admin > System shows it (SEC-AUD-567).
- A search after a container recreate, and after an upgrade of an old Compose install (`sr-cli reindex` once) (SEC-AUD-570).
- Compose's env merge on a real engine (`environment:` over `env_file:`), and `required: false` needing Compose 2.24 or newer (SEC-AUD-569, 576).
- GitHub's real notes limit and `gh release create` itself; the first tag is the test (SEC-AUD-561).
- The ghcr packages' visibility, and the source label linking the images to the repository (SEC-AUD-563).
- The installer's first sign-in printout on a real terminal (SEC-AUD-568).

## The record

| Check | Row | Done on (date) | By | Result |
|---|---|---|---|---|
| 1. GO-LIVE 1 to 4 | RDY-OWN-003 | | | |
| 2. Backup restore | GO-LIVE 5 | | | |
| 3. R2 test | D-1029 | | | |
| 4. Voice, two networks | RDY-VOX-002 | | | |
| 5a. Safari, music | MUS-JB-001 | | | |
| 5b. Safari, Live TV | TV-HLS-005 | | | |
| 6. Payment walk | RDY-PAY-001 | | | |
| 7. Playbooks | RDY-DOC-003 | | | |
