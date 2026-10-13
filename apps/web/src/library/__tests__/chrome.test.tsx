import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { AgeGateProvider, useAgeGate, withAgeGate } from '../../lib/ageGate';
import { ApiError } from '../../api/client';
import { renderAt, useLibraryApi } from './harness';

describe('useAgeGate (shared with the Lounge interstitial)', () => {
  function Probe({ onResult }: { onResult: (v: boolean) => void }) {
    const { ensureAdult } = useAgeGate();
    return <button onClick={() => void ensureAdult().then(onResult)}>ask</button>;
  }

  it('asks once, remembers a yes, and shares one dialog between concurrent callers', async () => {
    const posts: unknown[] = [];
    server.use(
      http.post('*/api/v1/sr/age-gate', async ({ request }) => {
        posts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const results: boolean[] = [];
    const user = userEvent.setup();
    render(
      <AgeGateProvider>
        <Probe onResult={(v) => results.push(v)} />
      </AgeGateProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'ask' }));
    await user.click(screen.getByRole('button', { name: 'ask' })); // second caller while open
    expect(screen.getAllByRole('dialog')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: 'Yes' }));
    await waitFor(() => expect(results).toEqual([true, true]));
    await user.click(screen.getByRole('button', { name: 'ask' })); // already confirmed: no dialog
    await waitFor(() => expect(results).toEqual([true, true, true]));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(posts).toEqual([{ over_18: true }]);
  });

  it('withAgeGate retries once on 900011 and rethrows on a no', async () => {
    const gateErr = () => new ApiError(403, 'x', { code: 900011 });
    let n = 0;
    const fn = () => (n++ === 0 ? Promise.reject(gateErr()) : Promise.resolve('ok'));
    await expect(withAgeGate(fn, () => Promise.resolve(true))).resolves.toBe('ok');
    expect(n).toBe(2);
    await expect(
      withAgeGate(
        () => Promise.reject(gateErr()),
        () => Promise.resolve(false),
      ),
    ).rejects.toBeInstanceOf(ApiError);
    const other = new ApiError(403, 'csrf', { code: 1 });
    await expect(
      withAgeGate(
        () => Promise.reject(other),
        () => Promise.resolve(true),
      ),
    ).rejects.toBe(other);
  });
});

describe('library chrome', () => {
  it('the account menu toggles library.hide_age_restricted in the settings blob', async () => {
    useLibraryApi();
    const patches: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json({
          version: 2,
          settings: { library: { hide_age_restricted: true } },
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library');
    await user.click(await screen.findByRole('button', { name: 'Account menu' }));
    await user.click(await screen.findByRole('menuitemcheckbox', { name: 'Hide 18+ titles' }));
    await waitFor(() =>
      expect(patches).toEqual([{ settings: { library: { hide_age_restricted: true } } }]),
    );
  });

  it('the bell lists updates from followed series', async () => {
    useLibraryApi();
    const user = userEvent.setup();
    renderAt('/library');
    await user.click(await screen.findByRole('button', { name: 'Notifications' }));
    const menu = await screen.findByRole('menu', { name: 'Notifications' });
    expect(await within(menu).findByText(/Frieren: Chapter 3 · English/)).toBeInTheDocument();
  });

  it('on a phone the sidebar is a drawer and quick links sit at the bottom', async () => {
    useLibraryApi();
    const original = window.matchMedia;
    window.matchMedia = ((q: string) => ({
      matches: q.includes('max-width'),
      media: q,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    })) as unknown as typeof window.matchMedia;
    try {
      const user = userEvent.setup();
      renderAt('/library');
      expect(
        await screen.findByRole('navigation', { name: 'Library quick links' }),
      ).toBeInTheDocument();
      expect(screen.queryByRole('navigation', { name: 'Library' })).toBeNull();
      await user.click(screen.getByRole('button', { name: 'Open library menu' }));
      const drawer = await screen.findByRole('dialog', { name: 'The library' });
      expect(within(drawer).getByText('Reading lists')).toBeInTheDocument();
    } finally {
      window.matchMedia = original;
    }
  });
});
