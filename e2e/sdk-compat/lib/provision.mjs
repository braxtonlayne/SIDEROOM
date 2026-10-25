// Sets up the test server for one SDK compatibility run, over the public REST API only (M-16).
// A cookie session of the bootstrapped Owner creates a server (guild), an application with a bot, and
// installs the bot through the OAuth2 flow a person would use. The Owner is also the "person" of the run:
// writes messages and invokes the slash command.

export class Person {
  #cookie = '';
  #csrf = '';
  constructor(base, origin) {
    this.base = base.replace(/\/$/, '');
    // The API's Origin/CSRF allow-list is the server's public URL (SR_SERVER__PUBLIC_URL).
    this.origin = origin ?? this.base;
  }

  async #raw(method, path, body, auth = true) {
    const headers = { Origin: this.origin };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (auth) {
      headers.Cookie = this.#cookie;
      if (this.#csrf && method !== 'GET') headers['X-SR-CSRF'] = this.#csrf;
    }
    return fetch(`${this.base}/api/v1${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  }

  async call(method, path, body) {
    const res = await this.#raw(method, path, body);
    const text = await res.text();
    if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${text}`);
    return text ? JSON.parse(text) : undefined;
  }
  get = (path) => this.call('GET', path);
  post = (path, body) => this.call('POST', path, body);
  patch = (path, body) => this.call('PATCH', path, body);

  async login(username, password) {
    const res = await this.#raw('POST', '/sr/auth/login', { login: username, password }, false);
    if (!res.ok) throw new Error(`login -> ${res.status}: ${await res.text()}`);
    const cookies = res.headers.getSetCookie().map((c) => c.split(';')[0]);
    if (!cookies.some((c) => c.startsWith('sr_session='))) {
      throw new Error('login set no session cookie');
    }
    this.#cookie = cookies.join('; ');
    this.#csrf = (await this.get('/sr/csrf')).token;
  }
}

/** Waits for `GET /readyz` for up to `seconds`. */
export async function waitReady(base, seconds = 60) {
  const until = Date.now() + seconds * 1000;
  for (;;) {
    try {
      if ((await fetch(`${base}/readyz`)).ok) return;
    } catch {
      /* not up yet */
    }
    if (Date.now() > until) throw new Error(`${base}/readyz did not become ready in ${seconds}s`);
    await new Promise((r) => setTimeout(r, 1000));
  }
}

/**
 * Makes an application with its bot and installs it in `guildId` the way the consent screen does
 * (POST /oauth2/authorize with guild_id). Call it again for a second bot in the same server.
 * @returns {Promise<{ appId: string, token: string }>}
 */
export async function installBot(person, guildId, name, permissions) {
  const created = await person.post('/applications', { name });
  const appId = created.application.id;
  await person.patch(`/applications/${appId}`, { bot_public: true });
  await person.post('/oauth2/authorize', {
    client_id: appId,
    scope: 'bot applications.commands',
    guild_id: guildId,
    permissions,
    authorize: true,
  });
  return { appId, token: created.bot_token };
}

/**
 * Creates the server, the application with its bot and the install.
 * @returns {Promise<{ person: Person, token: string, appId: string, guildId: string, channelId: string, enableMessageContent: () => Promise<void> }>}
 */
export async function provision({
  base,
  owner,
  password,
  stamp = Date.now().toString(36),
  // VIEW_CHANNEL, SEND_MESSAGES, READ_MESSAGE_HISTORY unless a run needs more (the voice bot adds CONNECT, SPEAK).
  permissions = String((1n << 10n) | (1n << 11n) | (1n << 16n)),
}) {
  const person = new Person(base);
  await person.login(owner, password);

  const guild = await person.post('/guilds', { name: `SDK compat ${stamp}` });
  const channel = guild.channels.find((c) => c.type === 0);
  if (!channel) throw new Error('the new server has no text channel');

  const { appId, token } = await installBot(person, guild.id, `Compat bot ${stamp}`, permissions);

  return {
    person,
    token,
    appId,
    guildId: guild.id,
    channelId: channel.id,
    // A development application may switch the privileged intent on for itself.
    enableMessageContent: () =>
      person.patch(`/applications/${appId}`, { privileged_intents: ['MESSAGE_CONTENT'] }),
  };
}
