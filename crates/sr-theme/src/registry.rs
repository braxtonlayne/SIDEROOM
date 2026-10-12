//! The token registry. Names mirror `packages/ui/src/tokens/themes.ts`; a test in
//! `tests/ui_parity.rs` fails if the UI kit uses a token that is missing here.

/// Value grammar a token accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Hex (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `rgb()/rgba()/hsl()/hsla()`, `transparent`.
    Color,
    /// `0` or a number with `px`, `rem`, `em`, `%`.
    Length,
    /// Comma-separated font families (quoted or bare).
    Font,
    /// Box shadow list.
    Shadow,
    /// Plain number.
    Number,
    /// `linear-gradient()` / `repeating-linear-gradient()` without URLs.
    Gradient,
}

#[derive(Debug, Clone, Copy)]
pub struct TokenSpec {
    pub name: &'static str,
    pub kind: TokenKind,
}

const fn t(name: &'static str, kind: TokenKind) -> TokenSpec {
    TokenSpec { name, kind }
}

use TokenKind::{Color, Font, Gradient, Length, Number, Shadow};

/// Every token a theme package may set, in output order.
pub const TOKENS: &[TokenSpec] = &[
    // Semantic UI tokens (every built-in theme defines all of these).
    t("--ui-bg", Color),
    t("--ui-bg-raised", Color),
    t("--ui-bg-overlay", Color),
    t("--ui-bg-hover", Color),
    t("--ui-border", Color),
    t("--ui-text", Color),
    t("--ui-text-muted", Color),
    t("--ui-text-faint", Color),
    t("--ui-accent", Color),
    t("--ui-accent-hover", Color),
    t("--ui-accent-ink", Color),
    t("--ui-danger", Color),
    t("--ui-danger-ink", Color),
    t("--ui-success", Color),
    t("--ui-warning", Color),
    t("--ui-info", Color),
    t("--ui-focus", Color),
    t("--ui-radius", Length),
    t("--ui-radius-lg", Length),
    t("--ui-font-body", Font),
    t("--ui-font-display", Font),
    t("--ui-font-mono", Font),
    t("--ui-shadow-overlay", Shadow),
    t("--ui-backdrop", Color),
    t("--ui-status-online", Color),
    t("--ui-status-idle", Color),
    t("--ui-status-dnd", Color),
    t("--ui-status-offline", Color),
    // Lounge (Discord-like) tokens, docs/10 section 3.
    t("--bg-base-lowest", Color),
    t("--bg-base-lower", Color),
    t("--bg-base-low", Color),
    t("--bg-surface-overlay", Color),
    t("--text-normal", Color),
    t("--text-muted", Color),
    t("--interactive-normal", Color),
    t("--interactive-hover", Color),
    t("--interactive-active", Color),
    t("--brand", Color),
    t("--mention-bg", Color),
    t("--danger", Color),
    // Lounge layout constants.
    t("--lounge-server-rail", Length),
    t("--lounge-channel-list", Length),
    t("--lounge-member-list", Length),
    t("--lounge-header", Length),
    t("--lounge-composer-min", Length),
    t("--lounge-message-gutter", Length),
    // Side Room tokens, docs/10 section 2.
    t("--sr-bg-0", Color),
    t("--sr-bg-1", Color),
    t("--sr-bg-2", Color),
    t("--sr-bg-olive", Color),
    t("--sr-line", Color),
    t("--sr-text", Color),
    t("--sr-text-2", Color),
    t("--sr-text-3", Color),
    t("--sr-gold", Color),
    t("--sr-gold-ink", Color),
    t("--sr-olive-accent", Color),
    t("--sr-font-display", Font),
    t("--sr-font-body", Font),
    t("--sr-font-mono", Font),
    t("--sr-radius", Length),
    t("--sr-eyebrow-size", Length),
    t("--sr-eyebrow-tracking", Length),
    t("--sr-eyebrow-rule", Length),
    // Side Room Broadcast (Live TV) layer.
    t("--sr-guide-cell", Color),
    t("--sr-guide-cell-now", Color),
    t("--sr-guide-channel", Color),
    t("--sr-guide-marker", Color),
    t("--sr-teletext-font", Font),
    t("--sr-crt-frame", Color),
    t("--sr-crt-radius", Length),
    t("--sr-scanline", Gradient),
    t("--sr-scanline-opacity", Number),
];

/// Look up a token by CSS custom property name.
pub fn token_spec(name: &str) -> Option<&'static TokenSpec> {
    TOKENS.iter().find(|s| s.name == name)
}
