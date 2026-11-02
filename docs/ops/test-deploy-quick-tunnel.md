# Test deployment through a cloudflared quick tunnel (OPS-TDEP-001)

A short, real-user UI/UX test of SIDE ROOM from this machine, reachable at one random
`https://<random>.trycloudflare.com` address. **Test only.** Decision rows D-450 to D-452, threat model T49.

## What is different from a real deployment

A quick tunnel gives exactly one host name: no own domain, no wildcard subdomains, no UDP. So:

- **Activities run on the main host.** Normally every Activity (game) is served from its own origin,
  `<application-id>.activities.<host>`. A quick tunnel cannot serve those names, so the test runs with
  `SR_ACTIVITIES__ORIGIN_MODE=same_host_sandboxed`: bundles are served from `/_activity/<application-id>/` on the main
  host. The server sends exactly the same Activity policy (it starts with `sandbox allow-scripts allow-pointer-lock`,
  so every game document still gets an opaque origin, and `frame-ancestors` is the app only), the iframe keeps its
  `sandbox` attribute, and `connect-src 'self'` is narrowed to the game's own `/_activity/<id>/` path.
  **Trade-off:** the games share the main host's origin namespace; their isolation rests on the CSP sandbox and the
  iframe sandbox alone. The server logs a `TEST DEPLOYMENT` warning at startup and every signed-in page shows a small
  "Test deployment" notice. The mode can only be set in the config file or the environment, never from an admin
  screen, and it touches neither end-to-end encryption, authorization nor the audit and access logs.
- **No voice.** The tunnel carries HTTP and WebSockets only, not the UDP media voice needs; keep
  `SR_VOICE__ENABLED` unset (off). Activities still run in **text channels**, but a text-channel Activity has no
  shared data channel (that rides the voice SFU), so multiplayer games do not sync between people there.
- **Storage.** There is no S3 service on this machine (Garage has no Windows build). With
  `SR_STORAGE__ENDPOINT=memory:` every upload (attachments, avatars, Activity bundles) lives in the server process
  only: it is gone after a restart, and presigned download links do not work. Re-upload the Activities after any
  restart. Point `SR_STORAGE__*` at a real S3 bucket instead if the test needs files to survive.

## Before the test (once)

All commands from the repo root in Git Bash. Use your own Postgres password; never paste it into chat or commit it.

```bash
# 1. The web app (the session gate needs apps/web/dist with app/csp.txt).
npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install --offline --frozen-lockfile
npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/wasm build
npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/web build

# 2. Release binaries (sr-server and sr-cli). Takes a while; -j 2 keeps the machine usable.
cargo build --release -j 2 -p sr-server -p sr-cli

# 3. A fresh database for the test (any name; keep it apart from sr_test and the dev databases).
"/c/Program Files/PostgreSQL/17/bin/psql.exe" -h 127.0.0.1 -U postgres -c "CREATE DATABASE sr_tunnel_test"

# 4. A server secret (64 hex characters). Required because the public URL is not localhost.
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))" > ~/.sr_tunnel_secret
```

## Start the test

### 1. Start the tunnel first

The tunnel address is only known once cloudflared runs, and the server must be told it. cloudflared keeps the same
address for as long as it runs, even while the server restarts behind it.

```bash
/c/Users/braxt/Desktop/fable/tools/cloudflared.exe tunnel --url http://127.0.0.1:8080
```

Copy the `https://<random>.trycloudflare.com` address it prints. It answers 502 until the server is up.

### 2. Start sr-server (second terminal)

```bash
export TUNNEL_URL="https://<random>.trycloudflare.com"   # from step 1, no trailing slash

export SR_DATABASE__URL="postgres://postgres:<password>@127.0.0.1:5432/sr_tunnel_test"
export SR_DATABASE__AUTO_MIGRATE=true
export SR_SERVER__BIND=127.0.0.1:8080             # loopback only: nobody reaches it except through the tunnel
export SR_SERVER__PUBLIC_URL="$TUNNEL_URL"
export SR_SERVER__SECRET_KEY="$(cat ~/.sr_tunnel_secret)"
export SR_SERVER__TRUST_FORWARDED_FOR=true        # only because BIND is 127.0.0.1 (see below)
export SR_STORAGE__ENDPOINT="memory:"             # or a real S3 bucket (see "Storage" above)
export SR_WEB__ROOT="$PWD/apps/web/dist"
export SR_WEB__SERVE_PUBLIC=true
export SR_ACTIVITIES__ENABLED=true
export SR_ACTIVITIES__ORIGIN_MODE=same_host_sandboxed   # TEST ONLY
# Voice stays off: do not set SR_VOICE__ENABLED.

./target/release/sr-server
```

Check the log for `TEST DEPLOYMENT: activities.origin_mode = same_host_sandboxed`. A second warning naming
`SR_SERVER__PUBLIC_URL` means the public URL is not the https tunnel address.

### 3. The Owner, the Showroom switch and invites (third terminal, same `SR_DATABASE__URL` and `SR_SERVER__PUBLIC_URL`)

```bash
SR_OWNER_PASSWORD='<owner password>' ./target/release/sr-cli bootstrap --owner-username owner --owner-email you@example.org
```

- Sign in at the tunnel address as the Owner, open **Admin > Features**, switch **`platform.showroom`** on (the
  populated preview) and give a reason.
- **Invite-only:** registration always needs an invite code. Make one per tester with
  `./target/release/sr-cli invite create --note "tester: Sam"` and send each person the tunnel address and their code.
  So testers cannot invite more people, set the invite setting `invites.default_credits` to `0` in **Admin** before
  they register.
- **Activities:** a game has to be uploaded as an application's Activity bundle and approved, as on any server
  (`node scripts/build-activities.mjs <whiteboard|holdem|tabletop|landio|arena3d>` builds the zips under
  `packages/starter-packs/activities/dist/`). With `memory:` storage, upload again after a restart.

## Why the server handles the tunnel correctly

- **`SR_SERVER__PUBLIC_URL`** is the one place the address matters, and it is read at startup. It sets the only
  origin the CSRF check accepts for signed-in writes (sign-in fails from any other address), the `Secure` flag on the
  session cookie (on, because the URL is https: the browser talks https to Cloudflare even though cloudflared talks
  plain http to 127.0.0.1), the activity frames' `frame-ancestors`, the guest socket's allowed origin, and the links
  the server writes (the bootstrap login link, invite and mail links). If the tunnel address changes (cloudflared
  restarted), restart sr-server with the new one; links made before are dead.
- **No `X-Forwarded-Proto` is read.** The scheme comes from the public URL, never from a request header, so a client
  cannot talk the server into thinking a request was https.
- **`X-Forwarded-For`** is trusted only with `SR_SERVER__TRUST_FORWARDED_FOR=true`, and then only its last entry
  (the one Cloudflare appends: the visitor's address). That is safe here because the server listens on 127.0.0.1
  only: every request comes through cloudflared, which forwards Cloudflare's header. Without it, every tester shares
  one address for the rate limits (registration, sign-in and guest joins would trip them). The server warns at
  startup if forwarded headers are trusted while it listens beyond loopback.
- **Cookies and games.** The session cookie is `HttpOnly; SameSite=Lax; Path=/`. The browser attaches it when the app
  frames `/_activity/<id>/` (same origin), but the activity proxy never reads or forwards it and never sets a cookie.
  The game's document has an opaque origin, so `document.cookie` and storage throw, its own requests are cross-site
  (no cookie), and its `connect-src` is its own path only, never `/api`. A same-origin request for a game file that is
  not a frame or a document (a script, style, worker or fetch from the app itself) is refused (403).

## After the test

Stop sr-server (Ctrl+C), then cloudflared (Ctrl+C). The quick-tunnel address dies with it. Drop the test database if
it is no longer needed: `psql -h 127.0.0.1 -U postgres -c "DROP DATABASE sr_tunnel_test"`.
