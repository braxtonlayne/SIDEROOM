import { render, screen, fireEvent } from '@testing-library/react';
import { readPlayerDefaults } from '../../../cinema/player/defaults';
import { DEFAULT_READER_PREFS, preloadCountOf, useReaderPrefsStore } from '../../../library/prefs';
import { preloadWindow } from '../../../library/reader/model';
import { CinemaDefaultsCard, LibraryDefaultsCard } from '../sections/ActivityHistory';

describe('Activity defaults (LNG-USET-035)', () => {
  beforeEach(() => {
    localStorage.clear();
    useReaderPrefsStore.setState({ global: {}, series: {} });
  });

  it('keeps the page preload count inside 1..10', () => {
    expect(DEFAULT_READER_PREFS.preloadCount).toBe(3);
    expect(preloadCountOf(0)).toBe(1);
    expect(preloadCountOf(99)).toBe(10);
    expect(preloadCountOf(4.4)).toBe(4);
    expect(preloadCountOf('x')).toBe(3);
    expect(preloadWindow(0, 20, preloadCountOf(5))).toEqual([1, 2, 3, 4, 5]);
  });

  it('saves the preload count as the default for every series', () => {
    render(<LibraryDefaultsCard />);
    fireEvent.change(screen.getByRole('slider', { name: 'Pages to load ahead' }), {
      target: { value: '6' },
    });
    expect(useReaderPrefsStore.getState().global.preloadCount).toBe(6);
  });

  it('saves the audio language and the quality cap with the player defaults', () => {
    expect(readPlayerDefaults()).toMatchObject({ audioLang: null, qualityCap: null });
    render(<CinemaDefaultsCard />);
    expect(screen.getByText('Preferred audio language')).toBeInTheDocument();
    expect(screen.getByText('Highest video quality')).toBeInTheDocument();
    localStorage.setItem(
      'sr.cinema.player.v1',
      JSON.stringify({ audioLang: 'ja', qualityCap: 720 }),
    );
    expect(readPlayerDefaults()).toMatchObject({ audioLang: 'ja', qualityCap: 720 });
    localStorage.setItem('sr.cinema.player.v1', JSON.stringify({ qualityCap: 1234 }));
    expect(readPlayerDefaults().qualityCap).toBeNull();
  });
});
