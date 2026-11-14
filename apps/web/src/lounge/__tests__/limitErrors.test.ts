import { describe, expect, it } from 'vitest';
import { ApiError } from '../../api/client';
import { t } from '../../i18n';
import { limitErrorText, limitFromMessage } from '../limitErrors';

const answer = (code: number, message: string) =>
  new ApiError(400, message, { code, message } as never);

describe('limitErrorText (SEC-AUD-184; D-659, D-660)', () => {
  it('takes the number in force from the message', () => {
    expect(limitFromMessage('Maximum number of guilds reached (200)')).toBe(200);
    expect(limitFromMessage('Maximum number of guilds reached')).toBeNull();
    expect(limitFromMessage(undefined)).toBeNull();
  });

  it('30001 says the account is in the most servers, with the number when it can be read', () => {
    const withNumber = limitErrorText(answer(30001, 'Maximum number of guilds reached (7)'), t);
    expect(withNumber).toBe(
      'You\u2019re in the most servers one account can be in here (7). Leave one to join or create another.',
    );
    const without = limitErrorText(answer(30001, 'Maximum number of guilds reached'), t);
    expect(without).toBe(
      'You\u2019re in the most servers one account can be in here. Leave one to join or create another.',
    );
  });

  it('30019 says the server is full, and for an app that it cannot be added', () => {
    const e = answer(30019, 'Maximum number of server members reached');
    expect(limitErrorText(e, t)).toBe('This server is full.');
    expect(limitErrorText(e, t, 'approve')).toBe('This server is full.');
    expect(limitErrorText(e, t, 'bot')).toBe('This server is full, so the app cannot be added.');
  });

  it('30001 on an approval names the applicant, not the approver', () => {
    const text = limitErrorText(
      answer(30001, 'This member is in the most servers one account can be in.'),
      t,
      'approve',
    );
    expect(text).toMatch(/^This member is in the most servers one account can be in/);
    expect(text).not.toMatch(/You\u2019re/);
  });

  it('leaves every other failure to the caller', () => {
    expect(limitErrorText(answer(10006, 'Unknown Invite'), t)).toBeNull();
    expect(limitErrorText(new Error('boom'), t)).toBeNull();
    expect(limitErrorText(undefined, t)).toBeNull();
  });
});
