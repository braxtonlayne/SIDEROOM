import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { crc32 } from 'node:zlib';
import ts from 'typescript';
import { repoRoot } from './cli';

/** A minimal zip writer (stored entries, no compression): enough for a test Activity bundle. */
export function zip(files: Record<string, string | Uint8Array>): Buffer {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let offset = 0;
  for (const [name, body] of Object.entries(files)) {
    const data = typeof body === 'string' ? Buffer.from(body, 'utf8') : Buffer.from(body);
    const nameBuf = Buffer.from(name, 'utf8');
    const crc = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4); // version needed
    local.writeUInt16LE(0x0800, 6); // UTF-8 names
    local.writeUInt16LE(0, 8); // stored
    local.writeUInt16LE(0, 10);
    local.writeUInt16LE(0x21, 12); // 1980-01-01
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    local.writeUInt16LE(0, 28);
    locals.push(local, nameBuf, data);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(0x031e, 4); // made by: Unix, 3.0
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(0, 10);
    central.writeUInt16LE(0, 12);
    central.writeUInt16LE(0x21, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBuf.length, 28);
    central.writeUInt32LE((0o100644 << 16) >>> 0, 38); // regular file
    central.writeUInt32LE(offset, 42);
    centrals.push(central, nameBuf);
    offset += 30 + nameBuf.length + data.length;
  }
  const dir = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  const count = Object.keys(files).length;
  end.writeUInt16LE(count, 8);
  end.writeUInt16LE(count, 10);
  end.writeUInt32LE(dir.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, dir, end]);
}

/** The Embedded App SDK (packages/embedded-app-sdk) as browser ES modules under `sdk/`. */
function sdkModules(): Record<string, string> {
  const src = resolve(repoRoot, 'packages', 'embedded-app-sdk', 'src');
  const out: Record<string, string> = {};
  for (const name of ['index', 'sdk', 'protocol']) {
    const js = ts.transpileModule(readFileSync(resolve(src, `${name}.ts`), 'utf8'), {
      compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ES2020 },
    }).outputText;
    out[`sdk/${name}.js`] = js.replace(/from '\.\/(\w+)'/g, "from './$1.js'");
  }
  return out;
}

/**
 * A test Activity: the real SDK, a handshake, the channel and participants, a participants subscription
 * and `authorize`. Each step is written into `#log` as `<li data-step=...>` so the test can read it from
 * the frame. Everything is same-origin and script-src 'self' clean (no inline script).
 */
export function testActivityBundle(): Buffer {
  const main = `import { SideRoomSDK } from './sdk/index.js';
const log = (step, value) => {
  const li = document.createElement('li');
  li.dataset.step = step;
  li.textContent = step + ': ' + value;
  document.getElementById('log').append(li);
};
const appId = location.hostname.split('.')[0];
const sdk = new SideRoomSDK(appId, { timeoutMs: 90000 });
try {
  const cfg = await sdk.ready();
  log('ready', cfg.instance_id + ' ' + cfg.channel_id);
  const ch = await sdk.commands.getChannel();
  log('channel', ch.name);
  const p = await sdk.commands.getInstanceConnectedParticipants();
  log('participants', String(p.participants.length));
  sdk.subscribe('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', (d) =>
    log('participants-update', String(d.participants.length)),
  );
  document.getElementById('auth').addEventListener('click', async () => {
    try {
      const r = await sdk.commands.authorize({ scope: ['identify'] });
      log('authorize', r.code ? 'code' : 'none');
    } catch (e) {
      log('authorize-error', e.code || String(e));
    }
  });
  log('subscribed', 'yes');
} catch (e) {
  log('error', e.code || String(e));
}
`;
  const html = `<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>E2E Activity</title><link rel="stylesheet" href="style.css"></head>
<body>
<h1>E2E Activity</h1>
<button id="auth" type="button">Authorize</button>
<ul id="log"></ul>
<script type="module" src="main.js"></script>
</body>
</html>
`;
  return zip({
    'index.html': html,
    'style.css': 'body { font-family: sans-serif; }\n',
    'main.js': main,
    ...sdkModules(),
  });
}
