# 10 — Design System and Theming

Decisions: D-040, D-063, D-141, D-142.

## 1. Two families, one token system

Every visual value is a CSS custom property (token). Components never hard-code colors, radii, or font families. Each section ships with a **default theme**:

| Section | Default theme | Character |
|---|---|---|
| Lounge, Staff Server, Developer portal | `lounge-dark` (also `lounge-light`, `lounge-ash`, `lounge-onyx`) | Discord look-alike: neutral greys, blurple accent, compact sans, 8px radii |
| Entrance, Cinema, Library, Support | `sideroom` | Deep olive-black, cream type, muted gold, editorial serif display, letter-spaced caps |
| Live TV page | `sideroom-broadcast` (extends `sideroom`) | Same palette with a monospaced "teletext" layer for times and channel numbers, CRT preview frame, scanline overlay |

Users can pick **one theme for everything** (e.g. "Side Room everywhere") or **per-section themes** (Settings → Appearance → Per-section themes). Server owners can set a server accent and banner that recolor that server's channel list and header, if the viewer allows server theming (default on).

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
| `--sr-text-3` | `#6f6c60` | muted labels |
| `--sr-gold` | `#c2b08f` | primary buttons, active tab underline, eyebrow labels |
| `--sr-gold-ink` | `#1b1a15` | text on gold |
| `--sr-olive-accent` | `#8c9a6a` | small accents ("Good reception" dot, quiet links) |
| `--sr-font-display` | `"Newsreader", "Source Serif 4", Georgia, serif` | headlines, italic accents ("*closed.*", "*back.*") |
| `--sr-font-body` | `"Hanken Grotesk", "Inter", system-ui, sans-serif` | body |
| `--sr-font-mono` | `"IBM Plex Mono", ui-monospace, monospace` | TV guide times and channel numbers |
| `--sr-eyebrow` | 12px, `letter-spacing: .28em`, uppercase, `--sr-gold`, preceded by a 40px hairline | "— GOOD THINGS STAY BETWEEN US." |
| `--sr-radius` | `2px` | near-square buttons and inputs |

Signature motifs: a door-ajar hero image with warm light; the `[|]` door logo mark; ↗ arrows on CTAs; poster grids with `+` add-to-list; footer "Not everywhere. *Just here.*"

Treat these hex values as a starting point. Calibrate them against the mockup PNGs (stored in `docs/assets/mockups/`) during Phase 0.

## 3. `lounge-dark` tokens

Mirror Discord's current dark palette semantics: `--bg-base-lowest` (server rail), `--bg-base-lower` (channel list), `--bg-base-low` (chat), `--bg-surface-overlay` (menus/modals), `--text-normal`, `--text-muted`, `--interactive-normal/hover/active`, `--brand` (blurple-like, `#5865f2`), `--status-online/idle/dnd/offline`, `--mention-bg`, `--danger`. Layout constants: server rail 72px, channel list 240px, member list 240px, header 48px, composer min-height 44px, message gutter 72px.

Use our own original icon set (`packages/ui/icons`, drawn to match the weight and size of Discord's but not copied) and our own logo. Do **not** ship Discord's logo, wordmark, Clyde, Wumpus, or other brand assets. "Continue with Discord" uses Discord's official OAuth button guidelines.

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
- An optional **restricted CSS subset** (Phase 6+) may target documented `data-sr-part="…"` hooks with a whitelist of properties (colors, backgrounds, borders, radii, shadows, fonts, spacing, filters). There are no selectors outside parts, no `url()` except package assets, no `@import`, no `position: fixed`, and no animations over 10s. The validator parses with `lightningcss` (Rust) and rejects anything else.
- Review flow: shared submissions pipeline (`kind = theme`). Once approved, it's installable by anyone (D-064).

## 5. Profile customization ("MySpace mode", Phase 6)

The profile is a **document** (`accounts.profile_doc`) made of approved blocks on a grid:

| Block | Content |
|---|---|
| Header | Name, avatar, banner, badges, pronouns, status |
| About me | Markdown (sr-markdown subset) |
| Top friends | 4/8/12 friend slots, ordered |
| Now watching / reading | Live activity (respects privacy) |
| Favorites | Pinned titles or series from the libraries |
| Gallery | Up to 12 images |
| Guestbook | Comments from friends (owner can moderate) |
| Song | Link to a track in an approved embed provider. Click to play (never autoplays) |
| Custom box | Rich text + image |

Styling: approved theme package or palette pickers, background image or pattern, font from an approved list, a cursor from an approved set, glitter/sparkle overlays from an approved set, and border styles. All are declarative and validated by `sr-theme`. Profiles render in a scoped container. The mini profile popout (Discord-style) shows a compact version.

## 6. Components

`packages/ui` exports two kits sharing primitives (Button, Input, Select, Checkbox, Switch, Radio, Slider, Tooltip, Popover, ContextMenu, Modal, Toast, Tabs, Avatar, Badge, Skeleton, ScrollArea, VirtualList, Kbd, EmptyState, ErrorBoundary). The `lounge` kit adds Discord-specific composites. The `sideroom` kit adds editorial composites (Eyebrow, DisplayHeading, PosterCard, PosterRow, Spotlight, GuideGrid, ReaderChrome). Every component has a Storybook-style story (Ladle) and an accessibility test (axe).

## 7. Accessibility

WCAG 2.2 AA. Full keyboard navigation (Discord keyboard shortcuts plus `Ctrl+/` shortcut sheet), screen-reader labels, reduced-motion support (disables scanlines and parallax), font scaling 12–24px, saturation and contrast settings (Discord parity), and captions/subtitles in the player.

## 8. Responsive / PWA

- Lounge on mobile follows Discord mobile patterns: swipe between server/channel list, chat, and member list. Bottom tab bar: Home, Notifications, You.
- Cinema and Library collapse to single-column rows with horizontal scrollers. The reader is full-screen with tap zones.
- Installable PWA with app shortcuts (Lounge, Cinema, Library, Live TV), Web Push notifications, and the Badging API for unread counts.
