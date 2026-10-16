//! A tiny bot: `/echo text:<string>` repeats the text, `/roll sides:<int>` rolls a die, and
//! "!ping" in a channel is answered with "pong".
//!
//! ```text
//! SR_BOT_TOKEN=... SR_API_URL=https://chat.example/api/v1 cargo run -p sr-sdk --example echo_bot
//! ```
//!
//! Needs the `MESSAGE_CONTENT` intent enabled for the application to see "!ping" text.

use async_trait::async_trait;
use sr_sdk::{
    Client, CommandDef, CommandRouter, Context, Event, EventHandler, OptionDef, Result, intents,
};

struct Ping;

#[async_trait]
impl EventHandler for Ping {
    async fn on_event(&self, ctx: Context, _shard: u32, event: Event) {
        if let Event::MessageCreate(m) = event
            && !m.author.bot
            && m.content == "!ping"
        {
            let _ = ctx.rest.create_message(&m.channel_id, "pong").await;
        }
    }

    async fn on_error(&self, _ctx: Context, error: sr_sdk::Error) {
        eprintln!("bot error: {error}");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = std::env::var("SR_BOT_TOKEN").expect("set SR_BOT_TOKEN");
    let api = std::env::var("SR_API_URL").expect("set SR_API_URL, e.g. https://host/api/v1");

    let router = CommandRouter::new()
        .command(
            CommandDef::chat_input("echo", "Repeat some text")
                .option(OptionDef::string("text", "What to repeat").required()),
            |cx| async move {
                let text = cx.args.str("text").unwrap_or_default().to_owned();
                cx.reply(text).await
            },
        )
        .command(
            CommandDef::chat_input("roll", "Roll a die")
                .option(OptionDef::integer("sides", "Faces").required()),
            |cx| async move {
                let sides = cx.args.int("sides").unwrap_or(6).clamp(2, 1000);
                let n = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_nanos()) as i64)
                    % sides
                    + 1;
                cx.reply_ephemeral(&format!("You rolled {n} (d{sides})"))
                    .await
            },
        );

    Client::builder(token, api)
        .intents(intents::DEFAULT | intents::MESSAGE_CONTENT)
        .handler(Ping)
        .commands(router)
        .build()?
        .start()
        .await
}
