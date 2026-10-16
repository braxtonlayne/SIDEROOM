# Load testing (XC-PERF-003)

`sr-loadtest` (crate `crates/sr-loadtest`) drives a running `sr-server` the way the product is used and checks the run against the targets of doc 12: **2,000 concurrent gateway sessions, 200 chat messages per second, REST p95 under 100 ms**. It is plain Rust (tokio + tokio-tungstenite, a small keep-alive HTTP client), so it needs no k6 or other download.

## What it does

1. Seeds accounts with ready-made sessions, spread over guilds that each have `#general`, straight into the database (argon2 logins would measure the wrong thing). It refuses a database whose name does not contain `load`, `test`, `scratch`, `smoke`, `e2e`, `dev` or `tmp` unless `--allow-any-database` is given.
2. Opens the gateway sessions at `--ramp-rate` per second (cookie auth, IDENTIFY, READY, heartbeats).
3. For `--duration` seconds posts `--msg-rate` messages per second (each carries its send time) and runs `--rest-rps` of profile and history reads on `--rest-workers` workers.
4. Reports sessions ready, achieved send rate, gateway fan-out latency (POST to dispatch), delivery ratio, REST p50/p95/p99 and error rate. Exit code 0 only when all targets are met; `--report file.json` writes the numbers.

## Running it

```
createdb sr_loadtest && sr-cli migrate              # SR_DATABASE__URL pointing at it
sr-server &                                         # a release build, on the same database
sr-loadtest --base-url http://127.0.0.1:8080        # the full 2,000 / 200 / 100 ms run
sr-loadtest --sessions 300 --guilds 20 --msg-rate 100 --rest-rps 100 --duration 15   # quick check
```

Use a release build of the server and a quiet machine for the real run; set `--run-id` to run again on the same database. Rate limits are per account, so one box generating the load is fine.

## Measured so far

On a shared Windows laptop with debug builds of both sides (not representative of a deployment):

| Run | Result |
|---|---|
| 60 sessions, 20 msg/s | pass |
| 300 sessions, 100 msg/s, 100 REST/s | pass: fan-out p95 27 ms, REST p95 18 ms, 100% delivered |
| 2,000 sessions, 200 msg/s, 300 REST/s | all sessions ready and every dispatch delivered, but 172 msg/s sent and REST p95 2.9 s: CPU bound (unoptimised server, 8 other builds running) |

The first release-build run on CI-class hardware decides whether the full targets hold; `.github/workflows/load-test.yml` runs it on demand.
