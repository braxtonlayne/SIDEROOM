//! Server Guide content (LNG-SSET-066): a welcome message, up to five to-dos for new
//! members and resource pages. Whether the guide shows is `CommunitySettings::guide_enabled`.

use serde::{Deserialize, Serialize};
use sr_core::{ChannelId, GuildId};

use super::error::{SettingsError, SettingsResult};

pub const MAX_WELCOME_MESSAGE: usize = 300;
pub const MAX_TODOS: usize = 5;
pub const MAX_TODO_TITLE: usize = 100;
pub const MAX_TODO_DESCRIPTION: usize = 200;
pub const MAX_RESOURCES: usize = 10;
pub const MAX_RESOURCE_TITLE: usize = 50;
pub const MAX_RESOURCE_CONTENT: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideTodo {
    pub id: u64,
    pub title: String,
    pub description: Option<String>,
    /// The channel the to-do points to ("say hi in #general").
    pub channel_id: Option<ChannelId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideResource {
    pub id: u64,
    pub title: String,
    /// Markdown.
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Guide {
    pub guild_id: GuildId,
    pub welcome_message: Option<String>,
    pub todos: Vec<GuideTodo>,
    pub resources: Vec<GuideResource>,
}

/// Validate and normalise (trim, drop empty optionals).
pub fn validate_guide(mut g: Guide) -> SettingsResult<Guide> {
    let bad = |field: &str, msg: &str| SettingsError::invalid(field, "GUIDE_INVALID", msg);
    g.welcome_message = g
        .welcome_message
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty());
    if g.welcome_message
        .as_deref()
        .is_some_and(|m| m.chars().count() > MAX_WELCOME_MESSAGE)
    {
        return Err(bad("welcome_message", "at most 300 characters"));
    }
    if g.todos.len() > MAX_TODOS {
        return Err(bad("todos", "at most 5 to-dos"));
    }
    for t in &mut g.todos {
        t.title = t.title.trim().to_owned();
        if !(1..=MAX_TODO_TITLE).contains(&t.title.chars().count()) {
            return Err(bad("todos", "to-do titles are 1-100 characters"));
        }
        t.description = t
            .description
            .take()
            .map(|d| d.trim().to_owned())
            .filter(|d| !d.is_empty());
        if t.description
            .as_deref()
            .is_some_and(|d| d.chars().count() > MAX_TODO_DESCRIPTION)
        {
            return Err(bad(
                "todos",
                "to-do descriptions are at most 200 characters",
            ));
        }
    }
    if g.resources.len() > MAX_RESOURCES {
        return Err(bad("resources", "at most 10 resource pages"));
    }
    for r in &mut g.resources {
        r.title = r.title.trim().to_owned();
        if !(1..=MAX_RESOURCE_TITLE).contains(&r.title.chars().count()) {
            return Err(bad("resources", "page titles are 1-50 characters"));
        }
        if r.content.chars().count() > MAX_RESOURCE_CONTENT {
            return Err(bad("resources", "pages are at most 2000 characters"));
        }
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide() -> Guide {
        Guide {
            guild_id: GuildId::new(1),
            ..Guide::default()
        }
    }

    #[test]
    fn limits_and_normalisation() {
        let mut g = guide();
        g.welcome_message = Some("  hi  ".into());
        assert_eq!(
            validate_guide(g.clone())
                .unwrap()
                .welcome_message
                .as_deref(),
            Some("hi")
        );
        g.welcome_message = Some("   ".into());
        assert_eq!(validate_guide(g).unwrap().welcome_message, None);

        let todo = |n: u64| GuideTodo {
            id: n,
            title: "Say hi".into(),
            description: None,
            channel_id: None,
        };
        let mut g = guide();
        g.todos = (0..6).map(todo).collect();
        assert!(validate_guide(g.clone()).is_err());
        g.todos.truncate(5);
        assert!(validate_guide(g.clone()).is_ok());
        g.todos[0].title = " ".into();
        assert!(validate_guide(g).is_err());

        let mut g = guide();
        g.resources = vec![GuideResource {
            id: 1,
            title: "Rules".into(),
            content: "x".repeat(MAX_RESOURCE_CONTENT + 1),
        }];
        assert!(validate_guide(g).is_err());
    }
}
