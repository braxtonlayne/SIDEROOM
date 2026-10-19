// Live smoke test of the admin API against a running sr-server.
// Usage: node e2e/admin-smoke.mjs <cliPath> <ownerPassword>
// Env: BASE (default http://127.0.0.1:8105), ORIGIN (default http://localhost:8105).
// Needs sr-cli on the same config/database as the server (SR_* env vars).
import { execFileSync } from 'node:child_process';

const ROOT = process.env.BASE ?? 'http://127.0.0.1:8105';
const ORIGIN = process.env.ORIGIN ?? 'http://localhost:8105';
const [cli, ownerPw] = process.argv.slice(2);
let failures = 0;
const check = (name, ok, detail) => {
  console.log(
    `${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : '  -> ' + JSON.stringify(detail)?.slice(0, 600)}`,
  );
  if (!ok) failures++;
};

class Client {
  cookie = '';
  csrf = '';
  async call(method, path, body, headers = {}) {
    const h = { Origin: ORIGIN, ...headers };
    if (this.cookie) h.Cookie = this.cookie;
    if (this.csrf && method !== 'GET') h['X-SR-CSRF'] = this.csrf;
    if (body !== undefined) h['Content-Type'] = 'application/json';
    const res = await fetch(`${ROOT}/api/v1${path}`, {
      method,
      headers: h,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    for (const sc of res.headers.getSetCookie?.() ?? []) {
      const [kv] = sc.split(';');
      const k = kv.split('=')[0];
      this.cookie = this.cookie
        .split('; ')
        .filter((c) => c && !c.startsWith(k + '='))
        .concat(kv)
        .join('; ');
    }
    const text = await res.text();
    let json;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = text;
    }
    return { status: res.status, body: json };
  }
  async login(login, password) {
    const r = await this.call('POST', '/sr/auth/login', { login, password });
    this.csrf = (await this.call('GET', '/sr/csrf')).body?.token ?? '';
    return r;
  }
}

const sfx = Math.floor(Math.random() * 1e6);
const pw = 'Smoke-Test-Passw0rd-xyz!';
const sh = (...args) => execFileSync(cli, args, { encoding: 'utf8' });
const inviteFor = (issuer) => sh('invite', 'create', '--issuer', issuer).split('\n')[0].trim();

async function register(name, issuer) {
  const c = new Client();
  const r = await c.call('POST', '/sr/register', {
    code: inviteFor(issuer),
    email: `${name}@example.test`,
    username: name,
    password: pw,
    accepted_house_notes: true,
    confirmed_min_age: 13,
  });
  c.csrf = (await c.call('GET', '/sr/csrf')).body?.token ?? '';
  return { c, id: r.body?.id, r };
}

// ---- Owner ---------------------------------------------------------------
const owner = new Client();
let r = await owner.login('owner', ownerPw);
check('owner logs in', r.status === 200 && r.body?.status === 'ok', r);
r = await owner.call('GET', '/admin/me');
check('owner has OWNER standing', r.status === 200 && r.body.is_owner === true, r);
const ownerId = r.body?.user_id;
r = await owner.call('GET', '/admin/staff');
check(
  'staff list shows the Owner with the Owner role',
  r.status === 200 && r.body[0]?.is_owner && r.body[0].roles.some((x) => x.name === 'Owner'),
  r,
);
r = await owner.call('GET', '/admin/staff/roles');
const roles = Object.fromEntries((r.body ?? []).map((x) => [x.name, x]));
check(
  'default staff roles exist (7)',
  [
    'Support',
    'App Reviewer',
    'Library Curator',
    'Media Curator',
    'Moderator',
    'Senior Staff',
    'Owner',
  ].every((n) => roles[n]),
  r.body?.map((x) => x.name),
);
check('Moderator bits are 7169', roles.Moderator?.platform_permissions === '7169', roles.Moderator);

// ---- users -----------------------------------------------------------------
const modName = `modu${sfx}`,
  victimName = `victim${sfx}`,
  childName = `child${sfx}`;
const mod = await register(modName, 'owner');
check('register moderator-to-be', mod.r.status < 300 && mod.id, mod.r);
const victim = await register(victimName, 'owner');
check('register victim', victim.r.status < 300 && victim.id, victim.r);
const child = await register(childName, victimName);
check('register child invited by victim', child.r.status < 300 && child.id, child.r);

// ---- before the appointment the user is refused ---------------------------
r = await mod.c.call('GET', '/admin/accounts');
check('non-staff gets 403 staff-only', r.status === 403 && r.body?.code === 900002, r);
r = await new Client().call('GET', '/admin/accounts');
check('anonymous gets 401', r.status === 401, r);

// ---- appoint a moderator ----------------------------------------------------
r = await owner.call('POST', '/admin/staff', {
  account_id: mod.id,
  role_ids: [roles.Moderator.id],
  reason: 'smoke: trusted helper',
});
check(
  'owner appoints the moderator',
  r.status === 200 && r.body?.roles?.[0]?.name === 'Moderator',
  r,
);
r = await owner.call('POST', '/admin/staff', {
  account_id: victim.id,
  role_ids: [roles.Owner.id],
  reason: 'x',
});
check('the Owner role cannot be assigned', r.status === 403, r);
r = await owner.call('POST', '/admin/staff', {
  account_id: victim.id,
  role_ids: [roles.Moderator.id],
});
check('appointing without a reason is refused', r.status === 400 && r.body?.code === 900003, r);

// ---- the moderator acts within, and not outside, its permissions ------------
r = await mod.c.call('GET', `/admin/accounts?q=${victimName}`);
check(
  'moderator searches accounts',
  r.status === 200 && r.body.items?.[0]?.username === victimName,
  r,
);
r = await mod.c.call('GET', '/admin/settings');
check('moderator refused settings (50013)', r.status === 403 && r.body?.code === 50013, r);
r = await mod.c.call('GET', '/admin/access-log');
check('moderator refused the access log', r.status === 403 && r.body?.code === 50013, r);
r = await mod.c.call('GET', `/admin/accounts/${child.id}/invite-tree`);
check('moderator refused the invite graph', r.status === 403 && r.body?.code === 50013, r);

// ---- suspend: the victim's sessions die ------------------------------------
r = await victim.c.call('GET', '/users/@me');
check('victim session works', r.status === 200, r);
r = await mod.c.call('POST', `/admin/accounts/${victim.id}/suspend`, {});
check('suspend without a reason is refused', r.status === 400 && r.body?.code === 900003, r);
r = await mod.c.call('POST', `/admin/accounts/${victim.id}/suspend`, { reason: 'smoke: spam' });
check('moderator suspends the victim', r.status === 200 && r.body?.state === 'deactivated', r);
r = await victim.c.call('GET', '/users/@me');
check("the victim's session is dead (401)", r.status === 401, r);
r = await new Client().login(victimName, pw);
check('the suspended victim cannot log in', r.status >= 400, r);
r = await mod.c.call(
  'POST',
  `/admin/accounts/${victim.id}/unsuspend`,
  {},
  { 'X-Audit-Log-Reason': 'smoke: reinstated (header reason)' },
);
check('unsuspend with the reason header', r.status === 200 && r.body?.state === 'active', r);
await victim.c.login(victimName, pw);
r = await victim.c.call('POST', '/sr/auth/logout-all', {});
r = await mod.c.call('POST', `/admin/accounts/${victim.id}/force-logout`, {
  reason: 'smoke: force logout',
});
check('force logout', r.status === 200 && typeof r.body?.sessions_revoked === 'number', r);

// ---- invite graph -----------------------------------------------------------
r = await owner.call('GET', `/admin/accounts/${child.id}/invite-tree`);
const chain = r.body?.ancestors?.map((n) => n.username);
check(
  'inviter tree: owner -> victim',
  JSON.stringify(chain) === JSON.stringify(['owner', victimName]),
  r.body,
);
r = await owner.call('GET', `/admin/accounts/${ownerId}/invite-descendants?depth=3`);
check(
  'descendants include victim and child',
  r.status === 200 &&
    [victimName, childName].every((n) => r.body.nodes.some((x) => x.username === n)),
  r.body,
);
r = await owner.call('POST', `/admin/accounts/${victim.id}/credits`, {
  delta: 2,
  reason: 'smoke: goodwill',
});
check('grant credits', r.status === 200 && r.body.balance >= 2, r);
r = await owner.call('GET', `/admin/accounts/${victim.id}/credits`);
check('ledger shows the staff grant', r.body?.entries?.[0]?.reason === 'staff_grant', r.body);

// ---- serious ban propagates to the inviter -----------------------------------
r = await mod.c.call('POST', `/admin/accounts/${child.id}/ban`, {
  category: 'serious',
  reason: 'smoke: fraud',
});
check('serious ban', r.status === 200 && r.body?.inviter_suspicion?.level === 1, r);
r = await owner.call('GET', '/admin/suspicion');
check(
  'inviter flagged',
  r.body?.some?.((p) => p.accounts.some((a) => a.username === victimName)),
  r.body,
);
r = await new Client().login(childName, pw);
check('banned child cannot log in', r.status >= 400, r);

// ---- limits: the account cap applies without a restart -----------------------
r = await owner.call('PATCH', '/admin/settings', {
  values: { 'accounts.cap': 1 },
  reason: 'smoke: close the door',
});
check('lower the account cap', r.status === 200, r);
const probe = inviteFor('owner');
r = await new Client().call('GET', `/sr/invites/${probe}`);
check(
  'invite check says closed at the cap',
  r.body?.valid === false && r.body?.reason === 'closed',
  r,
);
r = await owner.call('PATCH', '/admin/settings', {
  values: { 'accounts.cap': 20000 },
  reason: 'smoke: reopen',
});
r = await new Client().call('GET', `/sr/invites/${probe}`);
check('and reopens', r.body?.valid === true, r);
r = await owner.call('PATCH', '/admin/settings', { values: { 'accounts.cap': 0 } });
check('invalid limits are rejected', r.status === 400, r);

// ---- features / system / jobs -------------------------------------------------
r = await owner.call('GET', '/admin/features');
check('features list (registry may be empty)', r.status === 200 && Array.isArray(r.body), r);
r = await owner.call('GET', '/admin/system');
check(
  'system status',
  r.status === 200 &&
    r.body.probes?.length >= 1 &&
    r.body.probes.every((p) => typeof p.ok === 'boolean'),
  r.body,
);
console.log(
  '     probes:',
  JSON.stringify(r.body?.probes),
  'gateway_sessions:',
  r.body?.gateway_sessions,
  'counts:',
  JSON.stringify(r.body?.counts),
);
r = await owner.call('GET', '/admin/jobs?state=dead');
check('dead jobs list', r.status === 200 && Array.isArray(r.body?.items), r);

// ---- access log ----------------------------------------------------------------
r = await owner.call('GET', '/admin/access-log?limit=100');
const actions = new Set(r.body?.items?.map((x) => x.action));
check(
  'log has appoint, suspend, ban, denied rows',
  ['staff.appoint', 'account.suspend', 'account.unsuspend', 'person.ban', 'settings.patch'].every(
    (a) => actions.has(a),
  ) && r.body.items.some((x) => x.kind === 'denied'),
  [...actions],
);
const susp = r.body?.items?.find((x) => x.action === 'account.suspend');
check(
  'suspension row has actor, reason, target, ip',
  susp?.reason === 'smoke: spam' &&
    susp.actor_id === mod.id &&
    susp.target_id === victim.id &&
    !!susp.ip,
  susp,
);
r = await owner.call('GET', '/admin/access-log/verify');
check('hash chain verifies', r.status === 200 && r.body?.ok === true, r.body);
console.log('     chain:', r.body?.rows_checked, 'rows, head', r.body?.head_hash?.slice(0, 16));
try {
  const out = sh('verify-access-log');
  check('sr-cli verify-access-log', /OK/.test(out), out);
} catch (e) {
  check('sr-cli verify-access-log', false, String(e));
}

// ---- revoke: access ends on the next request -----------------------------------
r = await owner.call('DELETE', `/admin/staff/${mod.id}`, { reason: 'smoke: done' });
check('owner revokes the moderator', r.status === 204, r);
r = await mod.c.call('GET', '/admin/accounts');
check('revoked moderator is refused at once', r.status === 403 && r.body?.code === 900002, r);
r = await owner.call('DELETE', `/admin/staff/${ownerId}`, { reason: 'x' });
check('the Owner cannot be revoked', r.status === 403, r);

console.log(failures ? `\n${failures} FAILED` : '\nALL PASSED');
process.exit(failures ? 1 : 0);
