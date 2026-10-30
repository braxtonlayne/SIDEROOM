# Operator FAQ

**Do I have to use Docker?** No. `deploy/systemd/` has a hardened unit for the static binary, and `deploy/compose/prod.yml` is the Docker route. Both use the same configuration keys.

**Where do I put secrets?** In the environment or a mode-0600 file read by the service (`EnvironmentFile=`), never in `config.toml` that you commit, and never on a command line. See INSTALL.md section 3.

**The server will not start and complains about `secret_key`.** It must be 64 hex characters (`openssl rand -hex 32`). An empty key is only allowed when `public_url` is a localhost address.

**Which ports must be open?** 80 and 443 (TCP, plus UDP 443 for HTTP/3) to the proxy. Voice adds UDP 50000-50100 and TCP 3479 (`sr-voice`), and coturn adds 3478 (udp+tcp), 5349 and its relay range. Nothing else: Postgres, Garage and the metrics port stay on the private network.

**How do I see metrics?** `deploy/compose/prod.yml --profile monitoring` starts Prometheus and Grafana with the "SIDE ROOM overview" dashboard. Grafana listens on `127.0.0.1:3000`; use an SSH tunnel. The raw metrics are at `/internal/metrics` on `server.metrics_bind`, never public.

**Why does the browser show a "new version" toast?** The app is an installable web app. After a deploy the service worker downloads the new build in the background and asks before switching, so nobody loses a half-written message to a surprise reload.

**I changed the web app's CSP origins (a separate voice host, activities).** For `vite preview` set `SR_CSP_CONNECT`, `SR_CSP_FRAME` or `SR_CSP_MEDIA` (comma separated). In production run `node deploy/scripts/render-caddyfile.mjs --public-url <url> [--activities] [--voice-url <url>]`: it rewrites the generated public-policy block of `deploy/compose/caddy/Caddyfile.prod`, and the app build writes the app policy to `app/csp.txt`, which `sr-server` sends. `apps/web/src/security/csp.test.ts` fails if the file and `csp.ts` disagree.

**How do I restore from a backup?** BACKUP.md. Run the restore drill before you need it.

**Can I run two servers behind a load balancer?** It is not a tested configuration yet (the rate limiter and caches are per process, and nothing has exercised two gateways side by side). Scale the one host up first.

**How do I rotate the secret key?** BACKUP.md "Secrets rotation".

**A member lost their 2FA device.** `sr-cli user reset-2fa <username or email>`; it also revokes their sessions.

**Is anything sent to third parties?** No telemetry or analytics leave your server (AGENTS.md rule 5). Outbound traffic is your SMTP server, optional Discord OAuth, optional unfurls of links members post (through the SSRF-guarded fetcher), and anything you configure yourself.

**Why does `https://<domain>/anything` send me to the sign-in page, and `/app/anything.js` answer 401?** Since D-385 the app is served only to a signed-in session, and the answer is the same whether or not the path exists, so a visitor cannot learn what the app contains. A page opened without a session is a `302` to `/login?next=<path>`; a file under `/app/` is an empty `401`. The landing, sign-in, invite and legal pages, the fonts, icons and the service worker stay public. `sr-server` needs `SR_WEB__ROOT` (the web build) for this; without it the server serves no documents at all.

**The server will not start and says `app/csp.txt` is missing, or that the web root is not a directory.** `SR_WEB__ROOT` points at a web build that has no `app/csp.txt` (an old build, or a half-unpacked one), or at a folder that does not exist: the systemd unit sets it to `/opt/sideroom/web`, so a plain-binary install unpacks the build there (INSTALL.md, "Plain binary with systemd") or names another path in `server.env`. The server refuses to serve the app without its policy. Unpack the release's `apps/web/dist` completely and start again.
