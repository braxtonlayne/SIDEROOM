import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { patchSession, resetSession } from '../../../voice/session';
import { applyStageEvent, resetStage, useStage } from '../../../voice/stage';
import type { Channel } from '../../data';
import { MockDataProvider } from '../../mockData';
import { audienceOrder, StageView } from '../StageView';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

const perms = {
  connect: true,
  speak: true,
  useVad: true,
  muteMembers: false,
  deafenMembers: false,
  moveMembers: false,
  useSoundboard: true,
  requestToSpeak: true,
  manageChannels: false,
};
vi.mock('../perms', () => ({ useVoicePerms: () => perms }));

const stageChannel = (id: string): Channel =>
  ({ id, guildId: '1001', type: 'stage', name: 'Town hall', position: 0 }) as Channel;
const channel = stageChannel('9001');

const calls: { method: string; url: string; body: unknown }[] = [];

beforeEach(() => {
  calls.length = 0;
  resetStage();
  resetSession();
  Object.assign(perms, { muteMembers: false, moveMembers: false, manageChannels: false });
  const record =
    (method: string) =>
    async ({ request }: { request: Request }) => {
      let body: unknown = null;
      try {
        body = await request.json();
      } catch {
        /* no body */
      }
      calls.push({ method, url: new URL(request.url).pathname, body });
      return method === 'DELETE' || method === 'PATCH'
        ? new HttpResponse(null, { status: 204 })
        : HttpResponse.json({ channel_id: '9001', guild_id: '1001', topic: 'x' });
    };
  server.use(
    // The server's list is whatever the test has started.
    http.get('*/api/v1/guilds/1001/stage-instances', () =>
      HttpResponse.json(
        Object.values(useStage.getState().instances).map((i) => ({
          channel_id: i.channelId,
          guild_id: i.guildId,
          topic: i.topic,
        })),
      ),
    ),
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    http.post('*/api/v1/stage-instances', record('POST')),
    http.patch('*/api/v1/stage-instances/9001', record('PATCH')),
    http.delete('*/api/v1/stage-instances/9001', record('DELETE')),
    http.patch('*/api/v1/guilds/1001/voice-states/:user', record('PATCH')),
  );
});

const show = (ch: Channel = channel) => {
  const user = userEvent.setup();
  render(
    <MockDataProvider>
      <StageView channel={ch} />
    </MockDataProvider>,
  );
  return user;
};
const live = () =>
  act(() =>
    applyStageEvent('STAGE_INSTANCE_CREATE', {
      channel_id: '9001',
      guild_id: '1001',
      topic: 'Town hall: Q3',
    }),
  );
const live2 = () =>
  act(() =>
    applyStageEvent('STAGE_INSTANCE_CREATE', {
      channel_id: '9002',
      guild_id: '1001',
      topic: 'Quiet room',
    }),
  );
const join = (extra: Record<string, unknown> = {}) =>
  patchSession({ status: 'connected', guildId: '1001', channelId: '9001', ...extra });

describe('stage events', () => {
  it('keeps the running stages from GUILD_CREATE and STAGE_INSTANCE_*', () => {
    applyStageEvent('GUILD_CREATE', {
      id: '1001',
      stage_instances: [{ channel_id: '9001', guild_id: '1001', topic: 'A' }],
    });
    expect(useStage.getState().instances['9001']!.topic).toBe('A');
    applyStageEvent('STAGE_INSTANCE_UPDATE', { channel_id: '9001', guild_id: '1001', topic: 'B' });
    expect(useStage.getState().instances['9001']!.topic).toBe('B');
    applyStageEvent('STAGE_INSTANCE_DELETE', { channel_id: '9001', guild_id: '1001', topic: 'B' });
    expect(useStage.getState().instances['9001']).toBeUndefined();
    // A fresh GUILD_CREATE replaces that server's list only.
    applyStageEvent('STAGE_INSTANCE_CREATE', { channel_id: '5', guild_id: '2', topic: 'other' });
    applyStageEvent('GUILD_CREATE', { id: '1001', stage_instances: [] });
    expect(Object.keys(useStage.getState().instances)).toEqual(['5']);
    applyStageEvent('MESSAGE_CREATE', {});
  });

  it('puts raised hands first, longest waiting first', () => {
    const order = audienceOrder([
      { userId: 'a' },
      { userId: 'b', handRaisedAt: 30 },
      { userId: 'c' },
      { userId: 'd', handRaisedAt: 10 },
    ]);
    expect(order.map((m) => m.userId)).toEqual(['d', 'b', 'a', 'c']);
  });
});

describe('stage view (VOX-STG-002)', () => {
  it('shows the topic, the speakers as tiles and the audience with raised hands first', async () => {
    live();
    show();
    expect(await screen.findByTestId('stage-topic')).toHaveTextContent('Town hall: Q3');
    expect(screen.getByText('LIVE')).toBeInTheDocument();
    expect(screen.getByTestId('voice-tile-1')).toBeInTheDocument();
    const rows = screen.getAllByTestId(/^audience-/).map((r) => r.getAttribute('data-testid'));
    // User 5 raised a hand first, then user 2; user 3 never did.
    expect(rows).toEqual(['audience-5', 'audience-2', 'audience-3']);
    expect(
      within(screen.getByTestId('audience-5')).getByRole('img', { name: 'Hand raised' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Audience (3)' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Speakers (1)' })).toBeInTheDocument();
  });

  it('offers Join before you are in, with the counts, and no moderator buttons to a listener', async () => {
    live();
    show();
    expect(await screen.findByText('1 speaker, 3 listeners')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Join Stage' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'End Stage' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Start Stage' })).toBeNull();
    expect(screen.queryByRole('button', { name: /Invite/ })).toBeNull();
  });

  it('says so when no stage runs', async () => {
    show();
    expect(await screen.findByText('No stage is running right now.')).toBeInTheDocument();
  });

  it('lets a listener raise and lower a hand, and keeps their microphone shut', async () => {
    live2();
    patchSession({ status: 'connected', guildId: '1001', channelId: '9002' });
    const user = show(stageChannel('9002'));
    const mic = await screen.findByRole('button', { name: 'Mute' });
    expect(mic).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Raise hand' }));
    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0]).toMatchObject({
      method: 'PATCH',
      url: '/api/v1/guilds/1001/voice-states/@me',
    });
    const body = calls[0]!.body as { channel_id: string; request_to_speak_timestamp: string };
    expect(body.channel_id).toBe('9002');
    expect(Date.parse(body.request_to_speak_timestamp)).toBeGreaterThan(0);
    expect(screen.queryByRole('button', { name: 'Move to audience' })).toBeNull();
  });

  it('lets a speaker step back down to the audience', async () => {
    live();
    join();
    const user = show();
    await user.click(await screen.findByRole('button', { name: 'Move to audience' }));
    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0]).toMatchObject({
      method: 'PATCH',
      url: '/api/v1/guilds/1001/voice-states/@me',
      body: { channel_id: '9001', suppress: true },
    });
    expect(screen.queryByRole('button', { name: 'Raise hand' })).toBeNull();
  });

  it('lets a moderator start, edit and end the stage and invite the audience', async () => {
    Object.assign(perms, { muteMembers: true, moveMembers: true, manageChannels: true });
    const user = show();
    await user.click(await screen.findByRole('button', { name: 'Start Stage' }));
    const dialog = await screen.findByRole('dialog', { name: 'Start the stage' });
    const start = within(dialog).getByRole('button', { name: 'Start' });
    expect(start).toBeDisabled();
    await user.type(within(dialog).getByRole('textbox', { name: 'Topic' }), '  Weekly sync ');
    await user.click(start);
    await waitFor(() =>
      expect(calls.at(-1)).toMatchObject({ method: 'POST', url: '/api/v1/stage-instances' }),
    );
    expect(calls.at(-1)!.body).toEqual({
      channel_id: '9001',
      topic: 'Weekly sync',
      privacy_level: 2,
    });

    live();
    await user.click(await screen.findByRole('button', { name: 'Edit topic' }));
    const edit = await screen.findByRole('dialog', { name: 'Edit the topic' });
    const box = within(edit).getByRole('textbox', { name: 'Topic' });
    await user.clear(box);
    await user.type(box, 'Q4 plan');
    await user.click(within(edit).getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(calls.at(-1)).toMatchObject({ method: 'PATCH', url: '/api/v1/stage-instances/9001' }),
    );
    expect(calls.at(-1)!.body).toEqual({ topic: 'Q4 plan' });

    await user.click(screen.getAllByRole('button', { name: /^Invite .* to speak$/ })[0]!);
    await waitFor(() => expect(calls.at(-1)).toMatchObject({ method: 'PATCH' }));
    expect(calls.at(-1)!.url).toBe('/api/v1/guilds/1001/voice-states/5');
    expect(calls.at(-1)!.body).toEqual({ suppress: false });

    await user.click(screen.getByRole('button', { name: 'End Stage' }));
    await waitFor(() =>
      expect(calls.at(-1)).toMatchObject({ method: 'DELETE', url: '/api/v1/stage-instances/9001' }),
    );
  });

  it('shows a permission refusal', async () => {
    Object.assign(perms, { muteMembers: true, moveMembers: true, manageChannels: true });
    server.use(
      http.delete(
        '*/api/v1/stage-instances/9001',
        () =>
          new HttpResponse(JSON.stringify({ code: 50013, message: 'Missing Permissions' }), {
            status: 403,
          }),
      ),
    );
    live();
    const user = show();
    await user.click(await screen.findByRole('button', { name: 'End Stage' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('permission');
  });

  it('has no a11y violations', async () => {
    live();
    join();
    show();
    const view = await screen.findByLabelText('Town hall');
    expect(await axeViolations(view)).toEqual([]);
  });
});
