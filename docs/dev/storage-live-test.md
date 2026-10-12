# Live object-storage tests

The storage code (`sr-storage`, attachment slots, emoji upload, the abandoned-upload
sweep) is covered by unit tests on an in-memory backend. These two opt-in suites run
the same paths against a real S3-compatible server. Neither runs in CI by default.

| Suite | Where | What it proves |
|---|---|---|
| Rust, `ObjectStoreStorage::s3` | `crates/sr-storage/tests/s3_live.rs` | put/get/range/head/list/delete, presigned PUT then GET over HTTP with expiry and bad signatures, multipart above 64 MiB (and abort), EXIF strip on a real JPEG, error mapping for bad credentials and bucket |
| Node, full server | `e2e/support/storage-live.mjs` | `/readyz` storage check, upload slot, presigned PUT, message with attachment, signed CDN URL with and without `Range`, EXIF strip, mismatched type, size limits, emoji data-URI upload, abandoned-upload sweep |

## 1. An S3 server and a bucket

Use throwaway credentials and a throwaway data directory. Never commit them.

### Garage (the project's choice; Linux/macOS/Docker)

```sh
just dev                       # compose stack + deploy/compose/garage/bootstrap.sh
# endpoint http://127.0.0.1:3900, region garage, bucket/keys from deploy/compose/.env(.example)
```

By hand: `garage layout assign -z dc1 -c 1G <node-id>`, `garage layout apply --version 1`,
`garage key create sr-test`, `garage bucket create sr-test`,
`garage bucket allow --read --write --owner sr-test --key sr-test`. Region is `garage`.

### MinIO (stand-in on a platform without Garage)

```sh
MINIO_ROOT_USER=<random> MINIO_ROOT_PASSWORD=<random> \
  minio server <throwaway-data-dir> --address 127.0.0.1:9000 --console-address 127.0.0.1:9001
```

Create the bucket (for example `mc alias set t http://127.0.0.1:9000 <user> <pw>` and
`mc mb t/sr-test`). Use `SR_TEST_S3_REGION=us-east-1`. Use the root credentials as the
access/secret key for a local throwaway instance.

Endpoints must be plain `http://` (the tests speak raw HTTP). The server uses
path-style addressing, so no wildcard DNS is needed.

## 2. Rust tests

```sh
export SR_TEST_S3_ENDPOINT=http://127.0.0.1:9000   # or :3900 for Garage
export SR_TEST_S3_BUCKET=sr-test
export SR_TEST_S3_ACCESS_KEY=...
export SR_TEST_S3_SECRET_KEY=...
export SR_TEST_S3_REGION=us-east-1                 # default "garage"
cargo test -p sr-storage --test s3_live -- --ignored --test-threads=1
```

Objects are written under `sr-live-test/` and deleted by each test. The expiry test
takes about 10 seconds and the multipart test moves about 150 MiB.

## 3. End-to-end through sr-server

Ports: API `127.0.0.1:8092`, metrics `127.0.0.1:9192`. Database `sr_s3` on the local
PostgreSQL (create it, drop it when done).

```sh
createdb -U postgres sr_s3
export DB=postgres://postgres:<password>@localhost:5432/sr_s3
cargo build -p sr-server -p sr-cli

# Server (storage env names come from the [storage] section of sr-core config)
SR_DATABASE__URL=$DB \
SR_SERVER__BIND=127.0.0.1:8092 SR_SERVER__METRICS_BIND=127.0.0.1:9192 \
SR_SERVER__PUBLIC_URL=http://localhost:8092 \
SR_SERVER__SECRET_KEY=$(openssl rand -hex 32) \
SR_SEARCH__DIR=./data/search-s3 \
SR_STORAGE__ENDPOINT=$SR_TEST_S3_ENDPOINT SR_STORAGE__BUCKET=$SR_TEST_S3_BUCKET \
SR_STORAGE__REGION=${SR_TEST_S3_REGION:-garage} \
SR_STORAGE__ACCESS_KEY=$SR_TEST_S3_ACCESS_KEY SR_STORAGE__SECRET_KEY=$SR_TEST_S3_SECRET_KEY \
  target/debug/sr-server &

# Owner account (needed once so `sr-cli invite create` has an issuer)
SR_DATABASE__URL=$DB SR_OWNER_PASSWORD=<pw> target/debug/sr-cli bootstrap \
  --owner-username owner --owner-email owner@example.test

# Driver (needs psql on PATH, or PSQL=path)
SR_TEST_DATABASE_URL=$DB node e2e/support/storage-live.mjs
```

The driver registers one user through an `sr-cli` invite (registration is rate limited
per IP, so run it once per few minutes), then prints `PASS`/`FAIL` per check and exits
non-zero on any failure. It signs its own S3 requests to look at objects directly.

Known gap the driver will report: sr-server signs attachment URLs
(`/cdn/attachments/...?ex=&sig=`) but at the time of writing has no `/cdn/*` route
that verifies and serves them, so the CDN fetch and `Range` checks are expected to
fail until that route exists. The emoji image is checked straight from S3 for the
same reason.

To stop: kill the server and the S3 process, `dropdb -U postgres sr_s3`, delete the
throwaway data dirs.
