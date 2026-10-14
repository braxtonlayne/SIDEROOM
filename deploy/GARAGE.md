# Garage (object storage)

SIDE ROOM talks to any S3-compatible store; Garage is the reference because it is a single static binary and runs on one box.

## Single node (the default)

1. `cp deploy/compose/garage/garage.prod.toml.example deploy/compose/garage/garage.prod.toml` and replace every `CHANGE_ME` (`openssl rand -hex 32` for `rpc_secret`, `openssl rand -base64 32` for the two tokens).
2. `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod up -d garage`
3. Create the layout, key and bucket once. `deploy/compose/garage/bootstrap.sh` does this for the dev stack; for production run the same `garage` commands with your own key (`garage key create sideroom-prod`, `garage bucket create sideroom`, `garage bucket allow --read --write --owner sideroom --key sideroom-prod`) and put the printed key id and secret into `.env.prod` as `GARAGE_ACCESS_KEY` and `GARAGE_SECRET_KEY`.
4. Give the bucket its browser CORS rule: `sr-cli storage ensure-cors --origin https://<your-domain>` (uploads are presigned `PUT`s from the browser).

`replication_factor = 1` means one copy of every block: a dead disk loses data. The nightly rustic snapshot (BACKUP.md) is the safety net on a single node.

## Several nodes

Three nodes in three zones is the smallest layout that survives losing one.

1. On every node use the same `rpc_secret`, set `replication_factor = 3`, and set `rpc_public_addr` to an address the other nodes can reach (a private network or WireGuard address, not the public internet).
2. Start all nodes, then on one of them connect them: `garage node connect <node-id>@<addr>:3901` for each peer (`garage status` lists node ids).
3. Assign capacity and zones, then apply: `garage layout assign -z dc1 -c 500G <node-id>` (repeat per node, different `-z` per site), `garage layout show`, `garage layout apply --version 1`.
4. Point `SR_STORAGE__ENDPOINT` at any node, or at a local load balancer in front of all of them.
5. Changing the replication factor of an existing cluster is not supported in place; plan it before the first upload.

Back up at least one node's data with rustic as in BACKUP.md; replication protects against hardware loss, not against deletion by mistake.

## Checks

- `garage status` shows every node as up; `garage stats` shows free space.
- `GET /readyz` on sr-server reports `storage: ok` when the bucket is reachable.
- Garage exposes Prometheus metrics on the admin port (`/metrics`, with `metrics_token`); add it to `deploy/monitoring/prometheus.yml` if you want storage graphs.
