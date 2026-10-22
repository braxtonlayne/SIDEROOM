import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

beforeEach(() => {
  resetPrefs();
  signedIn();
});

const leveling = (over: object = {}) => ({
  id: 'community.leveling',
  name: 'Leveling',
  description: 'Members earn XP for chatting and rise through levels.',
  section: 'Community',
  settings: { announce_mine: true },
  overrides: {},
  schema: {
    fields: [
      {
        key: 'announce_mine',
        label: 'Announce my level-ups',
        help: 'Off: you still earn XP.',
        type: 'bool',
        default: true,
        scopes: ['user'],
      },
    ],
  },
  ...over,
});

describe('User Settings > Modules (LNG-USET-036)', () => {
  it('shows the personal settings of a module, saves only what changed and resets', async () => {
    const patches: unknown[] = [];
    let deleted = false;
    let state = leveling();
    server.use(
      http.get('*/api/v1/users/@me/sr/features', () => HttpResponse.json([state])),
      http.patch('*/api/v1/users/@me/sr/features/community.leveling', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body);
        state = leveling({ settings: body.settings, overrides: body.settings });
        return HttpResponse.json(state);
      }),
      http.delete('*/api/v1/users/@me/sr/features/community.leveling', () => {
        deleted = true;
        state = leveling();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('modules');
    const box = await screen.findByRole('checkbox', { name: /Announce my level-ups/ });
    expect(screen.getByText('Leveling')).toBeInTheDocument();
    expect(box).toBeChecked();
    expect(await axeViolations(document.body)).toEqual([]);
    expect(screen.getByRole('button', { name: 'Save Leveling' })).toBeDisabled();

    await user.click(box);
    await user.click(screen.getByRole('button', { name: 'Save Leveling' }));
    await waitFor(() => expect(patches).toEqual([{ settings: { announce_mine: false } }]));
    expect(await screen.findByText('Saved.')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Reset Leveling to the defaults' }));
    await waitFor(() => expect(deleted).toBe(true));
    await waitFor(() =>
      expect(screen.getByRole('checkbox', { name: /Announce my level-ups/ })).toBeChecked(),
    );
    expect(screen.queryByRole('button', { name: 'Reset Leveling to the defaults' })).toBeNull();
  });

  it('says so when no module has personal settings', async () => {
    server.use(http.get('*/api/v1/users/@me/sr/features', () => HttpResponse.json([])));
    openSettings('modules');
    expect(await screen.findByText('No module has personal settings yet.')).toBeInTheDocument();
  });

  it('shows an error when the save fails', async () => {
    server.use(
      http.get('*/api/v1/users/@me/sr/features', () => HttpResponse.json([leveling()])),
      http.patch('*/api/v1/users/@me/sr/features/community.leveling', () =>
        HttpResponse.json({ message: 'nope' }, { status: 500 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('modules');
    await user.click(await screen.findByRole('checkbox', { name: /Announce my level-ups/ }));
    await user.click(screen.getByRole('button', { name: 'Save Leveling' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  });
});
