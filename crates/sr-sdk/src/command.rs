//! Command framework: build command definitions, route interactions to async handlers and
//! read their arguments.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::cache::Cache;
use crate::error::Result;
use crate::http::{MessageBody, Rest};
use crate::model::{CallbackType, Interaction, interaction_type};

/// What every handler gets: the REST client and the cache.
#[derive(Debug, Clone)]
pub struct Context {
    pub rest: Rest,
    pub cache: Cache,
}

// ---- definitions ----

/// Application command option types (spec section 3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OptionKind {
    SubCommand = 1,
    SubCommandGroup = 2,
    String = 3,
    Integer = 4,
    Boolean = 5,
    User = 6,
    Channel = 7,
    Role = 8,
    Mentionable = 9,
    Number = 10,
    Attachment = 11,
}

#[derive(Debug, Clone)]
pub struct OptionDef(Map<String, Value>);

impl OptionDef {
    pub fn new(kind: OptionKind, name: &str, description: &str) -> Self {
        let mut m = Map::new();
        m.insert("type".into(), json!(kind as u8));
        m.insert("name".into(), json!(name));
        m.insert("description".into(), json!(description));
        Self(m)
    }

    pub fn string(name: &str, description: &str) -> Self {
        Self::new(OptionKind::String, name, description)
    }
    pub fn integer(name: &str, description: &str) -> Self {
        Self::new(OptionKind::Integer, name, description)
    }
    pub fn boolean(name: &str, description: &str) -> Self {
        Self::new(OptionKind::Boolean, name, description)
    }
    pub fn user(name: &str, description: &str) -> Self {
        Self::new(OptionKind::User, name, description)
    }
    pub fn attachment(name: &str, description: &str) -> Self {
        Self::new(OptionKind::Attachment, name, description)
    }
    pub fn sub_command(name: &str, description: &str) -> Self {
        Self::new(OptionKind::SubCommand, name, description)
    }

    pub fn required(mut self) -> Self {
        self.0.insert("required".into(), json!(true));
        self
    }

    pub fn autocomplete(mut self) -> Self {
        self.0.insert("autocomplete".into(), json!(true));
        self
    }

    /// A fixed choice (string or number value).
    pub fn choice(mut self, name: &str, value: impl Into<Value>) -> Self {
        let list = self
            .0
            .entry("choices")
            .or_insert_with(|| Value::Array(vec![]));
        if let Value::Array(a) = list {
            a.push(json!({"name": name, "value": value.into()}));
        }
        self
    }

    /// Nested option (for sub-commands and groups).
    pub fn option(mut self, o: OptionDef) -> Self {
        let list = self
            .0
            .entry("options")
            .or_insert_with(|| Value::Array(vec![]));
        if let Value::Array(a) = list {
            a.push(o.into_json());
        }
        self
    }

    pub fn into_json(self) -> Value {
        Value::Object(self.0)
    }
}

/// A command definition, serialised for the bulk-overwrite routes.
#[derive(Debug, Clone)]
pub struct CommandDef {
    name: String,
    json: Map<String, Value>,
}

impl CommandDef {
    /// A slash command (type 1).
    pub fn chat_input(name: &str, description: &str) -> Self {
        let mut m = Map::new();
        m.insert("name".into(), json!(name));
        m.insert("type".into(), json!(1));
        m.insert("description".into(), json!(description));
        Self {
            name: name.to_owned(),
            json: m,
        }
    }

    /// A user context-menu command (type 2).
    pub fn user(name: &str) -> Self {
        Self::context(name, 2)
    }

    /// A message context-menu command (type 3).
    pub fn message(name: &str) -> Self {
        Self::context(name, 3)
    }

    fn context(name: &str, kind: u8) -> Self {
        let mut m = Map::new();
        m.insert("name".into(), json!(name));
        m.insert("type".into(), json!(kind));
        Self {
            name: name.to_owned(),
            json: m,
        }
    }

    pub fn option(mut self, o: OptionDef) -> Self {
        let list = self
            .json
            .entry("options")
            .or_insert_with(|| Value::Array(vec![]));
        if let Value::Array(a) = list {
            a.push(o.into_json());
        }
        self
    }

    /// Bit string of the permissions needed by default (`"0"` = administrators only).
    pub fn default_member_permissions(mut self, bits: &str) -> Self {
        self.json
            .insert("default_member_permissions".into(), json!(bits));
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn into_json(self) -> Value {
        Value::Object(self.json)
    }
}

// ---- arguments ----

/// Parsed options of one invocation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    /// Sub-command group and sub-command names, outermost first.
    pub path: Vec<String>,
    values: HashMap<String, Value>,
    /// The option the user is typing, for autocomplete: `(name, text)`.
    pub focused: Option<(String, String)>,
}

impl Args {
    /// Read `data.options`, descending through sub-commands and groups.
    pub fn parse(data: &Value) -> Args {
        let mut a = Args::default();
        let mut opts = data.get("options").and_then(Value::as_array);
        while let Some(list) = opts {
            let mut next = None;
            for o in list {
                let kind = o["type"].as_u64().unwrap_or(0);
                let name = o["name"].as_str().unwrap_or_default().to_owned();
                if kind == 1 || kind == 2 {
                    a.path.push(name);
                    next = o.get("options").and_then(Value::as_array);
                } else if let Some(v) = o.get("value") {
                    if o["focused"].as_bool() == Some(true) {
                        a.focused = Some((name.clone(), v.as_str().unwrap_or_default().to_owned()));
                    }
                    a.values.insert(name, v.clone());
                }
            }
            opts = next;
        }
        a
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }
    pub fn str(&self, name: &str) -> Option<&str> {
        self.get(name)?.as_str()
    }
    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name)?.as_i64()
    }
    pub fn number(&self, name: &str) -> Option<f64> {
        self.get(name)?.as_f64()
    }
    pub fn bool(&self, name: &str) -> Option<bool> {
        self.get(name)?.as_bool()
    }
    /// A user, role, channel, mentionable or attachment option: the snowflake.
    pub fn id(&self, name: &str) -> Option<&str> {
        self.str(name)
    }
}

// ---- invocation context ----

/// One command invocation with reply helpers.
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub ctx: Context,
    pub interaction: Interaction,
    pub args: Args,
}

impl CommandContext {
    /// Answer with a message (callback type 4).
    pub async fn reply(&self, body: impl Into<MessageBody>) -> Result<()> {
        self.ctx
            .rest
            .interaction_callback(
                &self.interaction,
                CallbackType::ChannelMessage,
                Some(body.into().into_json()),
            )
            .await
    }

    /// Answer so only the invoking user sees it.
    pub async fn reply_ephemeral(&self, content: &str) -> Result<()> {
        self.reply(MessageBody::content(content).ephemeral()).await
    }

    /// Acknowledge now, answer later with [`CommandContext::edit_response`] (3 s rule).
    pub async fn defer(&self) -> Result<()> {
        self.ctx
            .rest
            .interaction_callback(
                &self.interaction,
                CallbackType::DeferredChannelMessage,
                None,
            )
            .await
    }

    /// Edit the original response; after a `defer` this creates it.
    pub async fn edit_response(&self, body: impl Into<MessageBody>) -> Result<()> {
        self.ctx
            .rest
            .interaction_edit_original(&self.interaction, body)
            .await
            .map(|_| ())
    }

    /// Send a further message for this interaction (valid for 15 minutes).
    pub async fn followup(&self, body: impl Into<MessageBody>) -> Result<()> {
        self.ctx
            .rest
            .interaction_followup(&self.interaction, body)
            .await
            .map(|_| ())
    }

    /// Autocomplete result (callback type 8, at most 25 choices).
    pub async fn autocomplete(&self, choices: Vec<(String, Value)>) -> Result<()> {
        let choices: Vec<Value> = choices
            .into_iter()
            .take(25)
            .map(|(n, v)| json!({"name": n, "value": v}))
            .collect();
        self.ctx
            .rest
            .interaction_callback(
                &self.interaction,
                CallbackType::Autocomplete,
                Some(json!({"choices": choices})),
            )
            .await
    }
}

type HandlerFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type Handler = Arc<dyn Fn(CommandContext) -> HandlerFuture + Send + Sync>;

/// Maps command names to handlers and produces the definitions to register.
#[derive(Clone, Default)]
pub struct CommandRouter {
    defs: Vec<CommandDef>,
    handlers: HashMap<String, Handler>,
    autocompletes: HashMap<String, Handler>,
}

impl std::fmt::Debug for CommandRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandRouter")
            .field(
                "commands",
                &self.defs.iter().map(CommandDef::name).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl CommandRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a definition and its handler (`async |cx| ...`).
    pub fn command<F, Fut>(mut self, def: CommandDef, handler: F) -> Self
    where
        F: Fn(CommandContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.handlers.insert(
            def.name().to_owned(),
            Arc::new(move |cx| Box::pin(handler(cx))),
        );
        self.defs.push(def);
        self
    }

    /// Handler for the autocomplete requests of command `name`.
    pub fn autocomplete<F, Fut>(mut self, name: &str, handler: F) -> Self
    where
        F: Fn(CommandContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.autocompletes
            .insert(name.to_owned(), Arc::new(move |cx| Box::pin(handler(cx))));
        self
    }

    /// The JSON array for `PUT /applications/{id}/commands`.
    pub fn definitions(&self) -> Vec<Value> {
        self.defs
            .iter()
            .cloned()
            .map(CommandDef::into_json)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Route an interaction. `Ok(false)` when it is not for a registered command.
    pub async fn dispatch(&self, ctx: &Context, interaction: Interaction) -> Result<bool> {
        let table = match interaction.kind {
            interaction_type::APPLICATION_COMMAND => &self.handlers,
            interaction_type::AUTOCOMPLETE => &self.autocompletes,
            _ => return Ok(false),
        };
        let Some(name) = interaction.command_name() else {
            return Ok(false);
        };
        let Some(handler) = table.get(name) else {
            return Ok(false);
        };
        let cx = CommandContext {
            ctx: ctx.clone(),
            args: Args::parse(&interaction.data),
            interaction,
        };
        handler(cx).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::http::fake::FakeTransport;

    fn ctx(t: Arc<FakeTransport>) -> Context {
        Context {
            rest: Rest::with_transport("https://x.test/api/v1", "tok", t),
            cache: Cache::new(),
        }
    }

    fn interaction(kind: u8, data: Value) -> Interaction {
        Interaction {
            id: "1".into(),
            application_id: "5".into(),
            kind,
            data,
            token: "itok".into(),
            ..Interaction::default()
        }
    }

    #[test]
    fn definitions_serialize_like_the_api_expects() {
        let def = CommandDef::chat_input("roll", "Roll dice")
            .option(
                OptionDef::integer("sides", "Faces")
                    .required()
                    .choice("d6", 6)
                    .choice("d20", 20),
            )
            .option(OptionDef::attachment("image", "A picture"))
            .default_member_permissions("0");
        assert_eq!(
            def.into_json(),
            json!({
                "name": "roll", "type": 1, "description": "Roll dice", "default_member_permissions": "0",
                "options": [
                    {"type": 4, "name": "sides", "description": "Faces", "required": true,
                     "choices": [{"name": "d6", "value": 6}, {"name": "d20", "value": 20}]},
                    {"type": 11, "name": "image", "description": "A picture"}
                ]
            })
        );
        assert_eq!(
            CommandDef::user("Profile").into_json(),
            json!({"name": "Profile", "type": 2})
        );
    }

    #[test]
    fn sub_commands_nest() {
        let def = CommandDef::chat_input("tag", "Tags").option(
            OptionDef::sub_command("add", "Add")
                .option(OptionDef::string("name", "Tag").required()),
        );
        assert_eq!(def.into_json()["options"][0]["options"][0]["name"], "name");
    }

    #[test]
    fn args_parse_through_sub_commands_and_report_the_focused_option() {
        let data = json!({"name": "tag", "options": [
            {"type": 1, "name": "add", "options": [
                {"type": 3, "name": "name", "value": "fun", "focused": true},
                {"type": 4, "name": "n", "value": 3},
                {"type": 5, "name": "loud", "value": true},
                {"type": 6, "name": "who", "value": "77"}]}]});
        let a = Args::parse(&data);
        assert_eq!(a.path, vec!["add"]);
        assert_eq!(
            (a.str("name"), a.int("n"), a.bool("loud"), a.id("who")),
            (Some("fun"), Some(3), Some(true), Some("77"))
        );
        assert_eq!(a.focused, Some(("name".into(), "fun".into())));
        assert_eq!(a.str("missing"), None);
    }

    #[tokio::test]
    async fn dispatch_runs_the_matching_handler_and_replies() {
        let t = FakeTransport::new(vec![FakeTransport::no_content()]);
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let s2 = seen.clone();
        let router =
            CommandRouter::new().command(CommandDef::chat_input("ping", "Pong"), move |cx| {
                let s = s2.clone();
                async move {
                    s.lock()
                        .expect("lock")
                        .push(cx.args.str("who").unwrap_or("nobody").to_owned());
                    cx.reply_ephemeral("pong").await
                }
            });
        assert_eq!(router.definitions().len(), 1);
        let i = interaction(
            2,
            json!({"name": "ping", "options": [{"type": 3, "name": "who", "value": "me"}]}),
        );
        assert!(
            router
                .dispatch(&ctx(t.clone()), i)
                .await
                .expect("dispatched")
        );
        assert_eq!(*seen.lock().expect("lock"), vec!["me"]);
        let reqs = t.seen.lock().expect("lock");
        assert_eq!(
            reqs[0].url,
            "https://x.test/api/v1/interactions/1/itok/callback"
        );
        let body: Value =
            serde_json::from_slice(reqs[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(body["data"]["flags"], 64);
    }

    #[tokio::test]
    async fn unknown_commands_and_other_interaction_kinds_are_not_handled() {
        let router =
            CommandRouter::new().command(CommandDef::chat_input("a", "A"), |_| async { Ok(()) });
        let c = ctx(FakeTransport::new(vec![]));
        assert!(
            !router
                .dispatch(&c, interaction(2, json!({"name": "zzz"})))
                .await
                .expect("ok")
        );
        assert!(
            !router
                .dispatch(&c, interaction(3, json!({"custom_id": "x"})))
                .await
                .expect("ok")
        );
        // Autocomplete only goes to autocomplete handlers.
        assert!(
            !router
                .dispatch(&c, interaction(4, json!({"name": "a"})))
                .await
                .expect("ok")
        );
    }

    #[tokio::test]
    async fn autocomplete_handlers_answer_with_at_most_25_choices() {
        let t = FakeTransport::new(vec![FakeTransport::no_content()]);
        let router = CommandRouter::new().autocomplete("pick", |cx| async move {
            let typed = cx.args.focused.clone().map(|f| f.1).unwrap_or_default();
            cx.autocomplete((0..40).map(|i| (format!("{typed}{i}"), json!(i))).collect())
                .await
        });
        let i = interaction(
            4,
            json!({"name": "pick", "options": [{"type": 3, "name": "q", "value": "a", "focused": true}]}),
        );
        assert!(router.dispatch(&ctx(t.clone()), i).await.expect("ok"));
        let reqs = t.seen.lock().expect("lock");
        let body: Value =
            serde_json::from_slice(reqs[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(body["type"], 8);
        assert_eq!(body["data"]["choices"].as_array().expect("array").len(), 25);
    }
}
