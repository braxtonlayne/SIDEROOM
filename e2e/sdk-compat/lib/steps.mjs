// The tier-2 walk of docs/dev/sdk-compat.md, shared by every library (M-16). A library's sample bot prints
// one JSON object per line (see crates/sr-server/examples/serenity_bot.rs for the list of events); this
// module drives the people (messages, the slash command) and turns what the bot saw into check results.

export const checkNames = [
  'ready',
  'guild-create',
  'command-registered',
  'message-content-gated',
  'forbidden-delete',
  'slash-command-round-trip',
  'message-content-delivered',
];

/** A running sample bot and the events it has printed. */
export class BotRun {
  constructor(child) {
    this.child = child;
    this.events = [];
    this.stderr = '';
    this.exited = null;
    this.waiters = [];
    let buf = '';
    child.stdout.on('data', (d) => {
      buf += d.toString();
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i).trim();
        buf = buf.slice(i + 1);
        if (!line.startsWith('{')) continue;
        try {
          this.#push(JSON.parse(line));
        } catch {
          /* a log line, not an event */
        }
      }
    });
    child.stderr.on('data', (d) => {
      this.stderr = (this.stderr + d).slice(-4000);
    });
    child.on('exit', (code) => {
      this.exited = code ?? -1;
      for (const w of [...this.waiters]) w.check();
    });
  }

  #push(e) {
    this.events.push(e);
    for (const w of [...this.waiters]) w.check();
  }

  /** Resolves with the first event matching `pred` (already seen or still to come). */
  until(pred, ms, what) {
    return new Promise((resolve, reject) => {
      const finish = (f) => {
        clearTimeout(timer);
        this.waiters = this.waiters.filter((x) => x !== waiter);
        f();
      };
      const waiter = {
        check: () => {
          const hit = this.events.find(pred);
          if (hit) return finish(() => resolve(hit));
          if (this.exited !== null) {
            return finish(() =>
              reject(
                new Error(`bot exited (${this.exited}) before ${what}; stderr: ${this.stderr}`),
              ),
            );
          }
        },
      };
      const timer = setTimeout(
        () =>
          finish(() =>
            reject(
              new Error(
                `timed out after ${ms} ms waiting for ${what}; events: ${this.events.map((e) => e.event).join(',')}; stderr: ${this.stderr}`,
              ),
            ),
          ),
        ms,
      );
      this.waiters.push(waiter);
      waiter.check();
    });
  }

  stop() {
    if (this.exited === null) this.child.kill();
  }
}

/** Runs one check, never throws; the result goes into the JUnit report. */
async function check(cases, name, fn) {
  const t0 = Date.now();
  try {
    await fn();
    cases.push({ name, ok: true, seconds: (Date.now() - t0) / 1000 });
  } catch (e) {
    cases.push({
      name,
      ok: false,
      message: String(e?.message ?? e),
      seconds: (Date.now() - t0) / 1000,
    });
  }
}

const expect = (cond, msg) => {
  if (!cond) throw new Error(msg);
};

/**
 * @param {{ start: (content: boolean) => BotRun, ctx: Awaited<ReturnType<typeof import('./provision.mjs').provision>>, log?: (s: string) => void, timeoutMs?: number }} o
 * @returns {Promise<{ name: string, ok: boolean, message?: string, seconds?: number }[]>}
 */
export async function walk({ start, ctx, log = () => {}, timeoutMs = 60_000 }) {
  const cases = [];
  const { person, channelId, guildId, appId } = ctx;

  // Run A: MESSAGE_CONTENT not enabled for the application, so the bot does not ask for it.
  log('run A: no MESSAGE_CONTENT');
  const a = start(false);
  try {
    await check(cases, 'ready', async () => {
      const ready = await a.until((e) => e.event === 'ready', timeoutMs, 'READY');
      expect(ready.bot === true, `READY user.bot was ${ready.bot}`);
      expect(
        String(ready.application) === appId,
        `READY application ${ready.application} is not ${appId}`,
      );
    });
    await check(cases, 'guild-create', async () => {
      const g = await a.until((e) => e.event === 'guild_create', timeoutMs, 'GUILD_CREATE');
      expect(String(g.guild) === guildId, `GUILD_CREATE for ${g.guild}, expected ${guildId}`);
    });
    let command;
    await check(cases, 'command-registered', async () => {
      command = await a.until(
        (e) => e.event === 'command_registered',
        timeoutMs,
        'the global command being registered',
      );
      expect(command.name === 'ping', `registered ${command.name}`);
    });

    const secret = `compat-secret-${Date.now().toString(36)}`;
    const sent = await person.post(`/channels/${channelId}/messages`, { content: secret });
    await check(cases, 'message-content-gated', async () => {
      const m = await a.until(
        (e) => e.event === 'message' && e.id === sent.id,
        timeoutMs,
        "MESSAGE_CREATE of a person's message",
      );
      expect(
        m.content === '',
        `without MESSAGE_CONTENT the library saw content ${JSON.stringify(m.content)}`,
      );
    });
    await check(cases, 'forbidden-delete', async () => {
      const d = await a.until((e) => e.event === 'delete_attempt', timeoutMs, 'the delete attempt');
      expect(d.status === 403, `delete answered ${d.status}, expected 403`);
      expect(d.code === 50013, `delete error code ${d.code}, expected 50013`);
    });
    await check(cases, 'slash-command-round-trip', async () => {
      expect(command, 'the command was never registered');
      await person.post('/interactions', {
        type: 2,
        application_id: appId,
        channel_id: channelId,
        nonce: `n${Date.now()}`,
        data: { id: command.id, options: [] },
      });
      await a.until(
        (e) => e.event === 'interaction' && e.name === 'ping',
        timeoutMs,
        'INTERACTION_CREATE',
      );
      await a.until((e) => e.event === 'interaction_replied', timeoutMs, 'the interaction reply');
      const list = await person.get(`/channels/${channelId}/messages?limit=10`);
      const pong = list.find((m) => m.author?.bot === true && /^pong/.test(m.content));
      expect(
        pong,
        `no bot message starting with "pong" in the channel: ${JSON.stringify(list.map((m) => m.content))}`,
      );
    });
  } finally {
    a.stop();
  }

  // Run B: the application enables the privileged intent and the bot asks for it.
  log('run B: MESSAGE_CONTENT');
  const b = (await ctx.enableMessageContent(), start(true));
  try {
    await check(cases, 'message-content-delivered', async () => {
      await b.until((e) => e.event === 'ready', timeoutMs, 'READY with MESSAGE_CONTENT');
      await b.until(
        (e) => e.event === 'guild_create',
        timeoutMs,
        'GUILD_CREATE with MESSAGE_CONTENT',
      );
      const words = `compat-visible-${Date.now().toString(36)}`;
      const sent = await person.post(`/channels/${channelId}/messages`, { content: words });
      const m = await b.until(
        (e) => e.event === 'message' && e.id === sent.id,
        timeoutMs,
        'MESSAGE_CREATE with content',
      );
      expect(
        m.content === words,
        `with MESSAGE_CONTENT the library saw ${JSON.stringify(m.content)}, expected ${JSON.stringify(words)}`,
      );
    });
  } finally {
    b.stop();
  }
  return cases;
}
