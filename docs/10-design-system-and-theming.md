# 10 — Design System and Theming

Decisions: D-040, D-063, D-141, D-142, D-470, D-476, D-480.

## 1. Two families, one token system

Every visual value is a CSS custom property (token). Components never hard-code colors, radii, or font families. Each section ships with a **default theme**:

| Section | Default theme | Character |
|---|---|---|
| Lounge | `lounge-editorial` (D-400; also `lounge-dark`, `lounge-light`, `lounge-ash`, `lounge-onyx`) | `lounge-editorial` is the default for the Lounge: warm charcoal, cream text, muted gold accent, serif display headers, built from tokens on the Lounge token set. The other themes are the Discord look-alike: neutral greys, blurple accent, compact sans, 8px radii |
| Staff Server, Developer portal | `lounge-dark` | The Discord look-alike (`DEFAULT_THEME_BY_SECTION`, `packages/ui/src/tokens/themes.ts:313`) |
| Entrance, Cinema, Library, Support | `sideroom` | Deep olive-black, cream type, muted gold, editorial serif display, letter-spaced caps |
| Arcade (`/games/**`, including the Emulator) | `lounge-editorial` whatever the member's Lounge theme (D-476, D-480) | An editorial games front page like the Cinema's. It is a themed section (`arcade`), so the member picks its look under Settings > Appearance > Sections and the Operator sets a platform default; every route under `/games/**` follows it, and the Emulator keeps it and carries the Arcade's rooms bar. Cover art is fixed content, never tinted from tokens (D-470) |
| Live TV page | `sideroom-broadcast` (extends `sideroom`) | Same palette with a monospaced "teletext" layer for times and channel numbers, CRT preview frame, scanline overlay |

Users can pick **one theme for everything** (e.g. "Side Room everywhere") or **per-section themes** (Settings → Appearance → Per-section themes). Server owners can set a server accent and banner that recolour the Lounge while that server is open (channel list, header, chat, members, voice and party rooms), never Cinema, Library, the Arcade or the Entrance (D-281), if the viewer allows server theming (default on). The look is layered, from the bottom: the section's built-in theme, the platform default (D-254), the member's own theme (global or per section), the server theme while that server is open, the server accent colour, and last a Theme Store preview while it is open (D-280). One switch, "Allow server themes", covers the server's theme and accent together; off, the member sees only their own theme. The manager's accent wins over the theme's accent token, which is only the author's default.

## 2. `sideroom` tokens (from the mockups)

| Token | Value | Use |
|---|---|---|
| `--sr-bg-0` | `#111110` | page background |
| `--sr-bg-1` | `#161613` | bands, footer |
| `--sr-bg-2` | `#1c1d18` | cards, inputs |
| `--sr-bg-olive` | `#23251d` | spotlight panels, active guide cells |
| `--sr-line` | `#2a2b24` | hairline borders |
| `--sr-text` | `#ece6d6` | primary (cream) |
| `--sr-text-2` | `#a9a493` | secondary |
| `--sr-text-3` | `#918d7e` | muted labels (lifted from the first calibration, `#6f6c60`, by RDY-A11Y-001) |
| `--sr-entrance-text-muted` | `#8c8878` | entrance copy that must be read (scoped to `.ent-root`, `apps/web/src/components/entrance.css`; 5.1:1 on `--sr-bg-1`; it was added when `--sr-text-3` was `#6f6c60`, about 3.6:1, OQ-XC-6) |
| `--sr-entrance-danger-text` | `#d97a68` | entrance danger text, e.g. a refused invite (scoped to `.ent-root`; the entrance palette's `--ui-danger-text`, 5.6:1 or better on `--sr-bg-0/1/2`, D-577) |
| `--sr-gold` | `#c2b08f` | primary buttons, active tab underline, eyebrow labels |
| `--sr-gold-ink` | `#1b1a15` | text on gold |
| `--sr-olive-accent` | `#8c9a6a` | small accents ("Good reception" dot, quiet links) |
| `--sr-font-display` | `"Newsreader", "Source Serif 4", Georgia, serif` | headlines, italic accents ("*closed.*", "*back.*") |
| `--sr-font-body` | `"Hanken Grotesk", "Inter", system-ui, sans-serif` | body |
| `--sr-font-mono` | `"IBM Plex Mono", ui-monospace, monospace` | TV guide times and channel numbers |
| `--sr-eyebrow-size`, `--sr-eyebrow-tracking`, `--sr-eyebrow-rule` | 12px, `.28em`, 40px; the eyebrow is uppercase, `--sr-gold`, preceded by a hairline of the rule width | "— GOOD THINGS STAY BETWEEN US." |
| `--sr-radius` | `2px` | near-square buttons and inputs |

Signature motifs: a door-ajar hero image with warm light; the `[|]` door logo mark; ↗ arrows on CTAs; poster grids with `+` add-to-list; footer "Not everywhere. *Just here.*"

These values were calibrated against the mockup images in `docs/assets/mockups/`; the values the app uses are those of `sideroom` in `packages/ui/src/tokens/themes.ts`, where the contrast tests (`packages/ui/src/theme/tokens-contrast.test.ts`) hold them.

## 3. `lounge-dark` tokens

Mirror Discord's current dark palette semantics: `--bg-base-lowest` (server rail), `--bg-base-lower` (channel list), `--bg-base-low` (chat), `--bg-surface-overlay` (menus/modals), `--text-normal`, `--text-muted`, `--interactive-normal/hover/active`, `--brand` (blurple-like, `#5865f2`), `--status-online/idle/dnd/offline`, `--mention-bg`, `--danger`. Layout constants: server rail 72px, channel list 240px, member list 240px, header 48px, composer min-height 44px, message gutter 72px.

**Status colours (D-741, D-756).** `--ui-status-online/idle/dnd/offline` are one set per theme in `packages/ui/src/tokens/themes.ts`; the Lounge themes keep Discord's values, Editorial's online colour is a clear green (`#34c25f`, 6.9:1 on its base surface, at least 3:1 on every surface the contrast test lists) and its idle, do-not-disturb and offline keep their values. A status is a colour and a size, not a colour alone: `tokens-contrast.test.ts` also asks that the online colour be a green hue of at least 50 % saturation. The phone glyph (D-702, D-711) is a solid smartphone shape, screen cut out, in the online colour (`Avatar.module.css` fills the `smartphone` icon).

The layout constants are the `--lounge-server-rail`, `--lounge-channel-list`, `--lounge-member-list`, `--lounge-header`, `--lounge-composer-min` and `--lounge-message-gutter` tokens (`packages/ui/src/tokens/themes.ts:7`).

**Danger colours (D-577).** One colour is one token: `--ui-danger` is the danger fill (buttons, borders), `--ui-danger-ink` is text on that fill, and `--ui-danger-text` is danger text on a surface (error lines, failed-load rows). Every built-in theme sets `--ui-danger-text` at 4.5:1 or better against `--ui-bg`, `--ui-bg-raised` and `--ui-bg-overlay` (`packages/ui/src/theme/tokens-contrast.test.ts`); the theme validator warns when a package's value is below 4.5:1 on its `--ui-bg`.

Use our own original icon set (`packages/ui/src/icons`, drawn to match the weight and size of Discord's but not copied) and our own logo. Do **not** ship Discord's logo, wordmark, Clyde, Wumpus, or other brand assets. "Continue with Discord" uses Discord's official OAuth button guidelines.

## 4. Theme packages (D-063)

A theme is a **declarative package**. It never contains executable code.

```
my-theme/
  theme.json       manifest: id, name, version, author, targets: ["lounge","cinema",...]
  tokens.json      token overrides (validated against the token schema)
  assets/          images (backgrounds, patterns): PNG/WebP/AVIF, ≤ 2 MB each, ≤ 10 MB total
  fonts/           optional: WOFF2 only, licence file required
```

- `sr-theme` (Rust, also WASM) validates: known tokens only, color formats, contrast checks (WCAG AA on text tokens; warning otherwise), asset sizes and types, and **no URLs** (assets are referenced by package-relative path and rewritten to signed CDN URLs).
- An optional **restricted CSS subset** (not built: the package model has no CSS field and `sr-theme` does not parse CSS or depend on `lightningcss`; EXT-PKG-002 and the Phase 6 answer in `docs/open-question-answers.md`) is meant to target documented `data-sr-part="…"` hooks with a whitelist of properties (colors, backgrounds, borders, radii, shadows, fonts, spacing, filters). There are no selectors outside parts, no `url()` except package assets, no `@import`, no `position: fixed`, and no animations over 10s. The validator is meant to parse with `lightningcss` (Rust) and reject anything else. The built validator (`crates/sr-theme/src/validate.rs`) checks the token names and values, contrast, asset and font sizes and types (2 MB each, 10 MB per package) and the absence of URLs.
- Review flow: shared submissions pipeline (`kind = theme`). Once approved, it's installable by anyone (D-064).

## 5. Profile customization ("MySpace mode", Phase 6)

The profile is a **document** (`accounts.profile_doc`) made of approved blocks on a grid:

| Block | Content |
|---|---|
| Header | Name, avatar, banner, badges, pronouns, status |
| About me | Markdown (sr-markdown subset) |
| Top friends | Friend slots, ordered |
| Now watching / reading | Live activity (respects privacy) |
| Favorites | Pinned titles or series from the libraries |
| Gallery | Up to 12 images |
| Guestbook | Comments from friends (owner can moderate) |
| Song | Link to a track in an approved embed provider. Click to play (never autoplays) |
| Custom box | Rich text + image |
| Links (`linktree`) | A stack of up to 12 links; a link can be a tip link (a payment URI shown with a QR for Bitcoin and Lightning) |
| Now playing (`now_playing`) | What the member is playing, through presence or typed by hand |
| Featured servers (`featured_guilds`) | Servers the member chooses to show |
| Media showcase (`media_showcase`) | Up to six audio or video objects from the member's own shelf (D-502); rendered by the host app, not by `packages/profile-blocks` |

Styling: approved theme package or palette pickers, background image or pattern, font from an approved list, a cursor from an approved set, glitter/sparkle overlays from an approved set, and border styles. All are declarative. The document is validated by `crates/sr-packages/src/profile_doc.rs` (a 12-column grid, at most 16 blocks, at most 32 KiB, one of each of the singleton block types) and the theme packages by `sr-theme`; the blocks are drawn by `packages/profile-blocks`. Profiles render in a scoped container. The mini profile popout (Discord-style) shows a compact version.

## 6. Components

`packages/ui` exports two kits sharing primitives (Button, Input, Select, Checkbox, Switch, Radio, Slider, Tooltip, Popover, ContextMenu, Modal, Toast, Tabs, Avatar, Badge, Skeleton, ScrollArea, VirtualList, Kbd, EmptyState, ErrorBoundary). The `lounge` kit adds Discord-specific composites. The `sideroom` kit adds editorial composites (`packages/ui/src/sideroom`: Eyebrow, DisplayHeading, ArrowLink, GoldButton, OutlineButton, ArrowButton, PosterCard, PosterRow, FilterChips, DoorLogo, SrHeader, SrFooter, SectionTopBar); there is no Spotlight, GuideGrid or ReaderChrome in `packages/ui`: the TV guide grid is `apps/web/src/cinema/tv/GuideGrid.tsx`. Stories are Ladle stories (`packages/ui/src/stories`, `pnpm --filter @sideroom/ui stories`); `packages/ui/src/test-axe.ts` is the shared accessibility helper.

### 6.1 The legal page's stylesheet (D-385)

`/house-notes` (the terms, privacy, rules and credits anchors) is plain HTML with no script, rendered per locale by `scripts/render-legal.mjs` and served by `sr-server`. Its one stylesheet (`/entrance/legal-<hash>.css`) is the `CSS` constant of that script: a small set of custom properties (`--bg`, `--bg-1`, `--line`, `--text`, `--text-2`, `--gold`, with `color-scheme: dark`) that copy the `sideroom` palette (its `--gold` is `#c8a96a`, not `--sr-gold`), with no component library and no script-set styles. A plain page cannot switch theme with JavaScript and this one is dark only. The offline and error pages (`apps/web/public/offline.css`, `404.html`, `50x.html`) use the same palette as literal values. `scripts/check-css-tokens.mjs` does not cover either: its targets are the BYOS storage screens and the player (`TARGETS` in the script).

## 7. Accessibility

WCAG 2.2 AA. Full keyboard navigation (Discord keyboard shortcuts plus `Ctrl+/` shortcut sheet), screen-reader labels, reduced-motion support (disables scanlines and parallax), font scaling 12–24px, saturation and contrast settings (Discord parity), and captions/subtitles in the player.

## 8. Responsive / PWA

- Lounge on mobile follows Discord mobile patterns: swipe between server/channel list, chat, and member list. Bottom tab bar: Home, Notifications, You.
- The primary rail is The Lounge, The Arcade and The Study (D-400, `apps/web/src/study/RoomsNav.tsx`); on phones the Lounge bar drops the brand (D-539).
- Cinema and Library collapse to single-column rows with horizontal scrollers. The reader is full-screen with tap zones.
- Installable PWA with app shortcuts (Lounge, Cinema, Library, Live TV; `apps/web/public/app-manifest.webmanifest`), Web Push notifications, and the Badging API for unread counts.
