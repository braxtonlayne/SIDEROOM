import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { VoiceController, setVoiceController } from '../../../voice/controller';
import { patchSession, resetSession } from '../../../voice/session';
import { useStore } from '../../../store';
import { resetSoundboard, useSoundboard } from '../../../voice/soundboard';
import { MockDataProvider } from '../../mockData';
import { SoundboardButton } from '../SoundboardPopover';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

const DEFAULTS = [
  { name: 'Chime', sound_id: '1', volume: 1, emoji_name: '\u{1F514}', available: true },
  { name: 'Ding', sound_id: '2', volume: 1, emoji_name: null, available: true },
];
const GUILD = [
  {
    name: 'Airhorn',
    sound_id: '77',
    volume: 0.5,
    emoji_name: null,
    guild_id: '1001',
    available: true,
  },
];

const play = vi.fn();
const previewSound = vi.fn();

beforeEach(() => {
  localStorage.clear();
  resetSoundboard();
  resetSession();
  play.mockReset();
  previewSound.mockReset();
  server.use(
    http.get('*/api/v1/soundboard-default-sounds', () => HttpResponse.json(DEFAULTS)),
    http.get('*/api/v1/guilds/1001/soundboard-sounds', () => HttpResponse.json({ items: GUILD })),
  );
  const c = new VoiceController();
  vi.spyOn(c, 'playSoundboard').mockImplementation(play);
  vi.spyOn(c, 'previewSound').mockImplementation(previewSound);
  setVoiceController(c);
  patchSession({ status: 'connected', guildId: '1001', channelId: '2011' });
});

function open() {
  const user = userEvent.setup();
  render(
    <MockDataProvider>
      <SoundboardButton guildId="1001" channelId="2011" />
    </MockDataProvider>,
  );
  return user;
}

async function popover(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Soundboard' }));
  const body = await screen.findByTestId('soundboard');
  await within(body).findByRole('button', { name: 'Play Chime' });
  return body;
}

describe('soundboard popover (VOX-SB-002)', () => {
  it('lists this server first and the default sounds, and plays one into the call', async () => {
    play.mockResolvedValue({ ok: true });
    const user = open();
    const body = await popover(user);
    expect(within(body).getByRole('heading', { name: 'This server' })).toBeInTheDocument();
    expect(within(body).getByRole('heading', { name: 'Default sounds' })).toBeInTheDocument();
    await user.click(within(body).getByRole('button', { name: 'Play Airhorn' }));
    expect(play).toHaveBeenCalledWith(expect.objectContaining({ id: '77', guildId: '1001' }));
    expect(within(body).queryByRole('alert')).toBeNull();
  });

  it('shows why a play was refused', async () => {
    play.mockResolvedValue({ ok: false, message: 'Wait a moment before playing another sound.' });
    const user = open();
    const body = await popover(user);
    await user.click(within(body).getByRole('button', { name: 'Play Ding' }));
    expect(await within(body).findByRole('alert')).toHaveTextContent('Wait a moment');
  });

  it('searches by name', async () => {
    const user = open();
    const body = await popover(user);
    await user.type(within(body).getByRole('searchbox', { name: 'Search sounds' }), 'air');
    expect(within(body).getByRole('button', { name: 'Play Airhorn' })).toBeInTheDocument();
    expect(within(body).queryByRole('button', { name: 'Play Chime' })).toBeNull();
    await user.clear(within(body).getByRole('searchbox'));
    await user.type(within(body).getByRole('searchbox'), 'zzz');
    expect(within(body).getByText('No sounds match.')).toBeInTheDocument();
  });

  it('keeps favorites in their own section at the top', async () => {
    const user = open();
    const body = await popover(user);
    await user.click(within(body).getByRole('button', { name: 'Favorite Ding' }));
    const heads = within(body)
      .getAllByRole('heading')
      .map((h) => h.textContent);
    expect(heads).toEqual(['Favorites', 'This server', 'Default sounds']);
    // Ding is listed under Favorites and under Default sounds, both starred.
    const stars = within(body).getAllByRole('button', { name: 'Favorite Ding' });
    expect(stars).toHaveLength(2);
    expect(stars.every((x) => x.getAttribute('aria-pressed') === 'true')).toBe(true);
    expect(useSoundboard.getState().favorites).toEqual(['default:2']);
    await user.click(stars[0]!);
    expect(within(body).queryByRole('heading', { name: 'Favorites' })).toBeNull();
  });

  it('shows favorites from the synced settings and follows a change from another session (H-45)', async () => {
    useStore.setState({
      userSettings: { version: 1, settings: { soundboardFavorites: ['default:2'] } },
    });
    const user = open();
    const body = await popover(user);
    expect(within(body).getByRole('heading', { name: 'Favorites' })).toBeInTheDocument();
    act(() => {
      useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', {
        version: 2,
        settings: { soundboardFavorites: [] },
      });
    });
    expect(within(body).queryByRole('heading', { name: 'Favorites' })).toBeNull();
    useStore.setState({ userSettings: null });
  });

  it('previews a sound for you alone and says when it cannot play', async () => {
    previewSound.mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    const user = open();
    const body = await popover(user);
    await user.click(within(body).getByRole('button', { name: /Preview Chime/ }));
    expect(previewSound).toHaveBeenCalledWith(expect.objectContaining({ id: '1' }));
    expect(play).not.toHaveBeenCalled();
    await user.click(within(body).getByRole('button', { name: /Preview Ding/ }));
    expect(await within(body).findByRole('alert')).toHaveTextContent('could not be played');
  });

  it('changes the volume of what you play', async () => {
    const user = open();
    const body = await popover(user);
    const slider = within(body).getByRole('slider', { name: /Soundboard volume/ });
    fireEvent.change(slider, { target: { value: '35' } });
    await waitFor(() => expect(useSoundboard.getState().volume).toBe(35));
  });

  it('disables playing while muted, and explains it', async () => {
    patchSession({ selfMute: true });
    const user = open();
    const body = await popover(user);
    expect(within(body).getByRole('button', { name: 'Play Chime' })).toBeDisabled();
    expect(within(body).getByText('Unmute and undeafen to play a sound.')).toBeInTheDocument();
    // Previewing is still allowed.
    expect(within(body).getByRole('button', { name: /Preview Chime/ })).toBeEnabled();
  });

  it('has no a11y violations', async () => {
    const user = open();
    const body = await popover(user);
    expect(await axeViolations(body)).toEqual([]);
  });
});
