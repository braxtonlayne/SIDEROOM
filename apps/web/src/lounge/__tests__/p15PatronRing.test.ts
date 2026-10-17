import { describe, expect, it } from 'vitest';
import { toLoungeUser } from '../liveMap';

describe('sr_patron on the user object (D-256, K-08)', () => {
  it('maps the flag to the lounge user, and only when true', () => {
    expect(toLoungeUser({ id: '1', username: 'a', sr_patron: true }).patron).toBe(true);
    expect(toLoungeUser({ id: '1', username: 'a', sr_patron: false }).patron).toBeUndefined();
    expect(toLoungeUser({ id: '1', username: 'a' }).patron).toBeUndefined();
  });
});
