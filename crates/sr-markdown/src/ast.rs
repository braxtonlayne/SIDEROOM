//! The markdown AST. Serializes to tagged JSON (`{"type":"bold","children":[..]}`)
//! for the React renderer.

use serde::{Deserialize, Serialize};

/// Timestamp display style (`<t:unix:STYLE>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampStyle {
    /// `t` short time.
    ShortTime,
    /// `T` long time.
    LongTime,
    /// `d` short date.
    ShortDate,
    /// `D` long date.
    LongDate,
    /// `f` short date/time (the default).
    ShortDateTime,
    /// `F` long date/time.
    LongDateTime,
    /// `R` relative.
    Relative,
}

impl TimestampStyle {
    /// Parse the single-letter style code.
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            't' => Self::ShortTime,
            'T' => Self::LongTime,
            'd' => Self::ShortDate,
            'D' => Self::LongDate,
            'f' => Self::ShortDateTime,
            'F' => Self::LongDateTime,
            'R' => Self::Relative,
            _ => return None,
        })
    }
}

/// How a link was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// `[text](url)`
    Masked,
    /// Bare `https://...`
    Auto,
    /// `<https://...>` (embed suppressed)
    AutoSuppressed,
}

/// A node of the parsed message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// Literal text (escapes already resolved).
    Text { text: String },
    /// A newline.
    LineBreak,
    /// `**x**`
    Bold { children: Vec<Node> },
    /// `*x*` or `_x_`
    Italic { children: Vec<Node> },
    /// `__x__`
    Underline { children: Vec<Node> },
    /// `~~x~~`
    Strike { children: Vec<Node> },
    /// `||x||`
    Spoiler { children: Vec<Node> },
    /// `` `x` ``
    InlineCode { code: String },
    /// Fenced code block.
    CodeBlock { lang: Option<String>, code: String },
    /// `> x` / `>>> x`
    Quote { children: Vec<Node> },
    /// `# x` (level 1 to 3)
    Heading { level: u8, children: Vec<Node> },
    /// `-# x`
    Subtext { children: Vec<Node> },
    /// Bulleted or numbered list; each item is a node list that may end in a nested `List`.
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<Vec<Node>>,
    },
    /// A hyperlink.
    Link {
        url: String,
        kind: LinkKind,
        children: Vec<Node>,
    },
    /// `<@id>` / `<@!id>`
    UserMention { id: u64 },
    /// `<@&id>`
    RoleMention { id: u64 },
    /// `<#id>`
    ChannelMention { id: u64 },
    /// `@everyone`
    Everyone,
    /// `@here`
    Here,
    /// `<:name:id>` / `<a:name:id>`
    CustomEmoji {
        name: String,
        id: u64,
        animated: bool,
    },
    /// `<t:unix[:style]>`
    Timestamp {
        unix: i64,
        style: Option<TimestampStyle>,
    },
    /// `</name:id>`
    SlashCommand { name: String, id: u64 },
    /// `<id:customize|browse|guide|linked-roles[:id]>`
    GuildNav { kind: String, id: Option<u64> },
    /// `<cinema:id>`
    Cinema { id: String },
    /// `<library:id>`
    Library { id: String },
}

impl Node {
    pub(crate) fn text(s: impl Into<String>) -> Self {
        Node::Text { text: s.into() }
    }
}

/// Merge adjacent `Text` nodes and drop empty ones.
pub(crate) fn merge_text(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for n in nodes {
        if let Node::Text { text } = &n {
            if text.is_empty() {
                continue;
            }
            if let Some(Node::Text { text: prev }) = out.last_mut() {
                prev.push_str(text);
                continue;
            }
        }
        out.push(n);
    }
    out
}
