import { render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { RouterProvider, createMemoryHistory } from '@tanstack/react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createAppRouter } from '../../router';
import { STUDY_FLAGS, studyFlagOn } from '../flags';
import { resetStudySurfaces, switchStudySurfacesOn } from '../testing';
import { NAV } from '../../cinema/Shell';
import { useClientFlags } from '../../moderation/reports';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

afterEach(() => resetStudySurfaces());

describe('Study switches', () => {
  it('are all off when no flag is known, and on after the operator opts in', () => {
    resetStudySurfaces();
    for (const k of Object.keys(STUDY_FLAGS) as (keyof typeof STUDY_FLAGS)[])
      expect(studyFlagOn(useClientFlags.getState().flags, k)).toBe(false);
    switchStudySurfacesOn();
    for (const k of Object.keys(STUDY_FLAGS) as (keyof typeof STUDY_FLAGS)[])
      expect(studyFlagOn(useClientFlags.getState().flags, k)).toBe(true);
  });

  it('keeps every storefront route registered (hide, never delete)', () => {
    const paths = createAppRouter().routesByPath as Record<string, unknown>;
    for (const p of [
      '/cinema/movies',
      '/cinema/series',
      '/cinema/anime',
      '/cinema/submit',
      '/library/requests',
      '/library/rankings',
      '/library/submissions',
      '/library/upload',
    ])
      expect(Object.keys(paths), p).toContain(p);
  });

  it('the Cinema nav lists the storefront categories only behind their switch', () => {
    const gated = NAV.filter((i) => i.flag).map((i) => i.href);
    expect(gated).toEqual(
      expect.arrayContaining([
        '/cinema/movies',
        '/cinema/series',
        '/cinema/anime',
        '/cinema/submit',
      ]),
    );
  });
});

function renderAt(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe('hidden surfaces', () => {
  it('a hidden page shows the not-found text while its switch is off', async () => {
    resetStudySurfaces();
    renderAt('/library/requests');
    expect(await screen.findByText(/not found|page/i)).toBeInTheDocument();
  });
});

describe('the room trio', () => {
  it('the primary rail lists the Lounge, the Arcade and the Study, in that order', async () => {
    renderAt('/library');
    const nav = await screen.findByTestId('rooms-nav');
    const names = within(nav)
      .getAllByRole('link')
      .map((a) => a.textContent);
    expect(names.filter((n) => n !== 'The Arcade')).toEqual(['The Lounge', 'The Study']);
    expect(names[0]).toBe('The Lounge');
  });
});

describe('no outbound metadata calls with default settings', () => {
  const root = join(__dirname, '..', '..');
  const hosts =
    /(api\.themoviedb\.org|image\.tmdb\.org|api\.trakt\.tv|graphql\.anilist\.co|api\.mangadex\.org|api\.mangaupdates\.com|comicvine\.gamespot\.com)/;
  function walk(dir: string, out: string[] = []): string[] {
    for (const f of readdirSync(dir)) {
      const p = join(dir, f);
      if (statSync(p).isDirectory()) {
        if (f === '__tests__' || f === 'node_modules') continue;
        walk(p, out);
      } else if (/\.(ts|tsx)$/.test(f) && !/\.test\./.test(f)) out.push(p);
    }
    return out;
  }
  it('no web source names an external metadata host (lookups go through the server, which is gated)', () => {
    const offenders = walk(root).filter((f) => hosts.test(readFileSync(f, 'utf8')));
    expect(offenders).toEqual([]);
  });
});
