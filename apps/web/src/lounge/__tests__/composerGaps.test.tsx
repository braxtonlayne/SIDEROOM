import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { pickableStickers, resetGuildStickers } from '../components/ExpressionPicker';
import { resetStickerCache } from '../msg/Sticker';
import {
  commandSuggestions,
  MAX_REMIND_MS,
  parseDuration,
  runSlash,
  type SlashContext,
} from '../msg/slashCommands';
import { computeWaveform, toBase64, voiceMimeType } from '../msg/voice';
import { useSaved } from '../shell/savedMessages';
import { at, baseHandlers, calls, me, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const CTX: SlashContext = { guild: true, threads: true, tts: true };

describe('slash command logic', () => {
  it('parses durations', () => {
    expect(parseDuration('90m take a break')).toEqual({ ms: 90 * 60_000, rest: 'take a break' });
    expect(parseDuration('1h30m tea')).toEqual({ ms: 90 * 60_000, rest: 'tea' });
    expect(parseDuration('in 2 hours water plants')).toEqual({
      ms: 2 * 3_600_000,
      rest: 'water plants',
    });
    expect(parseDuration('10 seconds')).toEqual({ ms: 10_000, rest: '' });
    expect(parseDuration('soon')).toBeNull();
    expect(parseDuration('0m x')).toBeNull();
  });

  it('turns the new commands into actions and checks their context', () => {
    expect(runSlash('/nick Bobby', CTX)).toEqual({ kind: 'nick', nick: 'Bobby' });
    expect(runSlash('/nick', CTX)).toEqual({ kind: 'nick', nick: null });
    expect(runSlash('/nick x', { ...CTX, guild: false }).kind).toBe('error');
    expect(runSlash('/nick ' + 'x'.repeat(33), CTX).kind).toBe('error');
    expect(runSlash('/thread Trip plans', CTX)).toEqual({ kind: 'thread', name: 'Trip plans' });
    expect(runSlash('/thread', CTX).kind).toBe('error');
    expect(runSlash('/thread a', { ...CTX, threads: false }).kind).toBe('error');
    expect(runSlash('/msg <@42> hello there', CTX)).toEqual({
      kind: 'msg',
      userId: '42',
      content: 'hello there',
    });
    expect(runSlash('/msg bob hi', CTX).kind).toBe('error');
    expect(runSlash('/msg <@42>', CTX).kind).toBe('error');
    expect(runSlash('/remind 2h call mum', CTX)).toEqual({
      kind: 'remind',
      ms: 2 * 3_600_000,
      text: 'call mum',
    });
    expect(runSlash('/remind 1s x', CTX).kind).toBe('error');
    expect(runSlash(`/remind ${Math.ceil(MAX_REMIND_MS / 86_400_000) + 1}d x`, CTX).kind).toBe(
      'error',
    );
    expect(runSlash('/tts hello', CTX)).toEqual({ kind: 'send', content: 'hello', tts: true });
    expect(runSlash('/tts hello', { ...CTX, tts: false }).kind).toBe('error');
  });

  it('only suggests commands that work where the viewer is', () => {
    const names = (c: SlashContext) => commandSuggestions('', c).map((s) => s.label);
    expect(names(CTX)).toEqual(expect.arrayContaining(['/nick', '/thread', '/tts', '/msg']));
    const dm = names({ guild: false, threads: false, tts: false });
    expect(dm).not.toContain('/nick');
    expect(dm).not.toContain('/thread');
    expect(dm).not.toContain('/tts');
    expect(dm).toContain('/remind');
  });
});

describe('voice and sticker helpers', () => {
  it('computes a normalised waveform', () => {
    const w = computeWaveform(new Float32Array([0, 0.1, -0.5, 0.25]), 4);
    expect(Array.from(w)).toEqual([0, 51, 255, 128]);
    expect(Array.from(computeWaveform(new Float32Array(0), 3))).toEqual([0, 0, 0]);
    expect(toBase64(new Uint8Array([1, 2, 3]))).toBe('AQID');
  });

  it('offers a recorder only where the browser records Ogg', () => {
    // jsdom has no MediaRecorder: no button, no upload of a format the server refuses.
    expect(voiceMimeType()).toBeNull();
  });

  it('filters stickers: available, not Lottie, matching name or tags', () => {
    const list = [
      { id: '1', name: 'wave', tags: 'hello', format_type: 1, available: true },
      { id: '2', name: 'spin', tags: 'lottie', format_type: 3, available: true },
      { id: '3', name: 'gone', tags: '', format_type: 1, available: false },
      { id: '4', name: 'dance', tags: 'party wave', format_type: 4, available: true },
    ];
    expect(pickableStickers(list, '').map((s) => s.id)).toEqual(['1', '4']);
    expect(pickableStickers(list, 'WAVE').map((s) => s.id)).toEqual(['1', '4']);
    expect(pickableStickers(list, 'hello').map((s) => s.id)).toEqual(['1']);
  });
});

describe('composer', () => {
  beforeEach(() => {
    baseHandlers();
    resetGuildStickers();
    resetStickerCache();
    useSaved.getState().reset();
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
  });

  it('sends a guild sticker from the sticker picker and explains the empty GIF tab', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/stickers', () =>
        HttpResponse.json([
          { id: '9', name: 'wave', tags: 'hi', format_type: 1, available: true },
          { id: '10', name: 'lottie', tags: '', format_type: 3, available: true },
        ]),
      ),
      http.get('*/api/v1/guilds/g1/stickers/:sid/url', () =>
        HttpResponse.json({ url: '/cdn/stickers/g1/9.png?sig=x', expires_at: 1 }),
      ),
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('200', { author: me }));
      }),
    );
    const { container } = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Open sticker picker' }));
    const tile = await screen.findByRole('button', { name: 'Send sticker wave' });
    expect(screen.queryByRole('button', { name: 'Send sticker lottie' })).not.toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole('tab', { name: 'GIFs' }));
    expect(screen.getByText(/GIFs are not set up on this server/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('tab', { name: 'Stickers' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Send sticker wave' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    const body = calls.find((c) => c.key === 'POST /channels/c1/messages')!.body as {
      sticker_ids: string[];
    };
    expect(body.sticker_ids).toEqual(['9']);
    expect(tile).toBeDefined();
  });

  it('lists the plus menu entries and starts a standalone thread', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/threads', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(
          {
            id: 't9',
            type: 11,
            guild_id: 'g1',
            parent_id: 'c1',
            name: 'Free thread',
            thread_metadata: { archived: false, locked: false },
            member: {},
          },
          { status: 201 },
        );
      }),
      http.get('*/api/v1/channels/t9/messages', () => HttpResponse.json([])),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add to message' }));
    expect(await screen.findByRole('menuitem', { name: 'Upload a File' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: /Use Apps/ })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByRole('menuitem', { name: /Share from Library/ })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    await userEvent.click(screen.getByRole('menuitem', { name: 'Create Thread' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create Thread' });
    const nameBox = within(dialog).getByRole('textbox', { name: 'Thread name' });
    await userEvent.clear(nameBox);
    await userEvent.type(nameBox, 'Free thread');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Thread' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'POST /channels/c1/threads')?.body).toEqual({
        name: 'Free thread',
        auto_archive_duration: 1440,
        type: 11,
      }),
    );
    expect(await screen.findByRole('complementary', { name: /Thread: Free thread/ })).toBeVisible();
  });

  it('locks the composer while the member is timed out', async () => {
    const until = new Date(Date.now() + 3_600_000).toISOString();
    useStore.setState((s) => ({
      members: {
        ...s.members,
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [], communication_disabled_until: until },
      },
    }));
    at('/channels/g1/c1');
    const note = await screen.findByRole('note');
    expect(note).toHaveTextContent(/timed out and cannot send messages until/);
    expect(screen.queryByRole('textbox', { name: /Message/ })).not.toBeInTheDocument();
  });

  it('runs /nick, /remind and /tts from the message box', async () => {
    server.use(
      http.patch('*/api/v1/guilds/g1/members/1', async (ctx) => {
        await record('PATCH')(ctx);
        return HttpResponse.json({});
      }),
      http.patch('*/api/v1/users/@me/settings', async (ctx) => {
        await record('PATCH')(ctx);
        return HttpResponse.json({ version: 2, settings: {} });
      }),
      http.post('*/api/v1/users/@me/reminders', async (ctx) => {
        const b = (await ctx.request.clone().json()) as Record<string, unknown>;
        return HttpResponse.json({
          id: '9',
          note: b.note,
          remind_at: b.remind_at,
          status: 'pending',
          channel_id: null,
          guild_id: null,
          message_id: null,
          created_at: '',
        });
      }),
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('201', { author: me }));
      }),
    );
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, '/nick Bobby{Enter}');
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PATCH /guilds/g1/members/1')?.body).toEqual({
        nick: 'Bobby',
      }),
    );
    expect(await screen.findByText('Your nickname is now Bobby')).toBeInTheDocument();
    expect(box).toHaveValue('');

    await userEvent.type(box, '/remind 1h pay the rent{Enter}');
    await waitFor(() => expect(useSaved.getState().reminders).toHaveLength(1));
    const r = useSaved.getState().reminders[0]!;
    expect(r.note).toBe('pay the rent');
    expect(Date.parse(r.remind_at) - Date.now()).toBeGreaterThan(59 * 60_000);

    await userEvent.type(box, '/tts read this out{Enter}');
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    expect(calls.find((c) => c.key === 'POST /channels/c1/messages')!.body).toMatchObject({
      content: 'read this out',
      tts: true,
    });
  });

  it('opens the thread dialog from /thread and reports bad commands', async () => {
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, '/msg nobody hi{Enter}');
    expect(await screen.findByText(/pick them from the @ list/)).toBeInTheDocument();
    await userEvent.clear(box);
    await userEvent.type(box, '/thread Planning{Enter}');
    expect(await screen.findByRole('dialog', { name: 'Create Thread' })).toBeInTheDocument();
  });

  it('sends a /msg as a direct message', async () => {
    server.use(
      http.post('*/api/v1/users/@me/channels', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({
          id: '900',
          type: 1,
          recipients: [{ id: '2', username: 'bob' }],
        });
      }),
      http.post('*/api/v1/channels/900/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ ...msg('901', { author: me }), channel_id: '900' });
      }),
    );
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, '/msg @Bo');
    await userEvent.click(await screen.findByRole('option', { name: /Bob/ }));
    await userEvent.type(box, 'hi there{Enter}');
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/900/messages')).toBe(true),
    );
    expect(calls.find((c) => c.key === 'POST /users/@me/channels')?.body).toEqual({
      recipient_id: '2',
    });
    expect(calls.find((c) => c.key === 'POST /channels/900/messages')!.body).toMatchObject({
      content: 'hi there',
    });
  });
});

describe('voice message recorder', () => {
  class FakeRecorder {
    static isTypeSupported = (t: string) => t.startsWith('audio/ogg');
    state = 'inactive';
    ondataavailable: ((e: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;
    constructor(
      readonly stream: unknown,
      readonly opts: { mimeType: string },
    ) {}
    start() {
      this.state = 'recording';
    }
    stop() {
      this.state = 'inactive';
      this.ondataavailable?.({ data: new Blob(['ogg-bytes'], { type: 'audio/ogg' }) });
      this.onstop?.();
    }
  }

  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
    vi.stubGlobal('MediaRecorder', FakeRecorder);
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: { getUserMedia: async () => ({ getTracks: () => [{ stop() {} }] }) },
    });
    URL.createObjectURL = () => 'blob:voice';
    URL.revokeObjectURL = () => undefined;
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    Reflect.deleteProperty(navigator, 'mediaDevices');
  });

  it('records Ogg, previews it and sends a voice message with duration and waveform', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/attachments', async ({ request }) => {
        const body = (await request.json()) as { files: { id: string; filename: string }[] };
        calls.push({ key: 'SLOTS', body });
        return HttpResponse.json({
          attachments: body.files.map((f) => ({
            id: f.id,
            upload_id: 'v1',
            upload_url: '/storage/put/v1',
            upload_filename: 'abc/voice-message.ogg',
            upload_url_expires_in: 900,
            max_size: 10_000_000,
          })),
        });
      }),
      http.put('*/storage/put/v1', () => new HttpResponse(null, { status: 200 })),
      http.post('*/api/v1/channels/c1/attachments/v1/complete', () =>
        HttpResponse.json({ id: 'v1', filename: 'voice-message.ogg', size: 9, url: '/cdn/x' }),
      ),
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('300', { author: me }));
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Record a voice message' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Stop' }));
    expect(await screen.findByLabelText('Voice message preview')).toBeInTheDocument();
    await userEvent.click(
      within(screen.getByRole('group', { name: 'Voice message' })).getByRole('button', {
        name: /Send/,
      }),
    );
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    const slots = calls.find((c) => c.key === 'SLOTS')!.body as {
      files: { filename: string; content_type: string }[];
    };
    expect(slots.files[0]).toMatchObject({
      filename: 'voice-message.ogg',
      content_type: 'audio/ogg',
    });
    const post = calls.find((c) => c.key === 'POST /channels/c1/messages')!.body as {
      flags: number;
      attachments: { duration_secs: number; waveform: string; uploaded_filename: string }[];
    };
    expect(post.flags).toBe(8192);
    expect(post.attachments[0]!.duration_secs).toBeGreaterThan(0);
    expect(post.attachments[0]!.waveform.length).toBeGreaterThan(0);
    expect(post.attachments[0]!.uploaded_filename).toBe('abc/voice-message.ogg');
  });

  it('discards a recording', async () => {
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Record a voice message' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Stop' }));
    await userEvent.click(
      within(await screen.findByRole('group', { name: 'Voice message' })).getByRole('button', {
        name: 'Delete',
      }),
    );
    expect(await screen.findByRole('textbox', { name: 'Message #general' })).toBeInTheDocument();
  });
});
