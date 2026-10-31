# Land.io: provenance and notice

Land.io is the SIDE ROOM owner's own game. It is developed in a separate repository (`activity-landio`) and
copied here so it ships as a built-in SIDE ROOM activity.

- Copied from `activity-landio` commit `63c6c93d7be843e1fc89738741e9ef85bc271d54` ("Room codes phase 3:
  join/leave toasts, host Remove from room, limits, rooms in the log"), working tree clean.
- That repository carries no licence file or copyright header of its own; the code is the owner's work and is
  included in SIDE ROOM under the owner's authority. No third-party code, fonts, images or audio files are
  bundled: every sound is synthesised in `client/sfx.js` and every picture is drawn on a canvas.
- Not copied: `.env`, `.env.example`, `server/auth.js` (sign-in secrets handling), `logs/`,
  `activity-landio.zip`, the Discord and plain-browser platform adapters, `server/main.js`, `server/static.js`,
  `server/log.js` and the HTTP API. See `docs/dev/handoff/feat-p36-act-landio.md` for what changed.
