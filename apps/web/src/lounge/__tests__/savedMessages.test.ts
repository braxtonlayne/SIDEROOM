import { http, HttpResponse, server } from '../../test/server';
import { toSaved, useSaved } from '../shell/savedMessages';

const ref = (id: string) => ({
  message_id: id,
  channel_id: '5',
  guild_id: '7',
  preview: `msg ${id}`,
});
const row = (id: string) => ({
  message_id: id,
  channel_id: '5',
  guild_id: '7',
  note: '',
  created_at: '2026-01-01T00:00:00Z',
});

describe('saved items (server-backed)', () => {
  beforeEach(() => useSaved.getState().reset());

  it('loads the three lists, soonest first', async () => {
    server.use(
      http.get('*/api/v1/users/@me/bookmarks', () =>
        HttpResponse.json([{ bookmark: row('1'), message: null }]),
      ),
      http.get('*/api/v1/users/@me/reminders', () =>
        HttpResponse.json([
          { id: 'b', note: '', remind_at: '2026-02-02T00:00:00Z', status: 'pending' },
          { id: 'a', note: '', remind_at: '2026-02-01T00:00:00Z', status: 'pending' },
        ]),
      ),
    );
    await useSaved.getState().load();
    const s = useSaved.getState();
    expect(Object.keys(s.bookmarks)).toEqual(['1']);
    expect(s.reminders.map((r) => r.id)).toEqual(['a', 'b']);
    expect(s.loaded).toBe(true);
  });

  it('toggles a bookmark optimistically and rolls back on failure', async () => {
    server.use(
      http.put('*/api/v1/channels/5/messages/9/bookmark', () =>
        HttpResponse.json({ message: 'no' }, { status: 500 }),
      ),
    );
    await useSaved.getState().toggleBookmark(ref('9'));
    expect(useSaved.getState().bookmarks).toEqual({});
    server.use(
      http.put('*/api/v1/channels/5/messages/9/bookmark', () => HttpResponse.json(row('9'))),
      http.delete('*/api/v1/users/@me/bookmarks/9', () => new HttpResponse(null, { status: 204 })),
    );
    await useSaved.getState().toggleBookmark(ref('9'));
    expect(Object.keys(useSaved.getState().bookmarks)).toEqual(['9']);
    await useSaved.getState().toggleBookmark(ref('9'));
    expect(useSaved.getState().bookmarks).toEqual({});
  });

  it('sends a reminder about a message, or about a note when the id is a placeholder', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post('*/api/v1/users/@me/reminders', async ({ request }) => {
        const b = (await request.json()) as Record<string, unknown>;
        bodies.push(b);
        return HttpResponse.json({
          id: String(bodies.length),
          note: b.note,
          remind_at: b.remind_at,
          status: 'pending',
          channel_id: null,
          guild_id: null,
          message_id: null,
          created_at: '',
        });
      }),
    );
    const at = new Date(Date.now() + 3_600_000);
    await useSaved.getState().addReminder({ note: '', remindAt: at, ref: ref('9') });
    await useSaved.getState().addReminder({ note: 'rent', remindAt: at, ref: ref('remind-1') });
    expect(bodies[0]).toMatchObject({ note: 'msg 9', channel_id: '5', message_id: '9' });
    expect(bodies[1]).toEqual({ note: 'rent', remind_at: at.toISOString() });
    expect(useSaved.getState().reminders).toHaveLength(2);
  });

  it('previews a message for the note', () => {
    const long = 'x'.repeat(200);
    const r = toSaved(
      { id: '1', channelId: '2', content: long, attachments: [] } as never,
      undefined,
    );
    expect(r.preview.length).toBe(141);
    expect(r.guild_id).toBeNull();
  });
});
