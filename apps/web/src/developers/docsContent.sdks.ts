// Documentation pages for the bot SDKs (DEV-SDK-001, DEV-SDK-002) and for porting a Discord bot.
import type { DocPage } from './docsContent';

export const SDK_PAGES: DocPage[] = [
  {
    id: 'sdks',
    title: 'Bot SDKs',
    summary: 'The Rust crate sr-sdk and the TypeScript package @sideroom/bot.',
    blocks: [
      {
        t: 'p',
        text: 'Both SDKs are small and typed. Each has a REST client with rate-limit retries, gateway shards (identify, resume with the missed events replayed, heartbeats, reconnect), a cache of guilds, channels and roles, and a command framework that registers your slash commands and routes interactions to handlers. They talk to the same API as any Discord library, so you can also keep using discord.js, discord.py, serenity or twilight.',
      },
      { t: 'h', text: 'Rust: sr-sdk' },
      {
        t: 'code',
        lang: 'rust',
        title: 'A bot with one slash command',
        code: 'use sr_sdk::{Client, CommandDef, CommandRouter, OptionDef, intents};\n\n#[tokio::main]\nasync fn main() -> sr_sdk::Result<()> {\n    let router = CommandRouter::new().command(\n        CommandDef::chat_input("echo", "Repeat some text")\n            .option(OptionDef::string("text", "What to repeat").required()),\n        |cx| async move {\n            let text = cx.args.str("text").unwrap_or_default().to_owned();\n            cx.reply(text).await\n        },\n    );\n    Client::builder(std::env::var("SR_BOT_TOKEN").unwrap(), "{origin}/api/v1")\n        .intents(intents::DEFAULT | intents::MESSAGE_CONTENT)\n        .commands(router)\n        .build()?\n        .start()\n        .await\n}',
      },
      { t: 'h', text: 'TypeScript: @sideroom/bot' },
      {
        t: 'code',
        lang: 'typescript',
        title: 'The same bot',
        code: "import { Client, Intents, SlashCommandBuilder } from '@sideroom/bot';\n\nconst client = new Client({\n  token: process.env.SR_BOT_TOKEN!,\n  apiUrl: '{origin}/api/v1',\n  intents: Intents.Guilds | Intents.GuildMessages | Intents.MessageContent,\n});\n\nclient.command(\n  new SlashCommandBuilder()\n    .setName('echo')\n    .setDescription('Repeat some text')\n    .addStringOption((o) => o.setName('text').setDescription('What to repeat').setRequired()),\n  (i) => i.reply(i.options.getString('text') ?? ''),\n);\nclient.on('fatal', (code, reason) => console.error(code, reason));\nawait client.login();",
      },
      {
        t: 'ul',
        items: [
          'Commands are registered globally the first time the gateway is ready. Turn that off to manage them yourself.',
          'A command handler that fails sends the person a short private error and reports the error to your handler.',
          'Sharding follows GET /gateway/bot by default; start one shard of many per process with an explicit shard id and count.',
          'File options (attachments) arrive in data.resolved.attachments with a signed download URL.',
          'Gateway reconnects: a shard that loses its connection resumes the session and gets the events it missed; when the server can no longer replay them it identifies again (the Rust SDK also joins the voice channel it was in again).',
          'Bots in voice (Rust): sr_sdk::voice_transport joins the channel the gateway names in VOICE_SERVER_UPDATE and moves Opus frames (feature voice-transport, on by default); sr_sdk::voice is the MLS half (feature voice-e2ee, on by default): an identity key, the call state machine and the frame transform. Calls are end-to-end encrypted, so the SDK never sends audio into an encrypted call without a transform. VoiceLink reconnects: it re-attaches a dropped media WebSocket with RESUME, joins again when the SFU forgot the session, and stops when a moderator removes the bot. The SDK ships no Opus encoder; examples/voice_bot.rs joins, answers the MLS handshake (E2eeCall, GatewayHandle::e2ee_mls), sends test frames under the epoch key, logs who speaks and can drop its own socket to show the resume.',
          'Bots in voice (TypeScript): import from @sideroom/bot/voice, which mirrors the Rust names. E2eeCall, E2eeMedia and Identity do the MLS key agreement over the same sr-e2ee WebAssembly bundle the web app uses (@sideroom/wasm); VoiceSession and VoiceLink join the SFU with the werift WebRTC stack for Node and reconnect the same way as in Rust. Wire them as in packages/bot/src/voice.live.test.ts: op 4 with client.voiceStateUpdate, E2EE_MLS dispatches into E2eeCall.handle and its actions back with client.e2eeMls, keys into E2eeMedia, and the media as the VoiceSession transform. The TypeScript shard resumes the gateway session but does not send the join again after a fresh session; do that on the ready event.',
        ],
      },
    ],
  },
  {
    id: 'porting',
    title: 'Porting from Discord',
    summary: 'What stays the same and what differs for a bot written for Discord.',
    blocks: [
      {
        t: 'p',
        text: 'Change the REST base URL, the gateway URL and the token. Most bots then run unchanged. These are the differences to check.',
      },
      {
        t: 'ul',
        items: [
          'Test servers: an unreleased app only installs in servers with 25 members or fewer that you or an accepted tester manage (error 900120 otherwise).',
          'The permissions a bot gets at install are capped to what the person installing it holds.',
          'A non-ephemeral interaction reply is a normal message from the bot and needs Send Messages in the channel.',
          'Privileged intents are reviewed when you release a version; until then the ones you ask for work in test servers.',
          'Sharding: use the count GET /gateway/bot recommends (one per 1000 servers).',
          'Interactions work in servers only: no direct messages and no user installs. Commands that register integration_types [1] are accepted (HTTP 200) and ignored: they are never listed, autocompleted or dispatched, and integration_types containing 0 is stored as [0].',
          'Buttons round trip on messages; select menus and modal text inputs pass through the interaction payload but are not stored on the message.',
          'Editing a posted interaction message changes its content only.',
          'Webhooks: username, avatar_url, wait and thread_id work. An executed message shows the webhook as its author.',
          'Application emojis can be uploaded in the portal; they cannot be used in messages yet.',
          'Not available: teams, role connections, monetization, activities, and bots in voice with a Discord library (voice needs the SIDE ROOM SDK: the media transport is not the Discord voice protocol).',
        ],
      },
    ],
  },
];
