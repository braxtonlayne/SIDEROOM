// H-38 / LNG-FRM-004: the New Post form takes files and sends them as the starter message's attachments.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, calls, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));
const uploads = vi.hoisted(() => ({ sent: [] as { channel: string; names: string[] }[] }));
vi.mock('../msg/upload', async (orig) => ({
  ...(await orig<typeof import('../msg/upload')>()),
  uploadFiles: vi.fn(async (channel: string, files: { id: string; name: string }[]) => {
    uploads.sent.push({ channel, names: files.map((f) => f.name) });
    return files.map((f, i) => ({ id: `u${i}`, uploadedFilename: `u${i}/${f.name}` }));
  }),
}));

describe('New Post with files', () => {
  beforeEach(() => {
    baseHandlers();
    uploads.sent.length = 0;
  });

  it('uploads to the forum slots, then posts with the attachment ids', async () => {
    seedLounge({
      messages: [],
      channels: { f1: { id: 'f1', type: 15, guild_id: 'g1', name: 'help', position: 1 } },
    });
    server.use(
      http.get('*/api/v1/channels/f1', () =>
        HttpResponse.json({ id: 'f1', type: 15, available_tags: [], flags: 0 }),
      ),
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({ threads: [], members: [], has_more: false }),
      ),
      http.get('*/api/v1/channels/f1/threads/archived/public', () =>
        HttpResponse.json({ threads: [], members: [], has_more: false }),
      ),
      http.post('*/api/v1/channels/f1/threads', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(
          { id: 'p9', type: 11, guild_id: 'g1', parent_id: 'f1' },
          { status: 201 },
        );
      }),
    );
    at('/channels/g1/f1');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New Post' }));
    const dialog = await screen.findByRole('dialog', { name: 'New Post' });
    await user.type(within(dialog).getByRole('textbox', { name: 'Title' }), 'Screenshots');
    // A file alone is enough for the message.
    await user.upload(
      within(dialog).getByTestId('new-post-files'),
      new File(['x'], 'shot.png', { type: 'image/png' }),
    );
    expect(await within(dialog).findByText('shot.png')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Post' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'POST /channels/f1/threads')?.body).toEqual({
        name: 'Screenshots',
        message: { content: '', attachments: [{ id: 'u0', uploaded_filename: 'u0/shot.png' }] },
      }),
    );
    expect(uploads.sent).toEqual([{ channel: 'f1', names: ['shot.png'] }]);
  });
});
