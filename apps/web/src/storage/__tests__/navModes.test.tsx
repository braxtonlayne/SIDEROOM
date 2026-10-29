import { act, renderHook, waitFor } from '@testing-library/react';
import { http, HttpResponse } from '../../test/server';
import { emitDispatch } from '../../store/events';
import { server } from '../../test/server';
import { byosMockHandlers } from '../../settings/user/storage/mocks';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { tabVisible, useNavVisibility } from '../navModes';

const summary = (cinema: number, library: number, cc?: number, cl?: number) => ({
  cinema,
  library,
  music: 0,
  ...(cc === undefined ? {} : { catalog: { cinema: cc, library: cl ?? 0 } }),
});

describe('tabVisible', () => {
  it('show and hide ignore the summary', () => {
    expect(tabVisible('show', 'cinema', summary(0, 0, 0, 0))).toBe(true);
    expect(tabVisible('hide', 'cinema', summary(5, 5, 5, 5))).toBe(false);
  });
  it('auto hides only when shelves and instance catalog are both known to be empty', () => {
    expect(tabVisible('auto', 'cinema', summary(0, 0, 0, 0))).toBe(false);
    expect(tabVisible('auto', 'cinema', summary(2, 0, 0, 0))).toBe(true);
    expect(tabVisible('auto', 'cinema', summary(0, 0, 3, 0))).toBe(true);
    expect(tabVisible('auto', 'library', summary(9, 0, 9, 0))).toBe(false);
  });
  it('auto keeps the tab while the summary is unknown or has no catalog counts', () => {
    expect(tabVisible('auto', 'cinema', null)).toBe(true);
    expect(tabVisible('auto', 'cinema', summary(0, 0))).toBe(true);
  });
});

describe('useNavVisibility', () => {
  afterEach(() => usePrefsStore.setState({ prefs: DEFAULT_PREFS }));

  it('shows both tabs by default without asking the server', () => {
    const { result } = renderHook(() => useNavVisibility());
    expect(result.current).toEqual({ cinema: true, library: true });
  });

  it('hides an empty Auto tab once the summary arrives', async () => {
    usePrefsStore.setState({
      prefs: { ...DEFAULT_PREFS, navCinema: 'auto', navLibrary: 'auto' },
    });
    server.use(...byosMockHandlers({ summary: summary(0, 4, 0, 0) }));
    const { result } = renderHook(() => useNavVisibility());
    await waitFor(() => expect(result.current).toEqual({ cinema: false, library: true }));
  });
});

describe('the shelves summary follows the gateway (BYOS-NAV-001)', () => {
  afterEach(() => usePrefsStore.setState({ prefs: DEFAULT_PREFS }));

  it('fetches again on STORAGE_MOUNT_UPDATE and on READY, and ignores other events', async () => {
    usePrefsStore.setState({
      prefs: { ...DEFAULT_PREFS, navCinema: 'auto', navLibrary: 'auto' },
    });
    let answer = summary(0, 0, 0, 0);
    let calls = 0;
    server.use(
      http.get('*/api/v1/users/@me/shelves/summary', () => {
        calls += 1;
        return HttpResponse.json(answer);
      }),
    );
    const { result } = renderHook(() => useNavVisibility());
    await waitFor(() => expect(result.current).toEqual({ cinema: false, library: false }));
    expect(calls).toBe(1);

    // A friend shares a shelf: the mount routes publish the event and Cinema appears.
    answer = summary(3, 0, 0, 0);
    act(() => emitDispatch('STORAGE_MOUNT_UPDATE', { mount_id: '9', change: 'shared' }));
    await waitFor(() => expect(result.current).toEqual({ cinema: true, library: false }));
    expect(calls).toBe(2);

    act(() => emitDispatch('GUILD_EMOJIS_UPDATE', {}));
    expect(calls).toBe(2);

    answer = summary(3, 2, 0, 0);
    act(() => emitDispatch('READY', {}));
    await waitFor(() => expect(result.current).toEqual({ cinema: true, library: true }));
    expect(calls).toBe(3);
  });

  it('does not listen while every tab is on show or hide', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/users/@me/shelves/summary', () => {
        calls += 1;
        return HttpResponse.json(summary(0, 0, 0, 0));
      }),
    );
    renderHook(() => useNavVisibility());
    act(() => emitDispatch('STORAGE_MOUNT_UPDATE', {}));
    await new Promise((r) => setTimeout(r, 20));
    expect(calls).toBe(0);
  });

  it('keeps the last answer when a refetch fails (fail visible)', async () => {
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, navCinema: 'auto' } });
    let fail = false;
    server.use(
      http.get('*/api/v1/users/@me/shelves/summary', () =>
        fail ? new HttpResponse(null, { status: 500 }) : HttpResponse.json(summary(0, 0, 0, 0)),
      ),
    );
    const { result } = renderHook(() => useNavVisibility());
    await waitFor(() => expect(result.current.cinema).toBe(false));
    fail = true;
    act(() => emitDispatch('STORAGE_MOUNT_UPDATE', {}));
    await new Promise((r) => setTimeout(r, 30));
    expect(result.current.cinema).toBe(false);
  });
});
