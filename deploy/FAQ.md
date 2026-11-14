# Operator FAQ

**Do I have to use Docker?** No. `deploy/systemd/` has a hardened unit for the static binary, and `deploy/compose/prod.yml` is the Docker route. Both use the same configuration keys.

**Where do I put secrets?** In the environment or a mode-0600 file read by the service (`EnvironmentFile=`), never in `config.toml` that you commit, and never on a command line. See INSTALL.md section 3.

**The server will not start and complains about `secret_key`.** It must be 64 hex characters (`openssl rand -hex 32`). An empty key is only allowed when `public_url` is a localhost address.

**Which ports must be open?** 80 and 443 (TCP, plus UDP 443 for HTTP/3) to the proxy. Voice adds UDP 50000-50099 (`voice.udp_port_min` to `voice.udp_port_max`; ICE-TCP is off unless `voice.tcp_port` is set), and coturn adds 3478 (udp+tcp), 5349 (tcp) and its relay range 49152-49999 (udp). Nothing else: Postgres, Garage and the metrics port stay on the private network.

**How do I see metrics?** `deploy/compose/prod.yml --profile monitoring` starts Prometheus and Grafana with the "SIDE ROOM overview" dashboard. Grafana listens on `127.0.0.1:3000`; use an SSH tunnel. The raw metrics are at `/internal/metrics` on `server.metrics_bind` (default `127.0.0.1:9100`), never public (Caddy answers 404 to `/internal/*`). A release build refuses a `metrics_bind` that is not loopback unless `server.metrics_token` is set.

**Why does the browser show a "new version" toast?** The app is an installable web app. After a deploy the service worker downloads the new build in the background and asks before switching, so nobody loses a half-written message to a surprise reload.

**I changed the web app's CSP origins (a separate voice host, activities).** For `vite preview` set `SR_CSP_CONNECT`, `SR_CSP_FRAME` or `SR_CSP_MEDIA` (comma separated). In production run `node deploy/scripts/render-caddyfile.mjs --public-url <url> [--activities] [--voice-url <url>]`: it rewrites the generated public-policy block of `deploy/compose/caddy/Caddyfile.prod`, and the app build writes the app policy to `app/csp.txt`, which `sr-server` sends. `apps/web/src/security/csp.test.ts` fails if the file and `csp.ts` disagree.

**Can a link to my PeerTube instance get a "Play here" button?** Yes, once you name it: set `SR_PEERTUBE_HOSTS=tube.example.org` (comma separated hosts, no paths) for `sr-server` (it offers the button for `/w/<id>` and `/videos/watch/<id>` links on that host) and for the web build (it adds `https://tube.example.org` to the app policy's `frame-src`, so the browser may show the embed after the click). YouTube (`youtube-nocookie.com`) and Vimeo (`player.vimeo.com`, `dnt=1`) are always listed and nothing else is. Since wave 50 a PeerTube card also offers Watch Together (D-515): the server asks the instance's public API once (`/api/v1/videos/<id>`, under the link-preview SSRF rules) and the party plays the web-video file (at most 1080p) or the HLS playlist in the browser's own player; live and password-protected videos are refused. The web build also adds `https://tube.example.org` to the app policy's `connect-src`, so hls.js can fetch playlists in browsers without native HLS.

**My PeerTube instance keeps its videos on object storage or a CDN.** Name that host too: `SR_MEDIA_HOSTS=cdn.example.net` (comma separated hosts, same rules as `SR_PEERTUBE_HOSTS`, empty by default), for `sr-server` and for the web build. A party plays a resolved address only when its host is the instance itself or one of these, and the web build adds each to the app policy's `connect-src` (D-517); the public pages' policies never change, and `connect-src` never gains a blanket `https:`. Without it, a party of such a video is refused with "That instance keeps its video on a host the operator has not allowed."

**How do I restore from a backup?** BACKUP.md. Run the restore drill before you need it.

**Can I run two servers behind a load balancer?** It is not a tested configuration yet (the rate limiter and caches are per process, and nothing has exercised two gateways side by side). Scale the one host up first.

**How do I rotate the secret key?** BACKUP.md "Secrets rotation".

**A member lost their 2FA device.** `sr-cli user reset-2fa <username or email>`; it also revokes their sessions.

**Is anything sent to third parties?** No telemetry or analytics leave your server (AGENTS.md rule 5). The crash notes a signed-in browser sends (wave 45) go to your own server and end as cards in the Staff Server's `#crash-triage` channel, nothing else. Outbound traffic is your SMTP server, optional Discord OAuth, optional unfurls of links members post (through the SSRF-guarded fetcher), and anything you configure yourself.

**Why does `https://<domain>/anything` send me to the sign-in page, and `/app/anything.js` answer 401?** Since D-385 the app is served only to a signed-in session, and the answer is the same whether or not the path exists, so a visitor cannot learn what the app contains. A page opened without a session is a `302` to `/login?next=<path>`; a file under `/app/` is an empty `401`. The landing, sign-in, invite and legal pages, the fonts, icons and the service worker stay public. `sr-server` needs `SR_WEB__ROOT` (the web build) for this; without it the server serves no documents at all.

**The server will not start and says `app/csp.txt` is missing, or that the web root is not a directory.** `SR_WEB__ROOT` points at a web build that has no `app/csp.txt` (an old build, or a half-unpacked one), or at a folder that does not exist: the systemd unit sets it to `/opt/sideroom/web`, so a plain-binary install unpacks the build there (INSTALL.md, "Plain binary with systemd") or names another path in `server.env`. The server refuses to serve the app without its policy. Unpack the release's `apps/web/dist` completely and start again.

**A game cannot load a prompt pack, or the Emulator cannot load a ROM, from a member's bucket (a blocked `connect-src` in the browser console, or in Admin > System > "Blocked content reports").** Those files are fetched by the app page itself, under the app's `connect-src 'self'`, so a bucket on another origin than yours has to be named in the web build: set `SR_CSP_CONNECT=https://<bucket host>` when you build `apps/web` (FAQ entry on CSP origins above) and ship that build. The bucket also needs its CORS rule for your origin (`sr-cli storage ensure-cors`, GARAGE.md). Games never get the host added to their own policy.

**A friend's guest link ("Share link" in a game) opens a blank page or says it is dead.** A dead, revoked or expired link (6 hours) always answers the same "not found". If every link fails: Activities must be on (`activities.enabled`, the guest socket exists only then), the Caddyfile must be the release's with the `/play/*` block (UPGRADE.md, guest links), and behind a proxy `server.trust_forwarded_for` must be true or all guests share one address and trip the 20-joins-a-minute limit.
