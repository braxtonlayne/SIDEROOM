import { describe, expect, it } from 'vitest';
import { t } from '../../i18n';
import { partyMessage } from '../pageMessage';

describe('partyMessage (XC-VIS-001)', () => {
  it('drops a description that only repeats the title', () => {
    const m = partyMessage(
      'error',
      { kind: 'gone', message: 'This watch party is not available.' },
      t,
    );
    expect(m.title).toBe('This watch party is not available');
    expect(m.description).toBeUndefined();
  });

  it('keeps a description that says something else', () => {
    const m = partyMessage('error', { kind: 'network', message: 'The server did not answer.' }, t);
    expect(m.title).toBe('We could not open this watch party');
    expect(m.description).toBe('The server did not answer.');
  });

  it('falls back to the connection hint and keeps the age copy', () => {
    expect(partyMessage('error', null, t).description).toBe('Check your connection and try again.');
    expect(partyMessage('error', { kind: 'age' }, t).title).toBe(
      'This party is showing age-restricted content',
    );
    expect(partyMessage('ended', { kind: 'removed' }, t)).toEqual({
      title: 'You are not in this party any more',
      description: 'Check your connection and try again.',
    });
  });
});
