# The owner's action checklist

Everything that only the owner (an account, the host, legal, a yes to a download) or the integrator (a push, a dispatch) can do, in one ordered list. Tick a box when it is done. The decisions behind the rows stay in [the decisions log](../01-decisions-log.md); [open-questions.md](../open-questions.md) points here instead of repeating the list. Source of the plan: [roadmap-p20.md](roadmap-p20.md) section 8 and the owner round (D-308 orders the human actions).

Nothing in this list is done by an agent: agents never push, merge, download or spend money without a clear yes.

How to read a row: **What** (the action), **Unblocks** (what becomes possible or provable when it is done).

## 1. Accounts

- [ ] **Register the second Discord redirect URI** on the existing Discord application: `<public_url>/api/v1/connections/discord/callback` (D-292). Admin > System shows the exact URI once the instance has its public URL. **Unblocks:** the "Connect Discord" button on Settings > Connections, which stays hidden until the URI is registered.

## 2. Host

Do these in the order of [deploy/GO-LIVE.md](../../deploy/GO-LIVE.md); this list only says what each one unblocks.

- [ ] **Provision and harden the host** (doc 12 section 8: firewall with only 22, 80, 443, 3478, 5349 and the voice ports open, SSH keys only, unattended upgrades, fail2ban, disk encryption where the provider supports it). **Unblocks:** everything below.
- [ ] **Point DNS** at the host (the public hostname, and the TURN hostname if it differs). **Unblocks:** TLS issuance, the installer, `sr-cli preflight`'s public URL check.
- [ ] **Run the first real install** from the shipped files (`deploy/install.sh`, then `sr-cli preflight`). **Unblocks:** the exit gates that have never run on a real host (doc 13), every operations proof, and the first invites.
- [ ] **Create the ingest folder** `/srv/sideroom/ingest`, or mount the Storage Box there and set `SR_INGEST_DIR` (D-296). **Unblocks:** local-folder import in the Library and Cinema ingest.
- [ ] **Order and connect the Storage Box** (SFTP target for backups, doc 12 section 5; the repository goes in `SR_BACKUP_REPOSITORY`). **Unblocks:** off-host copies of the dumps, the WAL archive and the object snapshots; without it a dead disk takes the backups with it.
- [ ] **Web Push keys: nothing to generate.** The VAPID key pair is created by `sr-server` on first start and stored sealed in the database under `server.secret_key` (`crates/sr-server/src/app.rs`, `sr_push::load_or_create_vapid`). The action is only to keep `server.secret_key` safe and in the backup (`deploy/BACKUP.md`). **Unblocks:** Web Push on the installed PWA.
- [ ] **Decide the Bitcoin path** for payments (D-288, D-312): either run a non-pruned `bitcoind` on the host and the `electrs` Compose profile (replace the image placeholder in `deploy/compose/prod.yml` with the pinned digest you trust, point `SR_ELECTRS_DAEMON_RPC_ADDR` at the node), or keep the documented interim `SR_PAYMENTS_ESPLORA_URL=https://mempool.space/api`. **Unblocks:** live invoice addresses without third-party address leakage (the interim works but sends derived addresses to a third party).
- [ ] **Set the lawyer settings** after the brief is answered (see section 4). **Unblocks:** the entrance pages and the privacy page showing the operator's country, contact and minimum age.
- [ ] **A real device pass:** Safari voice E2EE and the installed iOS PWA on a Mac or iPhone; the manual screen-reader pass (NVDA on Windows is free). **Unblocks:** removing "untested on Safari" and the accessibility gate.
- [ ] **Run the two real-use gates with friends:** two weeks of daily chat (phase 1) and a two-hour 10-person voice session (phase 2). **Unblocks:** those exit gates in doc 13.

## 3. GitHub

Pushes and dispatches are the owner's call; the integrator performs them once the owner says yes. Several of these workflows have never run on GitHub.

- [ ] **Push the integration branch** so the workflows below are on `main`. **Unblocks:** every row of this section.
- [ ] **Branch protection:** turn on "Require review from Code Owners" and make `Repo checks`, `Web` and `Release dry run` required checks (and `visual` when it exists). **Unblocks:** `.github/CODEOWNERS` taking effect; until then it is advisory.
- [ ] **First green run of `release.yml`** (static binaries and images, with `THIRD_PARTY_NOTICES.md` copied in). **Unblocks:** a release the installer and `UPGRADE.md` can point at.
- [ ] **First green run of `openapi.yml`.** **Unblocks:** the spec drift guard.
- [ ] **First green run of `storage.yml`** (real S3 multipart against Garage). **Unblocks:** the chat multipart upload and composer reattach proof.
- [ ] **First green run of `e2e.yml`.** **Unblocks:** FND-CI-004 and the three-browser proof.
- [ ] **First scheduled or dispatched run of `sdk-compat.yml`.** **Unblocks:** ticking DEV-SDK-003 (D-306).
- [ ] **Dispatch `load-test.yml`** (release build). **Unblocks:** the load target figure (2,000 sessions, 200 msg/s, REST p95 under 100 ms), which a debug build on a laptop missed.
- [ ] **First run of `backup-drill.yml`.** **Unblocks:** proof that the backup, restore and WAL scripts do not rot.
- [ ] **Dispatch `prod-smoke.yml`** once wave 20 is merged (it also runs on pull requests that touch `deploy/**`, the workflow, `crates/sr-cli/**` or `crates/sr-preflight/**`, D-354, so the phase-20 PR runs it first). **Unblocks:** the proof that the production stack boots from the shipped files. Make it a required check in branch protection only after it has been green on the default branch at least twice (D-354).
- [ ] **Dispatch `visual-baselines`** (the manual run of `e2e.yml`, D-307), then commit the downloaded `*-chromium-linux.png` files in a PR. **Unblocks:** the visual regression job (L-21, L-22) and its required check.
- [ ] **Triage the first `pnpm audit` and `shellcheck` output** in Actions (XC 4): fix or waive each finding. **Unblocks:** a clean `Repo checks`.

## 4. Legal

- [ ] **Hand the brief to a lawyer:** [docs/legal/lawyer-brief.md](../legal/lawyer-brief.md). It is questions only, not legal advice. **Unblocks:** every row below.
- [ ] **Set `accounts.minimum_age`** to what the lawyer says for the operator's country (default 13). Admin > Features > Compliance.
- [ ] **Set `compliance.operator_country`** (ISO two-letter code, for example `DE`). **Unblocks:** the privacy page naming the jurisdiction.
- [ ] **Set `compliance.operator_contact`** (the address copyright notices go to) and check that `/help` publishes it. **Unblocks:** the notice-and-action path being real: a notice address that nobody reads is worse than none.
- [ ] **Decide the two open build questions:** mailing the templated takedown reply (N-35) and the mandatory external reporting step (D-336 (h)). **Unblocks:** whether those two items are built.
- [ ] **Approve the repeat-infringer wording** for the house notes and `/help` (the brief names it). **Unblocks:** publishing the strike rule.

## 5. Downloads waiting for a yes

An agent downloads nothing without your word. Each row says what the download is and what it unblocks.

- [ ] **ffmpeg on the GitHub runner** (`apt-get install ffmpeg` in a workflow step; nothing on your machine). **Unblocks:** the media proofs of wave 22 (the transcode, playback and Live TV gate). This is the one download the plan depends on.
- [ ] **The `electrs` container image** for the `payments` profile (pinned by digest; see the Bitcoin row above). **Unblocks:** self-hosted invoice addresses.
- [ ] **Upstream licence texts** for the placeholders in `THIRD_PARTY_NOTICES.md` (the FreeType year, the NTP and Zlib holders; the note in that file names the upstream file each text comes from). **Unblocks:** a notices file with no placeholder. The alternative is to keep the "To complete" note.
- [ ] **Optional: ffmpeg on a development machine** (human action 5, D-269). **Unblocks:** running the pipeline and the TV slate locally. Not required.
- [ ] **Optional: gitleaks in CI.** **Unblocks:** a second secret scanner; wave 21 ships a no-download scanner first.
- [ ] **Optional: discord.py and twilight** for the compatibility suite, and a Node WebRTC library for TypeScript SDK voice (each with a licence check). **Unblocks:** wider SDK coverage.
- [ ] **Optional: one GPU rental** to run the NVENC path of `sr-transcoder` once (`deploy/TRANSCODER.md`). **Unblocks:** the proof of the hardware path; the CPU path in CI is enough for the plan.

## 6. Development machines

- [ ] **Install pnpm:** `corepack enable && corepack prepare pnpm@10.12.1 --activate` on every machine (D-308). **Unblocks:** dropping the `npx pnpm@10.12.1` workaround.
- [ ] **Optional: review a sample of the Markdown corpus against Discord** (100 cases, OQ-15). **Unblocks:** replacing the remaining guessed expectations. Not launch-blocking (D-308).
- [ ] **Check CSP report delivery from an ordinary browser once** (D-365, RDY-SEC-009). With `sr-server` and Vite running, open the dev site in your everyday browser (not Playwright: the Reporting API sends nothing under automation), sign in as an operator, run `document.body.insertAdjacentHTML('beforeend','<img src="https://blocked.example.test/x.png">')` in the console, wait up to a minute, then open Admin > System "Blocked content reports" (or `select * from csp_report_counters`) and look for the `img-src` row with origin `https://blocked.example.test`. Ignore the `style-src` rows: the dev page's Report-Only policy produces those by itself. **Unblocks:** the one unproven link of RDY-SEC-009 (a browser reaching `/internal/csp-report` through the proxy); the server side is already proven by tests. One minute, no download.
