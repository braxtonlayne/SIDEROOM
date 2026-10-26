import { renderHook, waitFor } from '@testing-library/react';
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
