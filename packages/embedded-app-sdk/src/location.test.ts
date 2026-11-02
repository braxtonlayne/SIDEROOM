import { describe, expect, it } from 'vitest';
import { ACTIVITY_PATH_PREFIX, applicationIdFromLocation, isActivityLocation } from './protocol';

describe('OPS-TDEP-001: the application id from the page address', () => {
  it('reads the activity host and a test deployment path', () => {
    expect(
      applicationIdFromLocation({ hostname: '123.activities.chat.example.org', pathname: '/' }),
    ).toBe('123');
    expect(
      applicationIdFromLocation({ hostname: 'abc.trycloudflare.com', pathname: '/_activity/123/' }),
    ).toBe('123');
    expect(
      applicationIdFromLocation({
        hostname: 'abc.trycloudflare.com',
        pathname: '/_activity/77/assets/index.html',
      }),
    ).toBe('77');
    expect(ACTIVITY_PATH_PREFIX).toBe('/_activity/');
  });

  it('does not take a made-up path segment for an id', () => {
    // Not the activity path: falls back to the host label, as before.
    expect(
      applicationIdFromLocation({ hostname: 'chat.example.org', pathname: '/_activity/abc/' }),
    ).toBe('chat');
    expect(
      applicationIdFromLocation({ hostname: 'chat.example.org', pathname: '/x/_activity/5/' }),
    ).toBe('chat');
  });

  it('tells an Activity page from any other', () => {
    expect(isActivityLocation({ hostname: '5.activities.h.example', pathname: '/' })).toBe(true);
    expect(isActivityLocation({ hostname: 'h.example', pathname: '/_activity/5/' })).toBe(true);
    expect(isActivityLocation({ hostname: 'h.example', pathname: '/_activity/5' })).toBe(false);
    expect(isActivityLocation({ hostname: 'h.example', pathname: '/app' })).toBe(false);
  });
});
