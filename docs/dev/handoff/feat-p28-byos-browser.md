# Handoff: feat/p28-byos-browser (BYOS-BRW-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-byos-browser`, base `integration/phase28`. Env: `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-byos-browser-target`, `-j 2`, scratch DB `sr_p28_byos_browser`.
No python on this machine: edit with node/Edit. Nothing pushed.

## Resume here

Slice 1 done (settings + caps). Next: slice 2, the sr-storage helpers and the ledger (migration 1440).

## Done

- Follow-up 3: `byos.max_mounts_per_user` (3) and `byos.max_mounts_per_guild` (5) in the sr-admin catalog (group `storage`),
  read by `mounts::cap_for` when the admin service is wired; `DEFAULT_MAX_MOUNTS_PER_*` stay the fallback. Also
  `byos.upload_parts_per_hour` (2000), used by the part route.
  MERGE NOTE: BYOS-FLG-001 (imm-flags) lists the same two keys in its `byos.*` block; the integrator must keep one copy.

## Still open
