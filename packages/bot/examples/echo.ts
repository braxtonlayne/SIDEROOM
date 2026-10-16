// SR_BOT_TOKEN=... SR_API_URL=https://chat.example/api/v1 node --experimental-strip-types examples/echo.ts
import { Client, Intents, SlashCommandBuilder } from '../src/index';

const client = new Client({
  token: process.env.SR_BOT_TOKEN ?? '',
  apiUrl: process.env.SR_API_URL ?? '',
  intents: Intents.Guilds | Intents.GuildMessages | Intents.MessageContent,
});

client.command(
  new SlashCommandBuilder()
    .setName('echo')
    .setDescription('Repeat some text')
    .addStringOption((o) => o.setName('text').setDescription('What to repeat').setRequired()),
  (i) => i.reply(i.options.getString('text') ?? ''),
);

client.on('messageCreate', async (m) => {
  if (!m.author.bot && m.content === '!ping') await client.rest.createMessage(m.channel_id, 'pong');
});
client.on('error', (e) => console.error('bot error:', e));
client.on('fatal', (code, reason) => console.error(`gateway closed for good: ${code} ${reason}`));

await client.login();
