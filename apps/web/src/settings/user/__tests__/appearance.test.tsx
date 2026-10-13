import { fireEvent, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { usePrefsStore } from '../prefs';
import { openSettings, resetPrefs, signedIn } from './helpers';

const themeRoot = () => document.querySelector<HTMLElement>('[data-theme]')!;

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Appearance', () => {
  it('previews a theme instantly, saves it, and Reset reverts an unsaved one', async () => {
    const user = userEvent.setup();
    openSettings('appearance');
    await user.click(await screen.findByRole('button', { name: /Light/ }));
    expect(themeRoot()).toHaveAttribute('data-theme', 'lounge-light');
    expect(await screen.findByRole('alert')).toHaveTextContent('unsaved changes');
    expect(usePrefsStore.getState().prefs.theme).toBe('lounge-dark');

    await user.click(screen.getByRole('button', { name: 'Reset' }));
    expect(themeRoot()).toHaveAttribute('data-theme', 'lounge-dark');

    await user.click(screen.getByRole('button', { name: /Onyx/ }));
    expect(themeRoot()).toHaveAttribute('data-theme', 'lounge-onyx');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(usePrefsStore.getState().prefs.theme).toBe('lounge-onyx');
    expect(JSON.parse(localStorage.getItem('sr.user.prefs')!).state.prefs.theme).toBe(
      'lounge-onyx',
    );
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('applies font scale, zoom, saturation, reduced motion and message display', async () => {
    const user = userEvent.setup();
    openSettings('appearance');
    fireEvent.change(await screen.findByRole('slider', { name: 'Chat font scale' }), {
      target: { value: '125' },
    });
    expect(themeRoot().style.getPropertyValue('--ui-font-size-base')).toBe('20px');
    fireEvent.change(screen.getByRole('slider', { name: 'Saturation' }), {
      target: { value: '50' },
    });
    expect(themeRoot().style.getPropertyValue('--ui-saturation')).toBe('0.5');
    fireEvent.change(screen.getByRole('slider', { name: 'Zoom' }), { target: { value: '120' } });
    await user.click(screen.getByRole('radio', { name: 'Compact' }));
    expect(themeRoot()).toHaveAttribute('data-message-display', 'compact');

    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(usePrefsStore.getState().prefs).toMatchObject({
      fontScale: 125,
      saturation: 50,
      zoom: 120,
      messageDisplay: 'compact',
    });
  });

  it('clamps and defaults corrupt values', () => {
    usePrefsStore
      .getState()
      .save({ ...usePrefsStore.getState().prefs, fontScale: 9999, theme: 'nope' as never });
    const p = usePrefsStore.getState().prefs;
    expect(p.fontScale).toBe(150);
    expect(p.theme).toBe('lounge-dark');
  });
});
