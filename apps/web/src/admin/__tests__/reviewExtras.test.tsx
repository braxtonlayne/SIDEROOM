import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { manifestDiff, safeThemeCss } from '../../review/ReviewExtras';
import { detail, preview, submission } from '../../review/__tests__/fixtures';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

describe('manifest diff', () => {
  it('lists added, removed and changed leaves', () => {
    const changes = manifestDiff(
      { name: 'App', scopes: ['a'], hooks: { url: 'https://a.test', old: 1 } },
      { name: 'App 2', scopes: ['a', 'b'], hooks: { url: 'https://a.test', fresh: true } },
    );
    expect(changes.map((c) => `${c.kind}:${c.path}`)).toEqual([
      'added:hooks.fresh',
      'removed:hooks.old',
      'changed:name',
      'changed:scopes',
    ]);
    expect(manifestDiff({ a: 1 }, { a: 1 })).toEqual([]);
  });
});

describe('package preview', () => {
  it('drops imports, urls and expressions from the theme css', () => {
    const css = safeThemeCss(
      '@import url(https://evil.test/x.css); body{background:url(https://evil.test/a.png);width:expression(alert(1));color:red}',
    );
    expect(css).not.toContain('evil');
    expect(css).not.toContain('expression');
    expect(css).toContain('color:red');
  });
});

describe('review page extras', () => {
  beforeEach(() => asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING'] }));

  function api(resolved: unknown[] = []) {
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({
          items: [submission('1', { title: 'Dungeon Tales' })],
          total: 1,
          limit: 25,
          offset: 0,
        }),
      ),
      http.get('*/api/v1/admin/review/submissions/:id', () =>
        HttpResponse.json(
          detail({
            id: '1',
            title: 'Dungeon Tales',
            state: 'in_review',
            claimed_by: '1',
            claim_expires_at_ms: Date.now() + 20 * 60_000,
          }),
        ),
      ),
      http.get('*/api/v1/admin/review/submissions/:id/preview', () =>
        HttpResponse.json({ preview: preview() }),
      ),
      http.get('*/api/v1/admin/review/submissions/:id/duplicates', () =>
        HttpResponse.json({
          matches: [
            {
              section: 'library',
              ref_id: '42',
              title: 'Dungeon Tales Remastered',
              year: 2020,
              score: 90,
              reasons: ['title', 'cover'],
              href: '/library/series/42',
            },
          ],
          resolution: null,
        }),
      ),
      http.put('*/api/v1/admin/review/submissions/:id/duplicate', async ({ request }) => {
        resolved.push(await readJson(request));
        return HttpResponse.json({
          matches: [],
          resolution: {
            action: 'merge',
            target: '42',
            note: null,
            decided_by: '1',
            decided_at_ms: 1,
          },
        });
      }),
      http.get('*/api/v1/admin/review/submissions/:id/media', () =>
        HttpResponse.json({
          items: [
            {
              ref_id: '5',
              label: 'S01E01',
              state: 'ready',
              url: '/cdn/transcode/1/1/proxy/proxy.mp4?ex=1&sig=x',
              duration_ms: 60000,
            },
            { ref_id: '6', label: 'S01E02', state: 'processing', url: null, duration_ms: null },
          ],
        }),
      ),
    );
  }

  it('shows duplicates and records a merge decision', async () => {
    const sent: unknown[] = [];
    api(sent);
    const user = userEvent.setup();
    openAdmin('review');
    await user.click(await screen.findByRole('button', { name: 'Open Dungeon Tales' }));
    expect(await screen.findByText(/Match 90%/)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Dungeon Tales Remastered' })).toHaveAttribute(
      'href',
      '/library/series/42',
    );
    await user.click(screen.getByRole('button', { name: 'Merge into Dungeon Tales Remastered' }));
    await waitFor(() => expect(sent).toEqual([{ action: 'merge', target: '42' }]));
    expect(await screen.findByText(/earns no credit/)).toBeInTheDocument();
  });

  it('plays a ready proxy with a scrubber and explains one that is still processing', async () => {
    api();
    const user = userEvent.setup();
    openAdmin('review');
    await user.click(await screen.findByRole('button', { name: 'Open Dungeon Tales' }));
    const video = await screen.findByLabelText('Video for S01E01');
    expect(video).toHaveAttribute(
      'src',
      expect.stringContaining('/cdn/transcode/1/1/proxy/proxy.mp4'),
    );
    expect(screen.getByRole('slider', { name: 'Position in S01E01' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'One frame back' })).toBeInTheDocument();
    expect(
      screen.getByText('The review proxy for S01E02 is still being made.'),
    ).toBeInTheDocument();
  });
});
