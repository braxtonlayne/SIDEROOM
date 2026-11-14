// SEC-AUD-207 (D-690): the reader keeps its position in step with the account's other devices.
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { useReaderPrefsStore } from '../prefs';
import {
  httpProgress,
  localProgress,
  setProgressApi,
  type ProgressApi,
  type ProgressInput,
  type SaveResult,
} from '../progress';
import type { ChapterProgress } from '../types';
import { manifest, renderAt, useLibraryApi } from './harness';

const OTHER: ChapterProgress = {
  page_index: 2,
  percent: 100,
  completed: false,
  updated_at_ms: 5000,
};

function fakeApi(o: Partial<ProgressApi> = {}) {
  const save = vi.fn<(p: ProgressInput) => Promise<SaveResult>>();
  save.mockResolvedValue({
    applied: true,
    progress: { page_index: 0, percent: 0, completed: false, updated_at_ms: 900 },
  });
  const api: ProgressApi = {
    save,
    recent: () => Promise.resolve([]),
    forSeries: () => Promise.resolve({}),
    get: () => Promise.resolve({ page_index: 0, percent: 0, completed: false, updated_at_ms: 500 }),
    bookmarks: () => Promise.resolve([]),
    addBookmark: vi.fn(),
    removeBookmark: vi.fn(),
    remove: vi.fn(),
    ...o,
  };
  setProgressApi(api);
  return { api, save };
}

function openReader() {
  // eslint-disable-next-line react-hooks/rules-of-hooks -- a test helper that registers handlers, not a hook
  useLibraryApi();
  server.use(http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(manifest(1))));
  return renderAt('/library/r/c1/1');
}

const status = () => screen.getByTestId('reader-status');
const turn = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.keyboard('{ArrowLeft}'); // RTL: left goes forward
};
const progressEvent = (o: Record<string, unknown> = {}) =>
  act(() =>
    emitDispatch('SR_LIBRARY_UPDATE', {
      type: 'progress',
      chapter_id: 'c1',
      series_id: '1',
      page_index: 2,
      percent: 100,
      cfi: null,
      completed: false,
      updated_at_ms: 7000,
      ...o,
    }),
  );

beforeEach(() => {
  localStorage.clear();
  useReaderPrefsStore.setState({ global: {}, series: {} });
  setProgressApi(localProgress);
});
afterEach(() => setProgressApi(null));

describe('a save names its base', () => {
  it('a_save_sends_the_base_it_started_from', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.put('*/api/v1/library/chapters/:id/progress', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({
          progress: { page_index: 1, percent: 50, completed: false, updated_at_ms: 42 },
          applied: true,
        });
      }),
    );
    const input = {
      chapterId: 'c1',
      seriesId: '1',
      seriesTitle: 'S',
      chapterNumber: 1,
      pageCount: 3,
      pageIndex: 1,
      percent: 50,
      completed: false,
    };
    const r = await httpProgress.save({ ...input, baseUpdatedAtMs: 500 });
    expect(body).toMatchObject({ page_index: 1, base_updated_at_ms: 500 });
    expect(r).toEqual({
      applied: true,
      progress: { page_index: 1, percent: 50, completed: false, updated_at_ms: 42 },
    });
    await httpProgress.save(input); // no base: the field is left out so it applies
    expect(body).not.toHaveProperty('base_updated_at_ms');
  });

  it('the reader saves with the position it opened from, then with the answer of the last save', async () => {
    const { save } = fakeApi();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1), { timeout: 4000 });
    expect(save.mock.calls[0]![0]).toMatchObject({ pageIndex: 1, baseUpdatedAtMs: 500 });
    await turn(user);
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2), { timeout: 4000 });
    expect(save.mock.calls[1]![0]).toMatchObject({ pageIndex: 2, baseUpdatedAtMs: 900 });
  });

  it('local_mode_always_applies', async () => {
    const r = await localProgress.save({
      chapterId: 'c1',
      seriesId: '1',
      seriesTitle: 'S',
      chapterNumber: 1,
      pageCount: 3,
      pageIndex: 1,
      percent: 50,
      completed: false,
      baseUpdatedAtMs: 1,
    });
    expect(r.applied).toBe(true);
  });
});

describe('the other device read further', () => {
  const conflicting = () => {
    const f = fakeApi();
    f.save.mockResolvedValueOnce({ applied: false, server: OTHER });
    return f;
  };

  it('applied_false_asks_once_and_stops_saving', async () => {
    const { save } = conflicting();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    const dialog = await screen.findByRole(
      'dialog',
      { name: 'You read further on another device' },
      { timeout: 4000 },
    );
    expect(dialog).toHaveTextContent('page 3 of 3');
    expect(screen.getByRole('button', { name: 'Continue from page 3' })).toBeInTheDocument();
    expect(save).toHaveBeenCalledTimes(1);
    // Closing it is "Stay here"; turning pages while it is open (it traps focus) saves nothing more.
    expect(screen.getAllByRole('dialog')).toHaveLength(1);
    await new Promise((r) => setTimeout(r, 2500));
    expect(save).toHaveBeenCalledTimes(1);
  });

  it('continue_there_jumps_and_resumes', async () => {
    const { save } = conflicting();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    await user.click(
      await screen.findByRole('button', { name: 'Continue from page 3' }, { timeout: 4000 }),
    );
    await waitFor(() => expect(status()).toHaveTextContent('Page 3 of 3'));
    expect(screen.queryByRole('dialog')).toBeNull();
    // Saving resumes from the server's position: the base is its updated_at_ms.
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2), { timeout: 4000 });
    expect(save.mock.calls[1]![0]).toMatchObject({ pageIndex: 2, baseUpdatedAtMs: 5000 });
  });

  it('stay_here_saves_over_and_resumes', async () => {
    const { save } = conflicting();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    await user.click(await screen.findByRole('button', { name: 'Stay here' }, { timeout: 4000 }));
    // The reader's own position is saved without a base, so it applies.
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
    expect(save.mock.calls[1]![0]).toMatchObject({ pageIndex: 1, baseUpdatedAtMs: null });
    expect(status()).toHaveTextContent('Page 2 of 3');
    await turn(user);
    await waitFor(() => expect(save).toHaveBeenCalledTimes(3), { timeout: 4000 });
    expect(save.mock.calls[2]![0]).toMatchObject({ pageIndex: 2, baseUpdatedAtMs: 900 });
  });

  it('an_unanswered_prompt_saves_nothing_on_unload', async () => {
    const { save } = conflicting();
    const user = userEvent.setup();
    const view = openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    await screen.findByRole('dialog', undefined, { timeout: 4000 });
    window.dispatchEvent(new Event('pagehide'));
    view.unmount();
    await new Promise((r) => setTimeout(r, 100));
    expect(save).toHaveBeenCalledTimes(1);
  });

  it('an other device that is behind is written over without asking', async () => {
    const f = fakeApi();
    f.save.mockResolvedValueOnce({
      applied: false,
      server: { page_index: 0, percent: 0, completed: false, updated_at_ms: 5000 },
    });
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    await waitFor(() => expect(f.save).toHaveBeenCalledTimes(2), { timeout: 4000 });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(f.save.mock.calls[1]![0]).toMatchObject({ baseUpdatedAtMs: null });
  });
});

describe('while reading', () => {
  it('a_progress_event_for_the_open_chapter_offers_a_jump_and_never_moves_the_page', async () => {
    fakeApi();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await progressEvent();
    const jump = await screen.findByRole('button', { name: 'Jump to page 3' });
    expect(status()).toHaveTextContent('Page 1 of 3');
    await user.click(jump);
    await waitFor(() => expect(status()).toHaveTextContent('Page 3 of 3'));
    expect(screen.queryByRole('button', { name: 'Jump to page 3' })).toBeNull();
  });

  it('a declined position is not offered again', async () => {
    fakeApi();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await progressEvent();
    await user.click(await screen.findByRole('button', { name: 'Dismiss' }));
    await progressEvent(); // the same position again (same time)
    await progressEvent({ updated_at_ms: 6000 });
    expect(screen.queryByRole('button', { name: 'Jump to page 3' })).toBeNull();
    await progressEvent({ updated_at_ms: 8000 }); // a newer one is a new offer
    expect(await screen.findByRole('button', { name: 'Jump to page 3' })).toBeInTheDocument();
  });

  it('the_readers_own_echo_is_ignored', async () => {
    const { save } = fakeApi();
    const user = userEvent.setup();
    openReader();
    await screen.findByTestId('reader-status');
    await turn(user);
    await turn(user);
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1), { timeout: 4000 });
    // The save went out for page 3 (index 2) and came back with updated_at_ms 900: its echo is the same time,
    await progressEvent({ updated_at_ms: 900 });
    // or arrives before the answer, with the position that was sent.
    await progressEvent({ updated_at_ms: 901, page_index: 2 });
    expect(screen.queryByRole('button', { name: /^Jump to/ })).toBeNull();
  });

  it('a_progress_event_refreshes_continue_reading', async () => {
    useLibraryApi();
    const recent = vi.fn(() => Promise.resolve([]));
    fakeApi({ recent });
    renderAt('/library');
    await waitFor(() => expect(recent).toHaveBeenCalledTimes(1));
    await progressEvent({ chapter_id: 'c9' });
    await waitFor(() => expect(recent).toHaveBeenCalledTimes(2));
  });
});
