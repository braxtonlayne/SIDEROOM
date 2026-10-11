# ADR-0002: Garage for object storage

- **Status:** accepted
- **Date:** 2026-10-11

## Context
We need S3-compatible storage for attachments, avatars, manga pages, video originals, and HLS segments, self-hosted, with presigned URLs and multipart uploads.

## Decision
Use **Garage** (Rust, S3-compatible, lightweight, geo-replication capable). Access it via the `object_store` crate. Deployments may instead point at any S3-compatible service (Backblaze B2, Wasabi, R2) through config.

## Alternatives considered
- **MinIO:** the community edition was stripped down in 2025 and later archived, so it's unsuitable.
- **SeaweedFS (Go):** capable, but not Rust.
- **Plain filesystem:** would block multi-node setups and presigned uploads.

## Consequences
The operator runs one more Rust daemon. Garage has a smaller S3 API surface (no object lock, limited lifecycle rules), so expiry and purge are done by our own jobs, not bucket lifecycle rules.
