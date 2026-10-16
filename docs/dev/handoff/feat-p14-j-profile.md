# Handoff: feat/p14-j-profile (J-06 to J-09, J-18, ticks for J-19 and J-20)

Branch `feat/p14-j-profile`, base `integration/phase14` (07ade74). Items from `docs/dev/answers-followups-p14.md`.

## Done

- **J-06 (a)** `POST /guilds` takes `icon` (data URI, the `PATCH` rules) for the plain and the template create. The image is
  stored after the server exists; a refused image removes the new server again and answers the error, so a retry leaves no
  duplicate (`crates/sr-api/src/routes/guilds/guild.rs` `set_initial_icon`). The route got the media body limit. The Create
  Server dialog (`lounge/AddServerDialog.tsx`, `store/rest.ts`, `store/store.ts`) now sends the icon in the create call and
  its second `PATCH` is gone.
- **J-06 (b)** Soundboard trimmer. `settings/server/sections/SoundTrimmer.tsx` and `soundTrim.ts`: a file over 5 s is decoded
  with WebAudio, a 5 s window is picked on an SVG waveform (slider), previewed, and re-encoded as 16-bit PCM WAV at the
  decoded rate. Stereo is kept when it fits 512 KiB; otherwise the window is mixed to mono; if even that is over the limit
  the person is refused with the size. If the browser cannot decode the file, the old "N seconds long" refusal stays.
  The server does not transcode (D-201).
- **J-07** Server traits. Migration `0962_guild_profile_traits.sql` (the column `guilds.traits` already existed since 0010 and
  nothing wrote it; the migration fixes its shape: JSON array of at most five). `sr-chat/src/guild_traits.rs` validates (label
  1 to 24 chars, no mentions or markdown, emoji id a snowflake). `sr_profile.traits` on the guild object (so on `GET /guilds/{id}`
  and `GUILD_UPDATE`), `PATCH /guilds/{id}/sr/profile` (`MANAGE_GUILD`), `traits` on the directory card, audit key `traits`.
  Web: `Traits.tsx` editor in the Overview, `TraitChips.tsx` on the directory card and the per-server profile dialog.
- **J-08** Role colour gradients. Migration `0963_role_colors.sql` (`secondary_color` existed since 0010; adds `tertiary_color`
  and a check that a tertiary stop needs a secondary one). `colors {primary_color, secondary_color, tertiary_color}` on the role
  object; `POST|PATCH` roles accept `colors` (replaces all three, wins over `color`; tertiary needs secondary, else 50035).
  Audit `changes` keys `secondary_color` and `tertiary_color`. Web: Solid / Gradient / Holographic picker in `Roles.tsx`
  (holographic is Discord's fixed triple), gradient names in messages and the member list (`lib/roleGradient.ts`,
  `background-clip: text` inside `@supports`, the first stop as plain `color` otherwise).
- **J-09** View Server As Role: dialog in the Roles section (`ViewAsRole.tsx`, `viewAs.ts`), channel visibility from the
  shared `sr-permissions` WASM engine (`permEngine`) over the loaded channels and overwrites. The channel editor got a Voice
  tab with the status line over `PUT /channels/{id}/voice-status` (clear sends null). The voice status field itself already
  existed (P13-VOX-2).
- **J-18** `cleanFavorites` in `voice/soundboard.ts`: shape `^(default|[0-9]{1,20}):[0-9]{1,20}$`, newest 50, entries of a server
  the person is not in are dropped on read (and again when the server list changes; nothing is dropped while the list is not
  known yet). Doc 05 lists `soundboardFavorites` and says the blob has no server-side key schema; note added to H-45 in
  `answers-followups-p13.md`.
- **J-19, J-20** ticked with notes (done by feat/p14-misc and feat/p14-e2ee).
- OpenAPI, `schema.d.ts` and MSW handlers regenerated; route-matrix row `PATCH /guilds/{guild_id}/sr/profile ... membership`
  (same as `PATCH /guilds/{guild_id}`); `KNOWN_CLASHES` is empty and the new schema names are unique.
- Inventory: LNG-SSET-007, 032 and 036 ticked; LNG-CHN-010 and 017, LNG-SSET-050 and 051 got the J-21 notes.

## Still open

- **J-21** is only partly mine: I did the P14-LNG-7 inventory notes (LNG-CHN-010, 017, LNG-SSET-050, 051) and the doc rows for
  my own items. The Activities, E2EE, member flags and incident rows belong to the other agents; the box stays unticked.
- LNG-CHN-010 stream preview and LNG-CHN-017 video quality (J-11) are not part of this branch.
- LNG-SSET-036 is a dialog, not a Lounge-wide preview mode with an exit banner.
- The traits editor offers a unicode emoji field only; a trait that already carries a custom emoji keeps it and shows it.

## Decisions and caveats for the owner

- The brief said `guild_profiles`; the real table is `guilds` (column `traits`).
- The server does not insist that a tertiary stop equals the holographic triple (Discord does); it only requires a secondary
  stop. Say so if you want it strict.
- The WAV keeps the sample rate of the WebAudio context (browsers resample on decode, usually 44.1 or 48 kHz).
- `sr-db` Postgres tests build their databases from a template named after the migration set (`sr_tpl_*_0963_*`); drop it
  from the scratch server when done.

## Tests run

- Rust (one cargo at a time, -j 2): `cargo fmt --check` and `clippy --all-targets -D warnings` on sr-chat, sr-db, sr-api; `cargo check -p sr-server --tests`; `cargo test -p sr-chat -p sr-api` (sr-chat 318, sr-api lib 583, `guilds` 23, `lounge` 29, `openapi_names` 2 with `KNOWN_CLASHES` empty, `route_matrix` 5); Postgres (own scratch templates on the local server, `--include-ignored --test-threads=1`): `p14_profile` 2, `chat_repo` 4, `guild_admin_repo` 8, `lounge_repo` 6.
- Web: `tsc --noEmit`, eslint and prettier on the touched files, full `vitest run --maxWorkers=2` (223 files; the one failure was my own wrong expectation in `soundboard.test.ts`, fixed and re-run green together with `shellFlows.test.tsx`), `vite build` and `scripts/check-bundle-budget.mjs` (settings 277 KB gzip of 600, lounge 396 of 600).
- Not run: Playwright (shared ports), the Postgres suites of crates I did not touch.
