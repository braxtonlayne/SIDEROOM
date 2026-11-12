import { describe, expect, it } from 'vitest';
import { templateOf } from '../pathTemplate';

const O = 'https://chat.example.org';
const t = (u: string) => templateOf(u, O);

describe('templateOf', () => {
  it('replaces a snowflake id', () => {
    expect(t(`${O}/api/v1/channels/1794500000123456789/messages`)).toBe(
      '/api/v1/channels/:id/messages',
    );
  });
  it('replaces two ids and keeps @me', () => {
    expect(t(`${O}/api/v1/guilds/123/members/456`)).toBe('/api/v1/guilds/:id/members/:id');
    expect(t(`${O}/api/v1/users/@me/settings`)).toBe('/api/v1/users/@me/settings');
  });
  it('drops the query (a token) and the fragment', () => {
    const r = t(`${O}/api/v1/files/list?token=SECRETtoken123#frag`);
    expect(r).toBe('/api/v1/files/list');
    expect(r).not.toContain('SECRET');
  });
  it('turns an invite code (mixed case or long) into :id', () => {
    expect(t(`${O}/api/v1/invites/aB3dE`)).toBe('/api/v1/invites/:id');
    expect(t(`${O}/api/v1/invites/abcdef0123456789abcdef`)).toBe('/api/v1/invites/:id');
  });
  it('is null for another origin, a path outside /api/, and a presigned bucket URL', () => {
    expect(t('https://evil.example.net/api/v1/x')).toBeNull();
    expect(t(`${O}/assets/app.js`)).toBeNull();
    expect(t(`${O}/apix/v1`)).toBeNull();
    expect(t('https://bucket.s3.amazonaws.com/api/v1/a?X-Amz-Signature=abc')).toBeNull();
  });
  it('is null for a result that fails the server check', () => {
    expect(t(`${O}/api/v1/a b`)).toBeNull();
    expect(t(`${O}/api/v1/${'ab/'.repeat(50)}x`)).toBeNull();
    expect(t(`${O}/api/v1/x123456y`)).toBeNull();
  });
});
