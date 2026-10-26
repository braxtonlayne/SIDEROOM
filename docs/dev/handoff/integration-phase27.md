# Integration handoff: wave 27 (BYOS core) on `integration/phase27`

Integrator: Opus 5.5, 2026-10-26. Base `5e11943d`. Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p27-integ`,
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-integ-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`,
scratch DB `sr_p27_integ`. Nothing pushed, no PR, nothing merged to main.

## Resume here

1. Merged: byos-core (058822d8). In progress: byos-instance (ee7101c3), conflict in `sr-storage/src/lib.rs`
   resolved; app.rs unified to one `mount_repo` / `mount_sealer` / `CachedMountResolver`.
2. Next: byos-ui, then (after the commander's word) byos-scopes, then the known merge work and the gates.

## Merges

| # | Branch | Head taken | Conflicts / merge work |
|---|---|---|---|
| 1 | feat/p27-byos-core | 058822d8 | none |
| 2 | feat/p27-byos-instance | ee7101c3 | `sr-storage/src/lib.rs` module list; app.rs: one repo, one sealer, one cached resolver shared by `MountStorages`, `MountsDeps` (`with_resolver`) and the CORS recheck job |

## Gates

(filled in as they run)

## Wave-28 follow-ups

- byos-instance: `POST /admin/storage/mounts/1/attach|detach` (mount 1 as a Guild Shelf) needs byos-scopes'
  `guild_storage_grants`; not built in wave 27.
