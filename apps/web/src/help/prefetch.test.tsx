// T-05 (D-371): the skeleton while the handbook's words arrive, and the prefetch from the footer help link.
import { act, render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('/help while the full catalog is still loading', () => {
  it('shows the entrance skeleton, then the handbook once the words arrive', async () => {
    vi.resetModules();
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { createAppRouter } = await import('../router');
    const store = await import('../i18n/messages/store');
    render(
      <RouterProvider
        router={createAppRouter(createMemoryHistory({ initialEntries: ['/help'] }))}
      />,
    );
    expect(store.isRestLoaded()).toBe(false);
    const skeleton = await screen.findByRole('status', { busy: true });
    expect(skeleton.querySelector('.ent-skeleton')).not.toBeNull();
    // It is the entrance chrome, not a blank page, and shows no message key.
    expect(document.body.textContent ?? '').not.toMatch(/\b[a-z]+\.[A-Za-z]+\.[A-Za-z.]+\b/);
    await act(async () => {
      await import('../i18n/messages/en.rest');
    });
    expect(
      await screen.findByRole('heading', { level: 2, name: 'Joining, invitations and credits' }),
    ).toBeInTheDocument();
    warn.mockRestore();
  });
});

describe('prefetch', () => {
  it("prefetchHelp loads the full catalog and the What's new chunk, and never throws", async () => {
    vi.resetModules();
    const loadFullCatalog = vi.fn().mockRejectedValue(new Error('offline'));
    vi.doMock('../i18n/runtime', () => ({ loadFullCatalog }));
    const { prefetchHelp } = await import('./prefetch');
    expect(() => prefetchHelp()).not.toThrow();
    expect(loadFullCatalog).toHaveBeenCalledTimes(1);
    vi.doUnmock('../i18n/runtime');
  });

  it('the footer help link of the Library calls it on pointer-enter and focus', () => {
    const src = readFileSync(resolve(__dirname, '../library/Home.tsx'), 'utf8');
    expect(src).toMatch(
      /href="\/help"\s+onPointerEnter=\{prefetchHelp\}\s+onFocus=\{prefetchHelp\}/,
    );
  });
});
