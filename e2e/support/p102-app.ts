import type { APIRequestContext } from '@playwright/test';
import { Client } from '../../packages/bot/src/index';
import { PUBLIC_URL } from './cli';
import { apiPost } from './fixtures';

/**
 * Wave 102 (D-968): an application with a slash command that has subcommands and a choice option, made by a named
 * function a spec calls, so the Lounge's Command form (and, with the bot, the app message's select menu and the
 * modal's select) can be reached on a stack. `world.ts` and `bot.ts` are not changed and nothing is added to the
 * sweep world's seed: every name carries `p102cmd-app-` and a random suffix (D-942), and `remove()` undoes it.
 *
 * ASSUMES: the stack's API is reachable at `PUBLIC_URL`'s `/api/v1` (the Vite proxy, as the specs use it) or at
 * `SR_P102_API_URL`; the Owner's request context is signed in (the sweep world's `owner.json`) and its password is
 * known (`bootstrapState().owner.password`, only for `remove()`: deleting an application re-confirms the password);
 * the guild is one the Owner can manage. NO SQL: every step is a product route.
 *
 * THE ROUTES, in order (what each returns is in the handoff):
 *  1. `POST /applications {name}` (session + CSRF): the application and its bot; the response holds `bot_token`.
 *  2. `POST /oauth2/authorize {client_id, scope: "bot applications.commands", guild_id, authorize: true,
 *     response_type: "code"}` (session + CSRF): installs the bot in the guild (the consent page's own call).
 *  3. `POST /applications/{id}/commands` with `Authorization: Bot <token>` (no session): registers `p102cmd`, a
 *     global command, with a subcommand group `team` (subcommands `add` and `drop`; `add` takes a string option
 *     `mode` with three choices) and a subcommand `status` (it takes the same choice option).
 *  remove(): `DELETE /applications/{id}/commands/{command_id}` (bot token), then `DELETE /applications/{id}`
 *  {password} (session + CSRF), which removes the bot everywhere.
 *
 * THE THREE LINES a later spec writes:
 *   const app = await seedChoiceCommand({ owner: ownerCtx.request, ownerPassword, guildId });
 *   try { /* open a channel, type "/p102cmd", take the suggestion: the Command form opens *\/ }
 *   finally { await app.remove(); }
 * The composer keeps a channel's command list for a minute: open the channel only after `seedChoiceCommand`.
 *
 * `startChoiceBot(app)` connects the bot to the gateway (the in-repo SDK, like `bot.ts`) and answers: `p102cmd status`
 * with a message that holds a select menu (reaches `msg/Components.tsx`); `p102cmd team drop` with a modal that holds
 * a select (reaches `apps/InteractionModal.tsx`); `p102cmd team add` with a plain reply. Nothing is registered by
 * the bot itself (the command was registered by route 3), so it never overwrites anything.
 */

const suffix = () => Math.random().toString(36).slice(2, 8);
const API = () =>
  (process.env.SR_P102_API_URL ?? `${new URL(PUBLIC_URL).origin}/api/v1`).replace(/\/$/, '');

export const CHOICES = [
  { name: 'Fast', value: 'fast' },
  { name: 'Careful', value: 'careful' },
  { name: 'Thorough', value: 'thorough' },
];

export interface SeededApp {
  applicationId: string;
  commandId: string;
  /** The application's name (carries the random suffix). */
  name: string;
  /** The bot token: for `startChoiceBot` only. Never print it. */
  botToken: string;
  /** Delete the command, then the application (and its bot). Safe to call twice. */
  remove: () => Promise<void>;
}

export interface SeedArgs {
  /** The Owner's (or any manager's) signed-in request context. */
  owner: APIRequestContext;
  /** That account's password: `remove()` needs it. */
  ownerPassword: string;
  guildId: string;
}

async function botFetch(token: string, method: string, path: string, body?: unknown) {
  const res = await fetch(`${API()}${path}`, {
    method,
    headers: {
      Authorization: `Bot ${token}`,
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${await res.text()}`);
  return res.status === 204 ? undefined : ((await res.json()) as Record<string, unknown>);
}

export async function seedChoiceCommand(a: SeedArgs): Promise<SeededApp> {
  const name = `p102cmd-app-${suffix()}`;
  const made = await apiPost<{
    application: { id: string };
    bot_token: string;
  }>(a.owner, '/applications', { name });
  const applicationId = made.application.id;
  const botToken = made.bot_token;
  let commandId = '';
  let removed = false;
  const remove = async () => {
    if (removed) return;
    removed = true;
    if (commandId)
      await botFetch(
        botToken,
        'DELETE',
        `/applications/${applicationId}/commands/${commandId}`,
      ).catch(() => undefined);
    const origin = new URL(PUBLIC_URL).origin;
    const csrf = await a.owner.get('/api/v1/sr/csrf', { headers: { Origin: origin } });
    const token = ((await csrf.json()) as { token: string }).token;
    await a.owner.delete(`/api/v1/applications/${applicationId}`, {
      headers: { Origin: origin, 'X-SR-CSRF': token },
      data: { password: a.ownerPassword },
    });
  };
  try {
    await apiPost(a.owner, '/oauth2/authorize', {
      client_id: applicationId,
      scope: 'bot applications.commands',
      guild_id: a.guildId,
      permissions: String((1 << 10) | (1 << 11) | (1 << 16)),
      response_type: 'code',
      authorize: true,
    });
    const mode = {
      type: 3,
      name: 'mode',
      description: 'How to run it',
      required: false,
      choices: CHOICES,
    };
    const cmd = await botFetch(botToken, 'POST', `/applications/${applicationId}/commands`, {
      type: 1,
      name: 'p102cmd',
      description: 'A command with subcommands and a choice',
      options: [
        {
          type: 2,
          name: 'team',
          description: 'Team actions',
          options: [
            { type: 1, name: 'add', description: 'Add one', options: [mode] },
            { type: 1, name: 'drop', description: 'Drop one' },
          ],
        },
        { type: 1, name: 'status', description: 'Show the status', options: [mode] },
      ],
    });
    commandId = String(cmd?.['id'] ?? '');
    if (!commandId) throw new Error('the command registration returned no id');
  } catch (e) {
    await remove().catch(() => undefined);
    throw e;
  }
  return { applicationId, commandId, name, botToken, remove };
}

/** Connects the app's bot to the gateway and answers the command (see the header). `stop()` disconnects. */
export function startChoiceBot(app: SeededApp): {
  ready: Promise<void>;
  errors: string[];
  stop(): void;
} {
  const errors: string[] = [];
  const client = new Client({
    token: app.botToken,
    apiUrl: API(),
    intents: 1 | (1 << 9),
    shards: 1,
    identifyGapMs: 0,
  });
  client.on('error', (e) => {
    errors.push(String(e));
  });
  client.on('fatal', (code, reason) => {
    errors.push(`gateway closed for good: ${code} ${reason}`);
  });
  const options = CHOICES.map((c) => ({ label: c.name, value: c.value }));
  client.on('interactionCreate', (i) => {
    const raw = i.raw;
    if (raw.type !== 2 || raw.data?.name !== 'p102cmd') return;
    const sub = JSON.stringify(raw.data?.options ?? []);
    const send = (type: number, body: unknown) =>
      client.rest.interactionCallback(raw, type, body as never).catch((e: unknown) => {
        errors.push(String(e));
      });
    if (sub.includes('"drop"')) {
      void send(9, {
        custom_id: 'p102cmd-modal',
        title: 'Drop one',
        components: [
          {
            type: 1,
            components: [
              { type: 3, custom_id: 'pick', label: 'Which one', placeholder: 'Pick one', options },
            ],
          },
        ],
      });
    } else if (sub.includes('"status"')) {
      void send(4, {
        content: 'p102cmd status',
        components: [
          {
            type: 1,
            components: [
              { type: 3, custom_id: 'p102cmd-menu', placeholder: 'Pick a speed', options },
            ],
          },
        ],
      });
    } else {
      void send(4, { content: 'p102cmd done' });
    }
  });
  const ready = new Promise<void>((resolve, reject) => {
    client.on('ready', () => {
      resolve();
    });
    client.on('fatal', (code, reason) => reject(new Error(`gateway: ${code} ${reason}`)));
  });
  client.login().catch((e: unknown) => {
    errors.push(String(e));
  });
  return { ready, errors, stop: () => client.destroy() };
}
