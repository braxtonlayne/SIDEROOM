import { Client, Intents, SlashCommandBuilder } from '../../packages/bot/src/index';

/**
 * A running bot of the in-repo `@sideroom/bot` SDK (RDY-DEV-005): it connects to the gateway with a bot token,
 * registers one global slash command at READY and answers every use of it. The gate spec starts it with the token
 * it copied from the developer portal, so the bot is exactly what a developer would run.
 *
 * `apiUrl` is the server itself (http://127.0.0.1:8080/api/v1 in CI and locally), not the Vite proxy: a bot talks
 * to the API and the gateway, never to a web page.
 */
export interface RunningBot {
  /** Resolves when READY arrived and the command registration was sent. */
  ready: Promise<{ applicationId: string; username: string }>;
  /** Names of the commands the bot answered so far. */
  answered: string[];
  errors: string[];
  stop(): void;
}

export function startBot(opts: {
  apiUrl: string;
  token: string;
  command: { name: string; description: string };
  reply: string;
}): RunningBot {
  const client = new Client({
    token: opts.token,
    apiUrl: opts.apiUrl,
    intents: Intents.Guilds | Intents.GuildMessages,
    // One shard, no waiting for the recommended count: the run is one bot in one server.
    shards: 1,
    identifyGapMs: 0,
  });
  const answered: string[] = [];
  const errors: string[] = [];

  client.command(
    new SlashCommandBuilder().setName(opts.command.name).setDescription(opts.command.description),
    async (i) => {
      await i.reply(opts.reply);
      answered.push(opts.command.name);
    },
  );
  client.on('error', (e) => errors.push(String(e)));
  client.on('fatal', (code, reason) => errors.push(`gateway closed for good: ${code} ${reason}`));

  const ready = new Promise<{ applicationId: string; username: string }>((resolve, reject) => {
    client.on('ready', (r) => {
      resolve({ applicationId: r.application?.id ?? '', username: r.user.username });
    });
    client.on('fatal', (code, reason) => reject(new Error(`gateway: ${code} ${reason}`)));
  });
  client.login().catch((e: unknown) => {
    errors.push(String(e));
  });

  return { ready, answered, errors, stop: () => client.destroy() };
}
