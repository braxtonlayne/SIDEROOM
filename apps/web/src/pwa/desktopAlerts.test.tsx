import { renderHook } from '@testing-library/react';
import { vi } from 'vitest';
import { DEFAULT_PREFS, usePrefsStore } from '../settings/user/prefs';
import { resetStore, useStore } from '../store';
import { emitDispatch } from '../store/events';
import type { Message } from '../store/types';
import { shouldAlert, useDesktopAlerts } from './useDesktopAlerts';

const msg = (over: Partial<Message> = {}): Message =>
  ({
    id: 'm1',
    channel_id: 'c1',
    guild_id: 'g1',
    author: { id: '2', username: 'bob', global_name: 'Bob' },
    content: 'hello there',
    timestamp: '2026-01-01T00:00:00Z',
    type: 0,
    ...over,
  }) as Message;

describe('shouldAlert', () => {
  it('alerts for DMs and mentions, not for ordinary server chatter or your own messages', () => {
    expect(shouldAlert(msg(), '1')).toBe(false);
    expect(shouldAlert(msg({ guild_id: undefined }), '1')).toBe(true);
    expect(shouldAlert(msg({ mentions: [{ id: '1' } as never] }), '1')).toBe(true);
    expect(shouldAlert(msg({ mention_everyone: true }), '1')).toBe(true);
    expect(shouldAlert(msg({ guild_id: undefined, author: { id: '1' } as never }), '1')).toBe(
      false,
    );
    expect(shouldAlert(msg({ guild_id: undefined, type: 7 }), '1')).toBe(false);
  });
});

describe('useDesktopAlerts', () => {
  const made: { title: string; opts: NotificationOptions | undefined }[] = [];
  beforeEach(() => {
    resetStore();
    useStore.setState({ currentUserId: '1' });
    made.length = 0;
    class FakeNotification {
      static permission = 'granted';
      constructor(title: string, opts?: NotificationOptions) {
        made.push({ title, opts });
      }
    }
    vi.stubGlobal('Notification', FakeNotification);
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, desktopNotifications: true } });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('raises a notification for a DM while the window is in the background', () => {
    renderHook(() => useDesktopAlerts());
    emitDispatch('MESSAGE_CREATE', msg({ guild_id: undefined }));
    expect(made).toHaveLength(1);
    expect(made[0]!.title).toBe('Bob sent you a message');
    expect(made[0]!.opts).toMatchObject({ body: 'hello there', tag: 'c1' });
  });

  it('stays quiet when focused, when switched off, and in Streamer Mode', () => {
    renderHook(() => useDesktopAlerts());
    vi.spyOn(document, 'hasFocus').mockReturnValue(true);
    emitDispatch('MESSAGE_CREATE', msg({ guild_id: undefined }));
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, desktopNotifications: false } });
    emitDispatch('MESSAGE_CREATE', msg({ guild_id: undefined }));
    usePrefsStore.setState({
      prefs: { ...DEFAULT_PREFS, desktopNotifications: true, streamerMode: true },
    });
    emitDispatch('MESSAGE_CREATE', msg({ guild_id: undefined }));
    expect(made).toHaveLength(0);
  });

  it('blinks the tab title until the window is focused again', () => {
    vi.useFakeTimers();
    document.title = 'SIDE ROOM';
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, titleBlink: true } });
    renderHook(() => useDesktopAlerts());
    emitDispatch('MESSAGE_CREATE', msg({ guild_id: undefined }));
    vi.advanceTimersByTime(1000);
    expect(document.title).toBe('Bob messaged you');
    vi.advanceTimersByTime(1000);
    expect(document.title).toBe('SIDE ROOM');
    vi.advanceTimersByTime(1000);
    window.dispatchEvent(new Event('focus'));
    expect(document.title).toBe('SIDE ROOM');
    vi.advanceTimersByTime(5000);
    expect(document.title).toBe('SIDE ROOM');
  });
});
