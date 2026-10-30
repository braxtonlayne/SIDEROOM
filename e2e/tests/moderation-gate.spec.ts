import type { APIRequestContext } from '@playwright/test';
import { expect, loginApi, registerUser, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import { nextWindowCode, secretFromOtpauth, totp } from '../support/totp';

/**
 * RDY-MOD-001, the Phase 7 exit gate (doc 13): the moderation and compliance flows, one after the other, on a real
 * server, with the audit trail read at the end.
 *
 *  1. Member A reports member B's message. Staff open the case, claim it, hold B's messages, ban B from the case
 *     and decide it.
 *  2. B appeals; a second reviewer (the Owner, because the staff member who banned may not decide the appeal)
 *     overturns it.
 *  3. A takedown notice is registered against a Library chapter, upheld (the chapter leaves the reader for a
 *     member), a counter-notice is filed, and staff restore the chapter (it is back).
 *  3b. (BYOS-LEG-001) A takedown notice against one object in B's own bucket delists it (the bucket stays), staff
 *     delist and relist the whole bucket, and a counter-notice restores the object.
 *  4. A second account of B is flagged as a likely duplicate; staff review the signal and fold it into the Person.
 *  5. A data-subject export is run for B and downloaded; an erasure runs with the deletion of B's messages and the
 *     content is gone for A.
 *  3c. (PRF-RNG-001) A status ring of B for the server is reported: held, 404 for a stranger, the viewer list
 *     is B's alone, and the case keeps its evidence after B deletes the ring.
 *  6. The Owner reads the access log and finds the row of every step above.
 *
 * Staff are a real staff member (Senior Staff, appointed with `sr-cli`) with a second factor: ban, upheld takedown
 * and data-request runs demand a fresh one (RDY-SEC-010), so the gate runs with the defaults of the `playwright` job.
 * The window-based deadlines (a data request due in 30 days, an extension, the 30-day purge) are a Postgres test:
 * `crates/sr-moderation/tests/deadline_windows.rs`. Steps that need a lawyer (N-35, D-336 h) are outside the gate.
 */

const API = '/api/v1';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

/** One JSON call with the cookie session and the CSRF header. */
async function call<T = any>(
  request: APIRequestContext,
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  data?: unknown,
  ok: number[] = [200, 201, 202, 204],
): Promise<T> {
  const headers: Record<string, string> = { Origin: ORIGIN };
  if (method !== 'GET') {
    const csrf = await request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
    headers['X-SR-CSRF'] = ((await csrf.json()) as { token: string }).token;
  }
  const res = await request.fetch(`${API}${path}`, {
    method,
    headers,
    ...(data !== undefined ? { data } : {}),
  });
  if (!ok.includes(res.status())) {
    throw new Error(`${method} ${path} -> ${res.status()}: ${await res.text()}`);
  }
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

test.describe('@gate Phase 7 moderation exit gate', () => {
  test('report to case to ban to appeal, takedown and counter-notice, duplicate account, data request, audit trail', async ({
    playwright,
    browser,
    owner,
    signedIn,
  }) => {
    test.setTimeout(600_000);
    const anon = await playwright.request.newContext({ baseURL: ORIGIN });
    const stamp = Date.now().toString(36);
    const startUs = Date.now() * 1000 - 5_000_000;

    // ---- people: A (reporter, server owner), B (reported), B2 (B's second account), S (staff with 2FA)
    const userA = await registerUser(anon, 'repa');
    const userB = await registerUser(anon, 'repb');
    const userS = await registerUser(anon, 'staff');
    const A = await signedIn(userA);
    const B = await signedIn(userB);
    const S = await signedIn(userS);
    const O = await signedIn(owner);
    const ownerId = (await call<{ id: string }>(O.context.request, 'GET', '/users/@me')).id;
    const a = A.context.request;
    const b = B.context.request;
    const s = S.context.request;
    const o = O.context.request;

    // The Owner is the other reviewer; S is appointed through the host command (no second factor needed for it).
    runCli([
      'staff',
      'appoint',
      userS.username,
      '--role',
      'Senior Staff',
      '--reason',
      'e2e moderation gate',
    ]);
    // S enrols TOTP; the second-factor window then opens with the next code (a code is spent once).
    const enroll = await call<{ otpauth_uri: string }>(s, 'POST', '/sr/auth/totp/enroll');
    const secret = secretFromOtpauth(enroll.otpauth_uri);
    await call(s, 'POST', '/sr/auth/totp/verify', { code: totp(secret) });
    const reauth = async () => {
      const st = await call<{ fresh: boolean }>(s, 'GET', '/admin/reauth');
      if (st.fresh) return;
      await call(s, 'POST', '/admin/reauth', {
        method: 'totp',
        code: await nextWindowCode(secret),
      });
    };
    await reauth();

    // ---- a server with A and B, one message of B reported, one more of B to hold
    const guild = await call<{ id: string; channels: { id: string; type: number }[] }>(
      a,
      'POST',
      '/guilds',
      { name: `Gate ${stamp}` },
    );
    const channelId = guild.channels.find((c) => c.type === 0)!.id;
    const invite = await call<{ code: string }>(a, 'POST', `/channels/${channelId}/invites`, {});
    await call(b, 'POST', `/invites/${invite.code}`);
    const reported = await call<{ id: string }>(b, 'POST', `/channels/${channelId}/messages`, {
      content: `reported line ${stamp}`,
    });
    const other = await call<{ id: string }>(b, 'POST', `/channels/${channelId}/messages`, {
      content: `second line ${stamp}`,
    });

    let caseId = '';
    await test.step('A reports the message; staff open, claim and work the case', async () => {
      await call(o, 'PATCH', '/admin/features/moderation.reports', {
        enabled: true,
        reason: 'e2e moderation gate',
      });
      const receipt = await call<{ id: string }>(a, 'POST', '/moderation/reports', {
        target_kind: 'message',
        target_id: reported.id,
        channel_id: channelId,
        guild_id: guild.id,
        category: 'harassment',
        note: 'This message is aimed at me.',
        moderators_may_see_name: false,
      });
      expect(receipt.id).toBeTruthy();
      // Harassment in a server goes to that server's moderators first (D-209): A, who owns it, opens the case,
      // claims it and escalates it; platform staff then take it over.
      const mine = await call<{ items: { id: string; target_id: string | null }[] }>(
        a,
        'GET',
        `/moderation/guilds/${guild.id}/cases?limit=50`,
      );
      const opened = mine.items.find((c) => c.target_id === reported.id);
      expect(opened, 'the report opened a case for the server moderators').toBeTruthy();
      caseId = opened!.id;
      await call(a, 'PUT', `/moderation/cases/${caseId}/assignee`);
      await call(a, 'POST', `/moderation/cases/${caseId}/escalate`, {
        reason: 'A ban is needed: beyond what a server can do.',
      });
      const theirs = await call<{ items: { id: string }[] }>(
        s,
        'GET',
        '/moderation/cases?open_only=true&limit=100',
      );
      expect(
        theirs.items.some((c) => c.id === caseId),
        'platform staff now see the case',
      ).toBe(true);
      await call(s, 'PUT', `/moderation/cases/${caseId}/assignee`);
      await call(s, 'POST', `/moderation/cases/${caseId}/state`, { to: 'investigating' });
      await call(s, 'POST', `/moderation/cases/${caseId}/notes`, {
        body: 'Read the thread; B has done this before.',
      });
    });

    await test.step('PRF-RNG-001: a reported status ring is held, only its audience could see it, and the case outlives the ring', async () => {
      const ring = await call<{ id: string }>(b, 'POST', '/users/@me/rings', {
        kind: 'text',
        text: `ring line ${stamp}`,
        audience: 'guilds',
        guild_ids: [guild.id],
      });
      // A shares the chosen server; S does not: S gets the same 404 as for a ring that does not exist.
      await call(a, 'GET', `/rings/${ring.id}`);
      const hidden = await s.fetch(`${API}/rings/${ring.id}`);
      const missing = await s.fetch(`${API}/rings/1`);
      expect(hidden.status()).toBe(404);
      expect(await hidden.json()).toEqual(await missing.json());
      // The viewer list is B's alone.
      await call(a, 'POST', `/rings/${ring.id}/view`);
      expect((await a.fetch(`${API}/users/@me/rings/${ring.id}/viewers`)).status()).toBe(404);
      expect(await call<unknown[]>(b, 'GET', `/users/@me/rings/${ring.id}/viewers`)).toHaveLength(
        1,
      );

      await call(a, 'POST', '/moderation/reports', {
        target_kind: 'status_ring',
        target_id: ring.id,
        category: 'harassment',
        note: 'This ring is aimed at me.',
        moderators_may_see_name: false,
      });
      const open = await call<{ items: { id: string; target_id: string | null }[] }>(
        s,
        'GET',
        '/moderation/cases?open_only=true&limit=100',
      );
      const ringCase = open.items.find((c) => c.target_id === ring.id);
      expect(ringCase, 'platform staff see the ring case').toBeTruthy();
      const detail = await call<{
        evidence: { kind: string; payload: { kind?: string; text?: string } }[];
        holds: { scope_kind: string | null }[];
      }>(s, 'GET', `/moderation/cases/${ringCase!.id}`);
      expect(detail.holds.map((h) => h.scope_kind)).toContain('status_ring');
      expect(detail.evidence[0]?.payload.kind).toBe('status_ring');

      // B deletes the ring: it is gone for A, and the case still has the evidence copy.
      await call(b, 'DELETE', `/users/@me/rings/${ring.id}`);
      expect((await a.fetch(`${API}/rings/${ring.id}`)).status()).toBe(404);
      const after = await call<{ evidence: { payload: { text?: string } }[] }>(
        s,
        'GET',
        `/moderation/cases/${ringCase!.id}`,
      );
      expect(after.evidence[0]?.payload.text).toBe(`ring line ${stamp}`);
    });

    await test.step('staff hold B’s messages and ban B from the case', async () => {
      const hold = await call<{ hold_id: string; messages: number }>(
        s,
        'POST',
        `/moderation/cases/${caseId}/holds`,
        {
          kind: 'user_messages',
          id: userB.id,
          reason: 'evidence for the case',
          since_ms: Date.now() - 3_600_000,
          until_ms: Date.now() + 3_600_000,
        },
      );
      expect(hold.hold_id).toBeTruthy();
      await reauth();
      await call(s, 'POST', `/moderation/cases/${caseId}/ban`, {
        reason_category: 'harassment',
        severity: 'standard',
        note: 'Harassment, second time.',
      });
      const decided = await call<{ case: { state: string }; holds: unknown[] }>(
        s,
        'GET',
        `/moderation/cases/${caseId}`,
      );
      expect(decided.case.state, 'the ban moved the case to actioned').toBe('actioned');
      expect(decided.holds.length, 'the case holds the evidence').toBeGreaterThan(0);
      const login = await anon.post(`${API}/sr/auth/login`, {
        headers: { Origin: ORIGIN },
        data: { login: userB.username, password: userB.password },
      });
      expect(login.ok() && ((await login.json()) as { status: string }).status === 'ok').toBe(
        false,
      );
    });

    // ---- B opens a second account after the ban (a likely duplicate)
    const userB2 = await registerUser(anon, 'repb2');

    // ---- the scan of the new account, while B is banned, names B; staff review the signal
    let signalId = '';
    let bPerson = '';
    let b2Person = '';
    await test.step('the second account is flagged as a likely duplicate and the signal is confirmed', async () => {
      const acct = (
        await call<{ account: { person_id: string } }>(o, 'GET', `/admin/accounts/${userB.id}`)
      ).account;
      const acct2 = (
        await call<{ account: { person_id: string } }>(o, 'GET', `/admin/accounts/${userB2.id}`)
      ).account;
      bPerson = acct.person_id;
      b2Person = acct2.person_id;
      expect(b2Person).not.toBe(bPerson);
      const scan = await call<{ raised: number }>(
        s,
        'POST',
        `/moderation/persons/${b2Person}/evasion-scan`,
      );
      expect(scan.raised).toBeGreaterThan(0);
      const signals = await call<
        { id: string; state: string; kind: string; person_id: string; related_person_id: string }[]
      >(s, 'GET', '/moderation/evasion-signals?state=open&limit=100');
      // The scan of the new account names every banned Person it resembles; this one is B's.
      const signal = signals.find(
        (x) =>
          x.person_id === b2Person &&
          x.related_person_id === bPerson &&
          x.kind === 'new_account_after_ban',
      );
      expect(signal, 'a new-account-after-ban signal names B').toBeTruthy();
      signalId = signal!.id;
      await call(s, 'POST', `/moderation/evasion-signals/${signalId}/review`, {
        decision: 'confirmed',
        reason: 'Same person: same address, registered right after the ban.',
      });
    });

    let appealId = '';
    await test.step('B appeals; the other reviewer overturns the ban', async () => {
      // B signs in and gets the ban notice with the appeal form (the account has no session to appeal from).
      const ctx = await browser.newContext({ baseURL: ORIGIN });
      try {
        const page = await ctx.newPage();
        await page.goto('/login');
        await page.getByLabel('Username or email').fill(userB.username);
        await page.getByLabel('Password', { exact: true }).fill(userB.password);
        await page.getByRole('button', { name: 'Step inside' }).click();
        await expect(page.getByText('This account is banned from SIDE ROOM.')).toBeVisible();
        await page
          .getByLabel('Why should this be looked at again?')
          .fill('I was wrong to say it, and I will not do it again.');
        await page.getByRole('button', { name: 'Send appeal' }).click();
        await expect(page.getByText(/Your appeal was sent/)).toBeVisible();
      } finally {
        await ctx.close();
      }
      const appeals = await call<{ id: string; state: string }[]>(
        o,
        'GET',
        '/moderation/appeals?open_only=true',
      );
      expect(appeals.length).toBeGreaterThan(0);
      appealId = appeals[0]!.id;
      await call(o, 'POST', `/moderation/appeals/${appealId}/decision`, {
        decision: 'overturned',
        reason: 'First offence on record; a warning is enough.',
      });
      const closed = await call<{ case: { state: string }; appeal: { state: string } | null }>(
        s,
        'GET',
        `/moderation/cases/${caseId}`,
      );
      expect(closed.appeal?.state).toBe('overturned');
      expect(closed.case.state, 'deciding the appeal closes the case').toBe('closed');
    });

    await test.step('a takedown against a Library chapter, a counter-notice, the chapter returns', async () => {
      const series = await call<{ id: string }>(o, 'POST', '/admin/library/series', {
        kind: 'manga',
        title: `Gate series ${stamp}`,
        synopsis: 'A series for the moderation gate.',
        genres: ['Drama'],
      });
      const edition = await call<{ id: string }>(
        o,
        'POST',
        `/admin/library/series/${series.id}/editions`,
        { language: 'en', label: 'Official', is_default: true },
      );
      const chapter = await call<{ id: string }>(
        o,
        'POST',
        `/admin/library/editions/${edition.id}/chapters`,
        { number: 1, title: 'One', format: 'images' },
      );
      await call(o, 'PUT', `/admin/library/chapters/${chapter.id}/pages`, [
        {
          key: `library/${series.id}/${chapter.id}/001.webp`,
          width: 160,
          height: 240,
          variants: {},
        },
      ]);
      await call(o, 'POST', `/admin/library/chapters/${chapter.id}/publish`);
      await call(o, 'POST', `/admin/library/series/${series.id}/publish`);
      const pages = () => a.get(`${API}/library/chapters/${chapter.id}/pages`);
      expect((await pages()).status(), 'a member can open the chapter').toBe(200);

      const notice = await call<{ id: string }>(s, 'POST', '/moderation/takedowns', {
        kind: 'copyright',
        notifier_name: 'Rights Holder Ltd',
        notifier_contact: 'rights@example.test',
        legal_basis: 'I own the work and have not licensed it.',
        locations: [`/library/chapters/${chapter.id}`],
        original_text: 'Please remove chapter 1.',
      });
      await call(s, 'POST', `/moderation/takedowns/${notice.id}/items`, {
        kind: 'library_chapter',
        id: chapter.id,
      });
      await reauth();
      await call(s, 'POST', `/moderation/takedowns/${notice.id}/uphold`, {
        reason: 'The notice is complete and the work is ours to remove.',
      });
      expect((await pages()).status(), 'the chapter left the reader').not.toBe(200);

      await call(s, 'POST', `/moderation/takedowns/${notice.id}/counter-notice`, {
        text: 'The uploader says they hold the licence.',
      });
      await call(s, 'POST', `/moderation/takedowns/${notice.id}/restore`, {
        reason: 'Counter-notice received and the notifier did not go to court.',
      });
      expect((await pages()).status(), 'the chapter is back').toBe(200);
    });

    await test.step('a takedown against one object of B’s own bucket delists it; staff delist and relist the whole bucket', async () => {
      // A bucket is only a row of ours plus sealed credentials: creating it connects to nothing.
      await loginApi(b, userB); // the ban ended B's sessions; the overturn does not restore them
      const mount = await call<{ id: string; status: string }>(
        b,
        'POST',
        '/users/@me/storage/mounts',
        {
          label: `Gate shelf ${stamp}`,
          provider: 's3',
          region: 'us-east-1',
          bucket: `gate-${stamp}`,
          path_style: true,
          access_key_id: 'AKIAGATEEXAMPLE',
          secret_access_key: 'gate-example-secret',
        },
      );
      expect(mount.status).toBe('active');
      const notice = await call<{ id: string }>(s, 'POST', '/moderation/takedowns', {
        kind: 'copyright',
        notifier_name: 'Rights Holder Ltd',
        notifier_contact: 'rights@example.test',
        legal_basis: 'I own the film and have not licensed it.',
        locations: [],
        original_text: 'Please stop showing films/leak.mkv.',
      });
      await call(s, 'POST', `/moderation/takedowns/${notice.id}/items`, {
        kind: 'storage_object',
        id: mount.id,
        key: 'films/leak.mkv',
      });
      await reauth();
      await call(s, 'POST', `/moderation/takedowns/${notice.id}/uphold`, {
        reason: 'The notice is complete; the object is delisted, not deleted.',
      });
      const listed = async () =>
        (
          await call<{ id: string; status: string; delisted_objects: number }[]>(
            s,
            'GET',
            '/admin/storage/mounts?limit=500',
          )
        ).find((m) => m.id === mount.id);
      expect((await listed())?.delisted_objects, 'the object is delisted').toBe(1);
      expect((await listed())?.status, 'the bucket itself is untouched').toBe('active');

      await reauth();
      await call(s, 'POST', `/admin/storage/mounts/${mount.id}/delist`, {
        reason: 'The whole bucket is the subject of the notice.',
      });
      expect((await listed())?.status).toBe('delisted');
      const mine = await call<{ id: string; status: string }[]>(
        b,
        'GET',
        '/users/@me/storage/mounts',
      );
      expect(mine.find((m) => m.id === mount.id)?.status, 'the owner sees it delisted').toBe(
        'delisted',
      );
      await call(s, 'POST', `/admin/storage/mounts/${mount.id}/relist`, {
        reason: 'Counter-notice received.',
      });
      expect((await listed())?.status).toBe('active');

      await call(s, 'POST', `/moderation/takedowns/${notice.id}/counter-notice`, {
        text: 'The member says they hold the licence.',
      });
      await call(s, 'POST', `/moderation/takedowns/${notice.id}/restore`, {
        reason: 'Counter-notice received and the notifier did not go to court.',
      });
      expect((await listed())?.delisted_objects, 'the object is listed again').toBe(0);
    });

    await test.step('the duplicate account is folded into B’s Person (nobody is banned any more)', async () => {
      await call(o, 'POST', `/admin/persons/${bPerson}/link`, {
        account_id: userB2.id,
        reason: 'Confirmed duplicate account.',
      });
      const folded = await call<{ account: { person_id: string } }>(
        o,
        'GET',
        `/admin/accounts/${userB2.id}`,
      );
      expect(folded.account.person_id, 'the second account now sits under B’s Person').toBe(
        bPerson,
      );
      const after = await call<{
        signal: { state: string; folded_from_person_id?: string | null };
      }>(s, 'GET', `/moderation/evasion-signals/${signalId}`);
      expect(after.signal.state).toBe('confirmed');
      expect(after.signal.folded_from_person_id, 'the link folded the signal').toBe(b2Person);
    });

    await test.step('B’s data is exported and downloaded, then erased with the messages', async () => {
      await loginApi(b, userB); // the ban ended B's sessions
      const req = await call<{ id: string }>(s, 'POST', '/admin/data-requests', {
        kind: 'export',
        account_id: userB.id,
        note: 'by email',
      });
      await call(s, 'POST', `/admin/data-requests/${req.id}/verify`, {
        reason: 'Replied from the account address.',
      });
      await reauth();
      await call(s, 'POST', `/admin/data-requests/${req.id}/run`);
      await expect
        .poll(
          async () =>
            (await call<{ download_url?: string }>(b, 'GET', '/users/@me/data-export'))
              .download_url ?? null,
        )
        .not.toBeNull();
      const status = await call<{ download_url: string }>(b, 'GET', '/users/@me/data-export');
      const dl = await b.get(`${API}${status.download_url}`);
      expect(dl.status()).toBe(200);
      expect((await dl.body()).length).toBeGreaterThan(100);

      const erase = await call<{ id: string }>(s, 'POST', '/admin/data-requests', {
        kind: 'erasure',
        account_id: userB.id,
        note: 'by email',
      });
      await call(s, 'POST', `/admin/data-requests/${erase.id}/verify`, {
        reason: 'Replied from the account address.',
      });
      await reauth();
      await call(s, 'POST', `/admin/data-requests/${erase.id}/run`, { delete_messages: true });
      await expect
        .poll(async () => {
          const msgs = await call<{ id: string }[]>(
            a,
            'GET',
            `/channels/${channelId}/messages?limit=50`,
          );
          return msgs.some((m) => m.id === reported.id || m.id === other.id);
        })
        .toBe(false);

      // The erasure met the two holds of the case: staff see them flagged, keep the reported message as evidence
      // with a stated basis, and release the wider hold (MOD-DSR-005).
      const holds = await call<
        {
          id: string;
          case_id: string | null;
          scope_kind: string | null;
          erasure_flagged?: boolean;
        }[]
      >(s, 'GET', '/moderation/holds?active_only=true&limit=100');
      const flagged = holds.filter((h) => h.case_id === caseId && h.erasure_flagged);
      expect(flagged.map((h) => h.scope_kind).sort(), 'both holds of the case are flagged').toEqual(
        ['message', 'user_messages'],
      );
      await call(
        s,
        'POST',
        `/moderation/holds/${flagged.find((h) => h.scope_kind === 'message')!.id}/retain`,
        {
          basis: 'Needed for an open legal claim: the harassment report.',
        },
      );
      await call(
        s,
        'POST',
        `/moderation/holds/${flagged.find((h) => h.scope_kind === 'user_messages')!.id}/release`,
        { reason: 'The wider hold is not needed once the evidence is kept.' },
      );
    });

    await test.step('the access log has a row for each step, and the chain is intact', async () => {
      type Row = { action: string; kind: string; at_us: number };
      const rows = async (actor: string) =>
        (await call<{ items: Row[] }>(o, 'GET', `/admin/access-log?actor_id=${actor}&limit=100`))
          .items;
      const count = (list: Row[], kind: string, action: string, since = 0) =>
        list.filter((r) => r.kind === kind && r.action === action && r.at_us >= since).length;

      // S is a fresh account: every row is from this run. One audit row (admin_action) per step they took.
      const staffRows = await rows(userS.id);
      const audit: [string, number][] = [
        ['moderation.case.claim', 1],
        ['moderation.case.note', 1],
        ['moderation.case.state', 1],
        ['moderation.hold.place', 1],
        ['moderation.case.ban', 1],
        ['person.ban', 1],
        ['moderation.takedown.create', 2],
        ['moderation.takedown.item_add', 2],
        ['moderation.takedown.uphold', 2],
        ['moderation.takedown.counter_notice', 2],
        ['moderation.takedown.restore', 2],
        ['storage.mount.delist', 1],
        ['storage.mount.relist', 1],
        ['moderation.evasion.resolve', 1],
        ['data_request.create', 2],
        ['data_request.verify', 2],
        ['data_request.run_export', 1],
        ['data_request.run_erasure', 1],
        ['moderation.hold.retain_after_erasure', 1],
        ['moderation.hold.release', 1],
      ];
      for (const [action, n] of audit) {
        expect(count(staffRows, 'admin_action', action), `audit rows for ${action}`).toBe(n);
      }
      // And the reads that were logged: the reporter's name, the signals, the requests.
      for (const action of [
        'moderation.evasion.scan',
        'moderation.evasion.list',
        'data_request.view',
      ]) {
        expect(count(staffRows, 'read', action), `access rows for ${action}`).toBeGreaterThan(0);
      }

      // The Owner is the other reviewer; their rows since the start of this run.
      const ownerRows = await rows(ownerId);
      for (const action of [
        'features.patch',
        'moderation.appeal.decide',
        'person.unban',
        'person.link',
        'library.series.create',
        'library.chapter.publish',
      ]) {
        expect(
          count(ownerRows, 'admin_action', action, startUs),
          `Owner audit rows for ${action}`,
        ).toBe(1);
      }

      const chain = await call<{ ok: boolean; problem: string | null }>(
        o,
        'GET',
        '/admin/access-log/verify',
      );
      expect(chain.ok, chain.problem ?? 'chain verifies').toBe(true);
    });

    await anon.dispose();
  });
});
