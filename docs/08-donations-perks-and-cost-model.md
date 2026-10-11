# 08 — Donations, Perks, and Cost Model

Decisions: D-025, D-042, D-043, D-050–D-052, D-070–D-075.

> **Principle:** almost everything is free. Donations exist to (1) keep the lights on and (2) gate the few things that measurably cost the operator money: lots of high-bitrate video, huge uploads, media downloads, and extra platform invites.

All prices are **operator-configurable** (Admin → Donations). The numbers below are the shipped defaults, derived from the cost model in §6.

## 1. Payment rails (D-071)

- **Bitcoin on-chain** (required, Phase 3). This is a self-custodial, watch-only setup: the operator pastes an **xpub/output descriptor** in Admin → Donations → Wallet. `sr-payments` (Rust, `bdk_wallet` + `bdk_esplora`) derives a fresh address per invoice and watches for payment through a configurable Esplora endpoint (self-hosted `electrs`/mempool, or mempool.space). The server **never holds private keys**.
- **Lightning** (optional, later): LDK node, or an operator-supplied LND/CLN connection. Recommended for $5 payments.
- **Monero** (optional, later): `monero-wallet-rpc` in view-only mode.
- No card or PayPal processors.

### Invoice lifecycle
`open` → (tx seen in mempool) `seen` → (N confirmations, default 1) `confirmed` → entitlement applied. Other outcomes: `expired` (no payment in 60 min; a late payment is still credited and flagged for staff), `underpaid` (> 1% short: staff alert, user can top up), `overpaid` (credited, excess noted).
- USD → BTC rate is locked at invoice creation for 30 minutes, from a configurable price source (default: two sources, median).
- The invoice UI shows the amount in sats and BTC, the address, a BIP21 QR (`bitcoin:addr?amount=`), a live status (via gateway `SR_INVOICE_UPDATE`), and a fee-estimate hint.

## 2. What donations buy

### 2.1 Personal support: "Patron" (default **$5 / month**)
Perks follow the user everywhere (D-072):

| Perk | Free | Patron |
|---|---|---|
| Chat upload size | 30 MB | **500 MB** (files > 30 MB expire after 7 days, D-043) |
| Your video/screen quality | 720p30 (or the server's level) | **1080p60** in any server, DM, or group call |
| Counts against a server's sender limit | yes | **no** (D-052) |
| Download Cinema video files | ✗ | ✅ (1080p or lower rendition, or original if staff allow) |
| Download Library chapters (CBZ/PDF/EPUB) | ✗ | ✅ |
| Profile badge "Patron" + donor ring | ✗ | ✅ (can hide) |
| Everything else (animated avatars, banners, themes, emoji anywhere, long messages, stickers…) | ✅ free | ✅ |

Patron is a 30-day entitlement from confirmation, plus a 3-day grace period (D-073). Paying again extends it. Users can prepay up to 12 months at once.

### 2.2 Server support: "Server Levels" (pooled, monthly)
Members contribute any amount (minimum $1) from **Server Settings → Server Support** (the replacement for Discord's "Boost Perks"). A server's **pool** is the sum of confirmed contributions in the last 30 days. Its **level** is the highest threshold the pool meets. If the pool drops below a threshold, the server keeps that level for the 3-day grace period, then downgrades.

| | Level 0 (free) | Level 1 | Level 2 | Level 3 |
|---|---|---|---|---|
| Monthly pool threshold | $0 | **$10** | **$25** | **$50** |
| Video senders per voice channel | **5** | 8 | 12 | 25 |
| Max stream/camera quality (non-Patrons) | 720p30 | 720p60 / 1080p30 | 1080p60 | 1080p60 |
| Viewers per stream ("Go Live" audience cap) | 25 | 50 | 50 | 100 |
| Voice audio bitrate | 96 kbps | 128 kbps | 256 kbps | 384 kbps |
| Concurrent watch parties in the server | 2 | 4 | 8 | 16 |
| Upload size for all members in this server | 30 MB | 50 MB | 100 MB | 250 MB |
| Emoji / sticker / sound slots | Generous for everyone (250/60/48): not cost-relevant | same | same | same |
| Vanity URL, invite splash, banner, animated icon | free | free | free | free |

Server levels never unlock anything for media downloads. Downloads are Patron-only (D-075).

### 2.3 Platform invites
Beyond the 5 lifetime credits: **$20 per credit** (D-025), paid in BTC. An entitlement of +1 credit is added on confirmation.

## 3. Safety valve: global egress governor

Perks are "up to" limits. `sr-voice` tracks total egress. When it exceeds `voice.egress_budget_mbps` (default 70% of the measured uplink), the SFU caps simulcast layers for new subscribers (Level 0 first, Patrons last) and shows a "Reduced quality: the room is busy" notice. One heavy server therefore can't saturate the box for everyone else.

## 4. Leaderboards (D-074)

Page `/support/leaderboards`, also linked from the Cinema and Library footers.

| Board | Metric | Privacy |
|---|---|---|
| Top donors | Lifetime and last-30-days total (USD at time of payment) | Donor can hide **name** ("Anonymous patron") and/or **amount** (rank only) |
| Top supporting servers | Last-30-days pool | Server owner can hide |
| Top Cinema contributors | Approved **movies** and **episodes**, separate tabs | Public (contributors can opt out of appearing) |
| Top Library contributors | Approved **chapters** | Same |

Contribution credit is granted once per unique item (`contribution_credits` unique on kind + item). Replacements, re-encodes, and duplicates get nothing. Credits are revoked if the item is removed for cause.

## 5. Entitlement engine

`sr-payments::entitlements` exposes `effective_limits(user, context) -> Limits`, where context is a guild/channel/DM. Limits = max(Free defaults, the guild's level limits if in a guild, Patron overrides if the user is a Patron). Every limit check (upload slot, voice join, stream start, download URL) goes through this one function. Limits are cached in moka and invalidated on `SR_ENTITLEMENT_UPDATE`.

## 6. Cost model (how the defaults were chosen)

Research snapshot: October 2026. **Verify prices before buying.** Hetzner raised prices on 15 June 2026. EUR→USD at 1.08. Prices exclude VAT and IPv4.

### 6.1 Hosting baseline

| Deployment | Hardware | ≈ USD / month |
|---|---|---|
| **S: friends (≤ 150 users)** | Hetzner AX42 (Ryzen 7 PRO 8700GE, 64 GB, 2×512 GB NVMe, 1 Gbit/s **unmetered**) + Storage Box BX31 (10 TB) for backups and originals | **≈ $128** (+ ~$53 one-time setup) |
| **S+: friends + real media library** | S + Hetzner SX-line storage server (e.g. SX135: 8×22 TB HDD) running Garage for HLS segments | **≈ $440** |
| **M: ~2,000 users** | 2× EX63 (app + SFU, failover) + SX135 | **≈ $630** |
| **L: 20,000 accounts / ~2,000 concurrent** | AX102 (Postgres) + 3× EX63 (app/SFU/HLS) + SX135, consider a 10 Gbit/s uplink add-on | **≈ $1,065+** |

Bulk storage alternatives per TB/month: Hetzner Storage Box ≈ $2.25, Hetzner SX raw ≈ $3, Backblaze B2 $6.95 (egress up to 3× stored is free), Wasabi $7.99 (no egress fees), Cloudflare R2 $15 (free egress).

**Transcoding** is not a monthly cost. Rent a GPU when needed: RTX 4090 ≈ $0.35–0.90/h on Vast.ai/RunPod. A 2-hour 1080p movie → 3-rung ladder ≈ 12 min of GPU time ≈ **$0.10–0.20**. A 24-minute episode ≈ **$0.02–0.04**. 1,000 episodes ≈ $20–40. Storage: ~4.7 GB per hour of content for the H.264 ladder (6 + 3 + 1.5 Mbps).

### 6.2 Real-time video egress

The SFU forwards and never transcodes. Egress ≈ Σ over viewers of (focused stream at full layer + other streams as 180p thumbnails at ~0.15 Mbps).

| Scenario | Peak egress |
|---|---|
| 10 people in voice only (Opus 96 kbps, ~3 talking) | ≈ 3 Mbps |
| Free channel: 5 senders @ 720p30 (2.5 Mbps), 10 viewers | 10 × (2.5 + 4×0.15) ≈ **31 Mbps** |
| Free worst case: 5 senders, 25 viewers | ≈ **78 Mbps** |
| Level 2: 12 senders @ 1080p60 (8 Mbps), 50 viewers | 50 × (8 + 11×0.15) ≈ **480 Mbps** ⚠️ |
| Level 3 worst case: 25 senders, 100 viewers | ≈ **1.2 Gbps**: exceeds a 1 Gbit port; the governor (§3) must engage |

On an **unmetered** 1 Gbit/s server, the real cost of video is **headroom**, not per-GB fees. About 600 Mbps is usable for media before the box needs a second server or a 10 Gbit uplink. A full-time reservation of 1 Mbps therefore costs ≈ $105 / 600 ≈ **$0.18/month**. Typical usage (30–40 streaming hours/month) brings the marginal cost of even a busy Level 2 server under **$2/month**.

On **metered** hosting (e.g. ~$1/TB overage): the free-channel scenario above, 3 h/day for a month ≈ 1.3 TB ≈ **$1.30**. A Level 3 server streaming heavily (200 Mbps for 60 h) ≈ 5.4 TB ≈ **$5.40**.

### 6.3 Why these prices
- Marginal cost is low, so the tiers mainly **fund the baseline** while keeping the heavy, risk-creating usage opt-in.
- Break-even for deployment S ($128): ≈ 26 Patrons, or 13 Level-1 servers, or any mix (e.g. 10 Patrons + 2 L2 + 1 L3 = $150).
- **Patron ($5)** should be worth it for anyone in 3+ servers. **Level 1 ($10 pooled)** is cheaper per head than Patron for any server with 3+ contributors. That matches the owner's intent: "pay as a group for one server, or pay personally if you're in many servers."
- Level thresholds roughly double, while sender counts and quality scale with the egress risk they create.

### 6.4 Formulas (kept in Admin → Donations → Cost calculator)
```
usable_mbps        = uplink_mbps × 0.6
cost_per_mbps_mo   = monthly_server_cost / usable_mbps
level_peak_mbps    = viewers_cap × (max_bitrate + (senders_cap − 1) × thumb_bitrate)
level_expected_mo  = level_peak_mbps × utilisation_hours / 720 × cost_per_mbps_mo
suggested_price    = max(level_expected_mo × safety_factor(3), floor_price)
```
The admin page lets the operator enter real hardware costs and shows suggested prices next to configured ones.
