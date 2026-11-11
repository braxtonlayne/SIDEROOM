// SEC-AUD-042 (D-574) and SEC-AUD-051 (D-581): every staff call that places, changes or ends a member restriction, suspension or perk
// answers 900004 until the second-factor window is open; the web helpers go through withReauth, so the prompt
// opens once and the call is retried once.
import { http, HttpResponse, server } from '../../test/server';
import { setDataRequestRestriction, suspendAccount, unsuspendAccount } from '../api';
import { revokeGrant } from '../donations/api';
import { registerReauthPrompt } from '../reauth';
import {
  decideAppeal,
  handleUnderage,
  liftAgeLock,
  setAgeRestricted,
  warnCase,
} from '../../moderation/api';
import {
  clearNode,
  decideRestrictionAppeal,
  freezeInvites,
  freezeNode,
  liftRestriction,
  revokeInvites,
  timeoutMember,
  warnMember,
} from '../../moderation/graduated';

type Case = [string, 'post' | 'delete', string, () => Promise<unknown>];

const CASES: Case[] = [
  ['suspend', 'post', '/admin/accounts/20/suspend', () => suspendAccount('20', 'r')],
  ['unsuspend', 'post', '/admin/accounts/20/unsuspend', () => unsuspendAccount('20', 'r')],
  ['entitlement revoke', 'delete', '/admin/donations/entitlements/9', () => revokeGrant('9', 'r')],
  [
    'ban appeal decision',
    'post',
    '/moderation/appeals/5/decision',
    () => decideAppeal('5', 'overturned', 'r'),
  ],
  [
    'age restriction',
    'post',
    '/moderation/age/restrictions',
    () => setAgeRestricted({ kind: 'channel', id: '1', restricted: true, reason: 'r' }),
  ],
  [
    'underage',
    'post',
    '/moderation/age/underage',
    () => handleUnderage({ account_id: '20', band: 'under_13', note: 'r' }),
  ],
  ['underage lift', 'post', '/moderation/age/underage/20/lift', () => liftAgeLock('20', 'r')],
  ['warn', 'post', '/admin/moderation/members/20/warn', () => warnMember('20', 'r')],
  ['timeout', 'post', '/admin/moderation/members/20/timeout', () => timeoutMember('20', 60, 'r')],
  [
    'invite freeze',
    'post',
    '/admin/moderation/members/20/invite-freeze',
    () => freezeInvites('20', 7, 'r'),
  ],
  [
    'revoke invites',
    'post',
    '/admin/moderation/members/20/revoke-invites',
    () => revokeInvites('20', 'r'),
  ],
  [
    'restriction lift',
    'post',
    '/admin/moderation/restrictions/3/lift',
    () => liftRestriction('3', 'r'),
  ],
  [
    'restriction appeal decision',
    'post',
    '/admin/moderation/restrictions/3/appeal-decision',
    () => decideRestrictionAppeal('3', 'upheld', 'r'),
  ],
  [
    'node freeze',
    'post',
    '/admin/moderation/subtree-cases/4/nodes/20/freeze',
    () => freezeNode('4', '20', 'r'),
  ],
  [
    'node clear',
    'post',
    '/admin/moderation/subtree-cases/4/nodes/20/clear',
    () => clearNode('4', '20', 'r'),
  ],
  // D-581: the case warning and the data-request restriction.
  ['case warn', 'post', '/moderation/cases/6/warn', () => warnCase('6', 'text')],
  [
    'data-request restriction',
    'post',
    '/admin/data-requests/8/restriction',
    () => setDataRequestRestriction('8', true, 'r'),
  ],
];

describe('D-574 and D-581 staff calls ask for the second factor', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
  });

  it.each(CASES)('%s prompts once and retries once', async (_name, method, path, run) => {
    let calls = 0;
    server.use(
      http[method](`*/api/v1${path}`, () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json(
              { code: 900004, message: 'Confirm it is you first.' },
              { status: 403 },
            )
          : HttpResponse.json({});
      }),
    );
    let prompts = 0;
    const off = registerReauthPrompt(async () => {
      prompts += 1;
      return true;
    });
    try {
      await run();
    } finally {
      off();
    }
    expect(prompts).toBe(1);
    expect(calls).toBe(2);
  });
});
