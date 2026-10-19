//! serenity sample bot for the weekly SDK compatibility run (M-16, D-306, DEV-SDK-003).
//!
//! Behind the `sdk-compat` feature, so a normal workspace build never compiles serenity:
//!
//! ```sh
//! SDK_COMPAT_BASE=http://127.0.0.1:8080 SDK_COMPAT_TOKEN=... SDK_COMPAT_CONTENT=1 \
//!   cargo run -p sr-server --example serenity_bot --features sdk-compat
//! ```
//!
//! The bot walks tier 2 of `docs/dev/sdk-compat.md` and prints one JSON object per line on stdout (the
//! protocol `e2e/sdk-compat/run.mjs` reads; `discordjs-bot.mjs` prints the same events):
//! `ready`, `guild_create`, `command_registered`, `message` (with the `content` the library saw),
//! `delete_attempt` (HTTP status and API error code of a delete the bot has no right to make),
//! `interaction` and `interaction_replied`. The harness drives the people; the bot only reacts.
//!
//! Environment: `SDK_COMPAT_BASE` (server origin, no path), `SDK_COMPAT_TOKEN` (bot token),
//! `SDK_COMPAT_CONTENT` (`1` asks for the privileged MESSAGE_CONTENT intent).

use serde_json::{Value, json};
use serenity::all::{
    ClientBuilder, Command, Context, CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage,
    EventHandler, GatewayIntents, Guild, HttpBuilder, Interaction, Message, Ready,
};
use serenity::async_trait;

fn emit(v: Value) {
    println!("{v}");
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        emit(json!({
            "event": "ready",
            "user": ready.user.id.to_string(),
            "bot": ready.user.bot,
            "application": ready.application.id.to_string(),
        }));
        let cmd = CreateCommand::new("ping").description("Answers pong");
        match Command::create_global_command(&ctx.http, cmd).await {
            Ok(c) => emit(json!({"event": "command_registered", "name": c.name, "id": c.id.to_string()})),
            Err(e) => emit(json!({"event": "error", "where": "create_global_command", "message": e.to_string()})),
        }
    }

    async fn guild_create(&self, _ctx: Context, guild: Guild, _is_new: Option<bool>) {
        emit(json!({"event": "guild_create", "guild": guild.id.to_string(), "name": guild.name}));
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        emit(json!({
            "event": "message",
            "id": msg.id.to_string(),
            "content": msg.content,
            "author": msg.author.id.to_string(),
        }));
        // The bot holds no MANAGE_MESSAGES: deleting a person's message must be a 403.
        let (status, code) = match msg.delete(&ctx.http).await {
            Ok(()) => (200, 0),
            Err(serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(r))) => {
                (u64::from(r.status_code.as_u16()), r.error.code)
            }
            Err(_) => (0, 0),
        };
        emit(json!({"event": "delete_attempt", "status": status, "code": code}));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Interaction::Command(command) = interaction else {
            return;
        };
        emit(json!({"event": "interaction", "name": command.data.name}));
        let reply = CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content("pong from serenity"),
        );
        match command.create_response(&ctx.http, reply).await {
            Ok(()) => emit(json!({"event": "interaction_replied"})),
            Err(e) => emit(json!({"event": "error", "where": "create_response", "message": e.to_string()})),
        }
    }
}

#[tokio::main]
async fn main() {
    let base = std::env::var("SDK_COMPAT_BASE").expect("SDK_COMPAT_BASE");
    let token = std::env::var("SDK_COMPAT_TOKEN").expect("SDK_COMPAT_TOKEN");
    let mut intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES;
    if std::env::var("SDK_COMPAT_CONTENT").as_deref() == Ok("1") {
        intents |= GatewayIntents::MESSAGE_CONTENT;
    }
    // The proxy setting replaces `https://discord.com` in every REST URL; `/api/v10` is an alias of `/api/v1`.
    let http = HttpBuilder::new(&token)
        .proxy(base.trim_end_matches('/'))
        .ratelimiter_disabled(true)
        .build();
    let mut client = ClientBuilder::new_with_http(http, intents)
        .event_handler(Handler)
        .await
        .expect("client");
    if let Err(e) = client.start().await {
        emit(json!({"event": "error", "where": "start", "message": e.to_string()}));
        std::process::exit(1);
    }
}
