# 11 — Extensibility and Developer Platform

Decisions: D-060–D-065, D-063.

The owner's goal: *"I don't want someone to need to find a bot for a feature that isn't already implemented. If someone wants a feature, it should be trivial to add it and just let it be a feature buried inside a settings menu."*

There are four layers of extensibility, from most to least trusted:

| Layer | Who writes it | Runs where | Review |
|---|---|---|---|
| 1. **Feature modules** | Platform developers (PRs to this repo) | Inside `sr-server` + web app | Code review + merge by the owner |
| 2. **Apps & bots** | App developers | Their own servers, talking to our API | Developer application approval, then per-version app review |
| 3. **Activities** | App developers | Sandboxed cross-origin iframe in the client | Per-version review |
| 4. **Declarative packages** (themes, profile layouts, sticker/emoji/sound packs, "recipes") | Anyone with a developer or creator approval | Data only. No code | Per-version review |

## 1. Feature modules (layer 1)

### 1.1 The registry
`sr-features` provides:

```rust
pub trait Feature: Send + Sync + 'static {
    fn id(&self) -> &'static str;                 // "lounge.slowmode_plus"
    fn meta(&self) -> FeatureMeta;                // name, description, section, phase, default_enabled
    fn settings_schema(&self) -> SettingsSchema;  // typed fields → auto-rendered UI
    fn scopes(&self) -> &[Scope];                 // Platform | Guild | Channel | User
    fn routes(&self) -> Option<axum::Router<AppState>> { None }
    fn on_event(&self, _ev: &DomainEvent, _ctx: &FeatureCtx) -> BoxFuture<'_, Result<()>> { ... }
    fn migrations(&self) -> &'static [Migration] { &[] }
}
```

- `SettingsSchema` is a typed, serializable description (bool, int range, enum, string, duration, role picker, channel picker, emoji picker, color, list-of, nested group) with labels, help text, and per-scope defaults.
- The web app renders **any** schema automatically in the right settings menu: Platform → Admin → Features. Guild → Server Settings → *Features* (new section). Channel → Edit Channel → *Features*. User → User Settings → *Features*.
- **Adding a small feature** means writing one Rust module that implements `Feature`, plus (optionally) a React component registered under the same id for custom UI. Settings UI, persistence, enable/disable, permissions gating (`MANAGE_GUILD` for guild scope by default), and audit logging come for free.
- Feature flags (`enabled` per scope, plus platform kill-switch) are evaluated by `features.is_enabled(id, scope)`, cached in moka.

### 1.2 Examples of built-in features delivered this way
Slowmode presets, auto-thread for channels, "starboard," welcome DMs, birthday reminders, scheduled messages, message bookmarks, server-wide word-of-the-day, voice-channel status text, reaction roles, temporary voice channels (join-to-create), sticky messages, polls templates, leveling/XP (optional), and an RSS/Atom feed poster. These are the things people normally add bots for, so the Lounge spec lists them as built-in features.

## 2. Apps and bots (layer 2)

### 2.1 Developer onboarding (D-062)
1. Any member opens `/developers` → **Apply to build apps**. The form asks: what they want to build, prior work, and agreement to developer terms.
2. Staff with `REVIEW_APPS` approve or deny. Approval sets the account flag `APP_DEVELOPER`.
3. Before and after approval, app developers can create applications. Apps in `development` state work only:
   - in guilds owned by the developer or by an accepted app tester (up to 25 testers per application, D-204), or where the developer or an accepted tester has `MANAGE_GUILD`, **and**
   - where the guild has **≤ 25 members** (test servers).
   Installs into larger guilds are refused with error `900120 App not released`.
4. **Release:** the developer submits a version (manifest: name, description, scopes, requested intents, command list, interaction endpoint, privacy policy URL) for review. Approved → `released`, installable anywhere. New versions that change scopes, intents, or activity code need re-review. Command or description changes need re-review only if flagged by the diff.

### 2.2 Developer portal (`/developers/applications`)
Discord parity: General Information, Installation (install link generator, default scopes/permissions), OAuth2 (redirects, URL generator), Bot (token reset, privileged intents toggles requiring review, public bot toggle), Emojis (app emoji), Webhooks, Rich Presence assets, App Testers, Activities (URL mappings), and **Review** (submission status, reviewer messages).

### 2.3 SDKs
- `sr-sdk` (Rust): ergonomics similar to `serenity`/`twilight`.
- `@sideroom/bot` (TS): discord.js-like (Phase 6).
- The porting guide in doc 05 §7 covers existing Discord libraries.

## 3. Activities (layer 3)

- Hosted at `https://<app-id>.activities.<host>/` (proxied through our server with URL mappings, as Discord does). Rendered in an iframe with `sandbox="allow-scripts allow-same-origin allow-forms allow-popups"` (same-origin refers to the activity's own origin) and a strict `Permissions-Policy`.
- The Embedded App SDK (`@sideroom/embedded-app-sdk`) uses a `postMessage` RPC: `authorize`, `authenticate`, `getChannel`, `getInstanceConnectedParticipants`, `setActivity`, `openExternalLink` (with confirmation), and voice-state events. The API surface mirrors Discord's Embedded App SDK.
- Activity bundles are reviewed per version. The proxy serves only the reviewed bundle hash. Network calls go through URL mappings declared in the manifest.

## 4. Declarative packages (layer 4)

`packages` + `package_versions` (doc 04 §6). Kinds:

| Kind | Payload | Validator |
|---|---|---|
| `theme` | tokens.json + assets (+ optional restricted CSS) | `sr-theme` |
| `profile_layout` | block grid template + style tokens | `sr-theme` |
| `emoji_pack`, `sticker_pack`, `sound_pack` | media files + names | size/format checks, `symphonia` for audio |
| `recipe` | A JSON preset for one or more **feature modules**' settings, e.g. "Anime club starter: auto-threads + starboard + watch-party channel" | Schema validation against the features' settings schemas |
| `server_template` | Discord-compatible template JSON | Structure validation |

Server owners browse approved packages in **Server Settings → Extensions & Themes**. Users browse them in **User Settings → Appearance → Theme Store** and **Profile → Layouts**. Approved packages enable with one click (D-064).

## 5. Review pipeline

All layer 2–4 submissions use `sr-review` (same as library submissions): a queue in the Staff Server's `#review-queue` channel with bot-posted cards (Approve / Request changes / Reject buttons, plus a link to the full review page with diff viewer for manifests and asset previews). See [specs/submissions-and-review.md](specs/submissions-and-review.md).
