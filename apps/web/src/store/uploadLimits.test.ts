// D-238 / H-34: the upload limit is REST-only; a changed entitlement is fetched again.
import { waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../test/server';
import { resetStore, useStore } from './index';

const MIB = 1024 * 1024;

function seed() {
  useStore.getState().applyDispatch('READY', {
    user: { id: '1', username: 'me' },
    guilds: [{ id: 'g1' }, { id: 'g2' }],
    private_channels: [{ id: 'd1', type: 1, recipients: [{ id: '2', username: 'bob' }] }],
  });
  for (const id of ['g1', 'g2'])
    useStore.getState().applyDispatch('GUILD_CREATE', {
      id,
      name: id,
      channels: [],
      roles: [],
      members: [],
      sr_max_upload_bytes: 30 * MIB,
    });
  useStore.setState((s) => ({
    channels: { ...s.channels, d1: { ...s.channels.d1!, sr_max_upload_bytes: 30 * MIB } },
  }));
}

beforeEach(() => {
  resetStore();
  seed();
});

describe('SR_ENTITLEMENT_UPDATE', () => {
  it('refetches only the named server for the guild shape', async () => {
    const asked: string[] = [];
    server.use(
      http.get('*/api/v1/guilds/:id', ({ params }) => {
        asked.push(String(params.id));
        return HttpResponse.json({ id: params.id, name: 'x', sr_max_upload_bytes: 100 * MIB });
      }),
    );
    useStore.getState().applyDispatch('SR_ENTITLEMENT_UPDATE', {
      type: 'guild',
      guild_id: 'g1',
      level: 2,
      pool_cents: 0,
      changed: true,
    });
    await waitFor(() => expect(useStore.getState().guilds.g1?.sr_max_upload_bytes).toBe(100 * MIB));
    expect(useStore.getState().guilds.g2?.sr_max_upload_bytes).toBe(30 * MIB);
    expect(asked).toEqual(['g1']);
    // The server's name and the rest of the object stay as they were.
    expect(useStore.getState().guilds.g1?.name).toBe('g1');
  });

  it('refetches every server and the DMs for the user shape', async () => {
    server.use(
      http.get('*/api/v1/guilds/:id', ({ params }) =>
        HttpResponse.json({ id: params.id, name: 'x', sr_max_upload_bytes: 500 * MIB }),
      ),
      http.get('*/api/v1/users/@me/channels', () =>
        HttpResponse.json([{ id: 'd1', type: 1, sr_max_upload_bytes: 500 * MIB }]),
      ),
    );
    useStore.getState().applyDispatch('SR_ENTITLEMENT_UPDATE', {
      type: 'user',
      user_id: '1',
      reason: 'granted',
      patron: { active: true },
      limits: {},
    });
    await waitFor(() =>
      expect(useStore.getState().channels.d1?.sr_max_upload_bytes).toBe(500 * MIB),
    );
    expect(useStore.getState().guilds.g1?.sr_max_upload_bytes).toBe(500 * MIB);
    expect(useStore.getState().guilds.g2?.sr_max_upload_bytes).toBe(500 * MIB);
  });

  it('keeps the old number when the refetch fails', async () => {
    server.use(
      http.get('*/api/v1/guilds/:id', () =>
        HttpResponse.json({ message: 'down' }, { status: 500 }),
      ),
    );
    useStore.getState().applyDispatch('SR_ENTITLEMENT_UPDATE', { type: 'guild', guild_id: 'g1' });
    await new Promise((r) => setTimeout(r, 50));
    expect(useStore.getState().guilds.g1?.sr_max_upload_bytes).toBe(30 * MIB);
  });
});
