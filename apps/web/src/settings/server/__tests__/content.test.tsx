import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../../store';
import { emojiNameFromFile, validEmojiName } from '../sections/Expressions';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const community = {
  guild_id: 'g1',
  community_enabled: false,
  rules_channel_id: null,
  public_updates_channel_id: null,
  safety_alerts_channel_id: null,
  description: null,
  preferred_locale: 'en-US',
  onboarding_enabled: false,
  welcome_screen_enabled: false,
  guide_enabled: false,
  invites_paused: false,
  raid_alert_joins_per_minute: 0,
  age_restricted: false,
};

describe('Invites', () => {
  const invite = (code: string, over: object = {}) => ({
    code,
    guild: { id: 'g1', name: 'Book Club' },
    channel: { id: 'c1', name: 'general', type: 0 },
    inviter: { id: '2', username: 'bob' },
    uses: 2,
    max_uses: 5,
    max_age: 0,
    created_at: '2026-01-01T00:00:00Z',
    expires_at: null,
    ...over,
  });

  it('lists invites, revokes one, and toggles pause', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    const pause = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/invites', () =>
        HttpResponse.json([invite('abc'), invite('def')]),
      ),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.delete('*/api/v1/invites/abc', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(invite('abc'));
      }),
      http.patch('*/api/v1/guilds/g1/sr/community', async ({ request }) => {
        await pause.record(request);
        return HttpResponse.json({ ...community, invites_paused: true });
      }),
    );
    await openSettings('invites');
    const table = await screen.findByRole('table');
    expect(within(table).getByText('abc')).toBeInTheDocument();
    expect(within(table).getAllByText('2 / 5')).toHaveLength(2);
    await userEvent.click(screen.getByRole('button', { name: 'Revoke invite abc' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    await waitFor(() => expect(screen.queryByText('abc')).toBeNull());
    await userEvent.click(await screen.findByRole('switch', { name: /Pause invites/ }));
    await waitFor(() => expect(pause.calls).toHaveLength(1));
    expect(pause.calls[0]!.body).toEqual({ invites_paused: true });
  });

  it('shows a revoke failure', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1/invites', () => HttpResponse.json([invite('abc')])),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.delete('*/api/v1/invites/abc', () =>
        HttpResponse.json({ message: 'no' }, { status: 403 }),
      ),
    );
    await openSettings('invites');
    await userEvent.click(await screen.findByRole('button', { name: 'Revoke invite abc' }));
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
  });
});

describe('Emoji & Stickers', () => {
  const emoji = {
    id: 'e1',
    name: 'party',
    roles: [],
    user: { id: '2', username: 'bob' },
    animated: false,
    managed: false,
    available: true,
  };

  it('shows slot counters and uploads a file as a data URI', async () => {
    seed({ perms: [MANAGE.expressions] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/emojis', () => HttpResponse.json([emoji])),
      http.get('*/api/v1/guilds/g1/stickers', () => HttpResponse.json([])),
      http.post('*/api/v1/guilds/g1/emojis', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ ...emoji, id: 'e2', name: 'blob' });
      }),
    );
    const { container } = await openSettings('emoji');
    expect(await screen.findByText('Static 1 / 250')).toBeInTheDocument();
    expect(screen.getByText('Animated 0 / 250')).toBeInTheDocument();
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File([new Uint8Array([137, 80, 78, 71])], 'my blob!.png', {
      type: 'image/png',
    });
    await userEvent.upload(input, file);
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    const body = cap.calls[0]!.body as { name: string; image: string };
    expect(body.name).toBe('my_blob');
    expect(body.image.startsWith('data:image/png;base64,')).toBe(true);
  });

  it('rejects oversized files and surfaces server errors', async () => {
    seed({ perms: [MANAGE.expressions] });
    server.use(
      http.get('*/api/v1/guilds/g1/emojis', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/stickers', () => HttpResponse.json([])),
      http.post('*/api/v1/guilds/g1/emojis', () =>
        HttpResponse.json(
          {
            message: 'Invalid Form Body',
            errors: { image: { _errors: [{ message: 'Must be a PNG.' }] } },
          },
          { status: 400 },
        ),
      ),
    );
    const { container } = await openSettings('emoji');
    await screen.findByText('Static 0 / 250');
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    const big = new File([new Uint8Array(300 * 1024)], 'big.png', { type: 'image/png' });
    await userEvent.upload(input, big);
    expect(await screen.findByText(/big.png is larger than 256 KB/)).toBeInTheDocument();
    await userEvent.upload(input, new File([new Uint8Array([1])], 'ok.png', { type: 'image/png' }));
    expect(await screen.findByText(/image: Must be a PNG\./)).toBeInTheDocument();
  });

  it('renames and validates', async () => {
    seed({ perms: [MANAGE.expressions] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/emojis', () => HttpResponse.json([emoji])),
      http.get('*/api/v1/guilds/g1/stickers', () => HttpResponse.json([])),
      http.patch('*/api/v1/guilds/g1/emojis/e1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ ...emoji, name: 'confetti' });
      }),
    );
    await openSettings('emoji');
    const name = await screen.findByLabelText('Name of :party:');
    await userEvent.clear(name);
    await userEvent.type(name, 'a!');
    expect(screen.getByText(/Use 2 to 32 letters/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save name for :party:' })).toBeDisabled();
    await userEvent.clear(name);
    await userEvent.type(name, 'confetti');
    await userEvent.click(screen.getByRole('button', { name: 'Save name for :party:' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ name: 'confetti' });
  });

  it('helper names', () => {
    expect(emojiNameFromFile('my cool-image.PNG')).toBe('my_cool_image');
    expect(emojiNameFromFile('a.png')).toBe('a_emoji');
    expect(validEmojiName('ok_1')).toBe(true);
    expect(validEmojiName('x')).toBe(false);
    expect(validEmojiName('no spaces')).toBe(false);
  });
});

describe('Integrations', () => {
  const tpl = {
    code: 'TPL123',
    name: 'Starter',
    description: null,
    usage_count: 3,
    creator: { id: '1', username: 'me' },
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-02T00:00:00Z',
    source_guild_id: 'g1',
    is_dirty: true,
  };

  it('creates a webhook and shows its one-time URL', async () => {
    seed({ perms: [MANAGE.webhooks] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/webhooks', () => HttpResponse.json([])),
      http.post('*/api/v1/channels/c1/webhooks', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({
          id: 'w1',
          type: 1,
          guild_id: 'g1',
          channel_id: 'c1',
          name: 'Deploys',
          avatar: null,
          token: 'tok',
          url: '/api/v1/webhooks/w1/tok',
        });
      }),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Webhooks' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Create webhook' }));
    await userEvent.type(screen.getByLabelText('Webhook name'), 'Deploys');
    await userEvent.click(
      within(screen.getByRole('dialog', { name: 'Create webhook' })).getByRole('button', {
        name: 'Create',
      }),
    );
    expect(await screen.findByText(/\/api\/v1\/webhooks\/w1\/tok/)).toBeInTheDocument();
    expect(cap.calls[0]!.body).toEqual({ name: 'Deploys' });
    expect(screen.getByRole('button', { name: 'Copy webhook URL' })).toBeInTheDocument();
  });

  it('creates a webhook with an avatar, then edits and removes it', async () => {
    seed({ perms: [MANAGE.webhooks] });
    const created = capture();
    const edited = capture();
    const hash = 'a'.repeat(32);
    const hook = {
      id: 'w1',
      type: 1,
      guild_id: 'g1',
      channel_id: 'c1',
      name: 'Deploys',
      avatar: hash,
    };
    server.use(
      http.get('*/api/v1/guilds/g1/webhooks', () => HttpResponse.json([hook])),
      http.post('*/api/v1/channels/c1/webhooks', async ({ request }) => {
        await created.record(request);
        return HttpResponse.json({ ...hook, token: 'tok', url: '/api/v1/webhooks/w1/tok' });
      }),
      http.patch('*/api/v1/webhooks/w1', async ({ request }) => {
        await edited.record(request);
        return HttpResponse.json({ ...hook, name: 'Deploys 2', avatar: null });
      }),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Webhooks' }));
    await screen.findByRole('list', { name: 'Webhooks' });
    expect(screen.getByRole('img', { name: 'Deploys' })).toHaveAttribute(
      'src',
      expect.stringContaining('/cdn/avatars/w1/' + hash),
    );

    await userEvent.click(screen.getByRole('button', { name: 'Create webhook' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create webhook' });
    await userEvent.type(within(dialog).getByLabelText('Webhook name'), 'Deploys');
    await userEvent.upload(
      within(dialog).getByTestId('file-webhook-avatar'),
      new File([new Uint8Array([137, 80, 78, 71])], 'bot.png', { type: 'image/png' }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create' }));
    await screen.findByText('/api/v1/webhooks/w1/tok', { exact: false });
    expect(created.calls[0]!.body).toMatchObject({ name: 'Deploys' });
    expect(
      (created.calls[0]!.body as { avatar: string }).avatar.startsWith('data:image/png;base64,'),
    ).toBe(true);

    await userEvent.click(screen.getByRole('button', { name: 'Edit webhook Deploys' }));
    const edit = await screen.findByRole('dialog', { name: 'Edit Deploys' });
    const name = within(edit).getByLabelText('Webhook name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Deploys 2');
    await userEvent.click(within(edit).getByRole('button', { name: 'Remove webhook avatar' }));
    await userEvent.click(within(edit).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(edited.calls).toHaveLength(1));
    expect(edited.calls[0]!.body).toEqual({ name: 'Deploys 2', avatar: null });
  });

  it('existing webhooks have no Copy URL; Reset URL shows the new URL once (D-164)', async () => {
    seed({ perms: [MANAGE.webhooks] });
    const cap = capture();
    const hook = {
      id: 'w1',
      type: 1,
      guild_id: 'g1',
      channel_id: 'c1',
      name: 'Deploys',
      avatar: null,
    };
    server.use(
      http.get('*/api/v1/guilds/g1/webhooks', () => HttpResponse.json([hook])),
      http.post('*/api/v1/webhooks/w1/reset-token', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ ...hook, token: 'fresh', url: '/api/v1/webhooks/w1/fresh' });
      }),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Webhooks' }));
    await screen.findByRole('list', { name: 'Webhooks' });
    expect(screen.queryByRole('button', { name: /Copy URL/ })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Reset URL for Deploys' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reset URL for Deploys' });
    expect(cap.calls).toHaveLength(0);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Reset URL' }));
    expect(await screen.findByText(/\/api\/v1\/webhooks\/w1\/fresh/)).toBeInTheDocument();
    expect(cap.calls).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Copy webhook URL' })).toBeInTheDocument();
    // Dismissing hides it for good; the list never shows a URL.
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText(/\/api\/v1\/webhooks\/w1\/fresh/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Copy webhook URL' })).toBeNull();
  });

  it('deletes a webhook; hides the templates tab without Manage Server', async () => {
    seed({ perms: [MANAGE.webhooks] });
    let gone = false;
    server.use(
      http.get('*/api/v1/guilds/g1/webhooks', () =>
        HttpResponse.json(
          gone
            ? []
            : [
                {
                  id: 'w1',
                  type: 1,
                  guild_id: 'g1',
                  channel_id: 'c1',
                  name: 'Deploys',
                  avatar: null,
                },
              ],
        ),
      ),
      http.delete('*/api/v1/webhooks/w1', () => {
        gone = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Webhooks' }));
    expect(screen.queryByRole('tab', { name: 'Server Template' })).toBeNull();
    await userEvent.click(await screen.findByRole('button', { name: 'Delete webhook Deploys' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete webhook' }));
    expect(await screen.findByText('No webhooks')).toBeInTheDocument();
  });

  it('syncs and deletes a template, shows its code', async () => {
    seed({ perms: [MANAGE.guild] });
    const sync = capture();
    let gone = false;
    server.use(
      http.get('*/api/v1/guilds/g1/templates', () => HttpResponse.json(gone ? [] : [tpl])),
      http.put('*/api/v1/guilds/g1/templates/TPL123', async ({ request }) => {
        await sync.record(request);
        return HttpResponse.json({ ...tpl, is_dirty: false });
      }),
      http.delete('*/api/v1/guilds/g1/templates/TPL123', () => {
        gone = true;
        return HttpResponse.json(tpl);
      }),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Server Template' }));
    expect(await screen.findByLabelText('Template code for Starter')).toHaveTextContent('TPL123');
    expect(screen.getByText('Out of date')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Sync template Starter' }));
    await waitFor(() => expect(sync.calls).toHaveLength(1));
    await userEvent.click(screen.getByRole('button', { name: 'Delete template Starter' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete template' }));
    expect(await screen.findByRole('button', { name: 'Generate template' })).toBeInTheDocument();
  });
});

describe('Community', () => {
  it('enables community through the three-step wizard and applies its safety settings', async () => {
    seed({ perms: [MANAGE.guild] });
    // @everyone holds Kick Members (bit 1) next to the harmless bits 10 and 11.
    useStore.setState((st) => ({
      roles: {
        ...st.roles,
        g1: { ...st.roles['g1']!, permissions: String((1n << 1n) | (1n << 10n) | (1n << 11n)) },
      },
    }));
    const guildCap = capture();
    const roleCap = capture();
    const comCap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () =>
        HttpResponse.json({
          id: 'g1',
          name: 'Book Club',
          icon: null,
          description: null,
          owner_id: '9',
          system_channel_id: null,
          verification_level: 0,
          default_message_notifications: 0,
          explicit_content_filter: 0,
          preferred_locale: 'en-US',
          features: [],
        }),
      ),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.patch('*/api/v1/guilds/g1', async ({ request }) => {
        await guildCap.record(request);
        return HttpResponse.json({});
      }),
      http.patch('*/api/v1/guilds/g1/roles/g1', async ({ request }) => {
        await roleCap.record(request);
        return HttpResponse.json({});
      }),
      http.patch('*/api/v1/guilds/g1/sr/community', async ({ request }) => {
        await comCap.record(request);
        return HttpResponse.json({
          ...community,
          community_enabled: true,
          rules_channel_id: 'c1',
          public_updates_channel_id: 'c2',
        });
      }),
    );
    await openSettings('community');
    await userEvent.click(await screen.findByRole('switch', { name: /Enable Community/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Enable Community' });
    expect(within(dialog).getByText('Step 1 of 3')).toBeInTheDocument();
    const verification = await within(dialog).findByRole('switch', {
      name: /Verified email required to join in/,
    });
    const filter = within(dialog).getByRole('switch', {
      name: /Media from all members is scanned/,
    });
    // Next stays disabled until the admin turns both switches on; nothing is sent yet.
    expect(within(dialog).getByRole('button', { name: 'Next' })).toBeDisabled();
    expect(guildCap.calls).toHaveLength(0);
    await userEvent.click(verification);
    await waitFor(() => expect(guildCap.calls).toHaveLength(1));
    expect(within(dialog).getByRole('button', { name: 'Next' })).toBeDisabled();
    await userEvent.click(filter);
    await waitFor(() => expect(guildCap.calls).toHaveLength(2));
    await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Next' })).toBeEnabled());
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    // Step 2 needs both channels.
    expect(within(dialog).getByRole('button', { name: 'Next' })).toBeDisabled();
    await userEvent.selectOptions(within(dialog).getByLabelText('Rules channel'), 'c1');
    await userEvent.selectOptions(within(dialog).getByLabelText('Community updates channel'), 'c2');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    // Step 3 needs the guidelines to be accepted.
    const finish = within(dialog).getByRole('button', { name: 'Enable Community' });
    expect(finish).toBeDisabled();
    await userEvent.click(within(dialog).getByRole('checkbox', { name: /community guidelines/ }));
    await userEvent.click(finish);
    await waitFor(() => expect(comCap.calls).toHaveLength(1));
    expect(guildCap.calls[0]!.body).toEqual({ verification_level: 1 });
    expect(guildCap.calls[1]!.body).toEqual({ explicit_content_filter: 2 });
    // Finish only writes the notification default; it never sets the safety levels itself.
    expect(guildCap.calls[2]!.body).toEqual({ default_message_notifications: 1 });
    expect(roleCap.calls[0]!.body).toEqual({ permissions: String((1n << 10n) | (1n << 11n)) });
    expect(comCap.calls[0]!.body).toEqual({
      community_enabled: true,
      rules_channel_id: 'c1',
      public_updates_channel_id: 'c2',
    });
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Enable Community' })).toBeNull(),
    );
  });

  it('validates onboarding questions and saves the welcome screen', async () => {
    seed({ perms: [MANAGE.guild] });
    const welcome = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.get('*/api/v1/guilds/g1/onboarding', () =>
        HttpResponse.json({ guild_id: 'g1', enabled: false, default_channel_ids: [], prompts: [] }),
      ),
      http.get('*/api/v1/guilds/g1/welcome-screen', () =>
        HttpResponse.json({ enabled: false, description: null, welcome_channels: [] }),
      ),
      http.patch('*/api/v1/guilds/g1/welcome-screen', async ({ request }) => {
        await welcome.record(request);
        return HttpResponse.json({ enabled: true, description: 'Hi', welcome_channels: [] });
      }),
    );
    await openSettings('community');
    await userEvent.click(await screen.findByRole('tab', { name: 'Onboarding' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Add question' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(await screen.findByText('Question 1 needs a title.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('tab', { name: 'Welcome Screen' }));
    await userEvent.click(await screen.findByRole('switch', { name: 'Enable welcome screen' }));
    await userEvent.type(screen.getByLabelText('Server description'), 'Hi');
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(welcome.calls).toHaveLength(1));
    expect(welcome.calls[0]!.body).toEqual({
      enabled: true,
      description: 'Hi',
      welcome_channels: [],
    });
  });

  it('previews onboarding: the answers hand out channels and roles', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.get('*/api/v1/guilds/g1/onboarding', () =>
        HttpResponse.json({
          guild_id: 'g1',
          enabled: true,
          default_channel_ids: ['c1'],
          prompts: [
            {
              title: 'What do you read?',
              single_select: false,
              required: true,
              in_onboarding: true,
              options: [
                { title: 'Manga', channel_ids: ['c2'], role_ids: ['mod'] },
                { title: 'Novels', channel_ids: [], role_ids: [] },
              ],
            },
          ],
        }),
      ),
    );
    await openSettings('community');
    await userEvent.click(await screen.findByRole('tab', { name: 'Onboarding' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Preview onboarding' }));
    const dialog = await screen.findByRole('dialog', { name: 'Onboarding preview' });
    expect(within(dialog).getByText(/Channels:/).parentElement).toHaveTextContent('#general');
    expect(within(dialog).getByText(/Channels:/).parentElement).not.toHaveTextContent('#random');
    expect(within(dialog).getByText(/must answer every required/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('checkbox', { name: 'Manga' }));
    expect(within(dialog).getByText(/Channels:/).parentElement).toHaveTextContent(
      '#general, #random',
    );
    expect(within(dialog).getByText(/Roles:/).parentElement).toHaveTextContent('Mod');
    expect(screen.queryByText(/must answer every required/)).toBeNull();
  });
});
