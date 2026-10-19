// discord.js sample bot for the weekly SDK compatibility run (M-16, D-306, DEV-SDK-003).
// Prints the same JSON-lines events as crates/sr-server/examples/serenity_bot.rs; run.mjs drives it.
//
// Environment: SDK_COMPAT_BASE (server origin, no path), SDK_COMPAT_TOKEN (bot token),
// SDK_COMPAT_CONTENT ("1" asks for the privileged MESSAGE_CONTENT intent).
import { Client, GatewayIntentBits, REST, Routes } from 'discord.js';

const base = (process.env.SDK_COMPAT_BASE ?? '').replace(/\/$/, '');
const token = process.env.SDK_COMPAT_TOKEN ?? '';
if (!base || !token) {
  console.error('SDK_COMPAT_BASE and SDK_COMPAT_TOKEN are required');
  process.exit(2);
}
const emit = (o) => console.log(JSON.stringify(o));

const intents = [GatewayIntentBits.Guilds, GatewayIntentBits.GuildMessages];
if (process.env.SDK_COMPAT_CONTENT === '1') intents.push(GatewayIntentBits.MessageContent);

// `/api/v10` is an alias of `/api/v1`; the gateway URL comes from GET /gateway/bot.
const restOptions = { api: `${base}/api`, version: '10', cdn: `${base}/cdn` };
const client = new Client({ intents, rest: restOptions });

client.once('clientReady', async (c) => {
  emit({ event: 'ready', user: c.user.id, bot: c.user.bot, application: c.application.id });
  try {
    const rest = new REST(restOptions).setToken(token);
    const cmd = await rest.post(Routes.applicationCommands(c.application.id), {
      body: { name: 'ping', description: 'Answers pong', type: 1 },
    });
    emit({ event: 'command_registered', name: cmd.name, id: cmd.id });
  } catch (e) {
    emit({ event: 'error', where: 'register command', message: String(e) });
  }
});

client.on('guildCreate', (g) => emit({ event: 'guild_create', guild: g.id, name: g.name }));

client.on('messageCreate', async (m) => {
  if (m.author.bot) return;
  emit({ event: 'message', id: m.id, content: m.content, author: m.author.id });
  // The bot holds no MANAGE_MESSAGES: deleting a person's message must be a 403 (API error 50013).
  try {
    await m.delete();
    emit({ event: 'delete_attempt', status: 200, code: 0 });
  } catch (e) {
    emit({ event: 'delete_attempt', status: e.status ?? 0, code: e.code ?? 0 });
  }
});

client.on('interactionCreate', async (i) => {
  if (!i.isChatInputCommand()) return;
  emit({ event: 'interaction', name: i.commandName });
  try {
    await i.reply('pong from discord.js');
    emit({ event: 'interaction_replied' });
  } catch (e) {
    emit({ event: 'error', where: 'reply', message: String(e) });
  }
});

client.on('error', (e) => emit({ event: 'error', where: 'client', message: String(e) }));

try {
  await client.login(token);
} catch (e) {
  emit({ event: 'error', where: 'login', message: String(e) });
  process.exit(1);
}
