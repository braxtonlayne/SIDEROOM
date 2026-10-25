// Static documentation for the developer hub. Source of truth for behaviour is docs/05-api-and-gateway.md and
// docs/11-extensibility-and-developer-platform.md; keep these short and link out to the generated OpenAPI
// reference at /api/v1/openapi.json. `{origin}` is replaced with the site origin when rendered.

import { PACKAGE_PAGES } from './docsContent.packages';
import { SDK_PAGES } from './docsContent.sdks';

export type Block =
  | { t: 'p'; text: string }
  | { t: 'h'; text: string }
  | { t: 'ul'; items: string[] }
  | { t: 'code'; lang: string; title?: string; code: string }
  | { t: 'note'; title: string; text: string }
  /** A section generated at run time (the OpenAPI list) or build time (the gateway tables). */
  | { t: 'ref'; what: 'openapi' | 'gateway' };

export interface DocPage {
  id: string;
  title: string;
  summary: string;
  blocks: Block[];
}

export const DOC_PAGES: DocPage[] = [
  {
    id: 'getting-started',
    title: 'Getting started',
    summary: 'Create an application, add a bot to a test server and make your first call.',
    blocks: [
      {
        t: 'p',
        text: 'SIDE ROOM has a Discord-shaped API (version 10 style), so a bot written with discord.js, discord.py, serenity or twilight ports by changing the base URLs and the token.',
      },
      { t: 'h', text: '1. Make an application' },
      {
        t: 'ul',
        items: [
          'Open Applications and choose New application. Give it a name.',
          'Copy the client secret and bot token when they are shown: you see them once. Every application comes with its own bot user.',
          'In the Installation tab, build an invite link and open it to add the bot to a server you manage. Use it as the owner: until the app is released only you and your accepted testers can add it.',
        ],
      },
      {
        t: 'note',
        title: 'Test servers',
        text: 'Until an app is released it only installs into servers you own or manage that have 25 members or fewer. Anywhere else the install fails with error 900120 (app not released). Submit a version in the Review tab to release it.',
      },
      { t: 'h', text: '2. Make your first request' },
      {
        t: 'code',
        lang: 'curl',
        title: 'curl',
        code: 'curl -H "Authorization: Bot $BOT_TOKEN" \\\n  {origin}/api/v1/users/@me',
      },
      {
        t: 'code',
        lang: 'typescript',
        title: 'TypeScript',
        code: "const res = await fetch('{origin}/api/v1/users/@me', {\n  headers: { Authorization: `Bot ${process.env.BOT_TOKEN}` },\n});\nif (!res.ok) throw new Error(`HTTP ${res.status}`);\nconsole.log(await res.json());",
      },
      { t: 'h', text: 'Base URLs' },
      {
        t: 'ul',
        items: [
          'REST: {origin}/api/v1 (and /api/v10, an alias for libraries that hardcode it)',
          'Gateway: wss://<host>/gateway?v=1&encoding=json',
          'OpenAPI reference: {origin}/api/v1/openapi.json',
        ],
      },
    ],
  },
  {
    id: 'authentication',
    title: 'Authentication',
    summary: 'Bot tokens, OAuth2 bearer tokens and the authorization code flow.',
    blocks: [
      {
        t: 'ul',
        items: [
          'Bots send Authorization: Bot <token>.',
          'Apps acting for a person send Authorization: Bearer <access token> from OAuth2.',
          'The web app uses a session cookie plus a CSRF header. Other clients cannot reuse it.',
        ],
      },
      { t: 'h', text: 'Authorization code flow' },
      {
        t: 'p',
        text: 'Send the person to the authorize URL. The URL generator in your application builds it. After they approve we redirect to one of your registered redirect URIs with a code and your state.',
      },
      {
        t: 'code',
        lang: 'text',
        title: 'Authorize URL',
        code: '{origin}/oauth2/authorize?client_id=APP_ID&response_type=code&scope=identify%20guilds&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback&state=RANDOM',
      },
      {
        t: 'code',
        lang: 'curl',
        title: 'Exchange the code',
        code: 'curl -X POST {origin}/api/v1/oauth2/token \\\n  -d grant_type=authorization_code \\\n  -d code=$CODE \\\n  -d redirect_uri=https://example.com/callback \\\n  -u "$CLIENT_ID:$CLIENT_SECRET"',
      },
      {
        t: 'note',
        title: 'Keep secrets off the client',
        text: 'The client secret and bot token belong on your server. Never ship them in a web page or mobile app, and reset them from the portal if they leak.',
      },
      { t: 'h', text: 'Rules' },
      {
        t: 'ul',
        items: [
          'Only response_type=code. There is no implicit grant and no client_credentials.',
          'Redirect URIs must be https (http only on localhost), registered exactly, with no fragment.',
          'PKCE is supported with S256; if you send a code_challenge you must send the code_verifier.',
          'Access tokens last 7 days, refresh tokens 90 days, and refresh tokens rotate on use.',
        ],
      },
      { t: 'h', text: 'Scopes' },
      {
        t: 'ul',
        items: [
          'identify, email: the account basics (email needs its own scope)',
          'guilds: the servers the person is in',
          'bot: add your bot to a server (shows the permissions you ask for)',
          'applications.commands: install your slash commands in a server',
        ],
      },
    ],
  },
  {
    id: 'gateway',
    title: 'Gateway',
    summary: 'Connect over WebSocket, identify with intents and keep the heartbeat.',
    blocks: [
      {
        t: 'p',
        text: 'The gateway uses Discord opcodes: HELLO, then IDENTIFY with your token and intents, then READY and a GUILD_CREATE per server. Send a heartbeat at the interval HELLO gives you.',
      },
      {
        t: 'code',
        lang: 'typescript',
        title: 'Identify',
        code: "const ws = new WebSocket('wss://<host>/gateway?v=1&encoding=json');\nws.onmessage = (e) => {\n  const msg = JSON.parse(e.data);\n  if (msg.op === 10) {\n    setInterval(() => ws.send(JSON.stringify({ op: 1, d: null })), msg.d.heartbeat_interval);\n    ws.send(\n      JSON.stringify({\n        op: 2,\n        d: { token: process.env.BOT_TOKEN, intents: 1 << 0, properties: {} },\n      }),\n    );\n  }\n};",
      },
      { t: 'h', text: 'Intents' },
      {
        t: 'p',
        text: 'Intent bits match Discord. Server members, presence and message content are privileged: switch them on in the Bot tab. While the app is in development they work; once it is released only intents staff approved are allowed, and asking for another closes the connection with code 4014. Unknown intent bits close with 4013.',
      },
      {
        t: 'p',
        text: 'Sharding works as on Discord: send shard: [id, count] in IDENTIFY (leave it out for a single shard; an id at or above the count closes with 4010). GET /gateway/bot reports one shard per 1000 servers the application is installed in. If the connection drops, send RESUME with your session id and the last sequence number you saw; the server replays what you missed and answers RESUMED, or INVALID_SESSION when it cannot (identify again). SIDE ROOM adds events prefixed SR_ (for example SR_LIBRARY_UPDATE); ignore events you do not know.',
      },
    ],
  },
  {
    id: 'rest',
    title: 'REST',
    summary: 'Conventions shared by every endpoint: ids, errors, pagination.',
    blocks: [
      {
        t: 'ul',
        items: [
          'Ids are snowflakes sent as strings; times are ISO 8601.',
          'Errors look like { "code": 50013, "message": "Missing Permissions", "errors": { ... } }. Codes reuse Discord codes; SIDE ROOM specific ones start at 900000.',
          'Lists paginate with before, after, around and limit.',
          'Send X-Audit-Log-Reason on moderation calls and it appears in the audit log.',
          'Permissions are decimal bitfields in strings, with the same bit numbers as Discord.',
        ],
      },
      {
        t: 'code',
        lang: 'curl',
        title: 'Send a message',
        code: 'curl -X POST {origin}/api/v1/channels/$CHANNEL_ID/messages \\\n  -H "Authorization: Bot $BOT_TOKEN" \\\n  -H "Content-Type: application/json" \\\n  -d \'{"content":"Hello from my bot"}\'',
      },
      {
        t: 'p',
        text: 'The full list of routes, generated from the server, is at {origin}/api/v1/openapi.json.',
      },
    ],
  },
  {
    id: 'rate-limits',
    title: 'Rate limits and status',
    summary: 'How limits are reported and how to back off.',
    blocks: [
      {
        t: 'p',
        text: 'Every response carries X-RateLimit-Limit, -Remaining, -Reset, -Reset-After, -Bucket and -Scope. A 429 response has a retry_after number (seconds) in its body and a Retry-After header.',
      },
      {
        t: 'ul',
        items: [
          'Each bot token gets 50 requests per second across all routes; a global 429 carries X-RateLimit-Global: true.',
          'Other limits are per route bucket. Routes in the same bucket share the same budget.',
          'Some routes are stricter: token exchange 30 a minute, authorize 20 a minute, creating applications 5 an hour, secret and token resets 5 an hour per application.',
          'Wait for retry_after, then retry once. Do not hammer the route.',
          'Repeated 429s on authentication routes grow the wait (30 seconds up to 15 minutes).',
          'Sharded bots should spread REST calls instead of bursting after a reconnect.',
        ],
      },
      {
        t: 'code',
        lang: 'typescript',
        title: 'Respect retry_after',
        code: 'async function call(url: string, init: RequestInit, tries = 3): Promise<Response> {\n  const res = await fetch(url, init);\n  if (res.status !== 429 || tries <= 1) return res;\n  const { retry_after } = await res.json();\n  await new Promise((r) => setTimeout(r, Math.ceil(retry_after * 1000)));\n  return call(url, init, tries - 1);\n}',
      },
      {
        t: 'note',
        title: 'Service status',
        text: 'There is no public status page or status API yet. If calls fail with 5xx errors, back off and retry; ask the server owner if it persists.',
      },
    ],
  },
  {
    id: 'interactions',
    title: 'Interactions',
    summary: 'Slash commands, components and modals, over the gateway or HTTP.',
    blocks: [
      {
        t: 'p',
        text: 'Register commands over REST with your bot token; the Commands tab builds and checks the request. When someone uses one, your bot receives INTERACTION_CREATE on the gateway. Or set an interactions endpoint URL in the General tab to receive signed HTTP requests instead.',
      },
      {
        t: 'ul',
        items: [
          'Answer within 3 seconds with POST /interactions/{id}/{token}/callback: respond, or defer and follow up with the interaction token for up to 15 minutes.',
          'Callback types 4 to 9 are supported (message, deferred, update, autocomplete, modal).',
          'Ephemeral replies (flag 64) are only shown to the person who used the command.',
          'A non-ephemeral reply is a normal message from the bot, so the bot needs Send Messages in the channel.',
          'Global commands take a short while to appear; server commands are immediate.',
        ],
      },
      {
        t: 'code',
        lang: 'curl',
        title: 'Register a global command',
        code: 'curl -X POST {origin}/api/v1/applications/$APP_ID/commands \\\n  -H "Authorization: Bot $BOT_TOKEN" \\\n  -H "Content-Type: application/json" \\\n  -d \'{"name":"ping","description":"Reply with pong","type":1}\'',
      },
    ],
  },
];

DOC_PAGES.push(...SDK_PAGES, ...PACKAGE_PAGES);

export const DOC_BY_ID = new Map(DOC_PAGES.map((p) => [p.id, p]));
