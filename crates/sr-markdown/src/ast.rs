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
    UserMention {
        #[serde(with = "id_string")]
        id: u64,
    },
    /// `<@&id>`
    RoleMention {
        #[serde(with = "id_string")]
        id: u64,
    },
    /// `<#id>`
    ChannelMention {
        #[serde(with = "id_string")]
        id: u64,
    },
    /// `@everyone`
    Everyone,
    /// `@here`
    Here,
    /// `<:name:id>` / `<a:name:id>`
    CustomEmoji {
        name: String,
        #[serde(with = "id_string")]
        id: u64,
        animated: bool,
    },
    /// `:shortcode:` resolved to a unicode emoji. `name` is the lowercase shortcode.
    Emoji { name: String, emoji: String },
    /// `<t:unix[:style]>`
    Timestamp {
        unix: i64,
        style: Option<TimestampStyle>,
    },
    /// `</name:id>`
    SlashCommand {
        name: String,
        #[serde(with = "id_string")]
        id: u64,
    },
    /// `<id:customize|browse|guide|linked-roles[:id]>`
    GuildNav {
        kind: String,
        #[serde(with = "opt_id_string", default)]
        id: Option<u64>,
    },
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

/// Ids are 64-bit and exceed JS's 2^53 safe-integer range, so the AST JSON carries them as
/// strings (matching `sr_core::Snowflake`). Deserialization also accepts numbers.
mod id_string {
    use serde::{Deserialize, Deserializer, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Num(u64),
        Str(String),
    }

    pub fn serialize<S: Serializer>(id: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(id)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        match Raw::deserialize(d)? {
            Raw::Num(n) => Ok(n),
            Raw::Str(s) => s.parse().map_err(serde::de::Error::custom),
        }
    }
}

mod opt_id_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(id: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        match id {
            Some(v) => s.collect_str(v),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(u64),
            Str(String),
        }
        match Option::<Raw>::deserialize(d)? {
            None => Ok(None),
            Some(Raw::Num(n)) => Ok(Some(n)),
            Some(Raw::Str(s)) => s.parse().map(Some).map_err(serde::de::Error::custom),
        }
    }
}
