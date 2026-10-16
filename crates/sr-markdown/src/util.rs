//! Mention extraction, plain-text rendering and content limits.

use crate::ast::Node;

/// Default maximum message length in characters.
pub const DEFAULT_MAX_CHARS: usize = 2000;
/// Extended maximum for patrons (doc 08; confirm against `effective_limits`).
pub const EXTENDED_MAX_CHARS: usize = 4000;
/// Default cap on distinct user + role mentions in one message (assumption, configurable).
pub const DEFAULT_MAX_MENTIONS: usize = 100;

/// Everything a message pings or references, deduplicated in first-seen order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mentions {
    /// Mentioned user ids.
    pub users: Vec<u64>,
    /// Mentioned role ids.
    pub roles: Vec<u64>,
    /// Mentioned channel ids.
    pub channels: Vec<u64>,
    /// Custom emoji ids used.
    pub emoji: Vec<u64>,
    /// Contains `@everyone`.
    pub everyone: bool,
    /// Contains `@here`.
    pub here: bool,
    /// Library and Cinema card tokens (`<library:id>`, `<cinema:id>`) outside code, counted with repeats.
    pub cards: usize,
}

fn push_unique(v: &mut Vec<u64>, id: u64) {
    if !v.contains(&id) {
        v.push(id);
    }
}

/// Collect mentions from a parsed message. Code spans/blocks never produce mentions.
pub fn extract_mentions(nodes: &[Node]) -> Mentions {
    let mut m = Mentions::default();
    walk(nodes, &mut m);
    m
}

fn walk(nodes: &[Node], m: &mut Mentions) {
    for n in nodes {
        match n {
            Node::UserMention { id } => push_unique(&mut m.users, *id),
            Node::RoleMention { id } => push_unique(&mut m.roles, *id),
            Node::ChannelMention { id } => push_unique(&mut m.channels, *id),
            Node::CustomEmoji { id, .. } => push_unique(&mut m.emoji, *id),
            Node::Everyone => m.everyone = true,
            Node::Here => m.here = true,
            Node::Library { .. } | Node::Cinema { .. } => m.cards += 1,
            Node::List { items, .. } => items.iter().for_each(|i| walk(i, m)),
            Node::Bold { children }
            | Node::Italic { children }
            | Node::Underline { children }
            | Node::Strike { children }
            | Node::Spoiler { children }
            | Node::Quote { children }
            | Node::Heading { children, .. }
            | Node::Subtext { children }
            | Node::Link { children, .. } => walk(children, m),
            _ => {}
        }
    }
}

/// Why content was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitError {
    /// Content exceeds `max` characters.
    TooLong {
        /// Allowed characters.
        max: usize,
        /// Actual characters.
        actual: usize,
    },
    /// More distinct user+role mentions than allowed.
    TooManyMentions {
        /// Allowed mentions.
        max: usize,
        /// Actual mentions.
        actual: usize,
    },
}

impl core::fmt::Display for LimitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLong { max, actual } => {
                write!(f, "message is {actual} characters; the limit is {max}")
            }
            Self::TooManyMentions { max, actual } => {
                write!(f, "message has {actual} mentions; the limit is {max}")
            }
        }
    }
}

impl std::error::Error for LimitError {}

/// Limits applied to message content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum characters (Unicode scalar values).
    pub max_chars: usize,
    /// Maximum distinct user + role mentions.
    pub max_mentions: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_chars: DEFAULT_MAX_CHARS,
            max_mentions: DEFAULT_MAX_MENTIONS,
        }
    }
}

/// Character count as counted for limits.
pub fn char_count(content: &str) -> usize {
    content.chars().count()
}

/// Validate content against limits. Length is checked before parsing so huge inputs are cheap to reject.
pub fn check_limits(content: &str, limits: &Limits) -> Result<Mentions, LimitError> {
    let actual = char_count(content);
    if actual > limits.max_chars {
        return Err(LimitError::TooLong {
            max: limits.max_chars,
            actual,
        });
    }
    let m = extract_mentions(&crate::parse(content));
    let actual = m.users.len() + m.roles.len();
    if actual > limits.max_mentions {
        return Err(LimitError::TooManyMentions {
            max: limits.max_mentions,
            actual,
        });
    }
    Ok(m)
}

/// Resolves ids to display names for plain-text rendering. All methods default to `None`.
pub trait Resolver {
    /// Display name for a user.
    fn user(&self, _id: u64) -> Option<String> {
        None
    }
    /// Name for a role.
    fn role(&self, _id: u64) -> Option<String> {
        None
    }
    /// Name for a channel.
    fn channel(&self, _id: u64) -> Option<String> {
        None
    }
}

/// A resolver that resolves nothing.
pub struct NoResolver;
impl Resolver for NoResolver {}

/// Render to plain text for search indexing and notifications.
/// Spoilers become `[spoiler]`, code keeps its content, emoji become `:name:`.
pub fn to_plain_text(nodes: &[Node]) -> String {
    to_plain_text_with(nodes, &NoResolver)
}

/// Like [`to_plain_text`] with name resolution.
pub fn to_plain_text_with(nodes: &[Node], r: &dyn Resolver) -> String {
    let mut s = String::new();
    plain(nodes, r, &mut s);
    s
}

fn plain(nodes: &[Node], r: &dyn Resolver, out: &mut String) {
    for n in nodes {
        match n {
            Node::Text { text } => out.push_str(text),
            Node::LineBreak => out.push('\n'),
            Node::InlineCode { code } | Node::CodeBlock { code, .. } => out.push_str(code),
            Node::Spoiler { .. } => out.push_str("[spoiler]"),
            Node::Bold { children }
            | Node::Italic { children }
            | Node::Underline { children }
            | Node::Strike { children }
            | Node::Quote { children }
            | Node::Heading { children, .. }
            | Node::Subtext { children }
            | Node::Link { children, .. } => {
                plain(children, r, out);
                if matches!(
                    n,
                    Node::Quote { .. } | Node::Heading { .. } | Node::Subtext { .. }
                ) {
                    out.push('\n');
                }
            }
            Node::List { items, .. } => {
                for item in items {
                    plain(item, r, out);
                    if !matches!(item.last(), Some(Node::List { .. })) {
                        out.push('\n');
                    }
                }
            }
            Node::UserMention { id } => {
                out.push('@');
                out.push_str(&r.user(*id).unwrap_or_else(|| "user".into()));
            }
            Node::RoleMention { id } => {
                out.push('@');
                out.push_str(&r.role(*id).unwrap_or_else(|| "role".into()));
            }
            Node::ChannelMention { id } => {
                out.push('#');
                out.push_str(&r.channel(*id).unwrap_or_else(|| "channel".into()));
            }
            Node::Everyone => out.push_str("@everyone"),
            Node::Here => out.push_str("@here"),
            Node::CustomEmoji { name, .. } => {
                out.push(':');
                out.push_str(name);
                out.push(':');
            }
            Node::Timestamp { unix, .. } => out.push_str(&unix.to_string()),
            Node::SlashCommand { name, .. } => {
                out.push('/');
                out.push_str(name);
            }
            Node::GuildNav { kind, .. } => out.push_str(kind),
            Node::Cinema { id } | Node::Library { id } => out.push_str(id),
        }
    }
}
