# Load testing (XC-PERF-003, RDY-PERF-001 to 004)

`sr-loadtest` (crate `crates/sr-loadtest`) drives a running `sr-server` the way the product is used and checks the run against the targets of doc 12 and the wave 24 roadmap: **2,000 concurrent gateway sessions, 200 chat messages per second, 300 REST requests per second, REST p95 under 100 ms, fan-out p95 under 250 ms, and first paint for a member of 60 servers under 1.5 s with no 429**. It is plain Rust (tokio + tokio-tungstenite, a small keep-alive HTTP client), so it needs no k6 or other download.

## Contents

1. [What it does](#what-it-does)
2. [Running it](#running-it)
3. [The seeded dataset (`sr-cli seed --scale`)](#the-seeded-dataset)
4. [The CI run (`load-test.yml`)](#the-ci-run)
5. [Results](#results)
6. [Jobs at scale](#jobs-at-scale)
7. [Gateway memory](#gateway-memory)
8. [Findings and limits](#findings-and-limits)

## What it does

1. Seeds accounts with ready-made sessions, spread over guilds that each have `#general`, straight into the database (argon2 logins would measure the wrong thing). It refuses a database whose name does not contain `load`, `test`, `scratch`, `smoke`, `e2e`, `dev` or `tmp` unless `--allow-any-database` is given. It also seeds one probe account that is a member of `--first-paint-guilds` servers (default 60).
2. Opens the gateway sessions at `--ramp-rate` per second (cookie auth, IDENTIFY, READY, heartbeats).
3. For `--duration` seconds posts `--msg-rate` messages per second (each carries its send time) and runs `--rest-rps` of profile and history reads on `--rest-workers` workers. Halfway through, the probe account loads the Lounge the way the web app does: a new gateway session plus `GET /users/@me`, `GET /users/@me/guilds` and `GET /users/@me/guild-settings` at the same time (RDY-SCALE-002: one request for the settings of all servers, not one per server).
4. Reports sessions ready, achieved send rate, gateway fan-out latency (POST to dispatch), delivery ratio, REST p50/p95/p99 and error rate, the probe's first paint (wall time, READY frame size, 429 count). Exit code 0 only when all targets are met; `--report file.json` writes the numbers.

## Running it

```
createdb sr_loadtest && sr-cli migrate              # SR_DATABASE__URL pointing at it
sr-server &                                         # a release build, on the same database
sr-loadtest --base-url http://127.0.0.1:8080        # the full 2,000 / 200 / 100 ms run
sr-loadtest --sessions 300 --guilds 20 --msg-rate 100 --rest-rps 100 --duration 15 --first-paint-guilds 10   # quick check
```

Use a release build of the server and a quiet machine for the real run; set `--run-id` to run again on the same database. Rate limits are per account, so one box generating the load is fine. `--fanout-p95-ms`, `--first-paint-ms` and `--p95-ms` change the limits.

## The seeded dataset

`sr-cli seed --scale` (crate `sr-cli`, `src/seed_scale.rs`) loads the dataset the sizing claim is about, through `COPY`, in one transaction:

| What | Full profile |
|---|---|
| Accounts | 20,000 |
| Guilds | 500, rank `r` has `max(3, 0.6 x accounts / r)` members (the biggest holds 60 % of the accounts, the 500th 24), 5 text channels and a category each, an `@everyone` role |
| Messages | 5,000,000 over the last 30 days, favouring big guilds and the first channels of a guild; 6 % carry an attachment row, 10 % carry one to three reactions |
| Library | 2,000 chapters in 200 series (one default edition each), all published |
| Cinema | 300 titles (movie, series, anime), published, no media |

```
sr-cli seed --scale                                   # the full profile, seed name "scale"
sr-cli seed --scale --seed-name mid --scale-accounts 3000 --scale-guilds 60 --scale-messages 300000 --scale-chapters 200 --scale-titles 60
```

- **Local only.** It refuses an instance whose `server.public_url` is not local unless `--force`, like `seed`.
- **Idempotent per seed name.** The marker is the account `<name>_00000`; a second run with the same name stops with a sentence. Another name loads beside it. The whole load is one transaction, so a failed run leaves nothing behind.
- **Reproducible.** Everything derives from the seed name and the counts (splitmix64, no extra crate). Ids are snowflake-shaped with the name's hash in the worker and process bits, so two names never collide with each other or with a running server's ids. Accounts all take the `--password` of `seed` (hash computed once); log in as `<name>_00000` and so on.
- **No objects.** Attachment and library rows point at keys that do not exist in storage. That is right for database load and `EXPLAIN`; opening a page in the reader will 404.
- **Statistics.** It runs `ANALYZE` on the loaded tables at the end so the planner sees the real sizes.
- **Test.** `seed_scale::tests::a_tiny_scale_seed_loads_once_and_keeps_its_shape` (Postgres) loads 40 accounts, 5 guilds and 300 messages, checks the counts, the membership skew, that every author is a member of the guild of the message, that `channels.last_message_id` is the newest message, that a second run is skipped and that a second name loads beside the first. The full 20,000-account load is **not** run on a development machine.
- **Speed.** Debug build on the shared laptop, 3,000 accounts, 60 guilds, 300,000 messages, 18,245 attachments, 60,568 reactions: 30 s (messages 11.7 s, attachments and reactions 15.7 s, `ANALYZE` 2.2 s). That is about 10,000 messages per second without an optimiser, so the 5 million message profile is expected in single-digit minutes on a release build; the CI run records the real figure (the target is under 30 minutes).

## The CI run

`.github/workflows/load-test.yml` (dispatch only, standard `ubuntu-latest` runner, release build, Postgres 17 service) builds `sr-server`, `sr-cli` and `sr-loadtest`, migrates, seeds the full dataset, starts the server, runs `sr-loadtest` with 2,000 sessions, 200 msg/s, 300 REST/s for 120 s and one probe member of 60 servers, then records the table sizes, the server's `VmRSS`/`VmHWM` and uploads `load-report.json` and the logs. **Written, not yet run**: the owner dispatches it (Actions > Load test > Run workflow). The first dispatch uses `seed_scale=true`, `sessions=1000`, `msg_rate=100`, `rest_rps=150`, `duration=120` (half the target load, to see whether a standard runner holds the seed and the sessions); later dispatches use the defaults (2,000 sessions, 200 msg/s, 300 REST/s, 120 s), and only a run at the defaults ticks RDY-PERF-002. After the first run, paste the numbers into [Results](#results) with the run link; every miss gets a profile (`tracing` spans and `EXPLAIN`; `tokio-console` is not added) and a fix or a documented limit under [Findings and limits](#findings-and-limits).

## Results

| Run | Result |
|---|---|
| Debug builds, shared Windows laptop: 60 sessions, 20 msg/s | pass |
| Same: 300 sessions, 100 msg/s, 100 REST/s | pass: fan-out p95 27 ms, REST p95 18 ms, 100 % delivered |
| Same: 2,000 sessions, 200 msg/s, 300 REST/s | all sessions ready and every dispatch delivered, but 172 msg/s sent and REST p95 2.9 s: CPU bound (unoptimised server, 8 other builds running) |
| Release build on CI, 2026-10-25, dispatch run 40226523570: seed 178 s; 1,000 sessions, 100 msg/s, 150 REST/s, 120 s | **fail**: 1,000/1,000 ready in 10 s; 95 msg/s sent; REST p50 6.4 s, p95 7.2 s; fan-out p50 19.4 s, p95 42.7 s; 76 % delivered before the sessions closed; first paint at 60 servers 37.7 s with no READY; server `VmRSS` 617 MB. Causes and fixes under [Findings and limits](#findings-and-limits) |
| Release build on CI, run 40234956676 (after the fixes below, head 4dcbcc62): same half profile | **pass**: REST p95 30.8 ms, fan-out p95 36.4 ms, 100 % delivered, first paint 596 ms, `VmRSS` 529 MB, seed 149 s |
| Release build on CI, run 40239291091 (head 4dcbcc62): the defaults, 2,000 sessions, 200 msg/s, 300 REST/s, 120 s | **fail**: 2,000/2,000 ready in 20 s, 100 % of 487,240 delivered; REST p50 49.5 ms, p95 294.9 ms; fan-out p50 86 ms, p95 538.5 ms; first paint 1,803 ms; `VmRSS` 945 MB; seed 114 s |
| **Release build on CI over the seeded dataset (RDY-PERF-002)** | **not yet passed at the defaults** (workflow written; owner dispatch needed). Record here: date, run link, sessions ready, msg/s, REST p50/p95/p99, fan-out p95, first paint at 60 servers and its 429 count, server `VmRSS`, seed time |

## Jobs at scale

Measured on the shared laptop with debug builds on the `mid` seed (3,000 accounts, 60 guilds, 300,000 messages of which 99,976 are marked deleted and due for purge, 50,449 pins, 179 MB database). Numbers scale roughly linearly; the 5 million message figures are extrapolations until the CI dataset is used. "Lock" is how long a job holds row locks other transactions could wait on.

| Job | Bounded how | Time on `mid` (debug) | Lock held | Notes |
|---|---|---|---|---|
| `messages.purge_deleted` | 1,000 messages per batch, at most 100 batches per run, each batch one transaction with `lock_timeout` 1 s and `statement_timeout` 30 s, rows chosen `FOR UPDATE SKIP LOCKED` | one 1,000-message batch deletes in 16 ms (70 ms before pins existed in the data; 1,603 ms with 50,449 pins and no index) | at most the 16 ms of a batch | the due-row query (`purge_after` index) takes 0.7 ms. Index `pins_message_idx` (migration 1300): the `ON DELETE CASCADE` from `messages` to `pins` scanned all of `pins` once per deleted message |
| `audit.purge` (retention) | 5,000 rows per batch, at most 500 batches per run, bounded transaction, `SKIP LOCKED`; cut by `created_at` through `audit_log_created_idx` | not measured at scale (the seed has no audit rows); test inserts 5,100 and purges them in two batches | at most one batch | was one unbounded `DELETE` |
| `search.reindex` | pages of 1,000 messages per channel by key, one attachment lookup per page (was one per message), a commit every 50,000 documents (was one commit and reader reload per 500) | 200,024 visible messages, debug build: 380 s before, 30 s after (12 times faster) | none (reads) | the index writer is single; the DB side is plain index range scans on `(channel_id, id)` |
| `insights.daily` (the aggregates behind Server Insights) | one transaction over the previous two days; the message range prunes to one partition and uses the `(channel_id, id)` index-only scan | 87 ms for a day of ~10,000 messages (about 1.5 s expected for a day at 5 million messages per 30 days) | writes only its own day's rows | unchanged |
| Suspicion scan (`moderation.suspicion_housekeeping`) | already sliced: login history purged in slices, scan limited to `SCAN_BATCH` recent sign-ins | the seed has no sign-in history, so there is nothing to time here | none | unchanged; it reschedules itself after 5 minutes while a backlog remains |
| Trending (cinema) | not a job: computed on request over the last 7 days of progress and cached (`with_trending_cache`) | with 60 titles and no progress rows, below the timer's resolution | none | unchanged |
| Backup (`pg_dump -Fc`, `sr-cli backup` and `backup.sh`) | one consistent snapshot; takes only `ACCESS SHARE` locks, so writers are not blocked | 2.1 s for the 179 MB database (12.5 MB dump) | no write lock | scales with the database size; the disk and the health job's `BackupMissingOrStale` rule watch it |

The batching helper is `sr_db::bounded` (a transaction with `lock_timeout` and `statement_timeout` set with `SET LOCAL`); `jobs_scale_pg_tests` proves a locked message is skipped and taken on the next run, `bounded` proves the timeouts apply to the transaction only and that a blocked batch gives up instead of waiting.

**Indexes.** Migration 1300 is the only index `EXPLAIN` demanded: `pins (message_id)`. A scan of every foreign key without a leading index found 145 others (mostly `created_by` and `decided_by` columns on tables that stay small, and per-guild feature tables); none is on the path of a job above, so they were left alone rather than added speculatively. Migrations 1301 to 1309 stay unused.

## Gateway memory

`crates/sr-gateway/src/memory_budget_tests.rs` (RDY-PERF-004) counts live heap bytes with the test-only allocator in `crates/sr-memcount` on a current-thread runtime, so every gateway task runs on the test thread:

| Figure | Measured | Budget (fails above) |
|---|---|---|
| Heap per session with one guild (includes the in-process test client side) | 7.4 KB | 11.2 KB |
| Heap per further guild subscription of a session | 14.3 KB | 21.5 KB |
| READY frame of a member of 100 servers (test directory, minimal user) | 3.5 KB | 5.4 KB |
| One GUILD_CREATE (identical for a guild with 0 or 5,000 roster members) | 113 B | 200 B |

The budget is 1.5 times the measured figure, so a change that doubles the cost of a session fails the test; a legitimate increase is re-measured (`cargo test -p sr-gateway memory_budget -- --nocapture`), written here and raised in the same commit. Member lists are lazy: `GUILD_CREATE` does not depend on the roster, and `REQUEST_GUILD_MEMBERS` returns chunks of at most 1,000 (the test pulls a 5,000 member guild in chunks). The test counts heap only; kernel socket buffers and the real READY payload (which carries the account's own object, settings and read state) are measured by the CI run (`ready_bytes` of the first-paint probe and the server's `VmRSS` divided by 2,000 sessions).

## Findings and limits

- **First CI run (40226523570).** The database was the bottleneck, not the seed, logging or the dead storage endpoint (no request touched storage; `tower_http` logged 18,957 requests and no errors). REST latency was 0.3 s in the first 5 s of the steady state, climbed to 6 s within 25 s and stayed there with the server just keeping up (about 115 requests/s): a fixed queue for pool connections, with every statement waiting about 300 ms. A local count of SQL statements (`sqlx::query` at debug) put two thirds of them in the gateway fan-out: each guild dispatch read channel, guild, member, roles and overwrites once per recipient, about 63 statements per message against 31 for the POST itself. Fixed: `GuildDirectory::dispatch_audience` decides for the whole audience from one read of the channel data and one member query. The firehose consumer inserted one job per message, fell behind its 4,096-event tap at about 45 s and queued a full `search.reindex` of the 5 million messages during the measured window; it now inserts the waiting events' jobs in one statement. Index jobs committed tantivy once per message (a few dozen per second at most); one handler run now takes up to 256 queued jobs into one commit. The "24 % not delivered" were late, not lost: the run closed the sessions 3 s after the steady state while the per-guild dispatch tasks were 20 to 44 s behind; `sr-loadtest` now waits for stragglers (up to 30 s). The first-paint probe got no READY because the hub built 60 GUILD_CREATEs one after the other, each reading its members one query per member; they are now read in one query and four guilds are built at once, and a missing READY is reported as such instead of as a failed request.
- **Run 40239291091 (the defaults).** REST p95 was about 110 ms in most 5 s buckets, but 20 s of it, 45 s into the run, reached 1.4 s with throughput down a third and nothing in `server.log`: Postgres had logged WAL checkpoints every 8 to 20 s through the seed and began a spread checkpoint 11 s before the server, and autovacuum's first pass over the fresh tables was due then. `load-test.yml` now runs `VACUUM (ANALYZE)` and `CHECKPOINT` between the seed and the server. A smaller bump at the midpoint is the first-paint probe. For the rest, the POST path was the largest database cost; AutoMod no longer reads the channel and member when a server has no rules (4 statements), the created message is rendered once for the answer and the dispatch (5), and GUILD_CREATE reads its overwrites in one query and its guild data once (1,766 to 1,295 statements for a 60-server READY with 3 members per server).
- **Still open after that run.** `POST /channels/{id}/messages` resolves the channel, guild, member, roles and overwrites twice (route access check and `ChatService::send_message`), now 24 statements per message with the AutoMod rules cached; passing the route's access into the service would save 5 more. `GET /users/@me/guilds` reads member and roles per guild (121 statements at 60 servers). Neither was the cause of the miss.
- The settings of 60 servers used to be 60 requests at load. `GET /users/@me/guild-settings` returns them all; the Vitest `p24SettingsBatch` proves a session in 100 servers sends one settings request (and none per server when the batch fails), and the load test's probe checks there is no 429 at 60 servers.
- The 5 million message reindex dominated by per-page commits is the one job speed finding fixed here; the rest is bounded and measured as above.
- Not measured on this machine: anything at the full 20,000 account size, release-build throughput, resident memory per session on the real server. Those wait for the first dispatched run.
