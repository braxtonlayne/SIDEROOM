import { describe, expect, it } from 'vitest';
import { cdnSize, cdnUrl } from './cdn';

describe('cdnUrl', () => {
  it('builds avatar and icon URLs', () => {
    expect(cdnUrl('avatars', '7', 'abc', { size: 64 })).toBe('/cdn/avatars/7/abc.png?size=64');
    expect(cdnUrl('icons', '9', 'a_ff', { format: 'gif' })).toBe('/cdn/icons/9/a_ff.gif');
    expect(cdnUrl('avatars', '7', 'abc', { base: 'https://x.test', preferWebp: true })).toBe(
      'https://x.test/cdn/avatars/7/abc.webp',
    );
    expect(cdnUrl('avatars', '7', 'a_abc', { preferWebp: true })).toBe('/cdn/avatars/7/a_abc.png');
  });
  it('returns null without a hash', () => {
    expect(cdnUrl('avatars', '7', null)).toBeNull();
    expect(cdnUrl('avatars', '7', undefined)).toBeNull();
  });
  it('rounds sizes to a valid power of two', () => {
    expect([1, 16, 17, 100, 128, 5000].map(cdnSize)).toEqual([16, 16, 32, 128, 128, 4096]);
  });
});
