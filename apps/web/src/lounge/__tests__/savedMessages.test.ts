import { http, HttpResponse, server } from '../../test/server';
import { dueReminders, parseSaved, useSaved, type Reminder } from '../shell/savedMessages';

const entry = (id: string) => ({
  message_id: id,
  channel_id: 'c1',
  guild_id: 'g1',
  preview: `msg ${id}`,
  author: 'Bob',
  saved_at: '2026-01-01T00:00:00Z',
});

describe('saved messages', () => {
  beforeEach(() => useSaved.getState().reset());

  it('keeps well-formed entries only', () => {
    expect(parseSaved(null)).toEqual({});
    expect(parseSaved([1, 2])).toEqual({});
    const parsed = parseSaved({ a: entry('1'), b: { message_id: 1 }, c: 'x' });
    expect(Object.keys(parsed)).toEqual(['a']);
    const r = parseSaved<Reminder>(
      { a: { ...entry('1'), due_at: 'soon' }, b: entry('2') },
      'due_at',
    );
    expect(Object.keys(r)).toEqual(['a']);
  });

  it('orders due reminders oldest first and leaves future ones', () => {
    const mk = (id: string, due: string): Reminder => ({ ...entry(id), due_at: due });
    const due = dueReminders(
      {
        a: mk('a', '2026-01-02T00:00:00Z'),
        b: mk('b', '2026-01-01T00:00:00Z'),
        c: mk('c', '2026-03-01T00:00:00Z'),
      },
      Date.parse('2026-02-01T00:00:00Z'),
    );
    expect(due.map((r) => r.message_id)).toEqual(['b', 'a']);
  });

  it('a settings blob that loads late does not undo a change still being saved', async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((r) => (release = r));
    server.use(
      http.patch('*/api/v1/users/@me/settings', async () => {
        await gate;
        return HttpResponse.json({ version: 2, settings: {} });
      }),
    );
    const s = useSaved.getState();
    s.toggleBookmark(entry('1'));
    s.addReminder(entry('2'), new Date(Date.now() + 60_000));
    // The blob fetched at boot arrives now, without either entry.
    useSaved.getState().adopt({ bookmarks: {}, reminders: {} });
    expect(Object.keys(useSaved.getState().bookmarks)).toEqual(['1']);
    expect(Object.keys(useSaved.getState().reminders)).toEqual(['2']);
    // Entries from the server that are not pending still arrive.
    useSaved.getState().adopt({ bookmarks: { '9': entry('9') } });
    expect(Object.keys(useSaved.getState().bookmarks).sort()).toEqual(['1', '9']);
    // A removal in flight stays removed too.
    useSaved.getState().toggleBookmark(entry('9'));
    useSaved.getState().adopt({ bookmarks: { '9': entry('9') } });
    expect(Object.keys(useSaved.getState().bookmarks)).toEqual(['1']);
    release();
    await vi.waitFor(() => expect(useSaved.getState().pending.bookmarks).toEqual({}));
    // Once saved, the server's copy is authoritative again.
    useSaved.getState().adopt({ bookmarks: {} });
    expect(useSaved.getState().bookmarks).toEqual({});
  });
});
