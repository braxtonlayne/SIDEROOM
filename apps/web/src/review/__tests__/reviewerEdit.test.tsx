import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { http, HttpResponse, server } from '../../test/server';
import { ReviewerEdit } from '../../admin/sections/ReviewerEdit';
import { ChapterRename } from '../ChapterRename';
import { eventLabel } from '../shared';
import type { DraftChapter } from '../types';
import { preview, submission } from './fixtures';

const chapter = (o: Partial<DraftChapter> = {}): DraftChapter => ({
  id: 'c1',
  series_id: '900',
  edition_id: 'e1',
  volume_id: null,
  number: 3,
  title: 'Old title',
  page_count: 20,
  format: 'epub',
  release_group: 'grp',
  published: false,
  ...o,
});

const wrap = (ui: React.ReactNode) =>
  render(<QueryClientProvider client={new QueryClient()}>{ui}</QueryClientProvider>);

describe('reviewer edit in place (K-06)', () => {
  beforeEach(() => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.get('*/api/v1/submissions/5/duplicates', () => HttpResponse.json({ matches: [] })),
      http.get('*/api/v1/submissions/lookup/providers', () =>
        HttpResponse.json({ enabled: false, providers: [] }),
      ),
    );
  });

  it('labels the panel and renames a chapter through the submission routes, keeping its other fields', async () => {
    const user = userEvent.setup();
    let body: unknown;
    server.use(
      http.get('*/api/v1/submissions/5/library/chapters', () =>
        HttpResponse.json({ chapters: [chapter({ volume_id: 'v1' })], volumes: [] }),
      ),
      http.put('*/api/v1/submissions/5/library/chapters/c1', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(chapter());
      }),
    );
    const onChanged = vi.fn();
    wrap(
      <ReviewerEdit
        submission={submission('5', {
          state: 'in_review',
          claimed_by: '9',
          claim_expires_at_ms: Date.now() + 600_000,
        })}
        preview={preview()}
        onChanged={onChanged}
      />,
    );
    expect(await screen.findByText('Editing as reviewer')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: /Edit Chapter 3/ }));
    const title = screen.getByRole('textbox', { name: 'Chapter title (optional)' });
    await user.clear(title);
    await user.type(title, 'Fixed');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(body).toBeDefined());
    expect(body).toEqual({
      number: 3,
      title: 'Fixed',
      format: 'epub',
      release_group: 'grp',
      volume_id: 'v1',
    });
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it('shows the server refusal when the claim is gone', async () => {
    const user = userEvent.setup();
    server.use(
      http.get('*/api/v1/submissions/5/library/chapters', () =>
        HttpResponse.json({ chapters: [chapter()] }),
      ),
      http.delete('*/api/v1/submissions/5/library/chapters/c1', () =>
        HttpResponse.json(
          { code: 900001, message: 'you do not hold the review claim on this submission' },
          { status: 409 },
        ),
      ),
    );
    wrap(
      <ReviewerEdit
        submission={submission('5', { state: 'in_review' })}
        preview={undefined}
        onChanged={() => undefined}
      />,
    );
    await user.click(await screen.findByRole('button', { name: /Remove Chapter 3/ }));
    expect(await screen.findByText(/do not hold the review claim/)).toBeInTheDocument();
  });

  it('names a reviewer edit on the timeline', () => {
    expect(
      eventLabel({
        id: '1',
        submission_id: '5',
        kind: 'metadata_edited',
        actor_id: '9',
        body: null,
        internal: false,
        data: { fields: ['cover', 'chapters'] },
        created_at_ms: 0,
      }),
    ).toBe('Edited by a reviewer: cover, chapters');
  });
});

describe('chapter rename keeps its volume (K-07)', () => {
  it('offers the volumes of the edition and sends the chosen one back', async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ChapterRename
        chapter={chapter({ volume_id: 'v1' })}
        volumes={[
          { id: 'v1', edition_id: 'e1', number: 1, title: null },
          { id: 'v2', edition_id: 'e1', number: 2, title: 'Second' },
          { id: 'v9', edition_id: 'other', number: 9, title: null },
        ]}
        busy={false}
        onSave={onSave}
        onCancel={() => undefined}
      />,
    );
    const select = screen.getByRole('combobox', { name: 'Volume' });
    expect(select).toHaveTextContent('Volume 1');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onSave).toHaveBeenCalledWith({ number: 3, title: 'Old title', volume_id: 'v1' });
  });

  it('shows no volume select when the series has no volumes', () => {
    render(
      <ChapterRename
        chapter={chapter()}
        busy={false}
        onSave={() => undefined}
        onCancel={() => undefined}
      />,
    );
    expect(screen.queryByRole('combobox', { name: 'Volume' })).not.toBeInTheDocument();
  });
});
