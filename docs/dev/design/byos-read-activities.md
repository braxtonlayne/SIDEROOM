# `byos_read` for Activities: one shelf object per ask (D-389, wave 46)

Status: **built and decided** (D-465): option 3 for small files, option 1 for big assets, option 2 never.

## What is built

- **SDK command** `shelfObjectUrl { shelf_id, object_hash } -> { url, expires_at }`
  (`packages/embedded-app-sdk/src/protocol.ts`, `sdk.commands.shelfObjectUrl`).
- **Member bridge** (`apps/web/src/activities/host.ts`): answers it only when the instance's reviewed bundle declares
  `byos_read`, the arguments name a snowflake shelf id and a lowercase hex SHA-256, and the member said yes in a
  dialog ("Let {game} open a file from your shelves?"). The yes is remembered for that one object in that one frame;
  a new object, or a reload, asks again. Then the page mints the URL with the member's own session through the route
  the Study already uses, `GET /shelves/{shelf_id}/objects/{object_hash}/url` (STU-ZK-001, 41-C). That route checks
  the grant, the kind rule and every delisting at the moment it mints and answers a presigned GET for that object,
  ten minutes at most (less when the session ends sooner). The URL carries the response-type overrides of D-641 (the bucket answers with the app's type table, not the object's stored type), and a mount whose stored connection test has a failed `response_type` row hands no URL at all (D-644, `crates/sr-api/src/routes/storage/shelf_access.rs:487`). No new server route, no new authorization row.
- **Guests** (D-397): the guest bridge is an allow-list (`apps/web/src/public/guest-play/capabilities.ts`);
  `shelfObjectUrl` is not on it, so a guest's game gets `NOT_AUTHORIZED` before any code runs (`guest.test.ts`
  walks every protocol command). `byos_read` is also not in `GUEST_MANIFEST_CAPABILITIES`.
- **arena3d** `ShelfAssetSource` (`assets/source.js`): path `<shelf id>/<object hash>`; asks for the URL through the
  SDK and fetches it with `credentials: 'omit'`. Unavailable (rejects with `AssetNotAvailableError`) outside SIDE
  ROOM, for a guest, or without the capability.

## The CSP question (decided: D-465)

The URL points at the shelf's **storage host** (the bucket endpoint of that mount), not at the activity origin. The
sandbox CSP is per host (`sr_activities::sandbox::csp`): `connect-src` is `'none'` unless the reviewed manifest's
`connect` list names hosts. So today a game can fetch a shelf URL only when its reviewed `connect` list names that
storage host, which a reviewer approves like any other host. Nothing widens the CSP automatically.

The options, and what was decided:

1. **Keep it (decided, for big assets).** Works for a server whose shelves live on one known bucket host that the
   game's developer lists in `connect`. Safe: no host is ever added to a game's CSP without review. Limits: a member's
   own bucket on another host is unreachable for that game.
2. **Per-instance CSP from the mounts (decided against, D-465).** The activity proxy adds the storage host of every mount the member can reach
   to `connect-src` for `byos_read` games. Needs the proxy to know the viewer (it is cookieless today) or a per-instance
   policy; it widens the policy to hosts no reviewer saw. Not built.
3. **Bytes through the bridge (decided, for small files; built in wave 47 as `shelfObjectBytes`, D-460; 1 MiB cap).** The member's page fetches the object (the mount's CORS is already checked for the app
   origin, `cors_ok_at`) and hands the frame the bytes (transferable `ArrayBuffer`, size-capped, like `saveFile` in
   reverse). No CSP change at all, works for every host; costs app-page memory for big assets and needs a size cap.
   The page stops reading at the cap (aborting a body with no content-length) and tells the game the file is over 1 MiB.

## Threat notes

- The game learns one object's bytes per member yes, never a credential, a listing or another object's name; a
  leaked URL is that object for at most ten minutes (the 41-C property).
- A game that declares `connect` hosts can send what it read to those hosts; the member's dialog says the game gets
  the file, and the reviewer saw the hosts.
- The command is refused for guests by the allow-list, and the guest runner never sees `byos_read`.
