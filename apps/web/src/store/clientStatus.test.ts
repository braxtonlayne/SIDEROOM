// SEC-AUD-206 (D-702): the platform the web app announces, and `client_status` kept by the store.
import { beforeEach, describe, expect, it } from 'vitest';
import { clientProperties } from './connect';
import { resetStore, useStore } from './index';
import { phoneOnly } from '../lounge/phoneOnly';

const os = (nav: Parameters<typeof clientProperties>[0]) => clientProperties(nav).os;
const ANDROID =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Mobile Safari/537.36';
const IPHONE =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1';
const IPAD_AS_MAC =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15';
const FIREFOX = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0';

describe('clientProperties', () => {
  it('says android on Android Chrome and ios on iPhone Safari', () => {
    expect(clientProperties({ userAgent: ANDROID })).toEqual({
      os: 'android',
      browser: 'sideroom',
      device: 'sideroom',
    });
    expect(os({ userAgent: IPHONE })).toBe('ios');
  });
  it('tells an iPad that says Macintosh by its touch points, and a Mac without them stays web', () => {
    expect(os({ userAgent: IPAD_AS_MAC, maxTouchPoints: 5 })).toBe('ios');
    expect(os({ userAgent: IPAD_AS_MAC, maxTouchPoints: 0 })).toBe('web');
  });
  it('says web on desktop Firefox, whatever the window width', () => {
    Object.defineProperty(window, 'innerWidth', { value: 320, configurable: true });
    expect(os({ userAgent: FIREFOX })).toBe('web');
  });
  it('lets userAgentData.mobile win when the browser has it', () => {
    expect(os({ userAgent: FIREFOX, userAgentData: { mobile: true, platform: 'Android' } })).toBe(
      'android',
    );
    expect(os({ userAgent: ANDROID, userAgentData: { mobile: false } })).toBe('web');
  });
  it('never sends the user-agent string', () => {
    expect(JSON.stringify(clientProperties({ userAgent: ANDROID }))).not.toContain('Pixel');
  });
});

describe('phoneOnly', () => {
  const p = (status: 'online' | 'idle' | 'dnd' | 'offline', client_status?: object) =>
    ({ status, client_status }) as Parameters<typeof phoneOnly>[0];
  it('is true for online on a phone alone', () => {
    expect(phoneOnly(p('online', { mobile: 'online' }))).toBe(true);
  });
  it('is false with a web or desktop session too, or none, or no client_status', () => {
    expect(phoneOnly(p('online', { mobile: 'online', web: 'online' }))).toBe(false);
    expect(phoneOnly(p('online', { mobile: 'online', desktop: 'idle' }))).toBe(false);
    expect(phoneOnly(p('online', {}))).toBe(false);
    expect(phoneOnly(p('online'))).toBe(false);
    expect(phoneOnly(undefined)).toBe(false);
  });
  it('keeps the dot for idle and do-not-disturb on a phone', () => {
    expect(phoneOnly(p('idle', { mobile: 'idle' }))).toBe(false);
    expect(phoneOnly(p('dnd', { mobile: 'dnd' }))).toBe(false);
  });
});

describe('client_status in the store', () => {
  beforeEach(() => resetStore());
  const dispatch = (e: string, d: unknown) => useStore.getState().applyDispatch(e, d);
  const mobile = { mobile: 'online' };

  it('keeps it through READY, PRESENCE_UPDATE, a member chunk and SR_ACTIVITY_UPDATE', () => {
    dispatch('READY', {
      user: { id: '1', username: 'me' },
      guilds: [],
      private_channels: [],
      presences: [{ user_id: '2', status: 'online', activities: [], client_status: mobile }],
    });
    expect(useStore.getState().presences['2']?.client_status).toEqual(mobile);

    dispatch('PRESENCE_UPDATE', {
      user: { id: '3' },
      status: 'online',
      activities: [],
      client_status: { web: 'idle', mobile: 'online' },
    });
    expect(useStore.getState().presences['3']?.client_status).toEqual({
      web: 'idle',
      mobile: 'online',
    });

    dispatch('GUILD_MEMBERS_CHUNK', {
      guild_id: 'g1',
      members: [{ user: { id: '4', username: 'd' }, roles: [] }],
      presences: [{ user_id: '4', status: 'online', activities: [], client_status: mobile }],
    });
    expect(useStore.getState().presences['4']?.client_status).toEqual(mobile);

    dispatch('SR_ACTIVITY_UPDATE', {
      user_id: '2',
      type: 'watching',
      activity: { name: 'Show' },
    });
    expect(useStore.getState().presences['2']?.client_status).toEqual(mobile);
    expect(useStore.getState().presences['2']?.activities).toHaveLength(1);
  });

  it('drops a malformed client_status, and unknown kinds and statuses inside one', () => {
    for (const bad of ['mobile', 5, null, ['mobile'], { mobile: 'asleep' }, { tv: 'online' }]) {
      dispatch('PRESENCE_UPDATE', { user: { id: '9' }, status: 'online', client_status: bad });
      expect(useStore.getState().presences['9']?.client_status).toBeUndefined();
    }
    dispatch('PRESENCE_UPDATE', {
      user: { id: '9' },
      status: 'online',
      client_status: { mobile: 'online', tv: 'online', web: 7 },
    });
    expect(useStore.getState().presences['9']?.client_status).toEqual(mobile);
  });
});
