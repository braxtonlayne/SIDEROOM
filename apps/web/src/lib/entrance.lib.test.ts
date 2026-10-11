import { formatInviteCode, isCompleteInviteCode } from './inviteCode';
import {
  minutesFromSeconds,
  safeNext,
  validateEmail,
  validatePassword,
  validateUsername,
} from './validation';

describe('formatInviteCode', () => {
  it('uppercases, strips junk, inserts the dash and caps at 10 chars', () => {
    expect(formatInviteCode('abcde')).toBe('ABCDE');
    expect(formatInviteCode(' ab-cdefgh jk99 ')).toBe('ABCDE-FGHJK');
  });
  it('maps Crockford look-alikes', () => {
    expect(formatInviteCode('oilab')).toBe('011AB');
  });
  it('knows when a code is complete', () => {
    expect(isCompleteInviteCode('ABCDE-FGHJK')).toBe(true);
    expect(isCompleteInviteCode('ABCDE-FGHJ')).toBe(false);
  });
});

describe('validation', () => {
  it('validates fields', () => {
    expect(validateEmail('a@b.co')).toBeNull();
    expect(validateEmail('nope')).not.toBeNull();
    expect(validateUsername('sam_1')).toBeNull();
    expect(validateUsername('Sam!')).not.toBeNull();
    expect(validatePassword('short')).not.toBeNull();
    expect(validatePassword('correct Horse 9 battery!')).toBeNull();
  });
  it('only allows same-origin next paths', () => {
    expect(safeNext('/invite/abc')).toBe('/invite/abc');
    expect(safeNext('//evil.example')).toBeNull();
    expect(safeNext('https://evil.example')).toBeNull();
    expect(safeNext(undefined)).toBeNull();
  });
  it('rounds lockout up to whole minutes', () => {
    expect(minutesFromSeconds(61)).toBe(2);
    expect(minutesFromSeconds(null)).toBe(1);
  });
});
